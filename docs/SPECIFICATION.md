# Universal Solar Monitor - Edge Device Specification

## Overview
A device-agnostic, single-binary solar monitoring solution for inverters, batteries, charge controllers, and energy meters. Starting with EG4 6000XP inverter support, designed for extensible local deployment on Raspberry Pi or Nucbox devices. Built with Rust backend and Preact/TypeScript frontend, extending the existing WebSocket bridge foundation.

## Core Principles

### Edge-First Design
- **Single Binary**: Complete solution in one executable (~10-20MB)
- **Local Only**: No cloud dependencies, local network access only
- **Resource Conscious**: <100MB RAM, minimal CPU usage
- **Simple Deployment**: Copy binary, edit config file, run

### Realistic Scope
- **Target**: 1-10 solar devices (inverters, batteries, charge controllers) on local network
- **Phase 1**: EG4 6000XP inverters via PI30 protocol
- **Phase 2**: Battery systems via Modbus TCP, charge controllers via CAN bus
- **Users**: 1-3 concurrent web dashboard users
- **Data**: 30-90 days of metrics in local SQLite database
- **Interface**: Modern web dashboard accessible on local network

## Architecture (Simplified)

### Single Process Design
```
┌─────────────────────────────────────────────────┐
│           Universal Solar Monitor               │
│  ┌─────────────────────────────────────────────┐│
│  │              Web Server                     ││
│  │  ┌─────────────┐  ┌─────────────────────┐  ││
│  │  │   REST API  │  │  Static Frontend    │  ││
│  │  │   (Axum)    │  │   (Embedded)        │  ││
│  │  └─────────────┘  └─────────────────────┘  ││
│  └─────────────────────────────────────────────┘│
│                                                 │
│  ┌─────────────────────────────────────────────┐│
│  │        Device-Agnostic Engine               ││
│  │  ┌─────────────────┐  ┌─────────────────┐  ││
│  │  │   Protocol      │  │   Universal     │  ││
│  │  │   Registry      │  │   Data Store    │  ││
│  │  │(EG4,Modbus,CAN) │  │   (SQLite)      │  ││
│  │  └─────────────────┘  └─────────────────┘  ││
│  └─────────────────────────────────────────────┘│
└─────────────────────────────────────────────────┘
```

### Technology Stack
- Backend: Rust with Tokio, Axum, SQLx, tokio-serial (RS485 first)
- Database: SQLite (embedded, single file)
- Frontend: Preact + TypeScript (embedded in binary)
- Types: Specta for Rust/TypeScript synchronization
- Deployment: Single binary with systemd service

## Core Components

### 1. Device-Agnostic Protocol System (RS485-first)
```rust
// Universal protocol interface (starting with EG4, extensible to others)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceType {
    SolarInverter,
    BatterySystem, 
    ChargeController,
    EnergyMeter,
}

pub struct SolarDevice {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub protocol: String,  // "eg4-pi30-rs485", "modbus-tcp" (future), "can-bus" (future)
    pub connection_params: HashMap<String, String>,
    enabled: bool,
    poll_interval_seconds: u32,
}

// Universal device data model
#[async_trait]
pub trait DeviceProtocol: Send + Sync {
    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>>;
    async fn discover_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>>;
    fn supported_device_types(&self) -> Vec<DeviceType>;
    fn protocol_name(&self) -> &'static str;
    fn metadata(&self) -> ProtocolMetadata;
}
```

### 2. Universal Data Model (Canonical)
```rust
// Device-agnostic data structure that works for all device types
#[derive(Serialize, Deserialize, Debug, Clone)] // #[derive(specta::Type)] in code
#[serde(rename_all = "camelCase")]
pub struct DeviceData {
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub device_type: DeviceType,
    pub metrics: DeviceMetrics,
    pub status: DeviceStatus,
    pub raw_data: Option<String>, // Protocol-specific response
}

// Universal metrics that normalize data across all device types
#[derive(Serialize, Deserialize, Debug, Clone)] // #[derive(specta::Type)] in code
#[serde(rename_all = "camelCase")]
pub struct DeviceMetrics {
    // Power metrics (all device types)
    pub input_power_watts: Option<f64>,
    pub output_power_watts: Option<f64>,
    pub load_percentage: Option<f64>,
    
    // Battery metrics (inverters, battery systems)
    pub battery_voltage: Option<f64>,
    pub battery_current: Option<f64>, 
    pub battery_soc_percentage: Option<f64>,
    pub battery_temperature_celsius: Option<f64>,
    
    // Solar metrics (inverters, charge controllers)
    pub pv_voltage: Option<f64>,
    pub pv_current: Option<f64>,
    pub pv_power_watts: Option<f64>,
    
    // Grid metrics (inverters, energy meters)
    pub grid_voltage: Option<f64>,
    pub grid_frequency: Option<f64>,
    pub grid_power_watts: Option<f64>,
    
    // Device-specific
    pub device_temperature_celsius: Option<f64>,
    pub efficiency_percentage: Option<f64>,
    pub fault_codes: Vec<String>,
    pub operating_mode: Option<String>,
    
    // Extension point for protocol-specific metrics
    pub custom_metrics: HashMap<String, f64>,
}

// Canonical device status for all components and API
#[derive(Serialize, Deserialize, Debug, Clone)] // #[derive(specta::Type)] in code
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub is_connected: bool,
    pub last_seen: DateTime<Utc>,
    pub health: HealthStatus,
    pub error_message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)] // #[derive(specta::Type)] in code
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Offline,
}
```

## Features

### Core Features (v1.0) - RS485 Foundation
- [x] Universal protocol system with EG4/PI30 over RS485 as first implementation
- [ ] Device-agnostic SQLite data persistence with JSON blob storage
- [ ] Universal web dashboard supporting all device types
- [ ] RS485 serial device discovery and setup
- [ ] Device-type-aware historical charts and metrics
- [ ] Configuration management for mixed device environments
- [ ] Protocol registry with compile-time registration

### Future Extensions (v1.1+)
- [ ] Modbus TCP protocol for battery systems
- [ ] CAN bus protocol for advanced battery management
- [ ] RS485 for charge controllers
- [ ] Authentication and authorization
- [ ] Email alerts for system issues
- [ ] Data export (CSV/JSON)
- [ ] Mobile-responsive dashboard
- [ ] Advanced device discovery across protocols

### Explicitly Out of Scope
- Dynamic plugin loading (compile-time only)
- Cloud connectivity or external dependencies
- Advanced analytics or ML features
- Multi-user authentication
- Enterprise monitoring features
- Horizontal scaling or distributed deployment
- Complex data transformation pipelines

## Development Plan

### Phase 1: Universal Foundation (Week 1-2)
1. Implement device-agnostic protocol system with EG4 as first protocol
2. Create universal SQLite schema with JSON blob storage
3. Build device-agnostic REST API endpoints
4. Embed universal frontend supporting multiple device types

### Phase 2: EG4 Implementation (Week 3-4)
1. Complete EG4/PI30 protocol implementation and testing
2. Build universal dashboard with device-type-aware UI
3. Implement multi-device discovery and management
4. Add device-type-specific charts and metrics visualization

### Phase 3: Protocol Extension Ready (Week 5-6)
1. Validate protocol abstraction with mock Modbus implementation
2. Create protocol registration system documentation
3. Implement configuration management for mixed device environments
4. Add monitoring, logging, and systemd service integration

## Resource Requirements (Device-Agnostic Scaling)

### Minimum Hardware (Edge Device Optimized)
- **CPU**: 1 core @ 1GHz (Raspberry Pi 3+ level)
- **RAM**: 512MB (will use <100MB for 10 devices)
- **Storage**: 4GB available (universal database will use <1GB/year)
- **Network**: Local network access to solar devices
- **Network**: 100Mbps local network

### Target Performance
- **Startup time**: <5 seconds
- **Memory usage**: 50-100MB steady state
- **Response time**: <100ms for dashboard loads
- **Data polling**: Every 10-30 seconds per device
- **Concurrent users**: 3-5 simultaneous dashboard users

## Single Binary Implementation

### Build Configuration (single binary)
```toml
[package]
name = "solar-monitor"
version = "1.0.0"
edition = "2021"

[[bin]]
name = "solar-monitor"
path = "src/main.rs"

[dependencies]
tokio = { version = "1.0", features = ["full"] }
axum = { version = "0.7", features = ["macros"] }
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "sqlite"] }
serde = { version = "1.0", features = ["derive"] }
specta = "1.0"
rust-embed = "8.0"
tokio-serial = "5.4"

[profile.release]
lto = true
codegen-units = 1
panic = "abort"
strip = "symbols"
opt-level = "z" # Optimize for size
```

### Embedded Assets
- Frontend built into binary using `rust-embed`
- Default configuration embedded
- Database migrations embedded (sqlx::migrate!)
- No external files required at runtime

### Deployment
```bash
# Single command deployment
curl -L https://releases.../solar-monitor-arm64 -o solar-monitor
chmod +x solar-monitor
sudo ./solar-monitor --install  # Creates service and config
sudo systemctl start solar-monitor
```

## Configuration

### Configuration Model

- Bootstrap config (optional): TOML file for first run only.
- Runtime configuration: Devices are stored in SQLite `devices` table. The web UI and API manage devices on the fly (add/update/remove/test) without editing TOML.
- Export/Import: Provide endpoints to export/import device config as JSON for backup/restore.

### Simple Bootstrap Configuration (RS485)
```toml
# /etc/solar-monitor/config.toml
[server]
port = 8080
bind_address = "0.0.0.0"

[database]
path = "/var/lib/solar-monitor/data.db"
retention_days = 90

[devices]
poll_interval_seconds = 30

[[device]]
id = "550e8400-e29b-41d4-a716-446655440000" # UUID
name = "Main Inverter"
protocol = "eg4-pi30-rs485"
device_type = "SolarInverter"
enabled = true
poll_interval_seconds = 30

[device.connection_params]
serial_port = "/dev/ttyUSB0"
baud_rate = "2400"
data_bits = "8"
parity = "none"
stop_bits = "1"

[[device]]
id = "c56a4180-65aa-42ec-a945-5fd21dec0538"
name = "Backup Inverter"
protocol = "eg4-pi30-rs485"
device_type = "SolarInverter"
enabled = true
poll_interval_seconds = 30

[device.connection_params]
serial_port = "/dev/ttyUSB1"
baud_rate = "2400"
data_bits = "8"
parity = "none"
stop_bits = "1"
```

### RS485 Connection Parameters
- Required: `serial_port` (e.g., `/dev/ttyUSB0`), `baud_rate` (default `2400`)
- Optional defaults: `data_bits=8`, `parity=none`, `stop_bits=1`, `timeout_seconds=3`
- Validation: use `QID`/`QPIGS` test before persisting device.

### Auto-Discovery (RS485)
- Scan available serial ports for EG4 inverters
- Verify PI30 protocol compatibility via QID
- Suggest configuration via web interface

### Web UI Device Management (v1)
- Add device: provide name, protocol, device_type, RS485 params; system validates connectivity before saving.
- Update/remove device at runtime; polling restarts automatically.
- Test connection: one-off PI30 QID/QPIGS to verify serial parameters.
- Import/export: backup and restore device configurations via API (JSON).

## Data Flow (Simplified)

1. **Device Polling**: Simple per-device polling of configured EG4 devices over RS485
2. **Data Parsing**: PI30 response parsing into structured data
3. **Storage**: Direct SQLite storage with simple schema
4. **Web API + WS**: REST endpoints and WS upgrade on same port
5. **Dashboard**: Preact frontend displays real-time metrics and charts
6. **Type Safety**: Specta ensures Rust/TypeScript consistency

## Success Criteria

### User Experience
- Install on Raspberry Pi in <5 minutes
- Access dashboard immediately after setup
- See real-time solar data without configuration
- View daily/weekly trends

### Technical Goals
- <20MB binary size
- <100MB RAM usage
- <5 second startup time
- 99% uptime on stable connections

### Business Value
- Replace proprietary monitoring solutions
- Enable local data ownership
- Support multiple EG4 inverters
- Foundation for future solar equipment support

This specification focuses on delivering real value with minimal complexity, building on the solid foundation of the existing WebSocket bridge rather than over-engineering an enterprise solution.
## Common Types (Source of Truth)

These types are canonical for all components. API DTOs and DB schemas must map directly to these.

```rust
#[derive(Serialize, Deserialize, Debug, Clone)] // #[derive(specta::Type)] in code
#[serde(rename_all = "camelCase")]
pub enum DeviceType { SolarInverter, BatterySystem, ChargeController, EnergyMeter }

#[derive(Serialize, Deserialize, Debug, Clone)] // #[derive(specta::Type)] in code
#[serde(rename_all = "camelCase")]
pub enum HealthStatus { Healthy, Warning, Critical, Offline }

#[derive(Serialize, Deserialize, Debug, Clone)] // #[derive(specta::Type)] in code
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub is_connected: bool,
    pub last_seen: DateTime<Utc>,
    pub health: HealthStatus,
    pub error_message: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)] // #[derive(specta::Type)] in code
#[serde(rename_all = "camelCase")]
pub struct DeviceMetrics {
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

#[derive(Serialize, Deserialize, Debug, Clone)] // #[derive(specta::Type)] in code
#[serde(rename_all = "camelCase")]
pub struct DeviceData {
    pub device_id: String, // UUID
    pub timestamp: DateTime<Utc>,
    pub device_type: DeviceType,
    pub metrics: DeviceMetrics,
    pub status: DeviceStatus,
    pub raw_data: Option<String>,
}
```

JSON conventions
- Field names: camelCase
- Enum values: camelCase strings (serde rename rules)
- Timestamps: ISO 8601 (UTC)
