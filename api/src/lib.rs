//! Minimal API router: health + WebSocket + RS485 helpers

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use solar_monitor_core as core;
use tokio::sync::broadcast;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

pub struct AppState {
    pub registry: Arc<core::ProtocolRegistry>,
    pub store: Arc<solar_monitor_storage::DataStore>,
    pub tasks: tokio::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    pub devices: tokio::sync::Mutex<HashMap<String, core::DeviceConfig>>, // in-memory device registry
    pub tx: broadcast::Sender<contracts::DeviceData>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/ws", get(ws_upgrade))
        .route("/api/v1/system/serial-ports", get(list_serial_ports))
        .route("/api/v1/protocols/discovery", post(discover_devices))
        .route("/api/v1/devices/test-params", post(test_params))
        .route("/api/v1/devices", get(list_devices).post(add_device))
        .route("/api/v1/devices/:id", axum::routing::delete(remove_device))
        .route("/api/v1/devices/:id/data/latest", get(get_latest_data))
        .with_state(state)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "healthy" })
}

async fn ws_upgrade(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws(state, socket))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WsEnvelope<T> {
    message_type: &'static str,
    timestamp: String,
    data: T,
}

async fn handle_ws(state: Arc<AppState>, mut socket: WebSocket) {
    let mut rx = state.tx.subscribe();
    // Basic loop: forward device data to client as JSON
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(data) => {
                        let env = WsEnvelope { message_type: "device_data", timestamp: Utc::now().to_rfc3339(), data };
                        if let Ok(text) = serde_json::to_string(&env) {
                            if socket.send(Message::Text(text)).await.is_err() { break; }
                        }
                    }
                    Err(_) => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(30)) => {
                // heartbeat
                if socket.send(Message::Text("{\"messageType\":\"heartbeat\"}".into())).await.is_err() { break; }
            }
        }
    }
}

async fn list_serial_ports() -> Json<Vec<String>> {
    let ports = match serialport::available_ports() {
        Ok(p) => p.into_iter().map(|p| p.port_name).collect(),
        Err(_) => Vec::new(),
    };
    Json(ports)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryRequest {
    serial_ports: Vec<String>,
    timeout_seconds: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveredDeviceDto {
    id: String,
    name: String,
    device_type: contracts::DeviceType,
    protocol: String,
    connection_params: std::collections::HashMap<String, String>,
}

async fn discover_devices(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<DiscoveryRequest>,
) -> Result<Json<Vec<DiscoveredDeviceDto>>, axum::http::StatusCode> {
    let scan = core::ScanConfig {
        serial_ports: req.serial_ports,
        timeout_seconds: req.timeout_seconds.unwrap_or(3),
    };

    let mut found = Vec::new();
    if let Some(proto) = state.registry.get_protocol("eg4-pi30-rs485") {
        let devices = proto
            .discover_devices(&scan)
            .await
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        for d in devices {
            found.push(DiscoveredDeviceDto {
                id: d.id,
                name: d.name,
                device_type: d.device_type,
                protocol: d.protocol,
                connection_params: d.connection_params,
            });
        }
    }

    Ok(Json(found))
}

async fn list_devices(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> Result<Json<Vec<contracts::DeviceListItemDto>>, axum::http::StatusCode> {
    // Read from persistent storage
    let configs = state
        .store
        .list_device_configs()
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    // Snapshot of active tasks for isPolling
    let tasks = state.tasks.lock().await;
    let list = configs
        .into_iter()
        .map(|c| {
            let is_polling = tasks.contains_key(&c.id);
            contracts::DeviceListItemDto {
                id: c.id,
                name: c.name,
                device_type: c.device_type,
                protocol_name: c.protocol,
                enabled: c.enabled,
                poll_interval_seconds: c.poll_interval_seconds,
                connection_params: c.connection_params,
                is_polling,
            }
        })
        .collect();

    Ok(Json(list))
}


async fn add_device(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<contracts::AddDeviceRequestDto>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    if !req.enabled {
        return Ok(Json(serde_json::json!({ "status": "disabled", "id": req.id })));
    }

    // Build core config
    let cfg = core::DeviceConfig {
        id: req.id.clone(),
        name: req.name.clone(),
        device_type: req.device_type,
        protocol: req.protocol_name.clone(),
        connection_params: req.connection_params.clone(),
        enabled: req.enabled,
        poll_interval_seconds: req.poll_interval_seconds,
    };

    // Persist device
    state.store.upsert_device_config(&cfg).await.map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    // Spawn polling task
    start_polling(state.clone(), cfg.clone())
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    // Save to in-memory map
    {
        let mut devices = state.devices.lock().await;
        devices.insert(req.id.clone(), cfg.clone());
    }

    Ok(Json(serde_json::json!({ "status": "ok", "id": req.id })))
}

async fn get_latest_data(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<Option<contracts::DeviceData>>, axum::http::StatusCode> {
    let data = state
        .store
        .get_latest_device_data(&id)
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(data))
}

pub async fn start_polling(state: Arc<AppState>, cfg: core::DeviceConfig) -> anyhow::Result<()> {
    // Avoid duplicate tasks
    {
        let tasks = state.tasks.lock().await;
        if tasks.contains_key(&cfg.id) {
            return Ok(());
        }
    }

    let proto = state
        .registry
        .get_protocol(&cfg.protocol)
        .ok_or_else(|| anyhow::anyhow!("unsupported protocol"))?;

    // Connect before spawning, to validate params
    let mut conn = proto.connect(&cfg).await?;
    let id = cfg.id.clone();
    let poll_every = cfg.poll_interval_seconds.max(5);
    let store = state.store.clone();
    let tx = state.tx.clone();
    let handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(poll_every as u64));
        loop {
            interval.tick().await;
            match conn.read_data().await {
                Ok(data) => {
                    let _ = store.store_device_data(&data).await;
                    let _ = tx.send(data);
                }
                Err(e) => {
                    tracing::warn!("poll {} failed: {}", id, e);
                }
            }
        }
    });

    let mut tasks = state.tasks.lock().await;
    tasks.insert(cfg.id.clone(), handle);
    Ok(())
}

async fn remove_device(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    // Stop task if running
    if let Some(handle) = state.tasks.lock().await.remove(&id) {
        handle.abort();
    }
    // Remove from in-memory registry
    state.devices.lock().await.remove(&id);
    let _ = state.store.delete_device_config(&id).await;
    Ok(Json(serde_json::json!({"status": "removed", "id": id })))
}

async fn test_params(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<contracts::TestConnectionParamsDto>,
) -> Result<Json<contracts::TestConnectionResponseDto>, axum::http::StatusCode> {
    // Build a transient DeviceConfig
    let cfg = core::DeviceConfig {
        id: format!("test-{}", req.protocol_name),
        name: "test".to_string(),
        device_type: req.device_type,
        protocol: req.protocol_name,
        connection_params: req.connection_params,
        enabled: false,
        poll_interval_seconds: 30,
    };
    let resp = match state
        .registry
        .get_protocol(&cfg.protocol)
        .ok_or_else(|| anyhow::anyhow!("unsupported protocol"))
    {
        Ok(p) => match p.connect(&cfg).await {
            Ok(_) => contracts::TestConnectionResponseDto { ok: true, message: None },
            Err(e) => contracts::TestConnectionResponseDto { ok: false, message: Some(e.to_string()) },
        },
        Err(e) => contracts::TestConnectionResponseDto { ok: false, message: Some(e.to_string()) },
    };
    Ok(Json(resp))
}
