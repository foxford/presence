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
    _is_notify_sentry: bool,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    DbConnAcquisitionFailed,
    DbQueryFailed,
    AccessDenied,
    AuthorizationFailed,
}

impl ErrorKind {
    pub fn _is_notify_sentry(self) -> bool {
        let properties: ErrorKindProperties = self.into();
        properties._is_notify_sentry
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
                _is_notify_sentry: true,
            },
            ErrorKind::DbQueryFailed => ErrorKindProperties {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                kind: "database_query_failed",
                title: "Database query failed",
                _is_notify_sentry: true,
            },
            ErrorKind::AccessDenied => ErrorKindProperties {
                status: StatusCode::FORBIDDEN,
                kind: "access_denied",
                title: "Access denied",
                _is_notify_sentry: false,
            },
            ErrorKind::AuthorizationFailed => ErrorKindProperties {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                kind: "authorization_failed",
                title: "Authorization failed",
                _is_notify_sentry: false,
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

    pub fn _notify_sentry(&self) {
        if !self.kind._is_notify_sentry() {
            return;
        }

        if let Err(e) = sentry::send(self.err.clone()) {
            tracing::error!("Failed to send error to sentry, reason = {:?}", e);
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response<BoxBody> {
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
