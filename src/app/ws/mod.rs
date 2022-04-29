use crate::classroom::ClassroomId;
use http::StatusCode;
use serde_derive::{Deserialize, Serialize};
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
    agent_label: String,
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
