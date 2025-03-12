use anyhow::Result;
use axum::{routing::get, Json, Router};
use serde::Serialize;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

pub async fn run_http_server(addr: &str) -> Result<()> {
    let app = Router::new().route("/health", get(health));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("HTTP server listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
