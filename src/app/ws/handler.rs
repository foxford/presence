use crate::app::ws::{ConnectFailure, ConnectRequest, Request, Response};
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
use std::sync::Arc;
use std::time::Duration;
use svc_agent::AgentId;
use svc_authn::{
    jose::ConfigMap, token::jws_compact::extract::decode_jws_compact_with_config, AccountId,
};
use tracing::info;

pub(crate) async fn handler(
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
            Ok(Message::Text(msg)) => match handle_request(msg, authn.clone()) {
                Ok(m) => {
                    let _ = sender.send(Message::Text(m)).await;
                    break; // start sending pings
                }
                Err(e) => {
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

fn handle_request(msg: String, authn: Arc<ConfigMap>) -> Result<String, String> {
    let result = serde_json::from_str::<Request>(&msg);
    match result {
        Ok(Request::ConnectRequest(ConnectRequest { token, .. })) => {
            match get_agent_id_from_token(token, authn) {
                Ok(_agent_id) => Ok(make_response(Response::ConnectSuccess)),
                Err(_) => Err(make_response(ConnectFailure::Unauthenticated.into())),
            }
        }
        Err(_) => Err(make_response(ConnectFailure::UnsupportedRequest.into())),
    }
}

fn get_agent_id_from_token(token: String, authn: Arc<ConfigMap>) -> Result<AgentId> {
    let data = decode_jws_compact_with_config::<String>(&token, authn.as_ref())?;
    let claims = data.claims;

    let account = AccountId::new(claims.subject(), claims.audience());
    let agent_id = AgentId::new("http", account);

    Ok(agent_id)
}

fn make_response(response: Response) -> String {
    serde_json::to_string(&response).unwrap_or_default()
}
