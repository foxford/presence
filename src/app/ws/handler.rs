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
use svc_agent::AgentId;
use svc_authn::{
    jose::ConfigMap, token::jws_compact::extract::decode_jws_compact_with_config, AccountId,
};
use tokio::time::{interval, timeout, MissedTickBehavior};
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

    // Close connection if there is no messages for some time
    let authn_timeout = timeout(
        state.config().websocket.authentication_timeout,
        receiver.next(),
    )
    .await;

    // Authentication
    match authn_timeout {
        Ok(Some(result)) => match result {
            Ok(msg) => match handle_authn_message(msg, authn) {
                Ok(m) => {
                    info!("Successful authentication");
                    let _ = sender.send(Message::Text(m)).await;
                }
                Err(e) => {
                    info!("Connection is closed (unsuccessful request)");
                    let resp = make_response(e.into());
                    let _ = sender.send(Message::Text(resp)).await;
                    let _ = sender.close().await;
                    return;
                }
            },
            Err(e) => {
                info!(error = %e, "An error occurred when receiving a message.");
                return;
            }
        },
        Ok(None) | Err(_) => {
            info!("Connection is closed (authentication timeout exceeded)");
            let _ = sender.close().await;
            return;
        }
    }

    let mut ping_sent = false;

    // Ping/Pong intervals
    let mut ping_interval = interval(state.config().websocket.ping_interval);
    let mut pong_expiration_interval = interval(state.config().websocket.pong_expiration_interval);

    // In order not to hurry
    ping_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    pong_expiration_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Get Pong/Close messages from client
            Some(result) = receiver.next() => {
                match result {
                    Ok(Message::Pong(_)) => {
                        info!("Pong received");
                        ping_sent = false;
                    },
                    Ok(Message::Close(frame)) => {
                        match frame {
                            Some(f) => {
                                info!(code = %f.code, reason = %f.reason, "An agent closed connection");
                            }
                            None => info!("An agent closed connection"),
                        }
                        return;
                    },
                    Err(e) => {
                        info!(error = %e, "An error occurred when receiving a message.");
                        return;
                    },
                    _ => {
                        // Ping/Text/Binary messages - do nothing
                    }
                }
            }
            // Ping authenticated clients with interval
            _ = ping_interval.tick() => {
                if sender.send(Message::Ping(Vec::new())).await.is_err() {
                    info!("An agent disconnected (ping not sent)");
                    return;
                }
                info!("Ping sent");
                ping_sent = true;
                pong_expiration_interval.reset();
            }
            // Close connection if pong exceeded timeout
            _ = pong_expiration_interval.tick() => {
                if ping_sent {
                    info!("Connection is closed (pong timeout exceeded)");
                    let _ = sender.close().await;
                    return;
                }
                info!("Pong is OK");
            }
        }
    }
}

fn handle_authn_message(message: Message, authn: Arc<ConfigMap>) -> Result<String, ConnectFailure> {
    let msg = match message {
        Message::Text(msg) => msg,
        _ => return Err(ConnectFailure::UnsupportedRequest),
    };

    let result = serde_json::from_str::<Request>(&msg);
    match result {
        Ok(Request::ConnectRequest(ConnectRequest { token, .. })) => {
            match get_agent_id_from_token(token, authn) {
                Ok(_agent_id) => Ok(make_response(Response::ConnectSuccess)),
                Err(_) => Err(ConnectFailure::Unauthenticated),
            }
        }
        Err(_) => Err(ConnectFailure::UnsupportedRequest),
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
