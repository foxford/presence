#![allow(unused_assignments)]
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
use tokio::time::{interval, MissedTickBehavior};
use tracing::info;

pub(crate) async fn handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<State>,
    Extension(authn): Extension<Arc<ConfigMap>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, authn, state))
}

async fn handle_socket(socket: WebSocket, authn: Arc<ConfigMap>, state: State) {
    let (mut sender, mut receiver) = socket.split();

    // Markers
    let mut authenticated = false;
    let mut ping_sent = false;
    let mut pong_received = false;

    // Intervals
    let mut ping_interval = interval(Duration::from_secs(state.config().websocket.ping_interval));
    let mut pong_expiration_interval = interval(Duration::from_secs(
        state.config().websocket.pong_expiration_interval,
    ));
    let mut auth_timeout_interval = interval(Duration::from_secs(
        state.config().websocket.authentication_timeout,
    ));

    // Because immediately completes
    auth_timeout_interval.tick().await;

    // In order not to hurry
    pong_expiration_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            // Get incoming messages from clients
            Some(result) = receiver.next() => {
                match result {
                    Ok(Message::Text(msg)) => match handle_request(msg, authn.clone()) {
                        Ok(m) => {
                            info!("Successful authentication");
                            authenticated = true;
                            let _ = sender.send(Message::Text(m)).await;
                            continue; // Start sending pings
                        }
                        Err(e) => {
                            info!("Connection is closed (unsuccessful request)");
                            authenticated = false;
                            let _ = sender.send(Message::Text(e)).await;
                            let _ = sender.close().await;
                            return;
                        }
                    },
                    Ok(Message::Pong(_)) => {
                        pong_received = true;
                        info!("Pong received");
                    },
                    Ok(Message::Close(frame)) => {
                        match frame {
                            Some(f) => {
                                info!(
                                    "An agent closed connection (code = {}, reason = {})",
                                    f.code, f.reason,
                                );
                            }
                            None => info!("An agent closed connection (no details)"),
                        }
                        return;
                    },
                    Err(e) => {
                        info!("An error occurred when receiving a message. {}", e);
                        return;
                    },
                    _ => {
                        // Ping/Binary messages - do nothing
                    }
                }
            }
            // Ping authenticated clients with interval
            _ = ping_interval.tick(), if authenticated => {
                if sender.send(Message::Ping(Vec::new())).await.is_err() {
                    info!("An agent disconnected (ping not sent)");
                    return;
                }
                info!("Ping sent");
                ping_sent = true;
                pong_received = false;
                pong_expiration_interval.tick().await;
            }
            // Close connection if pong exceeded timeout
            _ = pong_expiration_interval.tick(), if ping_sent => {
                if !pong_received {
                    info!("Connection is closed (pong timeout exceeded)");
                    let _ = sender.close().await;
                    return;
                }
                info!("Pong is OK");
                ping_sent = false;
            }
            // Close connection if there is no authentication for a while
            _ = auth_timeout_interval.tick(), if !authenticated => {
                info!("Connection is closed (authentication timeout exceeded)");
                let _ = sender.close().await;
                return;
            }
            // Stream has terminated
            else => {
                return;
            }
        }
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
