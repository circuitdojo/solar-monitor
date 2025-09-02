//! Minimal entrypoint

use anyhow::Result;
use axum::routing::get;
use axum::Router;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // Compose minimal router with health endpoint
    let registry = solar_monitor_protocols::create_registry();
    let store = Arc::new(solar_monitor_storage::DataStore::new("./data/solar.db").await?);
    let (tx, _rx) = tokio::sync::broadcast::channel::<contracts::DeviceData>(100);
    let state = Arc::new(solar_monitor_api::AppState {
        registry: Arc::new(registry),
        store,
        tasks: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        devices: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        tx,
    });
    // Auto-start polling for persisted, enabled devices
    {
        let configs = state.store.list_device_configs().await.unwrap_or_default();
        for cfg in configs.into_iter().filter(|c| c.enabled) {
            state.devices.lock().await.insert(cfg.id.clone(), cfg.clone());
            let _ = solar_monitor_api::start_polling(state.clone(), cfg).await;
        }
    }
    let app = solar_monitor_api::router(state)
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
