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
pub enum Command {
    Register(SessionKey, SessionValue),
    Terminate(SessionKey, Option<oneshot::Sender<Session>>),
}

/// Manages agent sessions by handling incoming commands.
/// Also, closes old agent sessions.
pub fn run(
    sessions: SessionMap,
    mut cmd_rx: mpsc::UnboundedReceiver<Command>,
    mut shutdown_rx: watch::Receiver<()>,
) -> JoinHandle<()> {
    tokio::task::spawn(async move {
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
                // Graceful shutdown
                _ = shutdown_rx.changed() => {
                    break;
                }
            }
        }
    })
}
