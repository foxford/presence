use crate::classroom::ClassroomId;
use http::StatusCode;
use serde::{de::Error, Deserialize, Deserializer};
use serde_derive::Serialize;
use svc_error::Error as SvcError;

pub use handler::handler;

pub mod event;
mod handler;

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
    UnsupportedRequest,
    Unauthenticated,
    AccessDenied,
    DbConnAcquisitionFailed,
    DbQueryFailed,
    SerializationFailed,
    MessagingFailed,
    CloseOldConnectionFailed,
    AuthTimedOut,
    PongTimedOut,
    Replaced,
}

#[allow(dead_code)]
enum RecoverableSessionError {
    Terminated,
}

impl From<UnrecoverableSessionError> for Response {
    fn from(e: UnrecoverableSessionError) -> Self {
        let mut builder = SvcError::builder();

        builder = match e {
            UnrecoverableSessionError::UnsupportedRequest => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("unsupported_request", "Unsupported Request"),
            UnrecoverableSessionError::Unauthenticated => builder
                .status(StatusCode::UNAUTHORIZED)
                .kind("unauthenticated", "Unauthenticated"),
            UnrecoverableSessionError::AccessDenied => builder
                .status(StatusCode::FORBIDDEN)
                .kind("access_denied", "Access Denied"),
            UnrecoverableSessionError::DbConnAcquisitionFailed => {
                builder.status(StatusCode::UNPROCESSABLE_ENTITY).kind(
                    "database_connection_acquisition_failed",
                    "Database connection acquisition failed",
                )
            }
            UnrecoverableSessionError::DbQueryFailed => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("database_query_failed", "Database query failed"),
            UnrecoverableSessionError::SerializationFailed => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("serialization_failed", "Serialization failed"),
            UnrecoverableSessionError::MessagingFailed => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("messaging_failed", "Messaging failed"),
            UnrecoverableSessionError::CloseOldConnectionFailed => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("close_old_connection_failed", "Close old connection failed"),
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
        let error = SvcError::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .kind("internal_server_error", "Internal server error")
            .detail(&e.to_string())
            .build();

        Response::UnrecoverableSessionError(error)
    }
}
