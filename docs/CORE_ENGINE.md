# Core Engine Architecture - RS485-First Edge Specification

## Overview
The Core Engine manages multiple types of solar/battery devices through protocol-specific modules, providing a unified interface while maintaining edge device efficiency. Built for extensibility without sacrificing simplicity.

## Architecture Components

### 1. Core Engine Structure (Device-Agnostic)
```rust
use std::collections::HashMap;
use tokio::sync::RwLock;
use async_trait::async_trait;

pub struct CoreEngine {
    /// Protocol registry (compile-time registered)
    protocols: HashMap<&'static str, Arc<dyn DeviceProtocol>>,
    
    /// Active device connections
    devices: Arc<RwLock<HashMap<String, Box<dyn DeviceConnection>>>>,
    
    /// Device configurations
    device_configs: HashMap<String, DeviceConfig>,
    
    /// Local SQLite database
    database: Arc<SqliteDatabase>,
    
    /// System configuration
    config: SystemConfig,
    
    /// Background polling tasks
    _polling_handles: Vec<tokio::task::JoinHandle<()>>,
}

impl CoreEngine {
    pub async fn new(config: SystemConfig) -> Result<Self> {
        let database = Arc::new(SqliteDatabase::new(&config.database.path).await?);
        let devices = Arc::new(RwLock::new(HashMap::new()));
        
        // Register protocol modules (compile-time)
        let mut protocols = HashMap::new();
        protocols.insert("eg4-pi30-rs485", Arc::new(EG4Protocol::new()) as Arc<dyn DeviceProtocol>);
        // Future: protocols.insert("modbus-tcp", Arc::new(ModbusProtocol::new()));
        // Future: protocols.insert("can-bus", Arc::new(CANProtocol::new()));
        
        // Initialize configured devices
        let mut device_configs = HashMap::new();
        let mut polling_handles = Vec::new();
        
        for device_config in &config.devices {
            let protocol = protocols.get(device_config.protocol.as_str())
                .ok_or_else(|| anyhow::anyhow!("Unsupported protocol: {}", device_config.protocol))?;
            
            let connection = protocol.connect(device_config).await?;
            devices.write().await.insert(device_config.id.clone(), connection);
            device_configs.insert(device_config.id.clone(), device_config.clone());
            
            // Start per-device polling task honoring poll_interval_seconds
            let handle = Self::start_device_polling(device_config.clone(), devices.clone(), database.clone());
            polling_handles.push(handle);
        }

        Ok(Self {
            protocols,
            devices,
            device_configs,
            database,
            config,
            _polling_handles: polling_handles,
        })
    }
    
    fn start_device_polling(
        device_config: DeviceConfig,
        devices: Arc<RwLock<HashMap<String, Box<dyn DeviceConnection>>>>,
        database: Arc<SqliteDatabase>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(
                device_config.poll_interval_seconds,
            ));
            loop {
                interval.tick().await;
                if let Some(conn) = devices.read().await.get(&device_config.id).map(|c| c.as_ref() as *const _ as usize) {
                    // SAFETY: We only use this to get a mutable reference within the lock scope; implementors should avoid long holds
                }
                let maybe_data = {
                    let mut guard = devices.write().await;
                    if let Some(conn) = guard.get_mut(&device_config.id) {
                        conn.read_data().await.ok()
                    } else { None }
                };
                if let Some(data) = maybe_data {
                    if let Err(e) = database.store_device_data(&data).await {
                        tracing::error!("Failed to store data for device {}: {}", device_config.id, e);
                    }
                }
            }
        })
    }
}
```

### 2. Device Protocol Abstraction
Generic interfaces for all device types with protocol-specific implementations.

```rust
// Universal device protocol interface
#[async_trait]
pub trait DeviceProtocol: Send + Sync {
    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>>;
    async fn discover_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>>;
    fn supported_device_types(&self) -> Vec<DeviceType>;
    fn protocol_name(&self) -> &'static str;
}

// Universal device connection interface  
#[async_trait]
pub trait DeviceConnection: Send + Sync {
    async fn read_data(&mut self) -> Result<DeviceData>;
    async fn send_command(&mut self, command: &str) -> Result<CommandResponse>;
    fn device_info(&self) -> &DeviceInfo;
    fn is_connected(&self) -> bool;
    async fn health_check(&mut self) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub device_type: DeviceType,
    pub connection_params: HashMap<String, String>, // host, port, unit_id, etc.
    pub poll_interval_seconds: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub protocol: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub firmware_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceData {
    pub device_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub device_type: DeviceType,
    pub metrics: DeviceMetrics,
    pub status: DeviceStatus,
    pub raw_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceType {
    SolarInverter,
    BatterySystem,
    ChargeController,
    EnergyMeter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub is_connected: bool,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub health: HealthStatus,
    pub error_message: Option<String>,
}

// EG4/PI30 Protocol Implementation (RS485)
pub struct EG4Protocol;

#[async_trait]
impl DeviceProtocol for EG4Protocol {
    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>> {
        let serial_port = config
            .connection_params
            .get("serial_port")
            .ok_or_else(|| anyhow::anyhow!("Missing serial_port parameter"))?;
        let baud_rate: u32 = config
            .connection_params
            .get("baud_rate")
            .unwrap_or(&"2400".to_string())
            .parse()
            .unwrap_or(2400);
        let connection = EG4SerialConnection::new(config.clone(), serial_port, baud_rate).await?;
        Ok(Box::new(connection))
    }
    
    async fn discover_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        let mut devices = Vec::new();
        for port in &scan_config.serial_ports {
            if let Ok(device) = self.test_eg4_serial_device(port).await {
                devices.push(device);
            }
        }
        Ok(devices)
    }
    
    fn supported_device_types(&self) -> Vec<DeviceType> {
        vec![DeviceType::SolarInverter]
    }
    
    fn protocol_name(&self) -> &'static str { "eg4-pi30-rs485" }
}

pub struct EG4SerialConnection {
    config: DeviceConfig,
    serial: Option<tokio_serial::SerialStream>,
    device_info: DeviceInfo,
    last_successful_read: Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
impl DeviceConnection for EG4SerialConnection {
    async fn read_data(&mut self) -> Result<DeviceData> {
        let response = self.send_pi30_command("QPIGS").await?;
        let metrics = self.parse_pi30_response(&response)?;
        
        self.last_successful_read = Some(chrono::Utc::now());
        
        Ok(DeviceData {
            device_id: self.device_info.id.clone(),
            timestamp: chrono::Utc::now(),
            device_type: DeviceType::SolarInverter,
            metrics,
            status: DeviceStatus { is_connected: true, last_seen: chrono::Utc::now(), health: HealthStatus::Healthy, error_message: None },
            raw_data: Some(response),
        })
    }
    
    fn parse_pi30_response(&self, response: &str) -> Result<DeviceMetrics> {
        if !response.starts_with('(') || !response.ends_with('\r') {
            return Err(anyhow::anyhow!("Invalid PI30 response format"));
        }
        
        let data = &response[1..response.len()-1];
        let fields: Vec<&str> = data.split(' ').collect();
        
        if fields.len() < 16 {
            return Err(anyhow::anyhow!("Insufficient PI30 response fields"));
        }
        
        Ok(DeviceMetrics {
            // Power metrics
            input_power_watts: fields.get(13).zip(fields.get(12))
                .and_then(|(v, i)| Some(v.parse::<f64>().ok()? * i.parse::<f64>().ok()?)),
            output_power_watts: fields.get(5).and_then(|s| s.parse().ok()),
            load_percentage: fields.get(6).and_then(|s| s.parse().ok()),
            
            // Battery metrics
            battery_voltage: fields.get(8).and_then(|s| s.parse().ok()),
            battery_soc_percentage: fields.get(10).and_then(|s| s.parse().ok()),
            battery_current: fields.get(9).and_then(|s| s.parse().ok()),
            
            // Solar metrics  
            pv_voltage: fields.get(13).and_then(|s| s.parse().ok()),
            pv_current: fields.get(12).and_then(|s| s.parse().ok()),
            pv_power_watts: fields.get(13).zip(fields.get(12))
                .and_then(|(v, i)| Some(v.parse::<f64>().ok()? * i.parse::<f64>().ok()?)),
            
            // Grid metrics
            grid_voltage: fields.get(0).and_then(|s| s.parse().ok()),
            grid_frequency: fields.get(1).and_then(|s| s.parse().ok()),
            
            // System metrics
            device_temperature_celsius: fields.get(11).and_then(|s| s.parse().ok()),
            efficiency_percentage: None,
            fault_codes: Vec::new(),
            operating_mode: Some("Normal".to_string()),
            custom_metrics: Default::default(),
        })
    }
}

    async fn send_command(&mut self, command: &str) -> Result<CommandResponse> {
        let response = self.send_pi30_command(command).await?;
        
        Ok(CommandResponse {
            success: true,
            response: response,
            error: None,
        })
    }
    
    fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }
    
    fn is_connected(&self) -> bool { self.serial.is_some() }
    
    async fn health_check(&mut self) -> Result<()> { self.send_pi30_command("QID").await.map(|_| () ) }
}
```

### 3. Device-Agnostic Data Storage
SQLite database with unified schema supporting all device types.

```rust
pub struct SqliteDatabase {
    pool: sqlx::SqlitePool,
}

// Use canonical DeviceMetrics from specification (see SPECIFICATION.md)

impl SqliteDatabase {
    pub async fn new(path: &str) -> Result<Self> {
        let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}", path)).await?;
        
        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;
        
        Ok(Self { pool })
    }
    
    pub async fn store_device_data(&self, data: &DeviceData) -> Result<()> {
        let metrics_json = serde_json::to_string(&data.metrics)?;
        let status_json = serde_json::to_string(&data.status)?;
        sqlx::query!(
            r#"
            INSERT INTO device_data 
            (device_id, device_type, timestamp, metrics, status, raw_data)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            data.device_id,
            // store as lowercase device type string for efficient indexing
            format!("{:?}", data.device_type).to_lowercase(),
            data.timestamp,
            metrics_json,
            status_json,
            data.raw_data
        ).execute(&self.pool).await?;
        
        Ok(())
    }
    
    pub async fn get_latest_device_data(&self, device_id: &str) -> Result<Option<DeviceData>> {
        let row = sqlx::query!(
            "SELECT * FROM device_data WHERE device_id = ? ORDER BY timestamp DESC LIMIT 1",
            device_id
        ).fetch_optional(&self.pool).await?;
        
        Ok(row.map(|r| {
            // device_type stored as lowercase string (e.g., "solarinverter")
            let device_type = match r.device_type.as_str() {
                "solarinverter" => DeviceType::SolarInverter,
                "batterysystem" => DeviceType::BatterySystem,
                "chargecontroller" => DeviceType::ChargeController,
                "energymeter" => DeviceType::EnergyMeter,
                _ => DeviceType::SolarInverter,
            };
            let metrics: DeviceMetrics = serde_json::from_str(&r.metrics).unwrap_or_default();
            let status: DeviceStatus = serde_json::from_str(&r.status).unwrap_or(DeviceStatus {
                is_connected: false,
                last_seen: chrono::Utc::now(),
                health: HealthStatus::Offline,
                error_message: Some("unavailable".to_string()),
            });

            DeviceData {
                device_id: r.device_id,
                timestamp: r.timestamp,
                device_type,
                metrics,
                status,
                raw_data: r.raw_data,
            }
        }))
    }
}

}

## Configuration

### System Configuration (RS485)
```toml
[server]
port = 8080
bind_address = "0.0.0.0"

[database]
path = "/var/lib/solar-monitor/data.db"
retention_days = 90

[devices]
poll_interval_seconds = 30

[[device]]
id = "550e8400-e29b-41d4-a716-446655440000"
name = "Main Inverter"
protocol = "eg4-pi30-rs485"
device_type = "SolarInverter"
enabled = true
poll_interval_seconds = 30

[device.connection_params]
serial_port = "/dev/ttyUSB0"
baud_rate = "2400"
```

### Database Schema (Device-Agnostic)
```sql
-- Universal device data table
CREATE TABLE device_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    device_type TEXT NOT NULL, -- "solar_inverter", "battery_system", etc.
    timestamp DATETIME NOT NULL,
    metrics TEXT NOT NULL,     -- JSON blob of DeviceMetrics
    status TEXT NOT NULL,      -- JSON blob of DeviceStatus  
    raw_data TEXT,            -- Protocol-specific raw response
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Device registry table
CREATE TABLE devices (
    id TEXT PRIMARY KEY, -- UUID string
    name TEXT NOT NULL,
    protocol TEXT NOT NULL,
    device_type TEXT NOT NULL,
    connection_params TEXT NOT NULL, -- JSON blob
    poll_interval_seconds INTEGER NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_device_data_device_time ON device_data(device_id, timestamp);
CREATE INDEX idx_device_data_type_time ON device_data(device_type, timestamp);
```

This simplified core engine focuses on the essential functionality: polling EG4 devices, storing data locally, and serving it via a web interface - all in a single, lightweight process suitable for edge deployment.

## Device Manager (Runtime CRUD + Test)

The `DeviceManager` encapsulates runtime device configuration stored in SQLite and manages the lifecycle (add/update/remove) including polling tasks.

```rust
pub struct DeviceManager {
    db: Arc<SqliteDatabase>,
    protocols: HashMap<&'static str, Arc<dyn DeviceProtocol>>, // from CoreEngine
    connections: Arc<RwLock<HashMap<String, Box<dyn DeviceConnection>>>>,
}

impl DeviceManager {
    pub async fn list_device_configs(&self) -> Result<Vec<DeviceConfig>> { /* SELECT * FROM devices */ unimplemented!() }

    pub async fn add_device(&self, req: AddDeviceRequest) -> Result<DeviceConfig> {
        // Validate: protocol exists, supported device type
        // Persist into `devices` table (generate UUID if needed)
        // Connect via protocol.connect, insert into connections map
        // Spawn per-device polling task
        unimplemented!()
    }

    pub async fn update_device(&self, id: Uuid, req: UpdateDeviceRequest) -> Result<DeviceConfig> {
        // Update row, reconnect if connection params changed, restart polling
        unimplemented!()
    }

    pub async fn remove_device(&self, id: Uuid) -> Result<()> {
        // Stop polling, drop connection, delete from DB
        unimplemented!()
    }

    pub async fn test_connection(&self, id: Uuid) -> Result<TestConnectionResult> {
        // Run QID/QPIGS with current connection; map to ok/message
        unimplemented!()
    }

    pub async fn test_connection_params(&self, params: TestConnectionParams) -> Result<TestConnectionResult> {
        // Create a transient connection based on params (no persistence), send QID
        unimplemented!()
    }

    pub async fn import_devices(&self, configs: Vec<DeviceConfig>) -> Result<ImportDevicesResult> {
        // Upsert by id (or name fallback), create/update connections and polling
        unimplemented!()
    }
}

pub struct TestConnectionResult { pub ok: bool, pub message: Option<String> }

pub struct AddDeviceRequest {
    pub name: String,
    pub device_type: DeviceType,
    pub protocol: String,
    pub poll_interval_seconds: u64,
    pub connection_params: HashMap<String, String>,
}

pub struct UpdateDeviceRequest { /* same as AddDeviceRequest but optional fields */ }
pub struct TestConnectionParams { pub device_type: DeviceType, pub protocol: String, pub connection_params: HashMap<String, String> }
pub struct ImportDevicesResult { pub added: u32, pub updated: u32, pub skipped: u32, pub errors: Vec<String> }
```
