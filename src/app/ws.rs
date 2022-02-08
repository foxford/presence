use axum::extract::ws::{Message, WebSocket};
use axum::extract::WebSocketUpgrade;
use axum::response::IntoResponse;

pub(crate) async fn ws_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    println!("ws_handler called");
    ws.on_upgrade(handle_socket)
}

pub(crate) async fn handle_socket(mut socket: WebSocket) {
    if let Some(msg) = socket.recv().await {
        println!("New incoming request");

        let msg = if let Ok(msg) = msg {
            match msg {
                Message::Close(_) => {
                    println!("Client disconnected/1");
                    return;
                }
                _ => msg,
            }
        } else {
            println!("Client disconnected/2");
            return;
        };

        if socket.send(msg).await.is_err() {
            println!("Client disconnected/3");
            return;
        }
    }
}
