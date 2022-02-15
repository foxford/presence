use crate::state::State;
use anyhow::Result;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Extension, WebSocketUpgrade,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde_derive::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use svc_agent::AgentId;
use svc_authn::{
    jose::ConfigMap, token::jws_compact::extract::decode_jws_compact_with_config, AccountId,
};
use tracing::{error, info};

#[derive(Debug, Deserialize)]
enum Command {
    #[serde(rename = "auth")]
    Auth(Token),
}

#[derive(Debug, Deserialize)]
struct Token(String);

#[derive(Debug, Serialize)]
struct Response<'a> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
}

pub(crate) async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<State>,
    Extension(authn): Extension<Arc<ConfigMap>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, authn, state))
}

async fn handle_socket(socket: WebSocket, authn: Arc<ConfigMap>, _state: State) {
    let (mut sender, mut receiver) = socket.split();

    while let Some(result) = receiver.next().await {
        match result {
            Ok(Message::Text(msg)) => match handle_command(msg, authn.clone()) {
                Ok(m) => {
                    let _ = sender.send(Message::Text(m)).await;
                    break; // start sending pings
                }
                Err(e) => {
                    error!("An error occurred: {e}");
                    let _ = sender.send(Message::Text(e)).await;
                    let _ = sender.close().await;
                    return;
                }
            },
            Ok(Message::Close(_)) => {
                info!("An agent closed connection");
                return;
            }
            Err(_) => {
                info!("An agent disconnected");
                return;
            }
            _ => {
                // ping/pong - do nothing
            }
        }
    }

    loop {
        if sender.send(Message::Ping("ping".into())).await.is_err() {
            info!("An agent disconnected");
            return;
        }

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn handle_command(msg: String, authn: Arc<ConfigMap>) -> Result<String, String> {
    let result = serde_json::from_str::<Command>(&msg);
    match result {
        Ok(Command::Auth(token)) => match get_agent_id_from_token(token, authn) {
            Ok(_agent_id) => Ok(make_response(true, None)),
            Err(_) => Err(make_response(false, Some("Unauthorized"))),
        },
        Err(_) => Err(make_response(false, Some("Unsupported command"))),
    }
}

fn get_agent_id_from_token(token: Token, authn: Arc<ConfigMap>) -> Result<AgentId> {
    let data = decode_jws_compact_with_config::<String>(&token.0, authn.as_ref())?;
    let claims = data.claims;

    let account = AccountId::new(claims.subject(), claims.audience());
    let agent_id = AgentId::new("http", account);

    Ok(agent_id)
}

fn make_response(success: bool, message: Option<&str>) -> String {
    let resp = Response { success, message };
    serde_json::to_string(&resp).unwrap_or_default()
}
