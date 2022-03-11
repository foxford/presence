use crate::app::ws::{ConnectError, ConnectRequest, Request, Response};
use crate::authz::AuthzObject;
use crate::classroom::ClassroomId;
use crate::db::{
    agent_session::{self, AgentSession},
    agent_session_history,
};
use crate::state::State;
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
use sqlx::Connection;
use std::ops::Bound;
use std::sync::Arc;
use svc_agent::AgentId;
use svc_authn::{
    jose::ConfigMap, token::jws_compact::extract::decode_jws_compact_with_config, AccountId,
    Authenticable,
};
use tokio::time::{interval, timeout, MissedTickBehavior};
use tracing::{error, info};

pub(crate) async fn handler<S: State>(
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
    let agent_session = match authn_timeout {
        Ok(Some(result)) => match result {
            Ok(msg) => match handle_authn_message(msg, authn, state.clone()).await {
                Ok((m, agent_session)) => {
                    info!("Successful authentication");
                    let _ = sender.send(Message::Text(m)).await;

                    agent_session
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
                info!(error = %e, "An error occurred when receiving a message.");
                return;
            }
        },
        Ok(None) | Err(_) => {
            info!("Connection is closed (authentication timeout exceeded)");
            let _ = sender.close().await;
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
                        info!("Pong received");
                        ping_sent = false;
                    },
                    Ok(Message::Close(frame)) => {
                        match frame {
                            Some(f) => {
                                info!(code = %f.code, reason = %f.reason, "An agent closed connection");
                            }
                            None => info!("An agent closed connection"),
                        }

                        move_session_to_history(state, agent_session).await;
                        return;
                    },
                    Err(e) => {
                        info!(error = %e, "An error occurred when receiving a message.");
                        return;
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
                    move_session_to_history(state, agent_session).await;
                    return;
                }
                info!("Ping sent");
                ping_sent = true;
                pong_expiration_interval.reset();
            }
            // Close connection if pong exceeded timeout
            _ = pong_expiration_interval.tick() => {
                if ping_sent {
                    info!("Connection is closed (pong timeout exceeded)");
                    move_session_to_history(state, agent_session).await;
                    let _ = sender.close().await;
                    return;
                }
                info!("Pong is OK");
            }
        }
    }
}

async fn handle_authn_message<S: State>(
    message: Message,
    authn: Arc<ConfigMap>,
    state: S,
) -> Result<(String, AgentSession), ConnectError> {
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
            let agent_session = save_agent_session(state, classroom_id, agent_id).await?;

            Ok((make_response(Response::ConnectSuccess), agent_session))
        }
        Err(err) => {
            error!(error = %err, "Failed to serialize a message");
            Err(ConnectError::SerializationFailed)
        }
    }
}

async fn save_agent_session<S: State>(
    state: S,
    classroom_id: ClassroomId,
    agent_id: AgentId,
) -> Result<AgentSession, ConnectError> {
    let mut conn = match state.get_conn().await {
        Ok(conn) => conn,
        Err(err) => {
            error!(error = %err, "Failed to get db connection");
            return Err(ConnectError::DbConnAcquisitionFailed);
        }
    };

    let insert_query = agent_session::InsertQuery::new(
        agent_id,
        classroom_id,
        state.replica_id(),
        OffsetDateTime::now_utc(),
    );

    match insert_query.execute(&mut conn).await {
        Ok(agent_session) => Ok(agent_session),
        Err(err) => {
            error!(error = %err, "Failed to create an agent session");
            Err(ConnectError::DbQueryFailed)
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
            account_id.audience().to_string(),
            account_id.clone(),
            object,
            "read".into(),
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

async fn move_session_to_history<S: State>(state: S, session: AgentSession) {
    let mut conn = match state.get_conn().await {
        Ok(conn) => conn,
        Err(err) => {
            error!(error = %err, "Failed to get db connection");
            return;
        }
    };

    let mut tx = match conn.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            error!(error = %err, "Failed to acquire transaction");
            return;
        }
    };

    let now = OffsetDateTime::now_utc();
    let check_query = agent_session_history::CheckLifetimeOverlapQuery::new(session.clone(), now);
    let session_history = match check_query.execute(&mut tx).await {
        Ok(history) => history,
        Err(err) => {
            error!(error = %err, "Failed to check agent_session_history lifetime overlap");
            return;
        }
    };

    match session_history {
        Some(history) => {
            let update_query = agent_session_history::UpdateLifetimeQuery::new(
                history.id,
                history.lifetime.start,
                Bound::Excluded(now),
            );
            if let Err(err) = update_query.execute(&mut tx).await {
                error!(error = %err, "Failed to update agent_session_history lifetime");
                return;
            }
        }
        None => {
            let insert_query = agent_session_history::InsertQuery::new(session.clone(), now);
            if let Err(err) = insert_query.execute(&mut tx).await {
                error!(error = %err, "Failed to create agent_session_history");
                return;
            }
        }
    }

    let delete_query = agent_session::DeleteQuery::new(session.id);
    if let Err(err) = delete_query.execute(&mut tx).await {
        error!(error = %err, "Failed to delete agent_session");
        return;
    }

    if let Err(err) = tx.commit().await {
        error!(error = %err, "Failed to commit transaction");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::prelude::*;
    use serde_json::json;
    use uuid::Uuid;

    mod handle_authn_message {
        use super::*;

        #[tokio::test]
        async fn unsupported_request() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let msg = Message::Binary("".into());
            let authn = Arc::new(authn::new());
            let state = TestState::new(db_pool, TestAuthz::new());

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
            let state = TestState::new(db_pool, TestAuthz::new());

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
            let classroom_id = ClassroomId { 0: Uuid::new_v4() };

            let cmd = json!({
                "type": "connect_request",
                "payload": {
                    "classroom_id": classroom_id,
                    "token": "1234"
                }
            });

            let msg = Message::Text(cmd.to_string());
            let authn = Arc::new(authn::new());
            let state = TestState::new(db_pool, TestAuthz::new());

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
            let classroom_id = ClassroomId { 0: Uuid::new_v4() };
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
            let state = TestState::new(db_pool, TestAuthz::new());

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
            let classroom_id = ClassroomId { 0: Uuid::new_v4() };
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
                "read",
            );
            let state = TestState::new(db_pool, authz);

            let (resp, session) = handle_authn_message(msg, authn, state)
                .await
                .expect("Failed to handle authentication message");

            let success = serde_json::to_string(&Response::ConnectSuccess)
                .expect("Failed to serialize response");

            assert_eq!(resp, success);
            assert_eq!(session.agent_id, agent.agent_id().to_owned());
            assert_eq!(session.classroom_id, classroom_id);
        }
    }

    mod move_session_to_history {
        use super::*;

        #[tokio::test]
        async fn create_history() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let classroom_id = ClassroomId { 0: Uuid::new_v4() };
            let agent = TestAgent::new("http", "user123", USR_AUDIENCE);

            let session = {
                let mut conn = db_pool.get_conn().await;

                agent_session::InsertQuery::new(
                    agent.agent_id().to_owned(),
                    classroom_id,
                    "replica".to_string(),
                    OffsetDateTime::now_utc(),
                )
                .execute(&mut conn)
                .await
                .expect("Failed to insert an agent session")
            };

            let state = TestState::new(db_pool.clone(), TestAuthz::new());
            move_session_to_history(state, session).await;

            let mut conn = db_pool.get_conn().await;
            let agents_count = factory::agent_session::AgentSessionCounter::count(&mut conn)
                .await
                .expect("Failed to count agent session");

            let history_count =
                factory::agent_session_history::AgentSessionHistoryCounter::count(&mut conn)
                    .await
                    .expect("Failed to count agent session history");

            assert_eq!(agents_count, 0);
            assert_eq!(history_count, 1);
        }

        #[tokio::test]
        async fn update_history() {
            let test_container = TestContainer::new();
            let postgres = test_container.run_postgres();
            let db_pool = TestDb::new(&postgres.connection_string).await;
            let classroom_id = ClassroomId { 0: Uuid::new_v4() };
            let agent = TestAgent::new("http", "user123", USR_AUDIENCE);

            let session = {
                let mut conn = db_pool.get_conn().await;

                let session = agent_session::InsertQuery::new(
                    agent.agent_id().to_owned(),
                    classroom_id,
                    "replica".to_string(),
                    OffsetDateTime::now_utc(),
                )
                .execute(&mut conn)
                .await
                .expect("Failed to insert an agent session");

                agent_session_history::InsertQuery::new(session.clone(), OffsetDateTime::now_utc())
                    .execute(&mut conn)
                    .await
                    .expect("Failed to insert an agent session history");

                session
            };

            let state = TestState::new(db_pool.clone(), TestAuthz::new());
            move_session_to_history(state, session).await;

            let mut conn = db_pool.get_conn().await;
            let history_count =
                factory::agent_session_history::AgentSessionHistoryCounter::count(&mut conn)
                    .await
                    .expect("Failed to count agent session history");

            assert_eq!(history_count, 1);
        }
    }
}
