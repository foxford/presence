use crate::{db, session::SessionKey, state::State};
use std::collections::HashMap;
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tracing::error;
use uuid::Uuid;

pub enum Session {
    NotFound,
    Found(Uuid),
}

pub enum Command {
    Register(SessionKey, (Uuid, oneshot::Sender<()>)),
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

        let mut sessions = HashMap::<SessionKey, (Uuid, oneshot::Sender<()>)>::new();

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
                    let mut conn = match state.get_conn().await {
                        Ok(conn) => conn,
                        Err(e) => {
                            error!(error = %e, "Failed to get db connection");
                            continue;
                        }
                    };

                    match db::agent_session::FindOutdatedQuery::by_replica(state.replica_id().as_str()).outdated(true)
                        .execute(&mut conn)
                        .await
                    {
                        Ok(session_ids) => session_ids.iter().for_each(|s| {
                            // After removing `oneshot::Sender<()>` from HashMap,
                            // oneshot::Receiver<()> will get `RecvError` and close old connection
                            if let Some((_, sender)) = sessions.remove(&s.session_key) {
                                if sender.send(()).is_err() {
                                    error!("Failed to send a message to session: {}", s.session_key);
                                }
                            }
                        }),
                        Err(e) => {
                            error!(error = %e, "Failed to get old agent sessions");
                        }
                    }
                }
                // Graceful shutdown
                _ = shutdown_rx.changed() => {
                    break;
                }
            }
        }
    })
}
