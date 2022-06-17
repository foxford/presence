use crate::{
    app::{
        self, history_manager,
        metrics::AuthzMeasure,
        nats::PRESENCE_SENDER_AGENT_ID,
        replica,
        session_manager::Session,
        state::State,
        ws::{
            event::{Event, Label},
            ConnectError, ConnectRequest, Request, Response,
        },
    },
    authz::AuthzObject,
    authz_hack,
    classroom::ClassroomId,
    db::agent_session::{self, InsertResult},
    session::SessionId,
    session::SessionKey,
};
use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Extension, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use serde::Serialize;
use sqlx::types::time::OffsetDateTime;
use std::{str::FromStr, sync::Arc};
use svc_agent::AgentId;
use svc_authn::{
    jose::ConfigMap, token::jws_compact::extract::decode_jws_compact_with_config, AccountId,
    Authenticable,
};
use tokio::time::{interval, timeout, MissedTickBehavior};
use tracing::{error, info};

pub async fn handler<S: State>(
    ws: WebSocketUpgrade,
    Extension(state): Extension<S>,
    Extension(authn): Extension<Arc<ConfigMap>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, authn, state))
}

async fn handle_socket<S: State>(socket: WebSocket, authn: Arc<ConfigMap>, state: S) {
    let (mut sender, mut receiver) = socket.split();

    // Close connection if there is no messages for some time
    let authn_timeout = timeout(
        state.config().websocket.authentication_timeout,
        receiver.next(),
    )
    .await;

    // Authentication
    let (session_id, session_key, mut nats_rx, mut close_rx) = match authn_timeout {
        Ok(Some(result)) => match result {
            Ok(msg) => match handle_authn_message(msg, authn, state.clone()).await {
                Ok((m, (session_id, session_key))) => {
                    // To close old connections from the same agents
                    let close_rx = match state.register_session(session_key.clone(), session_id) {
                        Ok(rx) => rx,
                        Err(e) => {
                            error!(error = %e, "Failed to register session_key: {}", session_key);
                            close_connection_with_error(sender, Response::from(e)).await;
                            return;
                        }
                    };

                    // Subscribe to events from nats
                    let nats_rx = match state
                        .nats_client()
                        .subscribe(session_key.clone().classroom_id)
                        .await
                    {
                        Err(e) => {
                            error!(error = ?e);
                            close_connection_with_error(sender, Response::from(e)).await;
                            return;
                        }
                        Ok(rx) => rx,
                    };

                    // Send agent.enter others
                    let event = Event::new(Label::AgentEnter).payload(session_key.clone().agent_id);
                    if let Err(e) = state
                        .nats_client()
                        .publish_event(session_key.clone(), event)
                    {
                        error!(error = %e, "Failed to send agent.enter notification");
                        close_connection_with_error(
                            sender,
                            Response::from(ConnectError::MessagingFailed),
                        )
                        .await;
                        return;
                    }

                    info!("Successful authentication, session_key: {}", &session_key);
                    let _ = sender.send(Message::Text(m)).await;
                    state.metrics().ws_connection_success().inc();

                    (session_id, session_key, nats_rx, close_rx)
                }
                Err(e) => {
                    info!("Connection is closed (unsuccessful request)");
                    state.metrics().ws_connection_error().inc();
                    close_connection_with_error(sender, Response::from(e)).await;
                    return;
                }
            },
            Err(e) => {
                error!(error = %e, "An error occurred when receiving an authn message");
                return;
            }
        },
        Ok(None) | Err(_) => {
            info!("Connection is closed (authentication timeout exceeded)");
            close_connection_with_error(sender, Event::new(Label::AuthTimedOut)).await;
            return;
        }
    };

    state.metrics().ws_connection_total().inc();

    let mut ping_sent = false;

    // Ping/Pong intervals
    let mut ping_interval = interval(state.config().websocket.ping_interval);
    let mut pong_expiration_interval = interval(state.config().websocket.pong_expiration_interval);

    // In order not to hurry
    ping_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    pong_expiration_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            Ok(msg) = nats_rx.recv() => {
                if let Some(sender_id) = msg.headers.and_then(|h| {
                    h.get(PRESENCE_SENDER_AGENT_ID)
                        .and_then(|s| AgentId::from_str(s).ok())
                }) {
                    // Don't send events to yourself
                    if session_key.agent_id == sender_id {
                        continue;
                    }
                }

                match String::from_utf8(msg.data) {
                    Ok(s) => {
                        if let Err(e) = sender.send(Message::Text(s)).await {
                            error!(error = ?e, "Failed to send notification");
                        }
                    },
                    Err(e)=> {
                        error!(error = ?e, "Conversion to str failed");
                    }
                }
            }
            // Get Pong/Close messages from client
            Some(result) = receiver.next() => {
                match result {
                    Ok(Message::Pong(_)) => {
                        ping_sent = false;
                    },
                    Ok(Message::Close(frame)) => {
                        match frame {
                            Some(f) => {
                                info!(code = %f.code, reason = %f.reason, "An agent closed connection");
                            }
                            None => info!("An agent closed connection"),
                        }

                        break;
                    },
                    Err(e) => {
                        error!(error = %e, "An error occurred when receiving a message");

                        break;
                    },
                    _ => {
                        // Ping/Text/Binary messages - do nothing
                    }
                }
            }
            // Ping authenticated clients with interval
            _ = ping_interval.tick() => {
                if sender.send(Message::Ping(Vec::new())).await.is_err() {
                    info!("An agent disconnected (ping not sent)");
                    break;
                }

                ping_sent = true;
                pong_expiration_interval.reset();
            }
            // Close connection if pong exceeded timeout
            _ = pong_expiration_interval.tick() => {
                if ping_sent {
                    info!("Connection is closed (pong timeout exceeded)");
                    close_connection_with_error(sender, Event::new(Label::PongTimedOut)).await;
                    break;
                }
            }
            // Close old connections
            result = &mut close_rx => {
                info!("Old connection is closed");

                let event = Event::new(Label::AgentReplaced).payload(session_key.agent_id.clone());
                close_connection_with_error(sender, event).await;
                state.metrics().ws_connection_total().dec();

                // Skip moving old session to history on the same replica
                if result.is_err() {
                    return;
                }

                if let Err(err) = history_manager::move_single_session(state, session_id).await {
                    error!(error = %err, "Failed to move session to history");
                }

                return;
            }
        }
    }

    state.metrics().ws_connection_total().dec();

    let event = Event::new(Label::AgentLeave).payload(session_key.agent_id.clone());
    if let Err(e) = state
        .nats_client()
        .publish_event(session_key.clone(), event)
    {
        error!(error = %e, "Failed to send agent.leave notification");
        return;
    }

    if let Err(e) = state.terminate_session(session_key.clone(), false).await {
        error!(error = %e, "Failed to terminate session_key: {}", session_key);
        return;
    }

    if let Err(err) = history_manager::move_single_session(state, session_id).await {
        error!(error = %err, "Failed to move session to history");
    }
}

/// Closes the WebSocket connection with an error message
async fn close_connection_with_error<T: Serialize>(
    mut sender: SplitSink<WebSocket, Message>,
    resp: T,
) {
    let resp = serialize_to_json(&resp);
    let _ = sender.send(Message::Text(resp)).await;
    let _ = sender.close().await;
}

async fn handle_authn_message<S: State>(
    message: Message,
    authn: Arc<ConfigMap>,
    state: S,
) -> Result<(String, (SessionId, SessionKey)), ConnectError> {
    let msg = match message {
        Message::Text(msg) => msg,
        _ => return Err(ConnectError::UnsupportedRequest),
    };

    let result = serde_json::from_str::<Request>(&msg);
    match result {
        Ok(Request::ConnectRequest(ConnectRequest {
            token,
            classroom_id,
            agent_label,
        })) => {
            let agent_id = get_agent_id_from_token(token, authn, agent_label).map_err(|e| {
                error!(error = %e, "Failed to authenticate an agent");
                ConnectError::Unauthenticated
            })?;

            authorize_agent(state.clone(), &agent_id, &classroom_id).await?;
            let session_id = create_agent_session(state, classroom_id, &agent_id).await?;

            Ok((
                serialize_to_json(&Response::ConnectSuccess),
                (session_id, SessionKey::new(agent_id, classroom_id)),
            ))
        }
        Err(err) => {
            error!(error = %err, "Failed to serialize a message");
            Err(ConnectError::SerializationFailed)
        }
    }
}

async fn create_agent_session<S: State>(
    state: S,
    classroom_id: ClassroomId,
    agent_id: &AgentId,
) -> Result<SessionId, ConnectError> {
    let mut conn = state.get_conn().await.map_err(|e| {
        error!(error = %e, "Failed to get db connection");
        ConnectError::DbConnAcquisitionFailed
    })?;

    // Attempt to close old session on the same replica
    let session_key = SessionKey::new(agent_id.clone(), classroom_id);
    // If the session is found, don't create a new session and return the previous id
    match state.terminate_session(session_key.clone(), true).await {
        Ok(Session::Found(session_id)) => {
            return Ok(session_id);
        }
        Err(e) => {
            error!(error = %e, "Failed to terminate session: {}", &session_key);
            return Err(ConnectError::MessagingFailed);
        }
        _ => {}
    }

    let insert_query = agent_session::InsertQuery::new(
        agent_id,
        classroom_id,
        state.replica_id(),
        OffsetDateTime::now_utc(),
    );

    match insert_query.execute(&mut conn).await {
        InsertResult::Ok(agent_session) => Ok(agent_session.id),
        InsertResult::Error(err) => {
            error!(error = %err, "Failed to create an agent session");
            Err(ConnectError::DbQueryFailed)
        }
        InsertResult::UniqIdsConstraintError => {
            use app::api::v1::session::Response as DeleteResponse;

            let replica = agent_session::GetReplicaQuery::new(session_key.clone())
                .execute(&mut conn)
                .await
                .map_err(|e| {
                    error!(error = %e, "Failed to get replica id from db");
                    ConnectError::DbQueryFailed
                })?;

            match replica::close_connection(state.clone(), replica.id, session_key).await {
                Ok(resp) => match resp {
                    DeleteResponse::DeleteSuccess => {
                        match agent_session::UpdateReplicaQuery::new(
                            agent_id.clone(),
                            classroom_id,
                            state.replica_id(),
                        )
                        .execute(&mut conn)
                        .await
                        {
                            Ok(session) => Ok(session.id),
                            Err(e) => {
                                error!(error = %e, "Failed to update replica_id");
                                Err(ConnectError::CloseOldConnectionFailed)
                            }
                        }
                    }
                    DeleteResponse::DeleteFailure(r) => {
                        error!(reason = %r, "Failed to close connection on another replica");
                        Err(ConnectError::CloseOldConnectionFailed)
                    }
                },
                Err(e) => {
                    error!(error = %e, "Failed to close connection on another replica");
                    Err(ConnectError::CloseOldConnectionFailed)
                }
            }
        }
    }
}

async fn authorize_agent<S: State>(
    state: S,
    agent_id: &AgentId,
    classroom_id: &ClassroomId,
) -> Result<(), ConnectError> {
    let account_id = agent_id.as_account_id();
    let object = AuthzObject::new(&["classrooms", &classroom_id.to_string()]).into();

    if let Err(err) = state
        .authz()
        .authorize(
            authz_hack::remove_unwanted_parts_from_audience(account_id.audience()),
            account_id.clone(),
            object,
            "connect".into(),
        )
        .await
        .measure()
    {
        error!(error = %err, "Failed to authorize action");
        return Err(ConnectError::AccessDenied);
    };

    Ok(())
}

fn get_agent_id_from_token(
    token: String,
    authn: Arc<ConfigMap>,
    agent_label: String,
) -> Result<AgentId> {
    let data = decode_jws_compact_with_config::<String>(&token, authn.as_ref())?;
    let claims = data.claims;

    let account = AccountId::new(claims.subject(), claims.audience());
    let agent_id = AgentId::new(&agent_label, account);

    Ok(agent_id)
}

fn serialize_to_json<T: Serialize>(response: &T) -> String {
    serde_json::to_string(&response).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::prelude::*;
    use serde_json::json;
    use uuid::Uuid;

    mod handle_authn_message {
        use super::*;
        use crate::db;
        use std::net::{IpAddr, Ipv4Addr};

        #[tokio::test]
        async fn unsupported_request() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let msg = Message::Binary("".into());
            let authn = Arc::new(authn::new());
            let state = TestState::new(db_pool, TestAuthz::new(), Uuid::new_v4());

            let result = handle_authn_message(msg, authn, state)
                .await
                .expect_err("Unexpectedly succeeded");

            assert_eq!(result, ConnectError::UnsupportedRequest);
        }

        #[tokio::test]
        async fn serialization_failed() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let msg = Message::Text("".into());
            let authn = Arc::new(authn::new());
            let state = TestState::new(db_pool, TestAuthz::new(), Uuid::new_v4());

            let result = handle_authn_message(msg, authn, state)
                .await
                .expect_err("Unexpectedly succeeded");

            assert_eq!(result, ConnectError::SerializationFailed);
        }

        #[tokio::test]
        async fn unauthenticated() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let classroom_id: ClassroomId = Uuid::new_v4().into();

            let cmd = json!({
                "type": "connect_request",
                "payload": {
                    "classroom_id": classroom_id,
                    "token": "1234",
                    "agent_label": "http"
                }
            });

            let msg = Message::Text(cmd.to_string());
            let authn = Arc::new(authn::new());
            let state = TestState::new(db_pool, TestAuthz::new(), Uuid::new_v4());

            let result = handle_authn_message(msg, authn, state)
                .await
                .expect_err("Unexpectedly succeeded");

            assert_eq!(result, ConnectError::Unauthenticated);
        }

        #[tokio::test]
        async fn unauthorized() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let classroom_id: ClassroomId = Uuid::new_v4().into();
            let agent = TestAgent::new("http", "user123", USR_AUDIENCE);
            let token = agent.token();

            let cmd = json!({
                "type": "connect_request",
                "payload": {
                    "classroom_id": classroom_id,
                    "token": token,
                    "agent_label": "http"
                }
            });

            let msg = Message::Text(cmd.to_string());
            let authn = Arc::new(authn::new());
            let state = TestState::new(db_pool, TestAuthz::new(), Uuid::new_v4());

            let result = handle_authn_message(msg, authn, state)
                .await
                .expect_err("Unexpectedly succeeded");

            assert_eq!(result, ConnectError::AccessDenied);
        }

        #[tokio::test]
        async fn success() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let classroom_id: ClassroomId = Uuid::new_v4().into();
            let agent = TestAgent::new("http", "user123", USR_AUDIENCE);
            let token = agent.token();

            let replica_id = {
                let mut conn = db_pool.get_conn().await;

                db::replica::InsertQuery::new(
                    "presence-1".into(),
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                )
                .expect("Failed to create insert query for replica")
                .execute(&mut conn)
                .await
                .expect("Failed to insert a replica")
                .id
            };

            let cmd = json!({
                "type": "connect_request",
                "payload": {
                    "classroom_id": classroom_id,
                    "token": token,
                    "agent_label": "http"
                }
            });

            let msg = Message::Text(cmd.to_string());
            let authn = Arc::new(authn::new());

            let mut authz = TestAuthz::new();
            authz.allow(
                agent.account_id(),
                vec!["classrooms", &classroom_id.to_string()],
                "connect",
            );
            let state = TestState::new(db_pool, authz, replica_id);

            let (resp, (_, session_key)) = handle_authn_message(msg, authn, state)
                .await
                .expect("Failed to handle authentication message");

            let success = serde_json::to_string(&Response::ConnectSuccess)
                .expect("Failed to serialize response");

            assert_eq!(resp, success);
            assert_eq!(
                session_key,
                SessionKey::new(agent.agent_id().to_owned(), classroom_id)
            );
        }
    }
}
