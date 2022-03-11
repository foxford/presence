use crate::app::ws::{ConnectError, ConnectRequest, Request, Response};
use crate::authz::AuthzObject;
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
    Authenticable,
};
use tokio::time::{interval, timeout, MissedTickBehavior};
use tracing::{error, info};

pub(crate) async fn handler<S: State>(
    ws: WebSocketUpgrade,
    Extension(state): Extension<S>,
    Extension(authn): Extension<Arc<ConfigMap>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, authn, state))
}

async fn handle_socket<S: State>(socket: WebSocket, authn: Arc<ConfigMap>, state: S) {
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
            Ok(msg) => match handle_authn_message(msg, authn, state.clone()).await {
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

async fn handle_authn_message<S: State>(
    message: Message,
    authn: Arc<ConfigMap>,
    state: S,
) -> Result<String, ConnectError> {
    let msg = match message {
        Message::Text(msg) => msg,
        _ => return Err(ConnectError::UnsupportedRequest),
    };

    let result = serde_json::from_str::<Request>(&msg);
    match result {
        Ok(Request::ConnectRequest(ConnectRequest {
            token,
            classroom_id,
        })) => {
            let agent_id = match get_agent_id_from_token(token, authn) {
                Ok(agent_id) => agent_id,
                Err(err) => {
                    error!(error = %err, "Failed to authenticate an agent");
                    return Err(ConnectError::Unauthenticated);
                }
            };

            let account_id = agent_id.as_account_id();
            let object = AuthzObject::new(&["classrooms", &classroom_id.to_string()]).into();

            if let Err(err) = state
                .authz()
                .authorize(
                    account_id.audience().to_string(),
                    account_id.clone(),
                    object,
                    "read".into(),
                )
                .await
            {
                error!("Failed to authorize action, reason = {:?}", err);
                return Err(ConnectError::AccessDenied);
            }

            Ok(make_response(Response::ConnectSuccess))
        }
        Err(_) => Err(ConnectError::UnsupportedRequest),
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
