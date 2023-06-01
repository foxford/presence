use crate::{
    app::{
        api::AppResult,
        error::{Error, ErrorExt, ErrorKind},
        session_manager::DeleteSession,
        state::State,
    },
    session::{SessionId, SessionKey},
};
use anyhow::Context;
use axum::{body, response::IntoResponse, Extension, Json};
use http::StatusCode;
use serde_derive::{Deserialize, Serialize};
use std::fmt;
use tracing::error;

#[derive(Deserialize, Serialize, Debug)]
pub struct DeletePayload {
    pub session_key: SessionKey,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Response {
    DeleteSuccess(SessionId),
    DeleteFailure(Reason),
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    NotFound,
    MessagingFailed,
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Reason::NotFound => "Not found",
            Reason::MessagingFailed => "Messaging failed",
        };
        write!(f, "{}", reason)
    }
}

pub async fn delete<S: State>(
    Extension(state): Extension<S>,
    Json(payload): Json<DeletePayload>,
) -> AppResult {
    do_delete(state, payload).await
}

async fn do_delete<S: State>(state: S, payload: DeletePayload) -> AppResult {
    let (status, resp) = match state.delete_session(payload.session_key).await {
        Ok(DeleteSession::Success(session_id)) => {
            return Ok(Json(Response::DeleteSuccess(session_id)).into_response());
        }
        Ok(DeleteSession::NotFound) => (
            StatusCode::NOT_FOUND,
            Response::DeleteFailure(Reason::NotFound),
        ),
        Err(e) => {
            error!(error = %e, "Failed to delete session");
            Error::new(ErrorKind::ReceivingResponseFailed, e).notify_sentry();

            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Response::DeleteFailure(Reason::MessagingFailed),
            )
        }
    };

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
