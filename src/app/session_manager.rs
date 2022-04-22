use crate::{app::state::State, db, session::SessionId, session::SessionKey};
use anyhow::{Context, Result};
use std::collections::HashMap;
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tracing::error;

#[derive(Debug)]
pub enum Session {
    NotFound,
    Found(SessionId),
    Skip,
}

#[derive(Debug)]
pub enum Command {
    Register(SessionKey, (SessionId, oneshot::Sender<()>)),
    Terminate(SessionKey, Option<oneshot::Sender<Session>>),
}

/// Manages agent sessions by handling incoming commands.
/// Also, closes old agent sessions.
pub fn run<S: State>(
    state: S,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
    mut shutdown_rx: watch::Receiver<()>,
) -> JoinHandle<()> {
    tokio::task::spawn(async move {
        let mut check_interval = interval(state.config().websocket.check_old_connection_interval);
        check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut sessions = HashMap::<SessionKey, (SessionId, oneshot::Sender<()>)>::new();

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        Command::Register(session_key, value) => {
                            sessions.insert(session_key, value);
                        }
                        Command::Terminate(session_key, sender) => {
                            // After removing `oneshot::Sender<()>` from HashMap,
                            // oneshot::Receiver<()> will get `RecvError` and close old connection
                            if let Some((session_id, _)) = sessions.remove(&session_key) {
                                sender.and_then(|s| s.send(Session::Found(session_id)).ok());
                                continue;
                            }

                            sender.and_then(|s| s.send(Session::NotFound).ok());
                        }
                    }
                }
                _ = check_interval.tick() => {
                    match get_outdated_sessions(state.clone()).await {
                        Ok(session_keys) => session_keys.iter().for_each(|key| {
                            // Close the old connection by sending a message
                            if let Some((_, sender)) = sessions.remove(key) {
                                if sender.send(()).is_err() {
                                    error!("Failed to send a message to session: {}", key);
                                }
                            }
                        }),
                        Err(e) => {
                            error!(error = %e);
                        }
                    };
                }
                // Graceful shutdown
                _ = shutdown_rx.changed() => {
                    break;
                }
            }
        }
    })
}

async fn get_outdated_sessions<S: State>(state: S) -> Result<Vec<SessionKey>> {
    let mut conn = state.get_conn().await.context("failed to get connection")?;

    db::agent_session::FindOutdatedQuery::by_replica(state.replica_id().as_str())
        .outdated(true)
        .execute(&mut conn)
        .await
        .context("failed to get outdated sessions")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{classroom::ClassroomId, db::agent_session, test_helpers::prelude::*};
    use sqlx::types::time::OffsetDateTime;
    use uuid::Uuid;

    #[tokio::test]
    async fn get_outdated_sessions_test() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();
        let db_pool = TestDb::new(&postgres.connection_string).await;
        let classroom_id: ClassroomId = Uuid::new_v4().into();
        let agent = TestAgent::new("http", "user123", USR_AUDIENCE);

        let _ = {
            let mut conn = db_pool.get_conn().await;

            let session = agent_session::InsertQuery::new(
                agent.agent_id(),
                classroom_id,
                "presence_1".to_string(),
                OffsetDateTime::now_utc(),
            )
            .outdated(true)
            .execute(&mut conn)
            .await
            .expect("failed to insert an outdated agent session");

            session
        };

        let state = TestState::new(db_pool, TestAuthz::new(), "presence_1");
        let result = get_outdated_sessions(state)
            .await
            .expect("failed to get outdated sessions");

        let session_key = SessionKey::new(agent.agent_id().to_owned(), classroom_id);
        assert_eq!(result, vec![session_key]);
    }
}
