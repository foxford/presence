use crate::{
    app::{
        api::AppResult,
        error::{ErrorExt, ErrorKind},
        session_manager::{ConnectionCommand, DeleteSession},
    },
    session::{SessionKey, SessionMap},
};
use anyhow::Context;
use axum::{body, response::IntoResponse, Extension, Json};
use http::StatusCode;
use serde_derive::{Deserialize, Serialize};
use std::fmt;
use tokio::sync::oneshot;
use tracing::error;

#[derive(Deserialize, Serialize)]
pub struct DeletePayload {
    pub session_key: SessionKey,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Response {
    DeleteSuccess,
    DeleteFailure(Reason),
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    NotFound,
    FailedToDelete,
    FailedToSendMessage,
    FailedToReceiveResponse,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Reason::NotFound => "Not found",
            Reason::FailedToDelete => "Failed to delete from DB",
            Reason::FailedToSendMessage => "Failed to send message",
            Reason::FailedToReceiveResponse => "Failed to receive response",
        };
        write!(f, "{}", reason)
    }
}

pub async fn delete(
    Extension(sessions): Extension<SessionMap>,
    Json(payload): Json<DeletePayload>,
) -> AppResult {
    do_delete(sessions, payload).await
}

async fn do_delete(sessions: SessionMap, payload: DeletePayload) -> AppResult {
    let (status, resp) = match sessions.remove(&payload.session_key) {
        Some((_, sender)) => {
            let (tx, rx) = oneshot::channel::<DeleteSession>();

            match sender.send(ConnectionCommand::Close(Some(tx))) {
                Ok(_) => {
                    match rx.await {
                        Ok(DeleteSession::Success) => {
                            return Ok(Json(Response::DeleteSuccess).into_response())
                        }
                        Ok(DeleteSession::Failure) => (
                            StatusCode::UNPROCESSABLE_ENTITY,
                            Response::DeleteFailure(Reason::FailedToDelete),
                        ),
                        Err(e) => {
                            // TODO: Send to sentry
                            error!(error = %e, "Failed to receive response");

                            (
                                StatusCode::UNPROCESSABLE_ENTITY,
                                Response::DeleteFailure(Reason::FailedToReceiveResponse),
                            )
                        }
                    }
                }
                Err(_) => {
                    // TODO: Send to sentry
                    error!("Failed to send close command to connection");

                    (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Response::DeleteFailure(Reason::FailedToSendMessage),
                    )
                }
            }
        }
        None => (
            StatusCode::NOT_FOUND,
            Response::DeleteFailure(Reason::NotFound),
        ),
    };

    // TODO: Send error to sentry

    let body = serde_json::to_string(&resp)
        .context("Failed to serialize response")
        .error(ErrorKind::SerializationFailed)?;

    let resp = http::Response::builder()
        .status(status)
        .body(body::boxed(body::Full::from(body)))
        .context("Failed to build response for delete session")
        .error(ErrorKind::ResponseBuildFailed)?;

    Ok(resp)
}
