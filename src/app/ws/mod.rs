use http::StatusCode;
use serde_derive::{Deserialize, Serialize};
use svc_error::Error as SvcError;

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
    ConnectFailure(SvcError),
}

enum ConnectFailure {
    UnsupportedRequest,
    Unauthenticated,
    // TODO: ULMS-1745
    // AccessDenied,
}

impl From<ConnectFailure> for Response {
    fn from(f: ConnectFailure) -> Self {
        let mut builder = SvcError::builder();

        builder = match f {
            ConnectFailure::UnsupportedRequest => builder
                .status(StatusCode::UNPROCESSABLE_ENTITY)
                .kind("unsupported_request", "Unsupported Request"),
            ConnectFailure::Unauthenticated => builder
                .status(StatusCode::UNAUTHORIZED)
                .kind("unauthenticated", "Unauthenticated"),
            // TODO: ULMS-1745
            // ConnectFailure::AccessDenied => builder
            //     .status(StatusCode::FORBIDDEN)
            //     .kind("access_denied", "Access Denied"),
        };

        let error = builder.build();

        Response::ConnectFailure(error)
    }
}
