use crate::session::{SessionId, SessionKey};
use std::{collections::HashMap, time::Duration};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
    time::MissedTickBehavior,
};

type SessionValue = (SessionId, mpsc::Sender<ConnectionCommand>);

#[derive(Debug)]
pub enum SessionCommand {
    Register(SessionKey, SessionValue),
    // To close connections on the same replica
    Terminate(SessionKey, oneshot::Sender<TerminateSession>),
    // To close connections on another replica (via internal API)
    Delete(SessionKey, oneshot::Sender<DeleteSession>),
}

#[derive(Debug)]
pub enum ConnectionCommand {
    Close,
    Terminate,
}

#[derive(Debug)]
pub enum TerminateSession {
    NotFound,
    Found(SessionId),
}

#[derive(Debug)]
pub enum DeleteSession {
    Success(SessionId),
    NotFound,
}

/// Manages agent sessions by handling incoming commands.
/// Also, closes old agent sessions.
pub fn run(
    mut cmd_rx: mpsc::UnboundedReceiver<SessionCommand>,
    mut shutdown_rx: watch::Receiver<()>,
    wait_before_terminate: Duration,
) -> JoinHandle<()> {
    tokio::task::spawn(async move {
        let mut sessions = HashMap::new();

        // We need to handle commands from another replica after starting graceful shutdown
        // This variable is a marker that a graceful shutdown has been started
        let mut terminate = false;
        let mut termination_interval = tokio::time::interval(wait_before_terminate);
        termination_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        SessionCommand::Register(session_key, value) => {
                            sessions.insert(session_key, value);
                        }
                        // Close connections on the same replica
                        SessionCommand::Terminate(session_key, resp) => {
                            match sessions.remove(&session_key) {
                                Some((session_id, cmd)) => {
                                    resp.send(TerminateSession::Found(session_id)).ok();
                                    cmd.send(ConnectionCommand::Close).await.ok();
                                }
                                None => {
                                    resp.send(TerminateSession::NotFound).ok();
                                }
                            }
                        }
                        // Close connections on another replica (via internal API)
                        SessionCommand::Delete(session_key, resp) => {
                            match sessions.remove(&session_key) {
                                Some((session_id, cmd)) => {
                                    resp.send(DeleteSession::Success(session_id)).ok();
                                    cmd.send(ConnectionCommand::Close).await.ok();
                                }
                                None => {
                                    resp.send(DeleteSession::NotFound).ok();
                                }
                            }
                        }
                    }
                }
                // Graceful shutdown
                _ = shutdown_rx.changed() => {
                    for (_, cmd) in sessions.values() {
                        cmd.send(ConnectionCommand::Terminate).await.ok();
                    }

                    // Start shutting down the session manager after 10 seconds
                    terminate = true;
                    termination_interval.reset();
                }
                //
                _ = termination_interval.tick() => {
                    if terminate {
                        break;
                    }
                }
            }
        }
    })
}
