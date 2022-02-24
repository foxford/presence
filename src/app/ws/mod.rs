use http::StatusCode;
use serde_derive::{Deserialize, Serialize};
use std::fmt::Debug;
use svc_error::Error as SvcError;
use thiserror::Error;

pub(crate) use handler::handler;

mod handler;

#[derive(Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub(crate) enum Request {
    ConnectRequest(ConnectRequest),
}

#[derive(Deserialize)]
pub(crate) struct ConnectRequest {
    // TODO: ULMS-1745
    _classroom_id: String,
    token: String,
}

#[derive(Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub(crate) enum Response {
    ConnectSuccess,
    ConnectError(SvcError),
}

#[derive(Error, Debug)]
pub enum ConnectError {
    #[error("Unsupported Request")]
    UnsupportedRequest,

    #[error("Unauthenticated")]
    Unauthenticated,
    // TODO: ULMS-1745
    // AccessDenied,
}

impl From<ConnectError> for Response {
    fn from(f: ConnectError) -> Self {
        let mut builder = SvcError::builder();

        builder = match f {
            ConnectError::UnsupportedRequest => builder.status(StatusCode::UNPROCESSABLE_ENTITY),
            ConnectError::Unauthenticated => builder.status(StatusCode::UNAUTHORIZED),
            // TODO: ULMS-1745
            // ConnectError::AccessDenied => builder
            //     .status(StatusCode::FORBIDDEN)
            //     .kind("access_denied", "Access Denied"),
        };

        let title = format!("{f}");
        let kind = title
            .to_lowercase()
            .replace(|c: char| !c.is_alphanumeric(), "_");

        let error = builder.kind(&kind, &title).build();

        Response::ConnectError(error)
    }
}
