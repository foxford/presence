use axum::body::{self};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use svc_error::{extension::sentry, Error as SvcError};
use tracing::error;

struct ErrorKindProperties {
    status: StatusCode,
    kind: &'static str,
    title: &'static str,
    is_notify_sentry: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    DbConnAcquisitionFailed,
    DbQueryFailed,
    SerializationFailed,
    ResponseBuildFailed,
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
        }
    }
}

pub struct Error {
    kind: ErrorKind,
    source: Box<dyn AsRef<dyn StdError + Send + Sync + 'static> + Send + Sync + 'static>,
}

impl Error {
    pub fn new<E>(kind: ErrorKind, source: E) -> Self
    where
        E: AsRef<dyn StdError + Send + Sync + 'static> + Send + Sync + 'static,
    {
        Self {
            kind,
            source: Box::new(source),
        }
    }

    pub fn to_svc_error(&self) -> SvcError {
        let properties: ErrorKindProperties = self.kind.into();

        SvcError::builder()
            .status(properties.status)
            .kind(properties.kind, properties.title)
            .detail(&self.source.as_ref().as_ref().to_string())
            .build()
    }

    pub fn notify_sentry(&self) {
        if !self.kind.is_notify_sentry() {
            return;
        }

        sentry::send(Arc::new(self.to_svc_error().into())).unwrap_or_else(|err| {
            error!("Error sending error to Sentry: {}", err);
        });
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let properties: ErrorKindProperties = self.kind.into();
        let body = serde_json::to_string(&self.to_svc_error()).expect("Infallible");

        Response::builder()
            .status(properties.status.as_u16())
            .body(body::boxed(body::Full::from(body)))
            .unwrap()
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("kind", &self.kind)
            .field("source", &self.source.as_ref().as_ref())
            .finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.source.as_ref().as_ref())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.source.as_ref().as_ref())
    }
}

pub trait ErrorExt<T> {
    fn error(self, kind: ErrorKind) -> Result<T, Error>;
}

impl<T, E: AsRef<dyn StdError + Send + Sync + 'static> + Send + Sync + 'static> ErrorExt<T>
    for Result<T, E>
{
    fn error(self, kind: ErrorKind) -> Result<T, Error> {
        self.map_err(|source| Error::new(kind, source))
    }
}
