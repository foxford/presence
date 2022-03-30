use crate::{
    authz::AuthzCache,
    db,
    state::{AppState, State},
};
use anyhow::{Context, Result};
use futures_util::StreamExt;
use signal_hook::consts::TERM_SIGNALS;
use sqlx::PgPool;
use std::{collections::HashMap, env::var};
use tokio::{
    sync::{mpsc, mpsc::UnboundedReceiver, oneshot, watch},
    time::{interval, MissedTickBehavior},
};
use tracing::{error, info};
use uuid::Uuid;

mod api;
mod error;
mod history_manager;
mod router;
mod ws;

pub enum Command {
    Register((Uuid, oneshot::Sender<()>)),
    Terminate(Uuid),
}

pub async fn run(db: PgPool, authz_cache: Option<AuthzCache>) -> Result<()> {
    let replica_id = var("APP_AGENT_LABEL").expect("APP_AGENT_LABEL must be specified");

    let config = crate::config::load().context("Failed to load config")?;
    info!("App config: {:?}", config);

    if let Some(sentry_config) = config.sentry.as_ref() {
        svc_error::extension::sentry::init(sentry_config);
    }

    let authz = svc_authz::ClientMap::new(&config.id, authz_cache, config.authz.clone(), None)
        .context("Error converting authz config to clients")?;

    // A channel for managing agent session via sending commands from WebSocket handler
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<Command>();

    let state = AppState::new(config.clone(), db, authz, replica_id.clone(), cmd_tx);
    let router = router::new(state.clone(), config.authn.clone());

    // Move hanging sessions from last time to history
    if let Err(e) = history_manager::move_all_sessions(state.clone(), &replica_id).await {
        error!(error = %e, "Failed to move all sessions to history");
    }

    // For graceful shutdown
    let (shutdown_tx, shutdown_rx) = watch::channel(());

    let mut shutdown_server_rx = shutdown_rx.clone();
    let server = tokio::spawn(
        axum::Server::bind(&config.listener_address)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async move {
                shutdown_server_rx.changed().await.ok();
            }),
    );

    let session_manager =
        tokio::task::spawn(manage_agent_sessions(state.clone(), cmd_rx, shutdown_rx));

    // Waiting for signals for graceful shutdown
    let mut signals_stream = signal_hook_tokio::Signals::new(TERM_SIGNALS)?.fuse();
    let signals = signals_stream.next();
    let _ = signals.await;
    // Initiating graceful shutdown
    shutdown_tx.send(()).ok();

    // Move hanging sessions to history
    if let Err(e) = history_manager::move_all_sessions(state.clone(), &replica_id).await {
        error!(error = %e, "Failed to move all sessions to history");
    }

    // Make sure server and session manager are stopped
    if let Err(err) = server.await {
        error!(error = %err, "Failed to await server completion");
    }

    if let Err(err) = session_manager.await {
        error!(error = %err, "Failed to await session manager completion");
    }

    Ok(())
}

// TODO: Move to another module ~ session_manager?
// Manages agent session by handling incoming commands.
// Also, closes old agent sessions.
async fn manage_agent_sessions<S: State>(
    state: S,
    mut cmd_rx: UnboundedReceiver<Command>,
    mut shutdown_rx: watch::Receiver<()>,
) {
    let mut check_interval = interval(state.config().websocket.check_old_connection_interval);
    check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    // (agent_id, classroom_id)
    let mut sessions = HashMap::<Uuid, oneshot::Sender<()>>::new();

    loop {
        tokio::select! {
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    Command::Register((session_id, sender)) => {
                        sessions.insert(session_id, sender);
                    }
                    Command::Terminate(session_id) => {
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
                            // After removing `oneshot::Sender<()>` from HashMap,
                            // oneshot::Receiver<()> will get `RecvError` and close old connection
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
}
