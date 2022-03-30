use crate::{db, state::State};
use std::collections::HashMap;
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tracing::error;
use uuid::Uuid;

pub enum Command {
    Register((Uuid, oneshot::Sender<()>)),
    Terminate(Uuid),
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

        // (agent_id, classroom_id)
        let mut sessions = HashMap::<Uuid, oneshot::Sender<()>>::new();

        let mut conn = match state.get_conn().await {
            Ok(conn) => conn,
            Err(e) => {
                error!(error = %e, "Failed to get db connection");
                return;
                // continue;
            }
        };

        match db::agent_session::FindQuery::by_replica(state.replica_id().as_str())
            .outdated(true)
            .execute(&mut conn)
            .await
        {
            Ok(session_ids) => session_ids.iter().for_each(|s| {
                if let Some(d) = sessions.get(&s.id) {
                    let d = &*d;
                    d.send(());
                }

                // d.send(());

                if let Some(sender) = sessions.get(&s.id) {
                    if let Err(_) = sender.clone().send(()) {
                        error!("Failed to send a message to session: {}", s.id);
                    }
                }
                sessions.remove(&s.id);
            }),
            Err(e) => {
                error!(error = %e, "Failed to get old agent sessions");
            }
        }

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        Command::Register((session_id, sender)) => {
                            sessions.insert(session_id, sender);
                        }
                        Command::Terminate(session_id) => {
                            // After removing `oneshot::Sender<()>` from HashMap,
                            // oneshot::Receiver<()> will get `RecvError` and close old connection
                            sessions.remove(&session_id);
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

                    match db::agent_session::FindQuery::by_replica(state.replica_id().as_str()).outdated(true)
                        .execute(&mut conn)
                        .await
                    {
                        Ok(session_ids) => {
                            session_ids.iter().for_each(|s| {
                                if let Some(sender) = sessions.get(&s.id) {
                                    if let Err(_) = sender.send(()) {
                                        error!("Failed to send a message to session: {}", s.id);
                                    }
                                }
                                sessions.remove(&s.id);
                            })
                        }
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
