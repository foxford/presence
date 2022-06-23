use crate::app::{
    api::v1,
    state::{AppState, State},
    ws,
};
use axum::{
    extract::Extension,
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use svc_utils::middleware::{CorsLayer, LogLayer, MeteredRoute};

pub fn new<S: State>(state: S, authn: svc_authn::jose::ConfigMap) -> Router {
    let router = api_router().merge(ws_router());

    router
        .layer(Extension(Arc::new(authn)))
        .layer(Extension(state))
        .layer(LogLayer::new())
}

fn api_router() -> Router {
    Router::new()
        .metered_route("/healthz", get(v1::healthz))
        .metered_route(
            "/api/v1/classrooms/:classroom_id/agents",
            get(v1::classroom::list_agents::<AppState>).options(v1::options),
        )
        .metered_route(
            "/api/v1/counters/agent",
            post(v1::counter::count_agents::<AppState>),
        )
        .layer(CorsLayer)
}

fn ws_router() -> Router {
    Router::new().route("/ws", get(ws::handler::<AppState>))
}

pub fn new_internal<S: State>(state: S) -> Router {
    Router::new()
        .route("/api/v1/sessions", delete(v1::session::delete::<AppState>))
        .layer(Extension(state))
        .layer(LogLayer::new())
}
