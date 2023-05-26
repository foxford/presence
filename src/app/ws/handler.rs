use crate::{
    app::{
        self,
        api::v1::session::Reason,
        history_manager,
        metrics::AuthzMeasure,
        replica,
        session_manager::ConnectionCommand,
        session_manager::TerminateSession,
        state::State,
        ws::{
            ConnectRequest, RecoverableSessionError, Request, Response, UnrecoverableSessionError,
        },
    },
    authz::AuthzObject,
    classroom::ClassroomId,
    db::{
        self,
        agent_session::{self, InsertResult},
    },
    session::SessionId,
    session::SessionKey,
};
use anyhow::{anyhow, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Extension, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures_util::{future::Either, stream::SplitSink, SinkExt, StreamExt};
use serde::Serialize;
use sqlx::types::time::OffsetDateTime;
use std::sync::Arc;
use svc_agent::AgentId;
use svc_authn::{
    jose::ConfigMap, token::jws_compact::extract::decode_jws_compact_with_config, AccountId,
    Authenticable,
};
use svc_error::extension::sentry;
use svc_events::{AgentEventV1 as AgentEvent, EventV1 as Event};
use tokio::time::{interval, timeout, MissedTickBehavior};
use tokio_stream::wrappers::BroadcastStream;
use tracing::{error, info, warn};

pub async fn handler<S: State>(
    ws: WebSocketUpgrade,
    Extension(state): Extension<S>,
    Extension(authn): Extension<Arc<ConfigMap>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, authn, state))
}

async fn handle_socket<S: State>(socket: WebSocket, authn: Arc<ConfigMap>, state: S) {
    let (mut sender, mut receiver) = socket.split();

    // Close connection if there is no authn message for some time
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
                            close_conn_with_msg(sender, Response::from(e)).await;
                            return;
                        }
                    };

                    let nats_rx = match state.nats_client() {
                        Some(nats_client) => {
                            match nats_client
                                .subscribe(session_key.clone().classroom_id)
                                .await
                            {
                                Ok(rx) => Either::Left(BroadcastStream::new(rx)),
                                Err(e) => {
                                    error!(error = ?e);
                                    close_conn_with_msg(sender, Response::from(e)).await;
                                    return;
                                }
                            }
                        }
                        None => Either::Right(tokio_stream::pending()),
                    };

                    if let Some(nats_client) = state.nats_client() {
                        // Send agent.enter others
                        let event = AgentEvent::Enter {
                            id: session_id.into(),
                            agent_id: session_key.clone().agent_id,
                        };
                        let event = Event::from(event);

                        if let Err(e) = nats_client
                            .publish_event(session_key.clone(), session_id, event)
                            .await
                        {
                            error!(error = %e, "Failed to send agent.enter notification");
                            close_conn_with_msg(
                                sender,
                                Response::from(UnrecoverableSessionError::InternalServerError),
                            )
                            .await;
                            return;
                        }
                    }

                    info!("Successful authentication, session_key: {}", &session_key);
                    let _ = sender.send(Message::Text(m)).await;
                    state.metrics().ws_connection_success().inc();

                    (session_id, session_key, nats_rx, close_rx)
                }
                Err(e) => {
                    error!(error = ?e, "Connection is closed (unsuccessful request)");
                    state.metrics().ws_connection_error().inc();
                    close_conn_with_msg(sender, Response::from(e)).await;
                    return;
                }
            },
            Err(e) => {
                error!(error = %e, "An error occurred when receiving an authn message");
                send_to_sentry(e.into());
                return;
            }
        },
        Ok(None) | Err(_) => {
            warn!("Connection is closed (authentication timeout exceeded)");
            close_conn_with_msg(
                sender,
                Response::from(UnrecoverableSessionError::AuthTimedOut),
            )
            .await;

            return;
        }
    };

    state.metrics().ws_connection_total().inc();

    let mut ping_sent = false;
    // Mark a connection as terminating on graceful shutdown
    let mut connect_terminating = false;

    // Ping/Pong intervals
    let mut ping_interval = interval(state.config().websocket.ping_interval);
    let mut pong_expiration_interval = interval(state.config().websocket.pong_expiration_interval);

    // In order not to hurry
    ping_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    pong_expiration_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            msg = nats_rx.next() => {
                tracing::debug!("got new event from nats");
                let msg = match msg {
                    Some(Ok(msg)) => msg,
                    _ => {
                        warn!("nats stream is over");
                        break;
                    },
                };

                let headers = match svc_nats_client::Headers::try_from(msg.headers.clone().unwrap_or_default()) {
                    Ok(headers) => headers,
                    Err(err) => {
                        error!(%err, "failed to parse nats headers");
                        send_to_sentry(err.into());
                        continue;
                    }
                };

                // Don't send events to yourself
                if session_key.agent_id == headers.sender_id().clone() {
                    continue;
                }

                if let Some(receiver_id) = headers.receiver_id() {
                    // If there is a receiver_id in the nats headers,
                    // then we send a message only to this agent
                    if session_key.agent_id != receiver_id.clone() {
                        continue;
                    }
                }

                match String::from_utf8(msg.payload.to_vec()) {
                    Ok(s) => {
                        if let Err(e) = sender.send(Message::Text(s)).await {
                            error!(error = ?e, "Failed to send notification");
                            send_to_sentry(e.into());
                        }
                    },
                    Err(e)=> {
                        error!(error = ?e, "Conversion to str failed");
                        send_to_sentry(e.into());
                    }
                }
            }
            // Get Pong/Close messages from client
            result = receiver.next() => {
                tracing::debug!("got new message from socket");
                let result = match result {
                    Some(result) => result,
                    None => {
                        warn!("Connection to agent is aborted");
                        break;
                    },
                };

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
                        send_to_sentry(e.into());

                        break;
                    },
                    _ => {
                        // Ping/Text/Binary messages - do nothing
                    }
                }
            }
            // Ping authenticated clients with interval
            _ = ping_interval.tick() => {
                tracing::debug!("going to send ping");
                if sender.send(Message::Ping(Vec::new())).await.is_err() {
                    warn!("An agent disconnected (ping not sent)");
                    break;
                }

                ping_sent = true;
                pong_expiration_interval.reset();
            }
            // Close connection if pong exceeded timeout
            _ = pong_expiration_interval.tick() => {
                tracing::debug!("ping expiration");
                if ping_sent {
                    warn!("Connection is closed (pong timeout exceeded)");
                    close_conn_with_msg(sender, Response::from(UnrecoverableSessionError::PongTimedOut)).await;
                    break;
                }
            }
            // Close connections
            cmd = close_rx.recv() => {
                if connect_terminating {
                    tracing::debug!("got some cmd, ignoring");
                    continue;
                }

                let cmd = match cmd {
                    Some(cmd) => cmd,
                    None => {
                        warn!("cmd channel is closed, leaving...");
                        break;
                    }
                };

                match cmd {
                    ConnectionCommand::Close => {
                        close_conn_with_msg(sender, Response::from(UnrecoverableSessionError::Replaced)).await;
                    }
                    ConnectionCommand::Terminate => {
                        connect_terminating = true;
                        let msg = serialize_to_json(&Response::from(RecoverableSessionError::Terminated));
                        sender.send(Message::Text(msg)).await.ok();
                        tracing::debug!("terminating, notification sent");

                        continue;
                    }
                }

                info!("Connection is closed");
                state.metrics().ws_connection_total().dec();

                return;
            }
        }
    }
    tracing::debug!("broke out of loop");

    state.metrics().ws_connection_total().dec();

    // Skip next steps if the connection is terminating
    if connect_terminating {
        tracing::debug!("closing handler earlier since we're terminating");
        return;
    }

    if let Some(nats_client) = state.nats_client() {
        let event = AgentEvent::Leave {
            id: session_id.into(),
            agent_id: session_key.clone().agent_id,
        };
        let event = Event::from(event);

        if let Err(e) = nats_client
            .publish_event(session_key.clone(), session_id, event)
            .await
        {
            error!(error = %e, "Failed to send agent.leave notification");
            send_to_sentry(e);
            return;
        }
    }

    if let Err(e) = state.terminate_session(session_key.clone()).await {
        error!(error = %e, "Failed to terminate session_key: {}", session_key);
        send_to_sentry(e);
        return;
    }

    if let Err(e) = history_manager::move_single_session(state, session_id).await {
        error!(error = %e, "Failed to move session to history");
        send_to_sentry(e);
    }
}

fn send_to_sentry(error: anyhow::Error) {
    if let Err(e) = sentry::send(Arc::new(error)) {
        error!(error = %e, "Failed to send error to sentry");
    }
}

/// Closes the WebSocket connection with a message
async fn close_conn_with_msg<T: Serialize>(mut sender: SplitSink<WebSocket, Message>, resp: T) {
    let resp = serialize_to_json(&resp);
    sender.send(Message::Text(resp)).await.ok();
    sender.close().await.ok();
}

async fn handle_authn_message<S: State>(
    message: Message,
    authn: Arc<ConfigMap>,
    state: S,
) -> Result<(String, (SessionId, SessionKey)), UnrecoverableSessionError> {
    let msg = match message {
        Message::Text(msg) => msg,
        _ => return Err(UnrecoverableSessionError::UnsupportedRequest),
    };

    let result = serde_json::from_str::<Request>(&msg);
    match result {
        Ok(Request::ConnectRequest(ConnectRequest {
            token,
            classroom_id,
            agent_label,
        })) => {
            let agent_id = get_agent_id_from_token(token, authn, agent_label).map_err(|e| {
                warn!(error = %e, "Failed to authenticate an agent");
                UnrecoverableSessionError::Unauthenticated
            })?;

            authorize_agent(state.clone(), &agent_id, &classroom_id).await?;
            let session_id = create_agent_session(state, classroom_id, &agent_id).await?;

            Ok((
                serialize_to_json(&Response::ConnectSuccess),
                (session_id, SessionKey::new(agent_id, classroom_id)),
            ))
        }
        Err(e) => {
            error!(error = %e, "Failed to deserialize a message");
            send_to_sentry(e.into());
            Err(UnrecoverableSessionError::SerializationFailed)
        }
    }
}

async fn create_agent_session<S: State>(
    state: S,
    classroom_id: ClassroomId,
    agent_id: &AgentId,
) -> Result<SessionId, UnrecoverableSessionError> {
    let mut conn = state.get_conn().await.map_err(|e| {
        error!(error = %e, "Failed to get db connection");
        send_to_sentry(e);
        UnrecoverableSessionError::InternalServerError
    })?;

    // Attempt to close old session on the same replica
    let session_key = SessionKey::new(agent_id.clone(), classroom_id);
    // If the session is found, don't create a new session and return the previous id
    match state.terminate_session(session_key.clone()).await {
        Ok(TerminateSession::Found(session_id)) => {
            return Ok(session_id);
        }
        Err(e) => {
            error!(error = %e, "Failed to terminate session: {}", &session_key);
            send_to_sentry(e);
            return Err(UnrecoverableSessionError::InternalServerError);
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
        InsertResult::Error(e) => {
            error!(error = %e, "Failed to create an agent session");
            send_to_sentry(e.into());
            Err(UnrecoverableSessionError::InternalServerError)
        }
        InsertResult::UniqIdsConstraintError => {
            use app::api::v1::session::Response as DeleteResponse;

            let replica_ip = db::replica::GetIpQuery::new(session_key.clone())
                .execute(&mut conn)
                .await
                .map_err(|e| {
                    error!(error = %e, "Failed to get replica ip");
                    send_to_sentry(e.into());
                    UnrecoverableSessionError::InternalServerError
                })?;

            match replica::close_connection(state.clone(), replica_ip.ip(), session_key).await {
                Ok(DeleteResponse::DeleteSuccess)
                | Ok(DeleteResponse::DeleteFailure(Reason::NotFound)) => {
                    match agent_session::InsertQuery::new(
                        agent_id,
                        classroom_id,
                        state.replica_id(),
                        OffsetDateTime::now_utc(),
                    )
                    .execute(&mut conn)
                    .await
                    {
                        InsertResult::Ok(session) => Ok(session.id),
                        InsertResult::Error(e) => {
                            error!(error = %e, "Failed to create new agent session");
                            send_to_sentry(e.into());
                            Err(UnrecoverableSessionError::InternalServerError)
                        }
                        InsertResult::UniqIdsConstraintError => {
                            error!("Failed to create new agent session due to unique constraint");
                            send_to_sentry(anyhow!(
                                "Failed to create new agent session due to unique constraint"
                            ));
                            Err(UnrecoverableSessionError::InternalServerError)
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to close connection on another replica");
                    send_to_sentry(e);
                    Err(UnrecoverableSessionError::InternalServerError)
                }
                Ok(DeleteResponse::DeleteFailure(r)) => {
                    error!(reason = %r, "Failed to delete connection on another replica");
                    send_to_sentry(anyhow!(
                        "Failed to delete connection on another replica, reason = {}",
                        r
                    ));
                    Err(UnrecoverableSessionError::InternalServerError)
                }
            }
        }
    }
}

async fn authorize_agent<S: State>(
    state: S,
    agent_id: &AgentId,
    classroom_id: &ClassroomId,
) -> Result<(), UnrecoverableSessionError> {
    let account_id = agent_id.as_account_id();
    let object = AuthzObject::new(&["classrooms", &classroom_id.to_string()]).into();

    let audience = state
        .lookup_known_authz_audience(account_id.audience())
        .unwrap_or(account_id.audience())
        .to_owned();

    if let Err(err) = state
        .authz()
        .authorize(audience, account_id.clone(), object, "connect".into())
        .await
        .measure()
    {
        error!(error = %err, "Failed to authorize action");
        return Err(UnrecoverableSessionError::AccessDenied);
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
    let agent_id = AgentId::new(agent_label, account);

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

            assert_eq!(result, UnrecoverableSessionError::UnsupportedRequest);
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

            assert_eq!(result, UnrecoverableSessionError::SerializationFailed);
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

            assert_eq!(result, UnrecoverableSessionError::Unauthenticated);
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

            assert_eq!(result, UnrecoverableSessionError::AccessDenied);
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
