use crate::app::ws::handler;
use crate::state::State;
use axum::{
    routing::get,
    {AddExtensionLayer, Router},
};
use std::sync::Arc;
use svc_utils::middleware::LogLayer;

pub(crate) fn new(state: State, authn: svc_authn::jose::ConfigMap) -> Router {
    let router = api_router().merge(ws_router());

    router
        .layer(AddExtensionLayer::new(Arc::new(authn)))
        .layer(AddExtensionLayer::new(state))
        .layer(LogLayer::new())
}

fn api_router() -> Router {
    Router::new() // TODO: ULMS-1743
}

fn ws_router() -> Router {
    Router::new().route("/ws", get(handler))
}
