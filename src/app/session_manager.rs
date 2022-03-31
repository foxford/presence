use crate::classroom::ClassroomId;
use crate::{db, state::State};
use std::collections::HashMap;
use svc_agent::AgentId;
// use tokio::sync::oneshot::Sender;
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tracing::error;
use uuid::Uuid;

pub enum Command {
    // Register((Uuid, oneshot::Sender<()>)),
    Register((AgentId, ClassroomId), (Uuid, oneshot::Sender<()>)),
    // Terminate(Uuid),
    Terminate(
        (AgentId, ClassroomId),
        Option<oneshot::Sender<Option<Uuid>>>,
    ),
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

        // let mut sessions = HashMap::<Uuid, oneshot::Sender<()>>::new();

        let mut sessions = HashMap::<(AgentId, ClassroomId), (Uuid, oneshot::Sender<()>)>::new();

        // let cmd = cmd_rx.recv().await.unwrap();
        // match cmd {
        //     Command::Register(identifier, value) => {
        //         sessions.insert(identifier, value);
        //     }
        //     Command::Terminate(identifier, sender) => {
        //         // After removing `oneshot::Sender<()>` from HashMap,
        //         // oneshot::Receiver<()> will get `RecvError` and close old connection
        //         if let Some((session_id, _)) = sessions.remove(&identifier) {
        //             sender.and_then(|s| s.send(Some(session_id)).ok());
        //             return; // continue;
        //         }
        //
        //         sender.and_then(|s| s.send(None).ok());
        //     }
        // }
        //
        // let mut conn = match state.get_conn().await {
        //     Ok(conn) => conn,
        //     Err(e) => {
        //         error!(error = %e, "Failed to get db connection");
        //         return;
        //         // continue;
        //     }
        // };
        //
        // match db::agent_session::FindQuery::by_replica(state.replica_id().as_str())
        //     .outdated(true)
        //     .execute(&mut conn)
        //     .await
        // {
        //     Ok(session_ids) => session_ids.iter().for_each(|s| {
        //         // After removing `oneshot::Sender<()>` from HashMap,
        //         // oneshot::Receiver<()> will get `RecvError` and close old connection
        //         if let Some((_, sender)) = sessions.remove(&s.id) {
        //             if let Err(_) = sender.send(()) {
        //                 error!("Failed to send a message to session: {:?}", s.id);
        //             }
        //         }
        //     }),
        //     Err(e) => {
        //         error!(error = %e, "Failed to get old agent sessions");
        //     }
        // }

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    // match cmd {
                    //     Command::Register((session_id, sender)) => {
                    //         sessions.insert(session_id, sender);
                    //     }
                    //     Command::Terminate(session_id) => {
                    //         // After removing `oneshot::Sender<()>` from HashMap,
                    //         // oneshot::Receiver<()> will get `RecvError` and close old connection
                    //         sessions.remove(&session_id);
                    //     }
                    // }
                    match cmd {
                        Command::Register(identifier, value) => {
                            sessions.insert(identifier, value);
                        }
                        Command::Terminate(identifier, sender) => {
                            // After removing `oneshot::Sender<()>` from HashMap,
                            // oneshot::Receiver<()> will get `RecvError` and close old connection
                            if let Some((session_id, _)) = sessions.remove(&identifier) {
                                sender.and_then(|s| s.send(Some(session_id)).ok());
                                continue;
                            }

                            sender.and_then(|s| s.send(None).ok());
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
                        // Ok(session_ids) => {
                        //     session_ids.iter().for_each(|s| {
                        //         if let Some(sender) = sessions.remove(&s.id) {
                        //             if let Err(_) = sender.send(()) {
                        //                 error!("Failed to send a message to session: {}", s.id);
                        //             }
                        //         }
                        //     })
                        // }
                        Ok(session_ids) => session_ids.iter().for_each(|s| {
                            // After removing `oneshot::Sender<()>` from HashMap,
                            // oneshot::Receiver<()> will get `RecvError` and close old connection
                            if let Some((_, sender)) = sessions.remove(&s.id) {
                                if sender.send(()).is_err() {
                                    error!("Failed to send a message to session: {:?}", s.id);
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
