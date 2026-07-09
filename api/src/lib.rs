//! Minimal API router: health + WebSocket + RS485 helpers

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{http::StatusCode, response::Response};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use solar_monitor_core as core;
use tokio::sync::broadcast;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

// Unified API error type
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    Internal(String),
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        let body = contracts::ErrorResponseDto {
            error: status.canonical_reason().unwrap_or("error").to_string(),
            details: message,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        (status, Json(body)).into_response()
    }
}

type ApiResult<T> = Result<T, ApiError>;

pub struct AppState {
    pub registry: Arc<core::ProtocolRegistry>,
    pub store: Arc<solar_monitor_storage::DataStore>,
    pub tasks: tokio::sync::Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    pub devices: tokio::sync::Mutex<HashMap<String, core::DeviceConfig>>, // in-memory device registry
    pub tx: broadcast::Sender<contracts::DeviceData>,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

pub fn router(state: Arc<AppState>) -> Router {
    let app = Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/status", get(system_status))
        .route("/api/v1/ws", get(ws_upgrade))
        .route("/api/v1/system/serial-ports", get(list_serial_ports))
        .route("/api/v1/protocols", get(list_protocols))
        .route("/api/v1/protocols/discovery", post(discover_devices))
        .route("/api/v1/devices/test-params", post(test_params))
        .route("/api/v1/devices", get(list_devices).post(add_device))
        .route(
            "/api/v1/devices/{id}",
            get(get_device).put(update_device).delete(remove_device),
        )
        .route("/api/v1/devices/{id}/settings", get(get_device_settings))
        .route(
            "/api/v1/devices/{id}/settings/{key}",
            axum::routing::put(write_device_setting),
        )
        .route("/api/v1/devices/{id}/data", get(get_device_data_range))
        .route("/api/v1/devices/{id}/data/latest", get(get_latest_data))
        .route("/api/v1/devices/export", get(export_devices))
        .route("/api/v1/devices/import", post(import_devices))
        .route("/api/v1/data/dashboard", get(dashboard_data))
        .with_state(state.clone());

    #[cfg(feature = "embed-frontend")]
    {
        app.merge(frontend_embed_router())
    }
    #[cfg(not(feature = "embed-frontend"))]
    {
        app.merge(frontend_fs_router())
    }
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
                            if socket.send(Message::Text(text.into())).await.is_err() { break; }
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
) -> ApiResult<Json<Vec<DiscoveredDeviceDto>>> {
    let scan = core::ScanConfig {
        serial_ports: req.serial_ports,
        timeout_seconds: req.timeout_seconds.unwrap_or(3),
    };

    let mut found = Vec::new();
    for proto in state.registry.protocols() {
        let devices = match proto.discover_devices(&scan).await {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("discovery via {} failed: {}", proto.protocol_name(), e);
                continue;
            }
        };
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
) -> ApiResult<Json<Vec<contracts::DeviceListItemDto>>> {
    // Read from persistent storage
    let configs = state
        .store
        .list_device_configs()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Snapshot of active tasks for isPolling
    let tasks = state.tasks.lock().await;
    let list = configs
        .into_iter()
        .map(|c| {
            let is_polling = tasks.contains_key(&c.id);
            let supports_settings = state
                .registry
                .get_protocol(&c.protocol)
                .map(|p| p.metadata().capabilities.supports_settings)
                .unwrap_or(false);
            contracts::DeviceListItemDto {
                id: c.id,
                name: c.name,
                device_type: c.device_type,
                protocol_name: c.protocol,
                enabled: c.enabled,
                poll_interval_seconds: c.poll_interval_seconds,
                connection_params: c.connection_params,
                is_polling,
                supports_settings,
            }
        })
        .collect();

    Ok(Json(list))
}

async fn add_device(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(req): Json<contracts::AddDeviceRequestDto>,
) -> ApiResult<Json<serde_json::Value>> {
    if !req.enabled {
        return Ok(Json(
            serde_json::json!({ "status": "disabled", "id": req.id }),
        ));
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
    state
        .store
        .upsert_device_config(&cfg)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Spawn polling task
    start_polling(state.clone(), cfg.clone())
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

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
) -> ApiResult<Json<Option<contracts::DeviceData>>> {
    let data = state
        .store
        .get_latest_device_data(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
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
        .ok_or_else(|| anyhow::anyhow!("unknown protocol '{}'", cfg.protocol))?;

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
) -> ApiResult<Json<serde_json::Value>> {
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
) -> ApiResult<Json<contracts::TestConnectionResponseDto>> {
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
            Ok(_) => contracts::TestConnectionResponseDto {
                ok: true,
                message: None,
            },
            Err(e) => contracts::TestConnectionResponseDto {
                ok: false,
                message: Some(e.to_string()),
            },
        },
        Err(e) => contracts::TestConnectionResponseDto {
            ok: false,
            message: Some(e.to_string()),
        },
    };
    Ok(Json(resp))
}

// ----- New endpoints per spec -----

async fn system_status(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> ApiResult<Json<contracts::SystemStatusDto>> {
    let uptime = chrono::Utc::now() - state.started_at;

    // Locks for counts and estimated data rate
    let devices = state.devices.lock().await;
    let active_devices = devices.len() as u32;
    let estimated_rate: f64 = devices
        .values()
        .filter(|c| c.enabled && c.poll_interval_seconds > 0)
        .map(|c| 1.0f64 / (c.poll_interval_seconds as f64))
        .sum();
    drop(devices);

    let active_connections = state.tasks.lock().await.len() as u32;
    let active_clients = state.tx.receiver_count() as u32;

    // System metrics via sysinfo
    let mut sys = sysinfo::System::new_all();
    sys.refresh_memory();
    sys.refresh_cpu_all();

    // Memory percent
    let total_mem = sys.total_memory() as f64; // in KiB
    let used_mem = sys.used_memory() as f64; // in KiB
    let mem_percent = if total_mem > 0.0 {
        (used_mem / total_mem) * 100.0
    } else {
        0.0
    };

    // CPU percent
    let cpu_percent = sys.global_cpu_usage() as f64; // 0..100

    // Disk usage (sum of all disks)
    let mut total_space: u128 = 0;
    let mut avail_space: u128 = 0;
    let mut disks = sysinfo::Disks::new_with_refreshed_list();
    for d in disks.iter_mut() {
        d.refresh();
        total_space += d.total_space() as u128;
        avail_space += d.available_space() as u128;
    }
    let used_space = total_space.saturating_sub(avail_space);
    let used_mb = (used_space as f64) / (1024.0 * 1024.0);
    let total_mb = (total_space as f64) / (1024.0 * 1024.0);
    let disk_percent = if total_mb > 0.0 {
        (used_mb / total_mb) * 100.0
    } else {
        0.0
    };

    let status = contracts::SystemStatusDto {
        uptime_seconds: uptime.num_seconds() as u64,
        version: solar_monitor_core::version().to_string(),
        active_devices,
        active_connections,
        active_clients,
        data_points_per_second: estimated_rate,
        memory_usage: contracts::ResourceUsageDto {
            current: mem_percent,
            peak: 0.0,
            average: 0.0,
            unit: "percent".into(),
        },
        cpu_usage: contracts::ResourceUsageDto {
            current: cpu_percent,
            peak: 0.0,
            average: 0.0,
            unit: "percent".into(),
        },
        storage_usage: contracts::StorageUsageDto {
            used_mb,
            total_mb,
            percent: disk_percent,
        },
    };
    Ok(Json(status))
}

async fn list_protocols(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> ApiResult<Json<Vec<contracts::ProtocolInfoDto>>> {
    let protos = state
        .registry
        .list_protocols()
        .into_iter()
        .map(|m| contracts::ProtocolInfoDto {
            protocol_name: m.protocol_name.to_string(),
            name: m.name.to_string(),
            version: m.version.to_string(),
            description: m.description.to_string(),
            supported_device_types: m.supported_device_types.to_vec(),
            capabilities: contracts::ProtocolCapabilitiesDto {
                supports_discovery: m.capabilities.supports_discovery,
                supports_settings: m.capabilities.supports_settings,
                supports_real_time: m.capabilities.supports_real_time,
                max_concurrent_connections: m.capabilities.max_concurrent_connections,
            },
        })
        .collect::<Vec<_>>();
    Ok(Json(protos))
}

async fn get_device(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Json<contracts::DeviceConfigDto>> {
    let cfg = state
        .store
        .get_device_config(&id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("device not found".to_string()))?;
    Ok(Json(to_dto(&cfg)))
}

async fn update_device(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<contracts::AddDeviceRequestDto>,
) -> ApiResult<Json<serde_json::Value>> {
    if id != req.id {
        return Err(ApiError::BadRequest("id mismatch".into()));
    }
    let cfg = core::DeviceConfig {
        id: req.id.clone(),
        name: req.name.clone(),
        device_type: req.device_type,
        protocol: req.protocol_name.clone(),
        connection_params: req.connection_params.clone(),
        enabled: req.enabled,
        poll_interval_seconds: req.poll_interval_seconds,
    };
    state
        .store
        .upsert_device_config(&cfg)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    // Restart task if enabled, stop if disabled
    if !cfg.enabled {
        if let Some(h) = state.tasks.lock().await.remove(&cfg.id) {
            h.abort();
        }
    } else {
        // replace in-memory config
        state
            .devices
            .lock()
            .await
            .insert(cfg.id.clone(), cfg.clone());
        start_polling(state.clone(), cfg)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    }
    Ok(Json(serde_json::json!({"status":"updated","id": id })))
}

/// Resolve a device's settings access via its protocol, or fail with a
/// client error if the device/protocol is unknown or has no settings.
async fn device_settings_access(
    state: &AppState,
    id: &str,
) -> Result<Box<dyn core::SettingsAccess>, ApiError> {
    let cfg = state
        .store
        .get_device_config(id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound("device not found".into()))?;
    let proto = state
        .registry
        .get_protocol(&cfg.protocol)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown protocol '{}'", cfg.protocol)))?;
    proto
        .settings(&cfg)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| {
            ApiError::BadRequest(format!(
                "settings not supported for protocol '{}'",
                cfg.protocol
            ))
        })
}

async fn get_device_settings(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> ApiResult<Json<Vec<contracts::DeviceSettingDto>>> {
    let access = device_settings_access(&state, &id).await?;
    let settings = access
        .read_settings()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(settings))
}

async fn write_device_setting(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path((id, key)): axum::extract::Path<(String, String)>,
    Json(req): Json<contracts::WriteSettingRequestDto>,
) -> ApiResult<Json<contracts::DeviceSettingDto>> {
    let access = device_settings_access(&state, &id).await?;
    let setting = access
        .write_setting(&key, &req.value)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(setting))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RangeQuery {
    start: Option<String>,
    end: Option<String>,
    limit: Option<u32>,
}

async fn get_device_data_range(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::Query(q): axum::extract::Query<RangeQuery>,
) -> ApiResult<Json<Vec<contracts::DeviceData>>> {
    let end = q
        .end
        .as_deref()
        .map(|s| chrono::DateTime::parse_from_rfc3339(s).map(|d| d.with_timezone(&chrono::Utc)))
        .transpose()
        .map_err(|_| ApiError::BadRequest("invalid end timestamp".into()))?
        .unwrap_or_else(chrono::Utc::now);
    let start = q
        .start
        .as_deref()
        .map(|s| chrono::DateTime::parse_from_rfc3339(s).map(|d| d.with_timezone(&chrono::Utc)))
        .transpose()
        .map_err(|_| ApiError::BadRequest("invalid start timestamp".into()))?
        .unwrap_or_else(|| end - chrono::Duration::hours(1));
    let data = state
        .store
        .get_device_data_range(&id, start, end, q.limit)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(data))
}

async fn dashboard_data(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> ApiResult<Json<Vec<contracts::DeviceData>>> {
    let configs = state
        .store
        .list_device_configs()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut out = Vec::new();
    for cfg in configs {
        if let Ok(Some(d)) = state.store.get_latest_device_data(&cfg.id).await {
            out.push(d);
        }
    }
    Ok(Json(out))
}

async fn export_devices(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> ApiResult<Json<Vec<contracts::DeviceConfigDto>>> {
    let configs = state
        .store
        .list_device_configs()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(configs.into_iter().map(|c| to_dto(&c)).collect()))
}

async fn import_devices(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(devs): Json<Vec<contracts::DeviceConfigDto>>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut imported = 0u32;
    for d in devs {
        let cfg = core::DeviceConfig {
            id: d.id.clone(),
            name: d.name.clone(),
            device_type: d.device_type,
            protocol: d.protocol_name.clone(),
            connection_params: d.connection_params.clone(),
            enabled: d.enabled,
            poll_interval_seconds: d.poll_interval_seconds,
        };
        state
            .store
            .upsert_device_config(&cfg)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        state
            .devices
            .lock()
            .await
            .insert(cfg.id.clone(), cfg.clone());
        if cfg.enabled {
            let _ = start_polling(state.clone(), cfg).await;
        }
        imported += 1;
    }
    Ok(Json(
        serde_json::json!({"status":"ok","imported": imported}),
    ))
}

fn to_dto(c: &core::DeviceConfig) -> contracts::DeviceConfigDto {
    contracts::DeviceConfigDto {
        id: c.id.clone(),
        name: c.name.clone(),
        device_type: c.device_type.clone(),
        protocol_name: c.protocol.clone(),
        enabled: c.enabled,
        poll_interval_seconds: c.poll_interval_seconds,
        connection_params: c.connection_params.clone(),
    }
}

#[cfg(feature = "embed-frontend")]
fn frontend_embed_router() -> Router {
    use axum::extract::Path;
    use rust_embed::RustEmbed;

    #[derive(RustEmbed)]
    #[folder = "../web/dist/"]
    struct WebAssets;

    fn serve(file_path: &str) -> Option<Response> {
        let content = WebAssets::get(file_path)?;
        let mime = mime_guess::from_path(file_path).first_or_octet_stream();
        Some(
            (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, mime.as_ref())],
                axum::body::Body::from(content.data),
            )
                .into_response(),
        )
    }

    async fn asset(Path(path): Path<String>) -> Response {
        let file_path = if path.is_empty() {
            "index.html"
        } else {
            path.as_str()
        };
        serve(file_path)
            .or_else(|| serve("index.html"))
            .unwrap_or_else(|| axum::http::StatusCode::NOT_FOUND.into_response())
    }

    Router::new()
        .route("/", get(|| async { asset(Path(String::new())).await }))
        .route("/{*path}", get(asset))
}

#[cfg(not(feature = "embed-frontend"))]
fn frontend_fs_router() -> Router {
    use tower_http::services::ServeDir;
    let service = ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../web/dist"));
    Router::new().fallback_service(service)
}

// Minimal OpenAPI document served as JSON when feature enabled
#[cfg(feature = "openapi")]
async fn openapi_json() -> Json<serde_json::Value> {
    let doc = serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Solar Monitor API",
            "version": solar_monitor_core::version(),
            "description": "OpenAPI description for Solar Monitor endpoints"
        },
        "servers": [{ "url": "/" }],
        "paths": {
            "/api/v1/health": {"get": {"summary": "Health", "responses": {"200": {"description": "OK"}}}},
            "/api/v1/status": {"get": {"summary": "System status", "responses": {"200": {"description": "OK"}}}},
            "/api/v1/ws": {"get": {"summary": "WebSocket", "responses": {"101": {"description": "Switching Protocols"}}}},
            "/api/v1/system/serial-ports": {"get": {"summary": "List serial ports", "responses": {"200": {"description": "OK"}}}},
            "/api/v1/protocols": {"get": {"summary": "List protocols", "responses": {"200": {"description": "OK"}}}},
            "/api/v1/protocols/discovery": {"post": {"summary": "Discover devices", "responses": {"200": {"description": "OK"}}}},
            "/api/v1/devices": {
                "get": {"summary": "List devices", "responses": {"200": {"description": "OK"}}},
                "post": {"summary": "Add device", "responses": {"200": {"description": "OK"}}}
            },
            "/api/v1/devices/{id}": {
                "parameters": [{"name": "id", "in": "path", "required": true, "schema": {"type": "string"}}],
                "get": {"summary": "Get device", "responses": {"200": {"description": "OK"}, "404": {"description": "Not Found"}}},
                "put": {"summary": "Update device", "responses": {"200": {"description": "OK"}}},
                "delete": {"summary": "Remove device", "responses": {"200": {"description": "OK"}}}
            },
            "/api/v1/devices/{id}/settings": {
                "get": {"summary": "Read device settings", "responses": {"200": {"description": "OK"}, "4XX": {"description": "Error"}}}
            },
            "/api/v1/devices/{id}/settings/{key}": {
                "put": {"summary": "Write device setting", "responses": {"200": {"description": "OK"}, "4XX": {"description": "Error"}}}
            },
            "/api/v1/devices/{id}/data": {
                "get": {"summary": "Device data range", "responses": {"200": {"description": "OK"}}}
            },
            "/api/v1/devices/{id}/data/latest": {
                "get": {"summary": "Latest device data", "responses": {"200": {"description": "OK"}}}
            },
            "/api/v1/devices/export": {"get": {"summary": "Export devices", "responses": {"200": {"description": "OK"}}}},
            "/api/v1/devices/import": {"post": {"summary": "Import devices", "responses": {"200": {"description": "OK"}}}},
            "/api/v1/data/dashboard": {"get": {"summary": "Dashboard summary", "responses": {"200": {"description": "OK"}}}}
        }
    });
    Json(doc)
}

#[cfg(feature = "openapi")]
pub fn router_with_openapi(state: Arc<AppState>) -> Router {
    router(state).route("/openapi.json", get(openapi_json))
}
