use crate::{
    app::{
        history_manager,
        session_manager::Session,
        ws::{ConnectError, ConnectRequest, Request, Response},
    },
    authz::AuthzObject,
    authz_hack,
    classroom::ClassroomId,
    db::agent_session::{self, InsertResult},
    session::SessionId,
    session::SessionKey,
    state::State,
};
use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Extension, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use sqlx::types::time::OffsetDateTime;
use std::sync::Arc;
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
    let (session_id, session_key) = match authn_timeout {
        Ok(Some(result)) => match result {
            Ok(msg) => match handle_authn_message(msg, authn, state.clone()).await {
                Ok((m, (session_id, session_key))) => {
                    info!("Successful authentication, session_key: {}", &session_key);
                    let _ = sender.send(Message::Text(m)).await;

                    (session_id, session_key)
                }
                Err(e) => {
                    info!("Connection is closed (unsuccessful request)");
                    let resp = make_response(e.into());
                    let _ = sender.send(Message::Text(resp)).await;
                    let _ = sender.close().await;
                    return;
                }
            },
            Err(e) => {
                error!(error = %e, "An error occurred when receiving a message");
                return;
            }
        },
        Ok(None) | Err(_) => {
            info!("Connection is closed (authentication timeout exceeded)");
            let _ = sender.close().await;
            return;
        }
    };

    // To close old connections from the same agents
    let mut close_rx = match state.register_session(session_key.clone(), session_id) {
        Ok(rx) => rx,
        Err(e) => {
            error!(error = %e, "Failed to register session_key: {}", session_key);
            return;
        }
    };

    let mut ping_sent = false;

    // Ping/Pong intervals
    let mut ping_interval = interval(state.config().websocket.ping_interval);
    let mut pong_expiration_interval = interval(state.config().websocket.pong_expiration_interval);

    // In order not to hurry
    ping_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    pong_expiration_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
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
                    let _ = sender.close().await;

                    break;
                }
            }
            // Close old connections
            result = &mut close_rx => {
                info!("Old connection is closed");
                let _ = sender.close().await;

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

    if let Err(e) = state.terminate_session(session_key.clone(), false).await {
        error!(error = %e, "Failed to terminate session_key: {}", session_key);
    }

    if let Err(err) = history_manager::move_single_session(state, session_id).await {
        error!(error = %err, "Failed to move session to history");
    }
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
        })) => {
            let agent_id = match get_agent_id_from_token(token, authn) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    error!(error = %err, "Failed to authenticate an agent");
                    return Err(ConnectError::Unauthenticated);
                }
            };

            authorize_agent(state.clone(), &agent_id, &classroom_id).await?;
            let session_id = create_agent_session(state, classroom_id, &agent_id).await?;

            Ok((
                make_response(Response::ConnectSuccess),
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
    let mut conn = match state.get_conn().await {
        Ok(conn) => conn,
        Err(err) => {
            error!(error = %err, "Failed to get db connection");
            return Err(ConnectError::DbConnAcquisitionFailed);
        }
    };

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
            // Set previous agent session as outdated
            if let Err(err) =
                agent_session::UpdateOutdatedQuery::by_agent_and_classroom(agent_id, classroom_id)
                    .outdated(true)
                    .execute(&mut conn)
                    .await
            {
                error!(error = %err, "Failed to set agent session as outdated");
                return Err(ConnectError::DbQueryFailed);
            }

            // Attempt to create a new agent session again
            return match insert_query.execute(&mut conn).await {
                InsertResult::Ok(agent_session) => Ok(agent_session.id),
                _ => {
                    error!("Failed to create an agent session");
                    Err(ConnectError::DbQueryFailed)
                }
            };
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
            authz_hack::remove_unwanted_paths_from_audience(account_id.audience()),
            account_id.clone(),
            object,
            "connect".into(),
        )
        .await
    {
        error!(error = %err, "Failed to authorize action");
        return Err(ConnectError::AccessDenied);
    };

    Ok(())
}

fn get_agent_id_from_token(token: String, authn: Arc<ConfigMap>) -> Result<AgentId> {
    let data = decode_jws_compact_with_config::<String>(&token, authn.as_ref())?;
    let claims = data.claims;

    let account = AccountId::new(claims.subject(), claims.audience());
    let agent_id = AgentId::new("http", account);

    Ok(agent_id)
}

fn make_response(response: Response) -> String {
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

        const REPLICA_ID: &str = "presence_1";

        #[tokio::test]
        async fn unsupported_request() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let msg = Message::Binary("".into());
            let authn = Arc::new(authn::new());
            let state = TestState::new(db_pool, TestAuthz::new(), REPLICA_ID);

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
            let state = TestState::new(db_pool, TestAuthz::new(), REPLICA_ID);

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
                    "token": "1234"
                }
            });

            let msg = Message::Text(cmd.to_string());
            let authn = Arc::new(authn::new());
            let state = TestState::new(db_pool, TestAuthz::new(), REPLICA_ID);

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
                    "token": token
                }
            });

            let msg = Message::Text(cmd.to_string());
            let authn = Arc::new(authn::new());
            let state = TestState::new(db_pool, TestAuthz::new(), REPLICA_ID);

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

            let cmd = json!({
                "type": "connect_request",
                "payload": {
                    "classroom_id": classroom_id,
                    "token": token
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
            let state = TestState::new(db_pool, authz, REPLICA_ID);

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
