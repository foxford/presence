use crate::session::{SessionId, SessionKey, SessionMap, SessionValue};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

#[derive(Debug)]
pub enum Session {
    NotFound,
    Found(SessionId),
    Skip,
}

#[derive(Debug)]
pub enum SessionCommand {
    Register(SessionKey, SessionValue),
    Terminate(SessionKey, Option<oneshot::Sender<Session>>),
}

#[derive(Debug)]
pub enum ConnectionCommand {
    Close(Option<oneshot::Sender<DeleteSession>>),
    #[allow(dead_code)]
    Terminate,
}

#[derive(Debug)]
pub enum DeleteSession {
    Success,
    Failure,
}

/// Manages agent sessions by handling incoming commands.
/// Also, closes old agent sessions.
pub fn run(
    sessions: SessionMap,
    mut cmd_rx: mpsc::UnboundedReceiver<SessionCommand>,
    mut shutdown_rx: watch::Receiver<()>,
) -> JoinHandle<()> {
    tokio::task::spawn(async move {
        loop {
            tokio::select! {
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        SessionCommand::Register(session_key, value) => {
                            sessions.insert(session_key, value);
                        }
                        SessionCommand::Terminate(session_key, resp) => {
                            if let Some((session_id, cmd)) = sessions.remove(&session_key) {
                                if cmd.send(ConnectionCommand::Close(None)).is_ok() {
                                    resp.and_then(|s| s.send(Session::Found(session_id)).ok());
                                }

                                continue;
                            }

                            resp.and_then(|s| s.send(Session::NotFound).ok());
                        }
                    }
                }
                // Graceful shutdown
                _ = shutdown_rx.changed() => {
                    // TODO: Send `ConnectionCommand::Terminate`
                    break;
                }
            }
        }
    })
}
