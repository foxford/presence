use axum::body::Body;
use axum::response::Response;

pub mod classroom;
pub mod counter;

pub async fn healthz() -> &'static str {
    "Ok"
}

pub async fn options() -> Response<Body> {
    Response::builder().body(Body::empty()).unwrap_or_default()
}
