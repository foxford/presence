use axum::response::Response;

pub mod v1;

pub type AppError = crate::app::error::Error;
pub type AppResult = Result<Response, AppError>;
