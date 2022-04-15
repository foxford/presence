use crate::classroom::ClassroomId;
use http::StatusCode;
use serde_derive::{Deserialize, Serialize};
use svc_error::Error as SvcError;

pub use handler::handler;

mod handler;

#[derive(Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub(crate) enum Request {
    ConnectRequest(ConnectRequest),
}

#[derive(Deserialize)]
pub(crate) struct ConnectRequest {
    classroom_id: ClassroomId,
    token: String,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub(crate) enum Response {
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
    fn from(f: ConnectError) -> Self {
        let mut builder = SvcError::builder();

        builder = match f {
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
