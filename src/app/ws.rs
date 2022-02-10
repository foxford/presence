use crate::state::State;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Extension, WebSocketUpgrade};
use axum::response::IntoResponse;
use tracing::info;

pub(crate) async fn ws_handler(ws: WebSocketUpgrade, state: Extension<State>) -> impl IntoResponse {
    info!("An agent connected");
    ws.on_upgrade(|socket| handle_ws_socket(socket, state))
}

pub(crate) async fn handle_ws_socket(mut socket: WebSocket, _state: Extension<State>) {
    if let Some(msg) = socket.recv().await {
        match msg {
            Ok(msg) => {
                if let Message::Close(_) = msg {
                    info!("An agent closed connection");
                };
            }
            Err(_) => {
                info!("An agent disconnected");
            }
        }
    }
}
