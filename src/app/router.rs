use crate::app::api::v1;
use crate::app::ws;
use crate::state::{AppState, State};
use axum::{
    routing::{get, options},
    {AddExtensionLayer, Router},
};
use std::sync::Arc;
use svc_utils::middleware::LogLayer;
use svc_utils::middleware::MeteredRoute;

pub fn new<S: State>(state: S, authn: svc_authn::jose::ConfigMap) -> Router {
    let router = api_router().merge(ws_router());

    router
        .layer(AddExtensionLayer::new(Arc::new(authn)))
        .layer(AddExtensionLayer::new(state))
        .layer(LogLayer::new())
}

fn api_router() -> Router {
    Router::new()
        .metered_route("/api/v1/healthz", get(v1::healthz))
        .metered_route(
            "/api/v1/classrooms/:classroom_id/agents",
            options(v1::options).get(v1::classroom::list_agents::<AppState>),
        )
}

fn ws_router() -> Router {
    Router::new().route("/ws", get(ws::handler::<AppState>))
}
