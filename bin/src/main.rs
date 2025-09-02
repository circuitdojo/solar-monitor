//! Minimal entrypoint

use anyhow::Result;
use axum::routing::get;
use axum::Router;

#[tokio::main]
async fn main() -> Result<()> {
    // Compose minimal router with health endpoint
    let app = Router::new()
        .merge(solar_monitor_api::router())
        .route("/", get(|| async { "solar-monitor" }));

    // Only start server if run with SERVE=1 to keep placeholder minimal
    if std::env::var("SERVE").ok().as_deref() == Some("1") {
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", 8080)).await?;
        axum::serve(listener, app).await?;
    }

    println!(
        "solar-monitor v{} | protocols: {:?}",
        solar_monitor_core::version(),
        solar_monitor_protocols::registered()
    );
    Ok(())
}
