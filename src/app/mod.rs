use crate::{
    authz::AuthzCache,
    db::{self, agent_session, old_agent_session},
    state::{AppState, State},
};
use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use signal_hook::consts::TERM_SIGNALS;
use sqlx::PgPool;
use std::collections::HashMap;
use std::env::var;
use tokio::{
    sync::{mpsc, mpsc::UnboundedReceiver, oneshot, oneshot::Receiver},
    time::{interval, MissedTickBehavior},
};
use tracing::{error, info};
use uuid::Uuid;

mod api;
mod error;
mod history;
mod router;
mod ws;

pub enum Command {
    Register((Uuid, oneshot::Sender<()>)),
    Terminate(Uuid),
}

pub async fn run(db: PgPool, authz_cache: Option<AuthzCache>) -> Result<()> {
    let replica_id = var("REPLICA_ID").expect("REPLICA_ID must be specified");

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
    if let Err(e) = move_sessions_to_history(state.clone(), replica_id.clone()).await {
        error!(error = %e, "Failed to move sessions to history");
    }

    // For graceful shutdown
    let (shutdown_server_tx, shutdown_server_rx) = oneshot::channel::<()>();
    let (shutdown_manager_tx, shutdown_manager_rx) = oneshot::channel::<()>();

    let server = tokio::spawn(
        axum::Server::bind(&config.listener_address)
            .serve(router.into_make_service())
            .with_graceful_shutdown(async {
                shutdown_server_rx.await.ok();
            }),
    );

    let session_manager = tokio::task::spawn(manage_agent_sessions(
        state.clone(),
        cmd_rx,
        shutdown_manager_rx,
    ));

    // Waiting for signals for graceful shutdown
    let mut signals_stream = signal_hook_tokio::Signals::new(TERM_SIGNALS)?.fuse();
    let signals = signals_stream.next();
    let _ = signals.await;
    // Initiating graceful shutdown
    let _ = shutdown_server_tx.send(());
    let _ = shutdown_manager_tx.send(());

    // Move hanging sessions to history
    if let Err(e) = move_sessions_to_history(state, replica_id).await {
        error!(error = %e, "Failed to move sessions to history");
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

// Manages agent session by handling incoming commands.
// Also, closes old agent sessions.
async fn manage_agent_sessions<S: State>(
    state: S,
    mut cmd_rx: UnboundedReceiver<Command>,
    mut shutdown_rx: Receiver<()>,
) {
    let mut conn = state.get_conn().await.expect("Failed to get db connection");

    let mut check_interval = interval(state.config().websocket.check_old_connection_interval);
    check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut sessions = HashMap::<Uuid, oneshot::Sender<()>>::new();

    loop {
        tokio::select! {
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    Command::Register((session_id, sender)) => {
                        info!("register {:?}", session_id);
                        sessions.insert(session_id, sender);
                    }
                    Command::Terminate(session_id) => {
                        info!("terminate {:?}", session_id);
                        sessions.remove(&session_id);
                    }
                }
            }
            _ = check_interval.tick() => {
                match db::old_agent_session::GetAllByReplicaQuery::new(state.replica_id())
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
            _ = &mut shutdown_rx => {
                break;
            }
        }
    }
}

async fn move_sessions_to_history<S: State>(state: S, replica_id: String) -> Result<()> {
    let mut conn = state
        .get_conn()
        .await
        .map_err(|e| anyhow!("Failed to get db connection: {:?}", e))?;

    let sessions = agent_session::GetAllByReplicaQuery::new(replica_id.clone())
        .execute(&mut conn)
        .await
        .map_err(|e| anyhow!("Failed to get agent sessions: {:?}", e))?;

    for s in sessions.clone() {
        history::move_session(&mut conn, s)
            .await
            .map_err(|e| anyhow!("Failed to move agent sessions: {:?}", e))?;
    }

    let old_sessions = old_agent_session::GetAllByReplicaQuery::new(replica_id)
        .execute(&mut conn)
        .await
        .map_err(|e| anyhow!("Failed to get old agent sessions: {:?}", e))?;

    for s in old_sessions.clone() {
        history::move_old_session(&mut conn, s)
            .await
            .map_err(|e| anyhow!("Failed to move agent sessions: {:?}", e))?;
    }

    for s in sessions {
        agent_session::DeleteQuery::new(s.id)
            .execute(&mut conn)
            .await
            .map_err(|e| anyhow!("Failed to delete agent session: {:?}", e))?;
    }

    for s in old_sessions {
        old_agent_session::DeleteQuery::new(s.id)
            .execute(&mut conn)
            .await
            .map_err(|e| anyhow!("Failed to delete old agent session: {:?}", e))?;
    }

    Ok(())
}
