use once_cell::sync::Lazy;
use prometheus::{
    register_histogram, register_int_counter_vec, register_int_gauge, Histogram, IntCounter,
    IntGauge,
};
use std::sync::Arc;

pub static AUTHZ_METRICS: Lazy<AuthzMetrics> = Lazy::new(AuthzMetrics::new);

pub struct AuthzMetrics {
    pub authz_time: Histogram,
}

impl AuthzMetrics {
    pub fn new() -> Self {
        Self {
            authz_time: register_histogram!("auth_time", "Authorization time")
                .expect("failed to register authz_time"),
        }
    }
}

pub trait AuthzMeasure {
    fn measure(self) -> Self;
}

impl AuthzMeasure for Result<chrono::Duration, svc_authz::error::Error> {
    fn measure(self) -> Self {
        if let Ok(Ok(d)) = self.as_ref().map(|d| d.to_std()) {
            let nanos = f64::from(d.subsec_nanos()) / 1e9;
            AUTHZ_METRICS.authz_time.observe(d.as_secs() as f64 + nanos)
        }
        self
    }
}

#[derive(Clone)]
pub struct Metrics {
    inner: Arc<InnerMetrics>,
}

impl Metrics {
    pub fn ws_connection_total(&self) -> &IntGauge {
        &self.inner.ws_connection_total
    }

    pub fn ws_connection_error(&self) -> &IntCounter {
        &self.inner.ws_connection_error
    }

    pub fn ws_connection_success(&self) -> &IntCounter {
        &self.inner.ws_connection_success
    }
}

struct InnerMetrics {
    ws_connection_total: IntGauge,
    ws_connection_error: IntCounter,
    ws_connection_success: IntCounter,
}

impl Metrics {
    pub fn new() -> Self {
        let counter =
            register_int_counter_vec!("ws_connection", "WebSocket connection types", &["status"])
                .expect("failed to register ws_counter");

        Self {
            inner: Arc::new(InnerMetrics {
                ws_connection_total: register_int_gauge!(
                    "ws_connection_total",
                    "WebSocket connection total"
                )
                .expect("failed to register ws_connection_total"),
                ws_connection_error: counter.with_label_values(&["error"]),
                ws_connection_success: counter.with_label_values(&["success"]),
            }),
        }
    }
}
