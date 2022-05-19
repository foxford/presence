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
    ConnectFailure(SvcError),
}

#[derive(PartialEq, Debug)]
enum ConnectError {
    UnsupportedRequest,
    Unauthenticated,
    AccessDenied,
    DbConnAcquisitionFailed,
    DbQueryFailed,
    SerializationFailed,
    MessagingFailed,
    CloseOldConnectionFailed,
}

impl From<ConnectError> for Response {
    fn from(e: ConnectError) -> Self {
        let mut builder = SvcError::builder();

        builder = match e {
            ConnectError::UnsupportedRequest => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("unsupported_request", "Unsupported Request"),
            ConnectError::Unauthenticated => builder
                .status(StatusCode::UNAUTHORIZED)
                .kind("unauthenticated", "Unauthenticated"),
            ConnectError::AccessDenied => builder
                .status(StatusCode::FORBIDDEN)
                .kind("access_denied", "Access Denied"),
            ConnectError::DbConnAcquisitionFailed => {
                builder.status(StatusCode::UNPROCESSABLE_ENTITY).kind(
                    "database_connection_acquisition_failed",
                    "Database connection acquisition failed",
                )
            }
            ConnectError::DbQueryFailed => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("database_query_failed", "Database query failed"),
            ConnectError::SerializationFailed => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("serialization_failed", "Serialization failed"),
            ConnectError::MessagingFailed => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("messaging_failed", "Messaging failed"),
            ConnectError::CloseOldConnectionFailed => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("close_old_connection_failed", "Close old connection failed"),
        };

        let error = builder.build();

        Response::ConnectFailure(error)
    }
}

impl From<anyhow::Error> for Response {
    fn from(e: anyhow::Error) -> Self {
        let error = SvcError::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .kind("internal_error", "Internal error")
            .detail(&e.to_string())
            .build();

        Response::ConnectFailure(error)
    }
}
