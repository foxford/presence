use crate::session::{SessionId, SessionKey};
use std::collections::HashMap;
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

type SessionValue = (SessionId, oneshot::Sender<ConnectionCommand>);

#[derive(Debug)]
pub enum SessionCommand {
    Register(SessionKey, SessionValue),
    // Only closes connections
    Terminate(SessionKey, oneshot::Sender<TerminateSession>),
    // Closes connections and removes them from DB
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
) -> JoinHandle<()> {
    tokio::task::spawn(async move {
        let mut sessions = HashMap::new();

        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        SessionCommand::Register(session_key, value) => {
                            sessions.insert(session_key, value);
                        }
                        // Only closes connections
                        SessionCommand::Terminate(session_key, resp) => {
                            match sessions.remove(&session_key) {
                                Some((session_id, cmd)) => {
                                    resp.send(TerminateSession::Found(session_id)).ok();
                                    cmd.send(ConnectionCommand::Close).ok();
                                }
                                None => {
                                    resp.send(TerminateSession::NotFound).ok();
                                }
                            }
                        }
                        // Closes connections and removes them from DB
                        SessionCommand::Delete(session_key, resp) => {
                            match sessions.remove(&session_key) {
                                Some((session_id, cmd)) => {
                                    cmd.send(ConnectionCommand::Close).ok();
                                    resp.send(DeleteSession::Success(session_id)).ok();
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
                    for (_, (_, cmd)) in sessions {
                        cmd.send(ConnectionCommand::Terminate).ok();
                    }
                    break;
                }
            }
        }
    })
}
