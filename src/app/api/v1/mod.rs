pub mod classroom;

pub async fn healthz() -> &'static str {
    "Ok"
}
