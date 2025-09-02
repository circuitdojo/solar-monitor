# API Design Specification (RS485-first, single port)

## Overview
RESTful API and WebSocket interface design for the universal solar monitoring solution, optimized for low-power edge devices with efficient data transfer and minimal overhead.

## Common Types

The API DTOs mirror the canonical types from SPECIFICATION.md. Field names use camelCase in JSON and map 1:1 to the Rust structs.

- DeviceTypeDto ↔ DeviceType
- HealthStatusDto ↔ HealthStatus
- DeviceMetricsDto: uses `device_temperature_celsius`, `battery_soc_percentage`; omit nulls when practical
- DeviceStatusDto: `is_connected`, `last_seen`, `health`, `error_message`

## API Architecture

### 1. RESTful API Design
```rust
use axum::{Router, extract::{Path, Query}, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn create_api_router() -> Router {
    Router::new()
        // System endpoints
        .route("/api/v1/health", get(health_check))
        .route("/api/v1/status", get(system_status))
        // WebSocket endpoint (same HTTP server)
        .route("/api/v1/ws", get(ws_upgrade))
        
        // Device management
        .route("/api/v1/devices", get(list_devices).post(add_device))
        .route("/api/v1/devices/:id", get(get_device).put(update_device).delete(remove_device))
        .route("/api/v1/devices/:id/status", get(device_status))
        .route("/api/v1/devices/:id/data", get(device_data))
        .route("/api/v1/devices/:id/commands", post(send_command))
        .route("/api/v1/devices/:id/test", post(test_device_connection))
        .route("/api/v1/devices/export", get(export_devices))
        .route("/api/v1/devices/import", post(import_devices))
        .route("/api/v1/devices/test-params", post(test_connection_params))
        
        // Historical data
        .route("/api/v1/data/historical", get(historical_data))
        .route("/api/v1/data/aggregated", get(aggregated_data))
        
        // Protocol management
        .route("/api/v1/protocols", get(list_protocols))
        .route("/api/v1/protocols/:name/info", get(protocol_info))
        .route("/api/v1/protocols/discovery", post(discover_devices))
        .route("/api/v1/system/serial-ports", get(list_serial_ports))
        
        // Configuration
        .route("/api/v1/config", get(get_config).put(update_config))
        
        // Authentication (future phase) – intentionally omitted in v1
}
```

### 1a. Device Manager Interface (engine facade)
```rust
use uuid::Uuid;

pub trait DeviceManagerApi: Send + Sync {
    async fn list_devices(&self, params: DeviceListParams) -> Result<Vec<DeviceConfigDto>>;
    async fn count_devices(&self) -> Result<u64>;
    async fn get_device(&self, id: Uuid) -> Result<Option<DeviceConfigDto>>;
    async fn add_device(&self, req: AddDeviceRequestDto) -> Result<DeviceConfigDto>;
    async fn update_device(&self, id: Uuid, req: AddDeviceRequestDto) -> Result<DeviceConfigDto>;
    async fn remove_device(&self, id: Uuid) -> Result<()>;

    async fn test_connection(&self, id: Uuid) -> Result<TestConnectionResponseDto>;
    async fn test_connection_params(&self, req: TestConnectionParamsDto) -> Result<TestConnectionResponseDto>;

    async fn export_devices(&self) -> Result<Vec<DeviceConfigDto>>;
    async fn import_devices(&self, configs: Vec<DeviceConfigDto>) -> Result<ImportDevicesResultDto>;
}

impl CoreEngine {
    pub fn device_manager(&self) -> &dyn DeviceManagerApi { self.device_manager.as_ref() }
}
```

### 2. Data Transfer Objects (DTOs)
```rust
// Shared types using Specta for frontend compatibility
#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)] in code
#[serde(rename_all = "camelCase")]
pub struct DeviceDto {
    pub id: String,
    pub name: String,
    pub device_type: DeviceTypeDto,      // Universal device type
    pub protocol_name: String,           // "eg4-pi30-rs485", etc.
    pub connection_params: HashMap<String, String>, // Protocol-specific params
    pub poll_interval_seconds: u32,
    pub enabled: bool,
    pub created_at: String, // ISO 8601
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DeviceTypeDto {
    SolarInverter,
    BatterySystem,
    ChargeController,
    EnergyMeter,
}

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatusDto {
    pub id: String,
    pub name: String,
    pub is_connected: bool,
    pub last_seen: Option<String>, // ISO 8601
    pub health: HealthStatusDto,
    pub error_message: Option<String>,
    pub current_metrics: Option<DeviceMetricsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatusDto { Healthy, Warning, Critical, Offline }

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMetricsDto {
    pub timestamp: String, // ISO 8601
    pub input_power_watts: Option<f64>,
    pub output_power_watts: Option<f64>,
    pub load_percentage: Option<f64>,
    pub battery_voltage: Option<f64>,
    pub battery_current: Option<f64>,
    pub battery_soc_percentage: Option<f64>,
    pub battery_temperature_celsius: Option<f64>,
    pub pv_voltage: Option<f64>,
    pub pv_current: Option<f64>,
    pub pv_power_watts: Option<f64>,
    pub grid_voltage: Option<f64>,
    pub grid_frequency: Option<f64>,
    pub grid_power_watts: Option<f64>,
    pub device_temperature_celsius: Option<f64>,
    pub efficiency_percentage: Option<f64>,
    pub fault_codes: Vec<String>,
    pub operating_mode: Option<String>,
    pub custom_metrics: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatusDto {
    pub uptime_seconds: u64,
    pub version: String,
    pub active_devices: u32,
    pub active_connections: u32,
    pub active_clients: u32,
    pub data_points_per_second: f64,
    pub memory_usage: ResourceUsageDto,
    pub cpu_usage: ResourceUsageDto,
    pub storage_usage: StorageUsageDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResourceUsageDto {
    pub current: f64,
    pub peak: f64,
    pub average: f64,
    pub unit: String, // "percent", "MB", etc.
}
```

### 2a. DTO ↔ Canonical Type Mapping (stubs)
```rust
// Enums
impl From<DeviceType> for DeviceTypeDto {
    fn from(v: DeviceType) -> Self { match v {
        DeviceType::SolarInverter => Self::SolarInverter,
        DeviceType::BatterySystem => Self::BatterySystem,
        DeviceType::ChargeController => Self::ChargeController,
        DeviceType::EnergyMeter => Self::EnergyMeter,
    }}
}
impl From<DeviceTypeDto> for DeviceType { fn from(v: DeviceTypeDto) -> Self { match v {
    DeviceTypeDto::SolarInverter => Self::SolarInverter,
    DeviceTypeDto::BatterySystem => Self::BatterySystem,
    DeviceTypeDto::ChargeController => Self::ChargeController,
    DeviceTypeDto::EnergyMeter => Self::EnergyMeter,
}}}

impl From<HealthStatus> for HealthStatusDto { fn from(v: HealthStatus) -> Self { match v {
    HealthStatus::Healthy => Self::Healthy,
    HealthStatus::Warning => Self::Warning,
    HealthStatus::Critical => Self::Critical,
    HealthStatus::Offline => Self::Offline,
}}}
impl From<HealthStatusDto> for HealthStatus { fn from(v: HealthStatusDto) -> Self { match v {
    HealthStatusDto::Healthy => Self::Healthy,
    HealthStatusDto::Warning => Self::Warning,
    HealthStatusDto::Critical => Self::Critical,
    HealthStatusDto::Offline => Self::Offline,
}}}

// Metrics
impl From<DeviceMetrics> for DeviceMetricsDto {
    fn from(m: DeviceMetrics) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            input_power_watts: m.input_power_watts,
            output_power_watts: m.output_power_watts,
            load_percentage: m.load_percentage,
            battery_voltage: m.battery_voltage,
            battery_current: m.battery_current,
            battery_soc_percentage: m.battery_soc_percentage,
            battery_temperature_celsius: m.battery_temperature_celsius,
            pv_voltage: m.pv_voltage,
            pv_current: m.pv_current,
            pv_power_watts: m.pv_power_watts,
            grid_voltage: m.grid_voltage,
            grid_frequency: m.grid_frequency,
            grid_power_watts: m.grid_power_watts,
            device_temperature_celsius: m.device_temperature_celsius,
            efficiency_percentage: m.efficiency_percentage,
            fault_codes: m.fault_codes,
            operating_mode: m.operating_mode,
            custom_metrics: m.custom_metrics,
        }
    }
}

// Status
impl From<(String, DeviceStatus, Option<DeviceMetrics>)> for DeviceStatusDto {
    fn from((name, s, cm): (String, DeviceStatus, Option<DeviceMetrics>)) -> Self {
        Self {
            id: String::new(),
            name,
            is_connected: s.is_connected,
            last_seen: Some(s.last_seen.to_rfc3339()),
            health: s.health.into(),
            error_message: s.error_message,
            current_metrics: cm.map(Into::into),
        }
    }
}

// DeviceConfig mapping
impl From<DeviceConfig> for DeviceConfigDto {
    fn from(c: DeviceConfig) -> Self {
        Self {
            id: c.id,
            name: c.name,
            device_type: c.device_type.into(),
            protocol_name: c.protocol,
            enabled: c.enabled,
            poll_interval_seconds: c.poll_interval_seconds as u32,
            connection_params: c.connection_params,
        }
    }
}

impl From<AddDeviceRequestDto> for DeviceConfig {
    fn from(req: AddDeviceRequestDto) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: req.name,
            device_type: req.device_type.into(),
            protocol: req.protocol_name,
            connection_params: req.connection_params,
            poll_interval_seconds: req.poll_interval_seconds as u64,
            enabled: true,
        }
    }
}
```

### 3. API Endpoints Implementation
```rust
// System endpoints
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        timestamp: Utc::now().to_rfc3339(),
        uptime_seconds: get_uptime_seconds(),
    })
}

pub async fn system_status(
    State(engine): State<Arc<CoreEngine>>
) -> Result<Json<SystemStatusDto>, ApiError> {
    let status = engine.get_system_status().await?;
    Ok(Json(SystemStatusDto::from(status)))
}

// Device management endpoints
pub async fn list_devices(
    State(engine): State<Arc<CoreEngine>>,
    Query(params): Query<DeviceListParams>,
) -> Result<Json<PaginatedResponse<DeviceDto>>, ApiError> {
    let devices = engine.device_manager()
        .list_devices(params.into())
        .await?;
    
    let device_dtos: Vec<DeviceDto> = devices
        .into_iter()
        .map(DeviceDto::from)
        .collect();
    
    Ok(Json(PaginatedResponse {
        data: device_dtos,
        total: engine.device_manager().count_devices().await?,
        page: params.page.unwrap_or(1),
        per_page: params.per_page.unwrap_or(50),
    }))
}

pub async fn get_device(
    State(engine): State<Arc<CoreEngine>>,
    Path(device_id): Path<String>,
) -> Result<Json<DeviceDto>, ApiError> {
    let uuid = Uuid::parse_str(&device_id)
        .map_err(|_| ApiError::InvalidDeviceId)?;
    
    let device = engine.device_manager()
        .get_device(uuid)
        .await?
        .ok_or(ApiError::DeviceNotFound)?;
    
    Ok(Json(DeviceDto::from(device)))
}

pub async fn device_data(
    State(engine): State<Arc<CoreEngine>>,
    Path(device_id): Path<String>,
    Query(params): Query<DataQueryParams>,
) -> Result<Json<Vec<DeviceMetricsDto>>, ApiError> {
    let uuid = Uuid::parse_str(&device_id)
        .map_err(|_| ApiError::InvalidDeviceId)?;
    
    let query = DataQuery {
        device_ids: vec![uuid],
        start_time: params.start_time
            .map(|s| DateTime::parse_from_rfc3339(&s))
            .transpose()?
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() - Duration::hours(24)),
        end_time: params.end_time
            .map(|s| DateTime::parse_from_rfc3339(&s))
            .transpose()?
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now()),
        limit: params.limit.unwrap_or(1000).min(10000), // Cap at 10k for edge devices
    };
    
    let data = engine.data_router()
        .query_data(&query)
        .await?;
    
    let metrics_dtos: Vec<DeviceMetricsDto> = data
        .into_iter()
        .map(|d| DeviceMetricsDto::from(d.metrics))
        .collect();
    
    Ok(Json(metrics_dtos))
}

// Command execution endpoint
pub async fn send_command(
    State(engine): State<Arc<CoreEngine>>,
    Path(device_id): Path<String>,
    Json(command_request): Json<CommandRequestDto>,
) -> Result<Json<CommandResponseDto>, ApiError> {
    let uuid = Uuid::parse_str(&device_id)
        .map_err(|_| ApiError::InvalidDeviceId)?;
    
    let command = DeviceCommand {
        id: Uuid::new_v4(),
        device_id: uuid,
        command: command_request.command,
        parameters: command_request.parameters,
        timeout: Duration::from_secs(command_request.timeout_seconds.unwrap_or(30)),
    };
    
    let response = engine.command_executor()
        .execute_command(command)
        .await?;
    
    Ok(Json(CommandResponseDto::from(response)))
}

// Test connection endpoint (validates serial params without saving)
pub async fn test_device_connection(
    State(engine): State<Arc<CoreEngine>>,
    Path(device_id): Path<String>,
) -> Result<Json<TestConnectionResponseDto>, ApiError> {
    let uuid = Uuid::parse_str(&device_id).map_err(|_| ApiError::InvalidDeviceId)?;
    let result = engine.device_manager().test_connection(uuid).await?;
    Ok(Json(TestConnectionResponseDto { ok: result.ok, message: result.message }))
}

// List available serial ports on host (RS485 discovery helper)
pub async fn list_serial_ports() -> Result<Json<Vec<String>>, ApiError> {
    let ports = crate::system::serial::list_ports()?; // Platform helper wraps serialport::available_ports()
    Ok(Json(ports))
}

// Export device configurations (for backup/migration)
pub async fn export_devices(
    State(engine): State<Arc<CoreEngine>>,
) -> Result<Json<Vec<DeviceConfigDto>>, ApiError> {
    let devices = engine.device_manager().list_device_configs().await?;
    Ok(Json(devices.into_iter().map(DeviceConfigDto::from).collect()))
}

// Import device configurations (idempotent upsert by id or name)
pub async fn import_devices(
    State(engine): State<Arc<CoreEngine>>,
    Json(req): Json<Vec<DeviceConfigDto>>,
) -> Result<Json<ImportDevicesResultDto>, ApiError> {
    let result = engine.device_manager().import_devices(req).await?;
    Ok(Json(result))
}

// Test connection with provided parameters (before persistence)
pub async fn test_connection_params(
    State(engine): State<Arc<CoreEngine>>,
    Json(req): Json<TestConnectionParamsDto>,
) -> Result<Json<TestConnectionResponseDto>, ApiError> {
    let result = engine.device_manager().test_connection_params(req.into()).await?;
    Ok(Json(TestConnectionResponseDto { ok: result.ok, message: result.message }))
}
```

### 4. Query Parameters and Filtering
```rust
#[derive(Debug, Deserialize)]
pub struct DeviceListParams {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub protocol: Option<String>,        // Filter by protocol
    pub device_type: Option<String>,     // Filter by device type
    pub enabled: Option<bool>,
    pub online: Option<bool>,
    pub search: Option<String>,
}

// DTOs for runtime device management
#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddDeviceRequestDto {
    pub name: String,
    pub device_type: DeviceTypeDto,
    pub protocol_name: String, // e.g. "eg4-pi30-rs485"
    pub poll_interval_seconds: u32,
    pub connection_params: HashMap<String, String>, // serial_port, baud_rate, etc.
}

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResponseDto { pub ok: bool, pub message: Option<String> }

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionParamsDto {
    pub device_type: DeviceTypeDto,
    pub protocol_name: String,
    pub connection_params: HashMap<String, String>,
}

// RS485 connection parameters
// Required: serial_port (e.g., /dev/ttyUSB0), baud_rate (default 2400)
// Optional (defaults): data_bits=8, parity=none, stop_bits=1, timeout_seconds=3

// Export/Import DTOs
#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceConfigDto {
    pub id: String, // UUID
    pub name: String,
    pub device_type: DeviceTypeDto,
    pub protocol_name: String,
    pub enabled: bool,
    pub poll_interval_seconds: u32,
    pub connection_params: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportDevicesResultDto {
    pub added: u32,
    pub updated: u32,
    pub skipped: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct DataQueryParams {
    pub start_time: Option<String>, // ISO 8601
    pub end_time: Option<String>,   // ISO 8601
    pub limit: Option<u32>,
    pub interval: Option<String>,   // "raw", "minute", "hour", "day"
}

#[derive(Debug, Deserialize)]
pub struct HistoricalDataParams {
    pub device_ids: Option<Vec<String>>,
    pub metrics: Option<Vec<String>>, // Filter specific metrics
    pub start_time: String,
    pub end_time: String,
    pub interval: Option<String>,
    pub aggregation: Option<String>, // "avg", "min", "max", "sum"
}
```

### 5. WebSocket API for Real-time Data
```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use tokio::sync::broadcast;

pub struct WebSocketManager {
    clients: Arc<RwLock<HashMap<String, ClientConnection>>>,
    data_broadcast: broadcast::Sender<WebSocketMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketMessage {
    pub message_type: WebSocketMessageType,
    pub timestamp: String, // ISO 8601
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum WebSocketMessageType {
    DeviceData,
    DeviceStatus,
    SystemStatus,
    Alert,
    CommandResponse,
    Error,
}

// WebSocket message handlers
#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionMessage {
    pub action: SubscriptionAction,
    pub device_ids: Option<Vec<String>>,
    pub message_types: Option<Vec<WebSocketMessageType>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)] // #[derive(specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionAction {
    Subscribe,
    Unsubscribe,
    ListSubscriptions,
}

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(ws_manager): State<Arc<WebSocketManager>>,
) -> axum::response::Response {
    ws.on_upgrade(|socket| handle_websocket(socket, ws_manager))
}

async fn handle_websocket(socket: WebSocket, ws_manager: Arc<WebSocketManager>) {
    let client_id = Uuid::new_v4().to_string();
    let (sender, mut receiver) = socket.split();
    let mut data_receiver = ws_manager.data_broadcast.subscribe();
    
    // Handle outgoing messages (data to client)
    let outgoing_task = tokio::spawn(async move {
        let mut sender = sender;
        
        while let Ok(message) = data_receiver.recv().await {
            let json_message = serde_json::to_string(&message).unwrap();
            
            if sender.send(Message::Text(json_message)).await.is_err() {
                break; // Client disconnected
            }
        }
    });
    
    // Handle incoming messages (subscriptions from client)
    let incoming_task = tokio::spawn(async move {
        while let Some(message) = receiver.next().await {
            if let Ok(Message::Text(text)) = message {
                if let Ok(subscription) = serde_json::from_str::<SubscriptionMessage>(&text) {
                    ws_manager.handle_subscription(client_id.clone(), subscription).await;
                }
            } else if let Ok(Message::Close(_)) = message {
                break;
            }
        }
        
        ws_manager.remove_client(&client_id).await;
    });
    
    // Wait for either task to complete
    tokio::select! {
        _ = outgoing_task => {},
        _ = incoming_task => {},
    }
}

impl WebSocketManager {
    pub async fn broadcast_device_data(&self, device_id: Uuid, metrics: DeviceMetricsDto) {
        let message = WebSocketMessage {
            message_type: WebSocketMessageType::DeviceData,
            timestamp: Utc::now().to_rfc3339(),
            data: serde_json::json!({
                "deviceId": device_id.to_string(),
                "metrics": metrics,
            }),
        };
        
        // Broadcast to all subscribed clients
        let _ = self.data_broadcast.send(message);
    }
    
    pub async fn broadcast_device_status(&self, status: DeviceStatusDto) {
        let message = WebSocketMessage {
            message_type: WebSocketMessageType::DeviceStatus,
            timestamp: Utc::now().to_rfc3339(),
            data: serde_json::to_value(status).unwrap(),
        };
        
        let _ = self.data_broadcast.send(message);
    }
}
```

### 5a. WebSocket Message Types and Protocol
- Endpoint: `GET /api/v1/ws` (upgrades from HTTP)
- Subscriptions: optional filter message to limit devices/metrics
- Heartbeat: server pings every 30s (configurable) and supports client pongs

Message envelopes:
```json
// Server -> Client: device data update
{
  "messageType": "device_data",
  "timestamp": "2025-01-01T00:00:00Z",
  "data": {
    "deviceId": "...",
    "deviceType": "solarInverter",
    "metrics": { "pvPowerWatts": 3200.0 },
    "status": { "isConnected": true, "lastSeen": "...", "health": "healthy" }
  }
}

// Client -> Server: subscribe
{
  "messageType": "subscribe",
  "data": { "deviceIds": ["..."], "metrics": ["pvPowerWatts","batteryVoltage"] }
}

// Server -> Client: heartbeat
{ "messageType": "heartbeat", "timestamp": "2025-01-01T00:00:30Z", "data": {} }
```

Limits:
- Max connections: 100 (configurable)
- Max message size: 100KB; batch updates permitted per tick
- Backpressure: drop oldest pending updates per client when buffers fill

### 6. Error Model (HTTP + JSON)
- 400 Bad Request: validation/parsing errors, invalid UUIDs
- 401 Unauthorized: when auth enabled and token invalid/missing
- 404 Not Found: device/config not found
- 409 Conflict: duplicate names/IDs on import
- 422 Unprocessable Entity: connection params invalid (test endpoints)
- 429 Too Many Requests: rate-limited
- 500 Internal Server Error: unexpected failures

JSON error body:
```json
{
  "error": "Invalid request",
  "details": "deviceId is malformed",
  "timestamp": "2025-01-01T00:00:00Z"
}
```

### 6. Error Handling
```rust
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Device not found")]
    DeviceNotFound,
    
    #[error("Invalid device ID format")]
    InvalidDeviceId,
    
    #[error("Protocol not found: {0}")]
    ProtocolNotFound(String),
    
    #[error("Device type not supported: {0}")]
    UnsupportedDeviceType(String),
    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Authentication failed")]
    Unauthorized,
    
    #[error("Permission denied")]
    Forbidden,
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Invalid request: {0}")]
    BadRequest(String),
    
    #[error("Internal server error")]
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_message) = match self {
            ApiError::DeviceNotFound => (StatusCode::NOT_FOUND, "Device not found"),
            ApiError::InvalidDeviceId => (StatusCode::BAD_REQUEST, "Invalid device ID"),
            ApiError::ProtocolNotFound(_) => (StatusCode::NOT_FOUND, "Protocol not found"),
            ApiError::UnsupportedDeviceType(_) => (StatusCode::BAD_REQUEST, "Device type not supported"),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "Authentication required"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "Permission denied"),
            ApiError::RateLimitExceeded => (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded"),
            ApiError::BadRequest(_) => (StatusCode::BAD_REQUEST, "Invalid request"),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
        };
        
        let body = Json(ErrorResponse {
            error: error_message.to_string(),
            details: self.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        });
        
        (status, body).into_response()
    }
}

#[derive(Serialize)] // #[derive(specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponse {
    pub error: String,
    pub details: String,
    pub timestamp: String,
}
```

### 7. API Configuration for Edge Devices
```toml
[api]
# HTTP server configuration
bind_address = "0.0.0.0"
port = 8080
request_timeout_seconds = 30
max_request_size_mb = 10

# WebSocket configuration (same HTTP server)
max_websocket_connections = 100
websocket_ping_interval_seconds = 30
max_message_size_kb = 100

# Response optimization for low bandwidth
compress_responses = true
compression_level = 6 # Balance of speed vs size for ARM
enable_etags = true
cache_static_assets = true

# Rate limiting (per IP)
rate_limit_requests_per_minute = 100
rate_limit_burst = 20

# Pagination defaults
default_page_size = 50
max_page_size = 1000

# Data query limits (to prevent resource exhaustion)
max_data_points_per_query = 10000
max_time_range_days = 365
default_query_limit = 1000

[api.cors]
enabled = true
allowed_origins = ["http://localhost:3000", "https://localhost:3000"]
allowed_methods = ["GET", "POST", "PUT", "DELETE", "OPTIONS"]
allowed_headers = ["Content-Type", "Authorization"]
max_age_seconds = 3600
```

### 8. OpenAPI Documentation
```rust
use utoipa::{OpenApi, ToSchema};

#[derive(OpenApi)]
#[openapi(
    paths(
        health_check,
        system_status,
        list_devices,
        get_device,
        device_data,
        send_command,
    ),
    components(
        schemas(
            DeviceDto,
            DeviceStatusDto,
            DeviceMetricsDto,
            SystemStatusDto,
            CommandRequestDto,
            CommandResponseDto,
            ErrorResponse,
        )
    ),
    tags(
        (name = "system", description = "System health and status endpoints"),
        (name = "devices", description = "Device management endpoints"),
        (name = "data", description = "Historical data query endpoints"),
        (name = "commands", description = "Device command execution"),
    ),
    info(
        title = "Universal Solar Monitor API",
        version = "1.0.0",
        description = "Device-agnostic solar monitoring API supporting inverters, batteries, charge controllers, and energy meters",
        contact(
            name = "Solar Monitor Team",
            email = "support@solarmonitor.local"
        )
    ),
)]
pub struct ApiDoc;

// Serve OpenAPI documentation
pub fn openapi_routes() -> Router {
    Router::new()
        .route("/api/docs/openapi.json", get(serve_openapi_spec))
        .route("/api/docs", get(serve_swagger_ui))
}

async fn serve_openapi_spec() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
```

## Performance Optimizations for Edge Devices

### Efficient Data Serialization
- **Compact JSON**: Remove null fields, use short field names
- **Response compression**: Gzip compression for larger responses
- **Pagination**: Limit response sizes to prevent memory exhaustion
- **Caching**: ETags and conditional requests for static data

### WebSocket Optimizations
- **Message batching**: Group multiple device updates into single messages
- **Selective subscriptions**: Only send data clients actually need
- **Connection limits**: Prevent resource exhaustion on low-power devices
- **Automatic reconnection**: Handle network interruptions gracefully

### Memory Management
- **Streaming responses**: For large data queries
- **Connection pooling**: Reuse database connections
- **Buffer limits**: Prevent unbounded memory growth
- **Graceful degradation**: Reduce functionality under resource pressure

## Device-Agnostic API Examples

### Multi-Device Dashboard Data
```json
GET /api/v1/data/dashboard
{
  "devices": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "deviceType": "solarInverter",
      "protocol": "eg4-pi30-rs485",
      "metrics": {
        "inputPowerWatts": 3200.0,
        "outputPowerWatts": 2950.0,
        "batteryVoltage": 52.4,
        "batterySocPercentage": 85.0,
        "pvVoltage": 98.2,
        "pvPowerWatts": 3200.0,
        "gridVoltage": 120.1
      }
    }
  ],
  "summary": {
    "totalSolarPower": 3200.0,
    "systemEfficiency": 92.2
  }
}
```

### Protocol Discovery (RS485)
```json
POST /api/v1/protocols/discovery
{
  "serialPorts": ["/dev/ttyUSB0", "/dev/ttyUSB1"],
  "protocols": ["eg4-pi30-rs485"],
  "timeoutSeconds": 10
}

Response:
{
  "discoveredDevices": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "name": "EG4 6000XP on /dev/ttyUSB0",
      "deviceType": "solarInverter",
      "protocolName": "eg4-pi30-rs485",
      "connectionParams": {
        "serial_port": "/dev/ttyUSB0",
        "baud_rate": "2400"
      }
    }
  ]
}
```

### List Serial Ports (RS485 helper)
```json
GET /api/v1/system/serial-ports
[
  "/dev/ttyUSB0",
  "/dev/ttyUSB1",
  "/dev/ttyAMA0"
]
```

### Add Device (via Web UI)
```json
POST /api/v1/devices
{
  "name": "Main Inverter",
  "deviceType": "solarInverter",
  "protocolName": "eg4-pi30-rs485",
  "pollIntervalSeconds": 30,
  "connectionParams": {
    "serial_port": "/dev/ttyUSB0",
    "baud_rate": "2400"
  }
}

Response:
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "Main Inverter",
  "deviceType": "solarInverter",
  "protocolName": "eg4-pi30-rs485",
  "enabled": true,
  "pollIntervalSeconds": 30,
  "connectionParams": { ... }
}
```

### Test Device Connection
```json
POST /api/v1/devices/{id}/test
{}

Response:
{
  "ok": true,
  "message": null
}
```

### Export Device Configs
```json
GET /api/v1/devices/export
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "Main Inverter",
    "deviceType": "solarInverter",
    "protocolName": "eg4-pi30-rs485",
    "enabled": true,
    "pollIntervalSeconds": 30,
    "connectionParams": {
      "serial_port": "/dev/ttyUSB0",
      "baud_rate": "2400"
    }
  }
]
```

### Import Device Configs
```json
POST /api/v1/devices/import
[
  {
    "id": "c56a4180-65aa-42ec-a945-5fd21dec0538",
    "name": "Backup Inverter",
    "deviceType": "solarInverter",
    "protocolName": "eg4-pi30-rs485",
    "enabled": true,
    "pollIntervalSeconds": 30,
    "connectionParams": {
      "serial_port": "/dev/ttyUSB1",
      "baud_rate": "2400"
    }
  }
]

Response:
{
  "added": 1,
  "updated": 0,
  "skipped": 0,
  "errors": []
}
```

### Device-Type Filtering
```json
GET /api/v1/devices?deviceType=batterySystem&protocol=modbus-tcp
{
  "devices": [
    {
      "id": "battery-bank-1",
      "name": "Main LiFePO4 Battery",
      "deviceType": "batterySystem",
      "protocolName": "modbus-tcp",
      "connectionParams": {
        "host": "192.168.1.101",
        "port": "502",
        "unitId": "1"
      },
      "enabled": true,
      "pollIntervalSeconds": 60
    }
  ]
}
```

This device-agnostic API design provides a universal, efficient interface optimized for edge device deployment while supporting all solar equipment types through consistent abstractions and protocol-specific extensions.
