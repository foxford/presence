use crate::{
    classroom::ClassroomId,
    session::{SessionId, SessionKey},
};
use http::StatusCode;
use serde::{de::Error, Deserialize, Deserializer};
use serde_derive::Serialize;
use std::sync::Arc;
use svc_error::{extension::sentry, Error as SvcError};

pub use handler::handler;

mod handler;

#[derive(Debug)]
struct Session {
    id: SessionId,
    key: SessionKey,
    kind: SessionKind,
}

impl Session {
    fn new(id: SessionId, key: SessionKey, kind: SessionKind) -> Self {
        Self { id, key, kind }
    }
}

#[derive(Debug)]
enum SessionKind {
    New,
    Replaced,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Request {
    ConnectRequest(ConnectRequest),
}

#[derive(Deserialize)]
pub struct ConnectRequest {
    classroom_id: ClassroomId,
    token: String,
    #[serde(deserialize_with = "deserialize_agent_label")]
    agent_label: String,
}

fn deserialize_agent_label<'de, D>(de: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(de)?;
    if s.is_empty() {
        return Err(D::Error::custom("agent_label is empty"));
    }

    Ok(s)
}

#[derive(Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Response {
    ConnectSuccess,
    UnrecoverableSessionError(SvcError),
    RecoverableSessionError(SvcError),
}

#[derive(PartialEq, Debug)]
enum UnrecoverableSessionError {
    AccessDenied,
    UnsupportedRequest,
    Unauthenticated,
    InternalServerError,
    SerializationFailed,
    AuthTimedOut,
    PongTimedOut,
    Replaced,
}

enum RecoverableSessionError {
    Terminated,
}

impl From<UnrecoverableSessionError> for Response {
    fn from(e: UnrecoverableSessionError) -> Self {
        let mut builder = SvcError::builder();

        builder = match e {
            UnrecoverableSessionError::UnsupportedRequest => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("unsupported_request", "Unsupported request"),
            UnrecoverableSessionError::Unauthenticated => builder
                .status(StatusCode::UNAUTHORIZED)
                .kind("unauthenticated", "Unauthenticated"),
            UnrecoverableSessionError::AccessDenied => builder
                .status(StatusCode::FORBIDDEN)
                .kind("access_denied", "Access denied"),
            UnrecoverableSessionError::InternalServerError => builder
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .kind("internal_server_error", "Internal server error"),
            UnrecoverableSessionError::SerializationFailed => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("serialization_failed", "Serialization failed"),
            UnrecoverableSessionError::AuthTimedOut => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("auth_timed_out", "Auth timed out"),
            UnrecoverableSessionError::PongTimedOut => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("pong_timed_out", "Pong timed out"),
            UnrecoverableSessionError::Replaced => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("replaced", "replaced"),
        };

        Response::UnrecoverableSessionError(builder.build())
    }
}

impl From<RecoverableSessionError> for Response {
    fn from(e: RecoverableSessionError) -> Self {
        let mut builder = SvcError::builder();

        builder = match e {
            RecoverableSessionError::Terminated => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("terminated", "terminated"),
        };

        Response::RecoverableSessionError(builder.build())
    }
}

impl From<anyhow::Error> for Response {
    fn from(e: anyhow::Error) -> Self {
        let err = SvcError::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .kind("internal_server_error", "Internal server error")
            .detail(&e.to_string())
            .build();

        if let Err(e) = sentry::send(Arc::new(e)) {
            tracing::error!(error = %e, "Failed to send error to sentry");
        }

        Response::UnrecoverableSessionError(err)
    }
}
