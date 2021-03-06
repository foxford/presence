use axum::{
    body::BoxBody,
    response::{IntoResponse, Response},
    Json,
};
use http::StatusCode;
use std::{fmt, sync::Arc};
use svc_error::{extension::sentry, Error as SvcError};

struct ErrorKindProperties {
    status: StatusCode,
    kind: &'static str,
    title: &'static str,
    is_notify_sentry: bool,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    DbConnAcquisitionFailed,
    DbQueryFailed,
    AccessDenied,
    AuthorizationFailed,
    SerializationFailed,
    ResponseBuildFailed,
    ShutdownFailed,
    MovingSessionToHistoryFailed,
    ReceivingResponseFailed,
}

impl ErrorKind {
    pub fn is_notify_sentry(self) -> bool {
        let properties: ErrorKindProperties = self.into();
        properties.is_notify_sentry
    }
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let properties: ErrorKindProperties = self.to_owned().into();
        write!(f, "{}", properties.title)
    }
}

impl From<ErrorKind> for ErrorKindProperties {
    fn from(k: ErrorKind) -> Self {
        match k {
            ErrorKind::DbConnAcquisitionFailed => ErrorKindProperties {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                kind: "database_connection_acquisition_failed",
                title: "Database connection acquisition failed",
                is_notify_sentry: true,
            },
            ErrorKind::DbQueryFailed => ErrorKindProperties {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                kind: "database_query_failed",
                title: "Database query failed",
                is_notify_sentry: true,
            },
            ErrorKind::AccessDenied => ErrorKindProperties {
                status: StatusCode::FORBIDDEN,
                kind: "access_denied",
                title: "Access denied",
                is_notify_sentry: false,
            },
            ErrorKind::AuthorizationFailed => ErrorKindProperties {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                kind: "authorization_failed",
                title: "Authorization failed",
                is_notify_sentry: false,
            },
            ErrorKind::SerializationFailed => ErrorKindProperties {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                kind: "serialization_failed",
                title: "Serialization failed",
                is_notify_sentry: true,
            },
            ErrorKind::ResponseBuildFailed => ErrorKindProperties {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                kind: "response_build_failed",
                title: "Response build failed",
                is_notify_sentry: true,
            },
            ErrorKind::ShutdownFailed => ErrorKindProperties {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                kind: "shutdown_failed",
                title: "Shutdown failed",
                is_notify_sentry: true,
            },
            ErrorKind::MovingSessionToHistoryFailed => ErrorKindProperties {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                kind: "move_sessions_to_history_failed",
                title: "Move sessions to history failed",
                is_notify_sentry: true,
            },
            ErrorKind::ReceivingResponseFailed => ErrorKindProperties {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                kind: "receiving_response_failed",
                title: "Receiving response failed",
                is_notify_sentry: true,
            },
        }
    }
}

pub struct Error {
    kind: ErrorKind,
    err: Arc<anyhow::Error>,
}

impl Error {
    pub fn new(kind: ErrorKind, err: anyhow::Error) -> Self {
        Self {
            kind,
            err: Arc::new(err),
        }
    }

    pub fn to_svc_error(&self) -> SvcError {
        let properties: ErrorKindProperties = self.kind.into();

        SvcError::builder()
            .status(properties.status)
            .kind(properties.kind, properties.title)
            .detail(&self.err.to_string())
            .build()
    }

    pub fn notify_sentry(&self) {
        if !self.kind.is_notify_sentry() {
            return;
        }

        if let Err(e) = sentry::send(self.err.clone()) {
            tracing::error!("Failed to send error to sentry, reason = {:?}", e);
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response<BoxBody> {
        self.notify_sentry();

        let properties: ErrorKindProperties = self.kind.into();
        (properties.status, Json(&self.to_svc_error())).into_response()
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("kind", &self.kind)
            .field("source", &self.err)
            .finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.err)
    }
}

impl From<svc_authz::Error> for Error {
    fn from(source: svc_authz::Error) -> Self {
        let kind = match source.kind() {
            svc_authz::ErrorKind::Forbidden(_) => ErrorKind::AccessDenied,
            _ => ErrorKind::AuthorizationFailed,
        };

        Self {
            kind,
            err: Arc::new(source.into()),
        }
    }
}

pub trait ErrorExt<T> {
    fn error(self, kind: ErrorKind) -> Result<T, Error>;
}

impl<T> ErrorExt<T> for Result<T, anyhow::Error> {
    fn error(self, kind: ErrorKind) -> Result<T, Error> {
        self.map_err(|source| Error::new(kind, source))
    }
}
