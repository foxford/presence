use crate::app::ws;
use crate::state::State;
use axum::routing::get;
use axum::{AddExtensionLayer, Router};
use std::sync::Arc;
use svc_utils::middleware::LogLayer;

pub(crate) fn new(ctx: State) -> Router {
    let router = api_router().merge(ws_router());

    router
        .layer(AddExtensionLayer::new(Arc::new(ctx)))
        .layer(LogLayer::new())
}

fn api_router() -> Router {
    Router::new() // TODO: ULMS-1743
}

fn ws_router() -> Router {
    Router::new().route("/ws", get(ws::ws_handler))
}
