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
                // Graceful shutdown
                _ = shutdown_rx.changed() => {
                    break;
                }
            }
        }
    })
}


