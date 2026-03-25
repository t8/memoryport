use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use metrics::{counter, histogram};
use metrics_exporter_prometheus::PrometheusHandle;
use tokio::time::Instant;

/// Initialize the Prometheus metrics recorder. Returns a handle for the /metrics endpoint.
pub fn init_metrics() -> PrometheusHandle {
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    builder
        .install_recorder()
        .expect("failed to install metrics recorder")
}

/// Middleware that records request count and latency.
pub async fn metrics_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let endpoint = request.uri().path().to_string();
    let method = request.method().to_string();
    let start = Instant::now();

    let response = next.run(request).await;

    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    let status = response.status().as_u16().to_string();

    counter!("uc_request_total", "endpoint" => endpoint.clone(), "method" => method, "status" => status).increment(1);
    histogram!("uc_request_duration_ms", "endpoint" => endpoint).record(duration_ms);

    response
}

/// Handler for GET /metrics — returns Prometheus text format.
pub async fn metrics_handler(
    axum::extract::State(handle): axum::extract::State<PrometheusHandle>,
) -> String {
    handle.render()
}
