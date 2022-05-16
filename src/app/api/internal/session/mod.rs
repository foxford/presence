use crate::{
    app::{
        api::AppResult,
        error::{ErrorExt, ErrorKind},
    },
    session::{SessionKey, SessionMap},
};
use anyhow::Context;
use axum::{body, response::IntoResponse, Extension, Json};
use http::StatusCode;
use serde_derive::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct DeletePayload {
    session_key: SessionKey,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
enum Response {
    DeleteSuccess,
    DeleteFailure(Reason),
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Reason {
    NotFound,
    FailedToSendMessage,
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
            if sender.send(()).is_ok() {
                return Ok(Json(Response::DeleteSuccess).into_response());
            }

            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Response::DeleteFailure(Reason::FailedToSendMessage),
            )
        }
        None => (
            StatusCode::NOT_FOUND,
            Response::DeleteFailure(Reason::NotFound),
        ),
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
