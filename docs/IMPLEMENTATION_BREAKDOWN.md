# Universal Solar Monitor - Implementation Breakdown

## Overview

This document provides a detailed, step-by-step implementation plan for building the universal solar monitoring system, starting with EG4 6000XP RS485 support and architected for extensibility to other device types.

## Project Structure

```
eg4-monitor/
├── Cargo.toml                    # Workspace configuration
├── system.toml                   # Runtime configuration
├── README.md
├── docs/                         # Architecture documentation
├── contracts/                    # Canonical types (Rust) + Typeshare
│   ├── Cargo.toml
│   └── src/lib.rs               # Source of truth for shared types
├── types/
│   └── ts/                      # Generated TypeScript types from contracts
├── core/                         # Core universal engine
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs               # Core module exports
│       ├── protocol/            # Protocol system
│       ├── device/              # Device management
│       ├── data/                # Data models
│       └── config/              # Configuration
├── protocols/                   # Protocol implementations
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs               # Protocol registry
│       ├── eg4_rs485/           # EG4 RS485 protocol
│       ├── modbus_rtu/          # Future: Modbus RTU
│       └── can_bus/             # Future: CAN bus
├── storage/                     # Data storage layer
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── sqlite/              # SQLite implementation
│       └── models/              # Database models
├── api/                         # REST API server
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── routes/              # API endpoints
│       ├── handlers/            # Request handlers
│       └── middleware/          # Auth, CORS, etc.
├── web/                         # Frontend application
│   ├── package.json
│   ├── vite.config.js
│   ├── index.html
│   └── src/
│       ├── main.tsx             # Entry point
│       ├── components/          # Universal UI components
│       ├── pages/               # Dashboard pages
│       ├── hooks/               # Data fetching hooks
│       └── types/               # Import from ../../types/ts/ (generated)
└── bin/                         # Main executable
    ├── Cargo.toml
    └── src/
        └── main.rs              # Application entry point
```

## Implementation Phases

### Phase 1: Core Universal Foundation (Week 1-2)

#### 1.1 Workspace Setup and Dependencies

**File: `Cargo.toml` (root)**
```toml
[workspace]
members = ["contracts", "core", "protocols", "storage", "api", "bin"]
edition = "2021"

[workspace.dependencies]
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
anyhow = "1.0"
tracing = "0.1"
async-trait = "0.1"
typeshare = "1.0"

[workspace.dependencies.sqlx]
version = "0.7"
features = ["runtime-tokio-rustls", "sqlite", "chrono", "uuid", "json"]
```

**File: `core/Cargo.toml`**
```toml
[package]
name = "solar-monitor-core"
version = "0.1.0"
edition.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
anyhow.workspace = true
tracing.workspace = true
async-trait.workspace = true
typeshare.workspace = true
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.0", features = ["v4", "serde"] }
```

#### 1.1a Contracts Crate and Typeshare Generation

Add a `contracts/` crate containing canonical Rust types (DeviceType, HealthStatus, DeviceStatus, DeviceMetrics, DeviceData, DeviceConfigDto) annotated with `#[typeshare]`. Configure `contracts/build.rs` to emit matching TypeScript types to `types/ts/` at repository root.

Command to generate TS types:
```
cargo build --manifest-path contracts/Cargo.toml --release
```
The frontend imports types from `types/ts/`.

#### 1.2 Core Data Models

**File: `core/src/data/mod.rs`**
```rust
use serde::{Deserialize, Serialize};
use typeshare::typeshare;
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceType {
    SolarInverter,
    BatterySystem,
    ChargeController,
    EnergyMeter,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMetrics {
    // Universal power metrics
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
    
    // Device health
    pub device_temperature_celsius: Option<f64>,
    pub efficiency_percentage: Option<f64>,
    pub fault_codes: Vec<String>,
    pub operating_mode: Option<String>,
    
    // Protocol-specific extensions
    pub custom_metrics: HashMap<String, f64>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Offline,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub is_connected: bool,
    pub last_seen: DateTime<Utc>,
    pub health: HealthStatus,
    pub error_message: Option<String>,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceData {
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub device_type: DeviceType,
    pub metrics: DeviceMetrics,
    pub status: DeviceStatus,
    pub raw_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub protocol: String,
    pub connection_params: HashMap<String, String>,
    pub enabled: bool,
    pub poll_interval_seconds: u32,
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub protocol: String,
    pub connection_info: String,
}

#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub protocol: String,
    pub connection_params: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub serial_ports: Vec<String>,
    pub can_interfaces: Vec<String>,
    pub timeout_seconds: u32,
}

#[typeshare]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResponse {
    pub success: bool,
    pub data: String,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}
```

#### 1.3 Protocol System Interface

**File: `core/src/protocol/mod.rs`**
```rust
use crate::data::*;
use async_trait::async_trait;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct ProtocolMetadata {
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub supported_device_types: &'static [DeviceType],
    pub capabilities: ProtocolCapabilities,
}

#[derive(Debug, Clone)]
pub struct ProtocolCapabilities {
    pub supports_discovery: bool,
    pub supports_commands: bool,
    pub supports_real_time: bool,
    pub max_concurrent_connections: Option<u32>,
}

#[async_trait]
pub trait DeviceProtocol: Send + Sync {
    fn protocol_name(&self) -> &'static str;
    fn metadata(&self) -> ProtocolMetadata;
    fn supported_device_types(&self) -> Vec<DeviceType>;
    
    async fn discover_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>>;
    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>>;
}

#[async_trait]
pub trait DeviceConnection: Send + Sync {
    async fn read_data(&mut self) -> Result<DeviceData>;
    async fn send_command(&mut self, command: &str) -> Result<CommandResponse>;
    fn device_info(&self) -> &DeviceInfo;
    fn is_connected(&self) -> bool;
    async fn health_check(&mut self) -> Result<()>;
}
```

#### 1.4 Protocol Registry

**File: `core/src/protocol/registry.rs`**
```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use anyhow::Result;
use crate::protocol::{DeviceProtocol, ProtocolMetadata};
use crate::data::{ScanConfig, DiscoveredDevice};

pub struct ProtocolRegistry {
    protocols: HashMap<&'static str, Arc<dyn DeviceProtocol>>,
    metadata: HashMap<&'static str, ProtocolMetadata>,
    discovered_cache: Arc<RwLock<HashMap<String, DiscoveredDevice>>>,
}

impl ProtocolRegistry {
    pub fn new() -> Self {
        Self {
            protocols: HashMap::new(),
            metadata: HashMap::new(),
            discovered_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn register_protocol(&mut self, protocol: Arc<dyn DeviceProtocol>) {
        let name = protocol.protocol_name();
        let metadata = protocol.metadata();
        
        self.protocols.insert(name, protocol);
        self.metadata.insert(name, metadata);
        
        tracing::info!("Registered protocol: {}", name);
    }
    
    pub fn get_protocol(&self, name: &str) -> Option<&dyn DeviceProtocol> {
        self.protocols.get(name).map(|p| p.as_ref())
    }
    
    pub fn list_protocols(&self) -> Vec<&ProtocolMetadata> {
        self.metadata.values().collect()
    }
    
    pub async fn discover_all_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        let mut all_devices = Vec::new();
        
        for protocol in self.protocols.values() {
            match protocol.discover_devices(scan_config).await {
                Ok(mut devices) => {
                    tracing::info!("Discovered {} devices with {}", devices.len(), protocol.protocol_name());
                    all_devices.append(&mut devices);
                }
                Err(e) => {
                    tracing::warn!("Discovery failed for {}: {}", protocol.protocol_name(), e);
                }
            }
        }
        
        // Cache discovered devices
        let mut cache = self.discovered_cache.write().await;
        for device in &all_devices {
            cache.insert(device.id.clone(), device.clone());
        }
        
        Ok(all_devices)
    }
}
```

#### 1.5 Core Library Setup

**File: `core/src/lib.rs`**
```rust
pub mod data;
pub mod protocol;

pub use data::*;
pub use protocol::*;

// Re-export commonly used types
pub use protocol::{DeviceProtocol, DeviceConnection, ProtocolRegistry};
```

### Phase 2: EG4 RS485 Protocol Implementation (Week 1-2)

#### 2.1 EG4 Protocol Structure

**File: `protocols/Cargo.toml`**
```toml
[package]
name = "solar-monitor-protocols"
version = "0.1.0"
edition.workspace = true

[dependencies]
solar-monitor-core = { path = "../core" }
tokio.workspace = true
serde.workspace = true
anyhow.workspace = true
tracing.workspace = true
async-trait.workspace = true
tokio-serial = "5.4"
```

**File: `protocols/src/eg4_rs485/mod.rs`**
```rust
mod protocol;
mod connection;
mod pi30;

pub use protocol::EG4Protocol;
pub use connection::EG4SerialConnection;
```

#### 2.2 EG4 Protocol Implementation

**File: `protocols/src/eg4_rs485/protocol.rs`**
```rust
use std::collections::HashMap;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use async_trait::async_trait;
use anyhow::Result;

use solar_monitor_core::{
    DeviceProtocol, DeviceConnection, DeviceConfig, DeviceType, 
    DiscoveredDevice, ScanConfig, ProtocolMetadata, ProtocolCapabilities
};

use super::{EG4SerialConnection, pi30::PI30Parser};

pub struct EG4Protocol {
    parser: PI30Parser,
}

impl EG4Protocol {
    pub fn new() -> Self {
        Self {
            parser: PI30Parser::new(),
        }
    }
}

#[async_trait]
impl DeviceProtocol for EG4Protocol {
    fn protocol_name(&self) -> &'static str {
        "eg4-pi30-rs485"
    }
    
    fn metadata(&self) -> ProtocolMetadata {
        ProtocolMetadata {
            name: "EG4/PI30 RS485",
            version: "1.0.0",
            description: "EG4 6000XP Inverter via RS485 connection using PI30 protocol",
            supported_device_types: &[DeviceType::SolarInverter],
            capabilities: ProtocolCapabilities {
                supports_discovery: true,
                supports_commands: true,
                supports_real_time: true,
                max_concurrent_connections: Some(1), // RS485 is single-threaded
            },
        }
    }
    
    fn supported_device_types(&self) -> Vec<DeviceType> {
        vec![DeviceType::SolarInverter]
    }
    
    async fn discover_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        let mut devices = Vec::new();
        
        for serial_port in &scan_config.serial_ports {
            if let Ok(device) = self.test_eg4_device(serial_port, scan_config.timeout_seconds).await {
                devices.push(device);
            }
        }
        
        Ok(devices)
    }
    
    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>> {
        let serial_port = config.connection_params.get("serial_port")
            .ok_or_else(|| anyhow::anyhow!("Missing serial_port parameter"))?;
        let baud_rate: u32 = config.connection_params.get("baud_rate")
            .unwrap_or(&"2400".to_string())
            .parse()
            .unwrap_or(2400);
        
        let connection = EG4SerialConnection::new(
            config.clone(),
            serial_port.clone(),
            baud_rate
        ).await?;
        
        Ok(Box::new(connection))
    }
}

impl EG4Protocol {
    async fn test_eg4_device(&self, port_path: &str, timeout_seconds: u32) -> Result<DiscoveredDevice> {
        // Test RS485 connection to EG4 6000XP
        let serial = tokio_serial::new(port_path, 2400)
            .data_bits(tokio_serial::DataBits::Eight)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .timeout(Duration::from_secs(timeout_seconds as u64))
            .open_native_async()?;
            
        let mut serial = SerialStream::new(serial)?;
        
        // Send QID command to identify device
        let qid_command = self.parser.build_command("QID");
        serial.write_all(qid_command.as_bytes()).await?;
        
        // Read response
        let mut buffer = vec![0; 1024];
        let bytes_read = tokio::time::timeout(
            Duration::from_secs(3),
            serial.read(&mut buffer)
        ).await??;
        
        buffer.truncate(bytes_read);
        let response = String::from_utf8_lossy(&buffer);
        
        // Validate PI30 response and check for EG4 6000XP
        if self.parser.is_valid_response(&response) {
            let device_info = self.parser.parse_device_info(&response)?;
            
            Ok(DiscoveredDevice {
                id: port_path.to_string(),
                name: format!("EG4 6000XP on {}", port_path),
                device_type: DeviceType::SolarInverter,
                protocol: "eg4-pi30-rs485".to_string(),
                connection_params: HashMap::from([
                    ("serial_port".to_string(), port_path.to_string()),
                    ("baud_rate".to_string(), "2400".to_string()),
                    ("data_bits".to_string(), "8".to_string()),
                    ("parity".to_string(), "none".to_string()),
                    ("stop_bits".to_string(), "1".to_string()),
                ]),
            })
        } else {
            Err(anyhow::anyhow!("No EG4 device found on {}", port_path))
        }
    }
}
```

#### 2.3 PI30 Parser Implementation

**File: `protocols/src/eg4_rs485/pi30.rs`**
```rust
use std::collections::HashMap;
use anyhow::Result;

pub struct PI30Parser;

impl PI30Parser {
    pub fn new() -> Self {
        Self
    }
    
    pub fn build_command(&self, command: &str) -> String {
        let crc = self.calculate_crc16(command.as_bytes());
        format!("{}{:04X}\r", command, crc)
    }
    
    pub fn is_valid_response(&self, response: &str) -> bool {
        response.starts_with('(') && response.ends_with('\r')
    }
    
    pub fn parse_device_info(&self, response: &str) -> Result<HashMap<String, String>> {
        // Parse QID response to extract device information
        let mut info = HashMap::new();
        
        if let Some(device_id) = self.extract_device_id(response) {
            info.insert("device_id".to_string(), device_id);
            info.insert("model".to_string(), "EG4-6000XP".to_string());
        }
        
        Ok(info)
    }
    
    pub fn parse_qpigs_response(&self, response: &str) -> Result<HashMap<String, f64>> {
        // Remove PI30 framing (parentheses and CR)
        if !self.is_valid_response(response) {
            return Err(anyhow::anyhow!("Invalid PI30 response format"));
        }
        
        let data = &response[1..response.len()-1];
        let fields: Vec<&str> = data.split(' ').collect();
        
        if fields.len() < 20 {
            return Err(anyhow::anyhow!("Insufficient QPIGS fields: got {}, need 20+", fields.len()));
        }
        
        let mut metrics = HashMap::new();
        
        // Parse each field by position (PI30 QPIGS format)
        self.safe_parse_field(&mut metrics, "grid_voltage", fields.get(0))?;
        self.safe_parse_field(&mut metrics, "grid_frequency", fields.get(1))?;
        self.safe_parse_field(&mut metrics, "output_voltage", fields.get(2))?;
        self.safe_parse_field(&mut metrics, "output_frequency", fields.get(3))?;
        self.safe_parse_field(&mut metrics, "output_apparent_power", fields.get(4))?;
        self.safe_parse_field(&mut metrics, "output_active_power", fields.get(5))?;
        self.safe_parse_field(&mut metrics, "output_load_percent", fields.get(6))?;
        self.safe_parse_field(&mut metrics, "bus_voltage", fields.get(7))?;
        self.safe_parse_field(&mut metrics, "battery_voltage", fields.get(8))?;
        self.safe_parse_field(&mut metrics, "battery_charging_current", fields.get(9))?;
        self.safe_parse_field(&mut metrics, "battery_capacity", fields.get(10))?;
        self.safe_parse_field(&mut metrics, "inverter_temperature", fields.get(11))?;
        self.safe_parse_field(&mut metrics, "pv_input_current", fields.get(12))?;
        self.safe_parse_field(&mut metrics, "pv_input_voltage", fields.get(13))?;
        self.safe_parse_field(&mut metrics, "battery_scc_voltage", fields.get(14))?;
        self.safe_parse_field(&mut metrics, "battery_discharge_current", fields.get(15))?;
        
        Ok(metrics)
    }
    
    fn safe_parse_field(&self, map: &mut HashMap<String, f64>, key: &str, value: Option<&str>) -> Result<()> {
        if let Some(val_str) = value {
            if let Ok(val) = val_str.parse::<f64>() {
                map.insert(key.to_string(), val);
            }
        }
        Ok(())
    }
    
    fn extract_device_id(&self, response: &str) -> Option<String> {
        // Extract device ID from QID response
        if response.len() > 10 {
            Some(response[1..9].to_string()) // Extract ID portion
        } else {
            None
        }
    }
    
    fn calculate_crc16(&self, data: &[u8]) -> u16 {
        let mut crc: u16 = 0xFFFF;
        for &byte in data {
            crc ^= byte as u16;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xA001;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc
    }
}
```

#### 2.4 EG4 Serial Connection

**File: `protocols/src/eg4_rs485/connection.rs`**
```rust
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use async_trait::async_trait;
use chrono::Utc;
use anyhow::Result;

use solar_monitor_core::{
    DeviceConnection, DeviceConfig, DeviceData, DeviceInfo, DeviceType,
    DeviceMetrics, DeviceStatus, HealthStatus, CommandResponse
};

use super::pi30::PI30Parser;

pub struct EG4SerialConnection {
    config: DeviceConfig,
    serial: Option<SerialStream>,
    device_info: DeviceInfo,
    parser: PI30Parser,
}

impl EG4SerialConnection {
    pub async fn new(config: DeviceConfig, port_path: String, baud_rate: u32) -> Result<Self> {
        // Open RS485 connection
        let serial_port = tokio_serial::new(&port_path, baud_rate)
            .data_bits(tokio_serial::DataBits::Eight)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .timeout(Duration::from_secs(5))
            .open_native_async()?;
            
        let serial = Some(SerialStream::new(serial_port)?);
        
        let device_info = DeviceInfo {
            id: config.id.clone(),
            name: config.name.clone(),
            device_type: DeviceType::SolarInverter,
            protocol: "eg4-pi30-rs485".to_string(),
            connection_info: format!("RS485: {} @ {}bps", port_path, baud_rate),
        };
        
        Ok(Self {
            config,
            serial,
            device_info,
            parser: PI30Parser::new(),
        })
    }
}

#[async_trait]
impl DeviceConnection for EG4SerialConnection {
    async fn read_data(&mut self) -> Result<DeviceData> {
        // Send QPIGS command to get general status
        let raw_response = self.send_pi30_command("QPIGS").await?;
        
        // Parse PI30 response to universal DeviceMetrics
        let pi30_fields = self.parser.parse_qpigs_response(&raw_response)?;
        let metrics = self.map_to_universal_metrics(&pi30_fields);
        
        let status = DeviceStatus {
            is_connected: true,
            last_seen: Utc::now(),
            health: HealthStatus::Healthy,
            error_message: None,
        };
        
        Ok(DeviceData {
            device_id: self.config.id.clone(),
            timestamp: Utc::now(),
            device_type: DeviceType::SolarInverter,
            metrics,
            status,
            raw_data: Some(raw_response),
        })
    }
    
    async fn send_command(&mut self, command: &str) -> Result<CommandResponse> {
        let start_time = std::time::Instant::now();
        let response = self.send_pi30_command(command).await?;
        let execution_time = start_time.elapsed();
        
        Ok(CommandResponse {
            success: true,
            data: response,
            error: None,
            execution_time_ms: execution_time.as_millis() as u64,
        })
    }
    
    fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }
    
    fn is_connected(&self) -> bool {
        self.serial.is_some()
    }
    
    async fn health_check(&mut self) -> Result<()> {
        // Use QID command as health check
        self.send_pi30_command("QID").await?;
        Ok(())
    }
}

impl EG4SerialConnection {
    async fn send_pi30_command(&mut self, command: &str) -> Result<String> {
        let serial = self.serial.as_mut()
            .ok_or_else(|| anyhow::anyhow!("RS485 port not connected"))?;
        
        // Build complete PI30 command with CRC
        let full_command = self.parser.build_command(command);
        
        // Send over RS485
        serial.write_all(full_command.as_bytes()).await?;
        
        // Read response
        let mut buffer = vec![0; 1024];
        let bytes_read = tokio::time::timeout(
            Duration::from_secs(3), // RS485 should be fast
            serial.read(&mut buffer)
        ).await??;
        
        buffer.truncate(bytes_read);
        Ok(String::from_utf8_lossy(&buffer).to_string())
    }
    
    fn map_to_universal_metrics(&self, pi30_fields: &std::collections::HashMap<String, f64>) -> DeviceMetrics {
        // Calculate derived metrics
        let pv_power = pi30_fields.get("pv_input_voltage")
            .zip(pi30_fields.get("pv_input_current"))
            .map(|(v, i)| v * i);
            
        let battery_current = pi30_fields.get("battery_charging_current")
            .zip(pi30_fields.get("battery_discharge_current"))
            .map(|(charge, discharge)| charge - discharge);
        
        DeviceMetrics {
            // Power metrics
            input_power_watts: pv_power,
            output_power_watts: pi30_fields.get("output_active_power").copied(),
            load_percentage: pi30_fields.get("output_load_percent").copied(),
            
            // Battery metrics
            battery_voltage: pi30_fields.get("battery_voltage").copied(),
            battery_current,
            battery_soc_percentage: pi30_fields.get("battery_capacity").copied(),
            battery_temperature_celsius: None, // Not available in PI30
            
            // Solar metrics
            pv_voltage: pi30_fields.get("pv_input_voltage").copied(),
            pv_current: pi30_fields.get("pv_input_current").copied(),
            pv_power_watts: pv_power,
            
            // Grid metrics
            grid_voltage: pi30_fields.get("grid_voltage").copied(),
            grid_frequency: pi30_fields.get("grid_frequency").copied(),
            grid_power_watts: None,
            
            // Device health
            device_temperature_celsius: pi30_fields.get("inverter_temperature").copied(),
            efficiency_percentage: None, // Calculate: output/input * 100
            fault_codes: Vec::new(), // TODO: Parse device status bits
            operating_mode: Some("Normal".to_string()),
            
            // EG4-specific extensions
            custom_metrics: std::collections::HashMap::from([
                ("bus_voltage".to_string(), pi30_fields.get("bus_voltage").copied().unwrap_or(0.0)),
                ("scc_voltage".to_string(), pi30_fields.get("battery_scc_voltage").copied().unwrap_or(0.0)),
                ("apparent_power".to_string(), pi30_fields.get("output_apparent_power").copied().unwrap_or(0.0)),
            ]),
        }
    }
}
```

#### 2.5 Protocol Registration

**File: `protocols/src/lib.rs`**
```rust
use std::sync::Arc;
use solar_monitor_core::ProtocolRegistry;

mod eg4_rs485;

pub use eg4_rs485::EG4Protocol;

pub fn create_protocol_registry() -> ProtocolRegistry {
    let mut registry = ProtocolRegistry::new();
    
    // Register EG4 RS485 protocol
    registry.register_protocol(Arc::new(EG4Protocol::new()));
    
    // Future protocols:
    // registry.register_protocol(Arc::new(ModbusRTUProtocol::new()));
    // registry.register_protocol(Arc::new(CANProtocol::new()));
    
    registry
}
```

### Phase 3: Data Storage Implementation (Week 2)

#### 3.1 Storage Layer Setup

**File: `storage/Cargo.toml`**
```toml
[package]
name = "solar-monitor-storage"
version = "0.1.0"
edition.workspace = true

[dependencies]
solar-monitor-core = { path = "../core" }
tokio.workspace = true
serde.workspace = true
serde_json = "1.0"
anyhow.workspace = true
tracing.workspace = true
sqlx.workspace = true
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.0", features = ["v4"] }
```

#### 3.2 Database Models and Migrations

**File: `storage/migrations/001_initial.sql`**
```sql
-- Universal device-agnostic schema
CREATE TABLE devices (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    device_type TEXT NOT NULL, -- lowercase string identifier (e.g., 'solarinverter', 'batterysystem')
    protocol TEXT NOT NULL,    -- 'eg4-pi30-rs485', 'modbus-rtu', etc.
    connection_params TEXT NOT NULL, -- JSON blob of connection parameters
    enabled BOOLEAN NOT NULL DEFAULT 1,
    poll_interval_seconds INTEGER DEFAULT 30,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Universal device data with JSON blob storage
CREATE TABLE device_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    device_type TEXT NOT NULL,
    metrics TEXT NOT NULL,     -- JSON blob of DeviceMetrics
    status TEXT NOT NULL,      -- JSON blob of DeviceStatus
    raw_data TEXT,            -- Protocol-specific raw response
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for efficient querying
CREATE INDEX idx_device_data_device_time ON device_data(device_id, timestamp);
CREATE INDEX idx_device_data_timestamp ON device_data(timestamp);
CREATE INDEX idx_device_data_type_time ON device_data(device_type, timestamp);
CREATE INDEX idx_devices_type ON devices(device_type);
CREATE INDEX idx_devices_protocol ON devices(protocol);

-- Daily aggregation for long-term storage efficiency
CREATE TABLE daily_summary (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    date DATE NOT NULL,
    device_type TEXT NOT NULL,
    
    -- Universal aggregated metrics
    avg_input_power REAL,
    avg_output_power REAL,
    max_power REAL,
    total_energy_kwh REAL,
    
    -- Device-type specific aggregations as JSON
    type_specific_metrics TEXT,
    
    sample_count INTEGER,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX idx_daily_summary_device_date ON daily_summary(device_id, date);
CREATE INDEX idx_daily_summary_type_date ON daily_summary(device_type, date);
```

#### 3.3 Storage Implementation

**File: `storage/src/lib.rs`**
```rust
use std::collections::HashMap;
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{SqlitePool, Row};
use serde_json;

use solar_monitor_core::{DeviceData, DeviceConfig, DeviceType, DeviceMetrics, DeviceStatus};

pub struct DataStore {
    pool: SqlitePool,
    config: StorageConfig,
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub database_path: String,
    pub retention_days: u32,
    pub cleanup_interval_hours: u32,
    pub enable_aggregation: bool,
}

impl DataStore {
    pub async fn new(config: StorageConfig) -> Result<Self> {
        // Create database file and directory if needed
        if let Some(parent) = std::path::Path::new(&config.database_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let pool = SqlitePool::connect(&format!("sqlite:{}", config.database_path)).await?;

        // Run migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        let store = Self { pool, config };

        // Start cleanup task
        store.start_cleanup_task().await;

        Ok(store)
    }

    pub async fn store_device_data(&self, data: &DeviceData) -> Result<()> {
        let metrics_json = serde_json::to_string(&data.metrics)?;
        let status_json = serde_json::to_string(&data.status)?;
        let device_type_str = format!("{:?}", data.device_type).to_lowercase();

        sqlx::query!(
            r#"
            INSERT INTO device_data (
                device_id, timestamp, device_type, metrics, status, raw_data
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
            data.device_id,
            data.timestamp,
            device_type_str,
            metrics_json,
            status_json,
            data.raw_data
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_latest_device_data(&self, device_id: &str) -> Result<Option<DeviceData>> {
        let row = sqlx::query!(
            "SELECT * FROM device_data WHERE device_id = ? ORDER BY timestamp DESC LIMIT 1",
            device_id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = row {
            let device_type = match r.device_type.as_str() {
                "solarinverter" => DeviceType::SolarInverter,
                "batterysystem" => DeviceType::BatterySystem,
                "chargecontroller" => DeviceType::ChargeController,
                "energymeter" => DeviceType::EnergyMeter,
                _ => DeviceType::SolarInverter,
            };

            let metrics: DeviceMetrics = serde_json::from_str(&r.metrics)?;
            let status: DeviceStatus = serde_json::from_str(&r.status)?;

            Ok(Some(DeviceData {
                device_id: r.device_id,
                timestamp: r.timestamp,
                device_type,
                metrics,
                status,
                raw_data: r.raw_data,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_device_data_range(
        &self,
        device_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceData>> {
        let limit = limit.unwrap_or(1000).min(10000); // Cap at 10k for edge devices

        let rows = sqlx::query!(
            r#"
            SELECT * FROM device_data
            WHERE device_id = ? AND timestamp BETWEEN ? AND ?
            ORDER BY timestamp ASC
            LIMIT ?
            "#,
            device_id,
            start_time,
            end_time,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        let mut device_data = Vec::new();
        for r in rows {
            let device_type = match r.device_type.as_str() {
                "solarinverter" => DeviceType::SolarInverter,
                "batterysystem" => DeviceType::BatterySystem,
                "chargecontroller" => DeviceType::ChargeController,
                "energymeter" => DeviceType::EnergyMeter,
                _ => DeviceType::SolarInverter,
            };

            let metrics: DeviceMetrics = serde_json::from_str(&r.metrics)?;
            let status: DeviceStatus = serde_json::from_str(&r.status)?;

            device_data.push(DeviceData {
                device_id: r.device_id,
                timestamp: r.timestamp,
                device_type,
                metrics,
                status,
                raw_data: r.raw_data,
            });
        }

        Ok(device_data)
    }

    pub async fn store_device_config(&self, config: &DeviceConfig) -> Result<()> {
        let connection_params_json = serde_json::to_string(&config.connection_params)?;
        let device_type_str = format!("{:?}", config.device_type).to_lowercase();

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO devices (
                id, name, device_type, protocol, connection_params, 
                enabled, poll_interval_seconds, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
            "#,
            config.id,
            config.name,
            device_type_str,
            config.protocol,
            connection_params_json,
            config.enabled,
            config.poll_interval_seconds as i64
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_device_configs(&self) -> Result<Vec<DeviceConfig>> {
        let rows = sqlx::query!("SELECT * FROM devices WHERE enabled = 1")
            .fetch_all(&self.pool)
            .await?;

        let mut configs = Vec::new();
        for r in rows {
            let device_type = match r.device_type.as_str() {
                "solarinverter" => DeviceType::SolarInverter,
                "batterysystem" => DeviceType::BatterySystem,
                "chargecontroller" => DeviceType::ChargeController,
                "energymeter" => DeviceType::EnergyMeter,
                _ => DeviceType::SolarInverter,
            };

            let connection_params: HashMap<String, String> = 
                serde_json::from_str(&r.connection_params)?;

            configs.push(DeviceConfig {
                id: r.id,
                name: r.name,
                device_type,
                protocol: r.protocol,
                connection_params,
                enabled: r.enabled,
                poll_interval_seconds: r.poll_interval_seconds as u32,
            });
        }

        Ok(configs)
    }

    pub async fn get_all_device_types(&self) -> Result<Vec<DeviceType>> {
        let rows = sqlx::query!("SELECT DISTINCT device_type FROM device_data")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().filter_map(|r| {
            match r.device_type.as_str() {
                "solarinverter" => Some(DeviceType::SolarInverter),
                "batterysystem" => Some(DeviceType::BatterySystem),
                "chargecontroller" => Some(DeviceType::ChargeController),
                "energymeter" => Some(DeviceType::EnergyMeter),
                _ => None,
            }
        }).collect())
    }

    async fn start_cleanup_task(&self) {
        let pool = self.pool.clone();
        let retention_days = self.config.retention_days;
        let cleanup_interval = self.config.cleanup_interval_hours;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(cleanup_interval as u64 * 3600)
            );

            loop {
                interval.tick().await;

                let cutoff_date = Utc::now() - chrono::Duration::days(retention_days as i64);

                if let Err(e) = sqlx::query!(
                    "DELETE FROM device_data WHERE timestamp < ?",
                    cutoff_date
                ).execute(&pool).await {
                    tracing::error!("Failed to cleanup old data: {}", e);
                } else {
                    tracing::info!("Cleaned up data older than {} days", retention_days);
                }
            }
        });
    }
}
```

### Phase 4: REST API Implementation (Week 2-3)

#### 4.1 API Server Setup

**File: `api/Cargo.toml`**
```toml
[package]
name = "solar-monitor-api"
version = "0.1.0"
edition.workspace = true

[dependencies]
solar-monitor-core = { path = "../core" }
solar-monitor-storage = { path = "../storage" }
solar-monitor-protocols = { path = "../protocols" }
tokio.workspace = true
serde.workspace = true
serde_json = "1.0"
anyhow.workspace = true
tracing.workspace = true
axum = "0.7"
tower = "0.4"
tower-http = { version = "0.5", features = ["cors"] }
typeshare.workspace = true
```

#### 4.2 API Routes and Handlers

**File: `api/src/routes/mod.rs`**
```rust
use axum::{Router, extract::State};
use tower_http::cors::CorsLayer;
use std::sync::Arc;

mod devices;
mod data;
mod protocols;
mod system;

use crate::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // System endpoints
        .route("/api/v1/health", axum::routing::get(system::health_check))
        .route("/api/v1/status", axum::routing::get(system::system_status))
        
        // Device management
        .route("/api/v1/devices", 
               axum::routing::get(devices::list_devices)
                   .post(devices::add_device))
        .route("/api/v1/devices/:id", 
               axum::routing::get(devices::get_device)
                   .put(devices::update_device)
                   .delete(devices::remove_device))
        .route("/api/v1/devices/:id/data", 
               axum::routing::get(devices::get_device_data))
        .route("/api/v1/devices/:id/commands", 
               axum::routing::post(devices::send_device_command))
        
        // Data endpoints
        .route("/api/v1/data/dashboard", 
               axum::routing::get(data::dashboard_data))
        .route("/api/v1/data/historical", 
               axum::routing::get(data::historical_data))
        
        // Protocol management
        .route("/api/v1/protocols", 
               axum::routing::get(protocols::list_protocols))
        .route("/api/v1/protocols/discovery", 
               axum::routing::post(protocols::discover_devices))
        
        .layer(CorsLayer::permissive())
        .with_state(state)
}
```

**File: `api/src/routes/devices.rs`**
```rust
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
    response::Json as ResponseJson,
};
use serde::{Deserialize, Serialize};
use typeshare::typeshare;
use std::collections::HashMap;
use std::sync::Arc;

use solar_monitor_core::{DeviceConfig, DeviceType, CommandResponse};
use crate::{AppState, ApiResult, ApiError};

#[typeshare]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDto {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub protocol: String,
    pub connection_params: HashMap<String, String>,
    pub enabled: bool,
    pub poll_interval_seconds: u32,
    pub created_at: String,
}

#[typeshare]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDeviceRequest {
    pub name: String,
    pub device_type: DeviceType,
    pub protocol: String,
    pub connection_params: HashMap<String, String>,
    pub poll_interval_seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct DeviceQuery {
    pub protocol: Option<String>,
    pub device_type: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CommandRequest {
    pub command: String,
}

pub async fn list_devices(
    Query(params): Query<DeviceQuery>,
    State(state): State<Arc<AppState>>,
) -> ApiResult<ResponseJson<Vec<DeviceDto>>> {
    let configs = state.storage.get_device_configs().await?;
    
    let filtered_devices: Vec<DeviceDto> = configs
        .into_iter()
        .filter(|config| {
            if let Some(protocol) = &params.protocol {
                if &config.protocol != protocol {
                    return false;
                }
            }
            
            if let Some(device_type) = &params.device_type {
                let device_type_str = format!("{:?}", config.device_type).to_lowercase();
                if &device_type_str != device_type {
                    return false;
                }
            }
            
            if let Some(enabled) = params.enabled {
                if config.enabled != enabled {
                    return false;
                }
            }
            
            true
        })
        .map(|config| DeviceDto {
            id: config.id.clone(),
            name: config.name.clone(),
            device_type: config.device_type.clone(),
            protocol: config.protocol.clone(),
            connection_params: config.connection_params.clone(),
            enabled: config.enabled,
            poll_interval_seconds: config.poll_interval_seconds,
            created_at: chrono::Utc::now().to_rfc3339(), // TODO: get from DB
        })
        .collect();

    Ok(ResponseJson(filtered_devices))
}

pub async fn add_device(
    State(state): State<Arc<AppState>>,
    Json(request): Json<AddDeviceRequest>,
) -> ApiResult<ResponseJson<DeviceDto>> {
    let device_id = uuid::Uuid::new_v4().to_string();
    
    let config = DeviceConfig {
        id: device_id.clone(),
        name: request.name,
        device_type: request.device_type,
        protocol: request.protocol,
        connection_params: request.connection_params,
        enabled: true,
        poll_interval_seconds: request.poll_interval_seconds.unwrap_or(30),
    };
    
    // Validate protocol exists
    if state.protocol_registry.get_protocol(&config.protocol).is_none() {
        return Err(ApiError::BadRequest("Unknown protocol".to_string()));
    }
    
    state.storage.store_device_config(&config).await?;
    
    let device_dto = DeviceDto {
        id: config.id,
        name: config.name,
        device_type: config.device_type,
        protocol: config.protocol,
        connection_params: config.connection_params,
        enabled: config.enabled,
        poll_interval_seconds: config.poll_interval_seconds,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    
    Ok(ResponseJson(device_dto))
}

pub async fn get_device_data(
    Path(device_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> ApiResult<ResponseJson<Option<solar_monitor_core::DeviceData>>> {
    let data = state.storage.get_latest_device_data(&device_id).await?;
    Ok(ResponseJson(data))
}

pub async fn send_device_command(
    Path(device_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<CommandRequest>,
) -> ApiResult<ResponseJson<CommandResponse>> {
    // Get device config
    let configs = state.storage.get_device_configs().await?;
    let config = configs.into_iter()
        .find(|c| c.id == device_id)
        .ok_or(ApiError::NotFound("Device not found".to_string()))?;
    
    // Get protocol and create connection
    let protocol = state.protocol_registry.get_protocol(&config.protocol)
        .ok_or(ApiError::BadRequest("Protocol not found".to_string()))?;
    
    let mut connection = protocol.connect(&config).await
        .map_err(|e| ApiError::InternalServerError(format!("Connection failed: {}", e)))?;
    
    // Send command
    let response = connection.send_command(&request.command).await
        .map_err(|e| ApiError::InternalServerError(format!("Command failed: {}", e)))?;
    
    Ok(ResponseJson(response))
}

// Additional handlers...
pub async fn get_device(Path(_id): Path<String>) -> ApiResult<ResponseJson<String>> {
    Ok(ResponseJson("Device details".to_string()))
}

pub async fn update_device(Path(_id): Path<String>) -> ApiResult<ResponseJson<String>> {
    Ok(ResponseJson("Device updated".to_string()))
}

pub async fn remove_device(Path(_id): Path<String>) -> ApiResult<StatusCode> {
    Ok(StatusCode::NO_CONTENT)
}
```

#### 4.3 API Application State

**File: `api/src/lib.rs`**
```rust
use std::sync::Arc;
use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;

use solar_monitor_core::ProtocolRegistry;
use solar_monitor_storage::DataStore;

pub mod routes;

pub struct AppState {
    pub storage: DataStore,
    pub protocol_registry: ProtocolRegistry,
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    NotFound(String),
    InternalServerError(String),
    DatabaseError(sqlx::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            ApiError::DatabaseError(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        };

        let body = Json(json!({
            "error": message,
            "status": status.as_u16()
        }));

        (status, body).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::InternalServerError(err.to_string())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        ApiError::DatabaseError(err)
    }
}

pub async fn create_server(storage: DataStore, protocol_registry: ProtocolRegistry) -> anyhow::Result<()> {
    let state = Arc::new(AppState {
        storage,
        protocol_registry,
    });

    let app = routes::create_router(state);
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("API server listening on http://0.0.0.0:8080");
    
    axum::serve(listener, app).await?;
    
    Ok(())
}
```

### Phase 5: Frontend Implementation (Week 3)

#### 5.1 Frontend Setup

**File: `web/package.json`**
```json
{
  "name": "solar-monitor-web",
  "private": true,
  "version": "0.0.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "preact": "^10.19.0",
    "@preact/signals": "^1.2.0",
    "wouter": "^3.0.0",
    "recharts": "^2.8.0",
    "clsx": "^2.0.0"
  },
  "devDependencies": {
    "@preact/preset-vite": "^2.7.0",
    "@types/node": "^20.0.0",
    "typescript": "^5.2.0",
    "vite": "^5.0.0",
    "tailwindcss": "^3.4.0",
    "autoprefixer": "^10.4.0",
    "postcss": "^8.4.0"
  }
}
```

#### 5.2 TypeScript Types (from Typeshare)

**File: `web/src/types/index.ts`**
```typescript
// Generated by Typeshare from Rust code

export type DeviceType = "SolarInverter" | "BatterySystem" | "ChargeController" | "EnergyMeter";

export interface DeviceMetrics {
  // Universal power metrics
  inputPowerWatts?: number;
  outputPowerWatts?: number;
  loadPercentage?: number;
  
  // Battery metrics
  batteryVoltage?: number;
  batteryCurrent?: number;
  batterySocPercentage?: number;
  batteryTemperatureCelsius?: number;
  
  // Solar metrics
  pvVoltage?: number;
  pvCurrent?: number;
  pvPowerWatts?: number;
  
  // Grid metrics
  gridVoltage?: number;
  gridFrequency?: number;
  gridPowerWatts?: number;
  
  // Device health
  deviceTemperatureCelsius?: number;
  efficiencyPercentage?: number;
  faultCodes: string[];
  operatingMode?: string;
  
  // Protocol-specific
  customMetrics: Record<string, number>;
}

export type HealthStatus = "Healthy" | "Warning" | "Critical" | "Offline";

export interface DeviceStatus {
  isConnected: boolean;
  lastSeen: string; // ISO datetime
  health: HealthStatus;
  errorMessage?: string;
}

export interface DeviceData {
  deviceId: string;
  timestamp: string; // ISO datetime
  deviceType: DeviceType;
  metrics: DeviceMetrics;
  status: DeviceStatus;
  rawData?: string;
}

export interface DeviceDto {
  id: string;
  name: string;
  deviceType: DeviceType;
  protocol: string;
  connectionParams: Record<string, string>;
  enabled: boolean;
  pollIntervalSeconds: number;
  createdAt: string;
}

export interface CommandResponse {
  success: boolean;
  data: string;
  error?: string;
  executionTimeMs: number;
}
```

#### 5.3 Universal Device Components

**File: `web/src/components/DeviceCard.tsx`**
```typescript
import { DeviceData, DeviceType, HealthStatus } from '../types';

interface DeviceCardProps {
  device: DeviceData;
}

export function DeviceCard({ device }: DeviceCardProps) {
  const healthColor = {
    Healthy: 'text-green-600 bg-green-100',
    Warning: 'text-yellow-600 bg-yellow-100', 
    Critical: 'text-red-600 bg-red-100',
    Offline: 'text-gray-600 bg-gray-100',
  }[device.status.health];

  const deviceIcon = {
    SolarInverter: '🔌',
    BatterySystem: '🔋',
    ChargeController: '⚡',
    EnergyMeter: '📊',
  }[device.deviceType];

  return (
    <div className="bg-white rounded-lg shadow p-6">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <span className="text-2xl">{deviceIcon}</span>
          <h3 className="text-lg font-semibold">{device.deviceId}</h3>
        </div>
        <span className={`px-2 py-1 rounded-full text-xs font-medium ${healthColor}`}>
          {device.status.health}
        </span>
      </div>
      
      {/* Universal metrics display */}
      <div className="grid grid-cols-2 gap-4">
        
        {/* Power metrics (all devices) */}
        {device.metrics.inputPowerWatts !== undefined && (
          <MetricItem 
            label="Input Power" 
            value={`${device.metrics.inputPowerWatts.toFixed(0)}W`} 
          />
        )}
        {device.metrics.outputPowerWatts !== undefined && (
          <MetricItem 
            label="Output Power" 
            value={`${device.metrics.outputPowerWatts.toFixed(0)}W`} 
          />
        )}
        
        {/* Battery metrics (inverters, battery systems) */}
        {device.metrics.batteryVoltage !== undefined && (
          <MetricItem 
            label="Battery Voltage" 
            value={`${device.metrics.batteryVoltage.toFixed(1)}V`} 
          />
        )}
        {device.metrics.batterySocPercentage !== undefined && (
          <MetricItem 
            label="Battery SOC" 
            value={`${device.metrics.batterySocPercentage.toFixed(0)}%`} 
          />
        )}
        
        {/* Solar metrics (inverters, charge controllers) */}
        {device.metrics.pvPowerWatts !== undefined && (
          <MetricItem 
            label="Solar Power" 
            value={`${device.metrics.pvPowerWatts.toFixed(0)}W`} 
          />
        )}
        
        {/* Grid metrics (inverters, energy meters) */}
        {device.metrics.gridVoltage !== undefined && (
          <MetricItem 
            label="Grid Voltage" 
            value={`${device.metrics.gridVoltage.toFixed(1)}V`} 
          />
        )}
      </div>
      
      {/* Connection info */}
      <div className="mt-4 pt-4 border-t text-xs text-gray-500">
        <p>Protocol: {device.deviceType}</p>
        <p>Last seen: {new Date(device.status.lastSeen).toLocaleString()}</p>
      </div>
    </div>
  );
}

function MetricItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="text-sm">
      <div className="text-gray-500">{label}</div>
      <div className="font-semibold">{value}</div>
    </div>
  );
}
```

#### 5.4 Universal Dashboard

**File: `web/src/pages/Dashboard.tsx`**
```typescript
import { useEffect, useState } from 'preact/hooks';
import { DeviceData, DeviceType } from '../types';
import { DeviceCard } from '../components/DeviceCard';

export function Dashboard() {
  const [devices, setDevices] = useState<DeviceData[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetchDashboardData();
    
    // Setup WebSocket for real-time updates
    const ws = new WebSocket('ws://localhost:8080/api/v1/ws');
    ws.onmessage = (event) => {
      const message = JSON.parse(event.data);
      if (message.type === 'device_data') {
        setDevices(prev => updateDeviceData(prev, message.data));
      }
    };

    return () => ws.close();
  }, []);

  const fetchDashboardData = async () => {
    try {
      const response = await fetch('/api/v1/data/dashboard');
      const data = await response.json();
      setDevices(data.devices || []);
    } catch (error) {
      console.error('Failed to fetch dashboard data:', error);
    } finally {
      setLoading(false);
    }
  };

  const updateDeviceData = (current: DeviceData[], newData: DeviceData): DeviceData[] => {
    const index = current.findIndex(d => d.deviceId === newData.deviceId);
    if (index >= 0) {
      const updated = [...current];
      updated[index] = newData;
      return updated;
    } else {
      return [...current, newData];
    }
  };

  // Group devices by type for organized display
  const devicesByType = devices.reduce((acc, device) => {
    const type = device.deviceType;
    if (!acc[type]) acc[type] = [];
    acc[type].push(device);
    return acc;
  }, {} as Record<DeviceType, DeviceData[]>);

  if (loading) {
    return <div className="flex justify-center items-center h-64">Loading...</div>;
  }

  return (
    <div className="p-6 max-w-6xl mx-auto">
      <h1 className="text-3xl font-bold mb-8">Universal Solar Monitor</h1>
      
      {/* System Summary */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4 mb-8">
        <SummaryCard
          title="Total Solar Power"
          value={`${getTotalSolarPower(devices).toFixed(0)}W`}
          icon="☀️"
        />
        <SummaryCard
          title="Battery Average"
          value={`${getAverageBatterySOC(devices).toFixed(0)}%`}
          icon="🔋"
        />
        <SummaryCard
          title="Active Devices"
          value={devices.filter(d => d.status.isConnected).length.toString()}
          icon="📊"
        />
        <SummaryCard
          title="System Health"
          value={getOverallHealth(devices)}
          icon="❤️"
        />
      </div>
      
      {/* Device sections by type */}
      {Object.entries(devicesByType).map(([type, typeDevices]) => (
        <div key={type} className="mb-8">
          <h2 className="text-xl font-semibold mb-4 capitalize">
            {type.replace(/([A-Z])/g, ' $1').trim()}s
          </h2>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {typeDevices.map((device) => (
              <DeviceCard key={device.deviceId} device={device} />
            ))}
          </div>
        </div>
      ))}
      
      {devices.length === 0 && (
        <div className="text-center py-12 text-gray-500">
          <p>No devices configured.</p>
          <p>Add a device to get started!</p>
        </div>
      )}
    </div>
  );
}

function SummaryCard({ title, value, icon }: { title: string; value: string; icon: string }) {
  return (
    <div className="bg-white rounded-lg shadow p-4">
      <div className="flex items-center gap-2 mb-2">
        <span className="text-2xl">{icon}</span>
        <h3 className="font-medium text-gray-600">{title}</h3>
      </div>
      <p className="text-2xl font-bold">{value}</p>
    </div>
  );
}

// Helper functions for system summary
function getTotalSolarPower(devices: DeviceData[]): number {
  return devices.reduce((total, device) => {
    return total + (device.metrics.pvPowerWatts || 0);
  }, 0);
}

function getAverageBatterySOC(devices: DeviceData[]): number {
  const batteryDevices = devices.filter(d => 
    d.metrics.batterySocPercentage !== undefined
  );
  if (batteryDevices.length === 0) return 0;
  
  const total = batteryDevices.reduce((sum, device) => 
    sum + (device.metrics.batterySocPercentage || 0), 0
  );
  return total / batteryDevices.length;
}

function getOverallHealth(devices: DeviceData[]): string {
  const healthCounts = devices.reduce((acc, device) => {
    acc[device.status.health] = (acc[device.status.health] || 0) + 1;
    return acc;
  }, {} as Record<string, number>);
  
  if (healthCounts['Critical'] > 0) return 'Critical';
  if (healthCounts['Warning'] > 0) return 'Warning';
  if (healthCounts['Healthy'] > 0) return 'Healthy';
  return 'Unknown';
}
```

#### 5.5 Device Management UI (Add/Update/Delete/Test)

State machine
- idle: viewing devices
- creating: editing form
- testing: POST /api/v1/devices/test-params
- test_success | test_error
- saving: POST /api/v1/devices
- updating: PUT /api/v1/devices/:id
- deleting: DELETE /api/v1/devices/:id

Key endpoints
- GET /api/v1/devices
- GET /api/v1/system/serial-ports
- POST /api/v1/devices/test-params
- POST /api/v1/devices
- PUT /api/v1/devices/:id
- DELETE /api/v1/devices/:id
- GET/POST export/import configs

Example: AddDeviceModal.tsx (outline)
```typescript
import { useEffect, useMemo, useState } from 'preact/hooks'

type DeviceType = 'solarInverter' | 'batterySystem' | 'chargeController' | 'energyMeter'

export function AddDeviceModal({ onClose, onSaved }: { onClose: () => void; onSaved: () => void }) {
  const [serialPorts, setSerialPorts] = useState<string[]>([])
  const [name, setName] = useState('')
  const [deviceType, setDeviceType] = useState<DeviceType>('solarInverter')
  const [protocolName] = useState('eg4-pi30-rs485')
  const [serialPort, setSerialPort] = useState('')
  const [baudRate, setBaudRate] = useState('2400')
  const [pollInterval, setPollInterval] = useState(30)
  const [testing, setTesting] = useState(false)
  const [testResult, setTestResult] = useState<string | null>(null)
  const canSave = useMemo(() => !!name && !!serialPort && !!baudRate && testResult === 'ok', [name, serialPort, baudRate, testResult])

  useEffect(() => {
    fetch('/api/v1/system/serial-ports').then(r => r.json()).then(setSerialPorts)
  }, [])

  async function onTest() {
    setTesting(true)
    setTestResult(null)
    const res = await fetch('/api/v1/devices/test-params', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        deviceType, protocolName,
        connectionParams: { serial_port: serialPort, baud_rate: baudRate }
      })
    })
    const data = await res.json()
    setTesting(false)
    setTestResult(data.ok ? 'ok' : (data.message || 'failed'))
  }

  async function onSave() {
    const res = await fetch('/api/v1/devices', {
      method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        name, deviceType, protocolName,
        pollIntervalSeconds: pollInterval,
        connectionParams: { serial_port: serialPort, baud_rate: baudRate }
      })
    })
    if (res.ok) { onSaved(); onClose(); }
  }

  return (
    <div>
      {/* form fields for name, deviceType, serialPort, baudRate, pollInterval */}
      <select value={serialPort} onChange={e => setSerialPort((e.target as HTMLSelectElement).value)}>
        <option value="" disabled>Select serial port</option>
        {serialPorts.map(p => <option key={p} value={p}>{p}</option>)}
      </select>
      <button onClick={onTest} disabled={testing}>Test</button>
      {testResult && <div>{testResult}</div>}
      <button onClick={onSave} disabled={!canSave}>Save</button>
    </div>
  )
}
```

Export/Import UI helpers
```typescript
export async function exportDevices() {
  const res = await fetch('/api/v1/devices/export')
  const data = await res.json()
  const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = 'devices-export.json'
  a.click()
  URL.revokeObjectURL(url)
}

export async function importDevices(file: File) {
  const text = await file.text()
  const res = await fetch('/api/v1/devices/import', {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: text
  })
  return await res.json() // { added, updated, skipped, errors }
}
```

### Phase 6: Main Application Assembly (Week 3)

#### 6.1 Main Binary

**File: `bin/Cargo.toml`**
```toml
[package]
name = "solar-monitor"
version = "0.1.0"
edition.workspace = true

[dependencies]
solar-monitor-core = { path = "../core" }
solar-monitor-storage = { path = "../storage" }
solar-monitor-protocols = { path = "../protocols" }
solar-monitor-api = { path = "../api" }
tokio.workspace = true
tracing.workspace = true
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow.workspace = true
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
clap = { version = "4.4", features = ["derive"] }
rust-embed = "8.0"
```

#### 6.2 Configuration Management

**File: `bin/src/config.rs`**
```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemConfig {
    pub system: SystemSettings,
    pub storage: StorageSettings,
    pub api: ApiSettings,
    pub device: Vec<DeviceConfigFile>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemSettings {
    pub bind_address: String,
    pub api_port: u16,
    pub websocket_port: u16,
    pub log_level: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageSettings {
    pub database_path: String,
    pub retention_days: u32,
    pub cleanup_interval_hours: u32,
    pub enable_aggregation: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiSettings {
    pub enable_cors: bool,
    pub max_request_size_mb: u32,
    pub rate_limit_requests_per_minute: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceConfigFile {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub device_type: String,
    pub enabled: bool,
    pub poll_interval_seconds: u32,
    pub connection_params: HashMap<String, String>,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            system: SystemSettings {
                bind_address: "0.0.0.0".to_string(),
                api_port: 8080,
                // WebSocket served on same port as API
                log_level: "info".to_string(),
            },
            storage: StorageSettings {
                database_path: "/var/lib/solar-monitor/data.db".to_string(),
                retention_days: 90,
                cleanup_interval_hours: 24,
                enable_aggregation: true,
            },
            api: ApiSettings {
                enable_cors: true,
                max_request_size_mb: 10,
                rate_limit_requests_per_minute: 100,
            },
            device: vec![
                DeviceConfigFile {
                    id: "main-inverter".to_string(),
                    name: "EG4 6000XP Inverter".to_string(),
                    protocol: "eg4-pi30-rs485".to_string(),
                    device_type: "SolarInverter".to_string(),
                    enabled: true,
                    poll_interval_seconds: 30,
                    connection_params: HashMap::from([
                        ("serial_port".to_string(), "/dev/ttyUSB0".to_string()),
                        ("baud_rate".to_string(), "2400".to_string()),
                    ]),
                },
            ],
        }
    }
}

impl SystemConfig {
    pub fn load_from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: SystemConfig = toml::from_str(&content)?;
        Ok(config)
    }
    
    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
```

#### 6.3 Device Manager

**File: `bin/src/device_manager.rs`**
```rust
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use anyhow::Result;

use solar_monitor_core::{DeviceConfig, DeviceConnection, DeviceProtocol, ProtocolRegistry};
use solar_monitor_storage::DataStore;

pub struct DeviceManager {
    protocol_registry: ProtocolRegistry,
    storage: DataStore,
    active_connections: Arc<RwLock<HashMap<String, Arc<RwLock<Box<dyn DeviceConnection>>>>>>,
    poll_tasks: Arc<RwLock<HashMap<String, JoinHandle<()>>>>,
}

impl DeviceManager {
    pub fn new(protocol_registry: ProtocolRegistry, storage: DataStore) -> Self {
        Self {
            protocol_registry,
            storage,
            active_connections: Arc::new(RwLock::new(HashMap::new())),
            poll_tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn start_device_polling(&self, configs: Vec<DeviceConfig>) -> Result<()> {
        for config in configs {
            if config.enabled {
                self.start_device_connection(config).await?;
            }
        }
        Ok(())
    }
    
    async fn start_device_connection(&self, config: DeviceConfig) -> Result<()> {
        tracing::info!("Starting device connection: {} ({})", config.name, config.protocol);
        
        // Get protocol and create connection
        let protocol = self.protocol_registry.get_protocol(&config.protocol)
            .ok_or_else(|| anyhow::anyhow!("Protocol not found: {}", config.protocol))?;
            
        let connection = protocol.connect(&config).await?;
        let connection = Arc::new(RwLock::new(connection));
        
        // Store connection
        {
            let mut connections = self.active_connections.write().await;
            connections.insert(config.id.clone(), connection.clone());
        }
        
        // Start polling task
        let poll_task = self.create_poll_task(config, connection).await;
        
        {
            let mut tasks = self.poll_tasks.write().await;
            tasks.insert(config.id.clone(), poll_task);
        }
        
        Ok(())
    }
    
    async fn create_poll_task(
        &self, 
        config: DeviceConfig, 
        connection: Arc<RwLock<Box<dyn DeviceConnection>>>
    ) -> JoinHandle<()> {
        let storage = self.storage.clone();
        let device_id = config.id.clone();
        let poll_interval = Duration::from_secs(config.poll_interval_seconds as u64);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(poll_interval);
            
            loop {
                interval.tick().await;
                
                // Read data from device
                let device_data = {
                    let mut conn = connection.write().await;
                    match conn.read_data().await {
                        Ok(data) => data,
                        Err(e) => {
                            tracing::error!("Failed to read data from {}: {}", device_id, e);
                            continue;
                        }
                    }
                };
                
                // Store data
                if let Err(e) = storage.store_device_data(&device_data).await {
                    tracing::error!("Failed to store data for {}: {}", device_id, e);
                }
                
                tracing::debug!("Collected data from {}", device_id);
            }
        })
    }
    
    pub async fn stop_all_devices(&self) {
        let mut tasks = self.poll_tasks.write().await;
        for (device_id, task) in tasks.drain() {
            tracing::info!("Stopping device polling: {}", device_id);
            task.abort();
        }
        
        let mut connections = self.active_connections.write().await;
        connections.clear();
    }
    
    pub async fn get_active_device_ids(&self) -> Vec<String> {
        let connections = self.active_connections.read().await;
        connections.keys().cloned().collect()
    }
}
```

#### 6.4 Main Application

**File: `bin/src/main.rs`**
```rust
use clap::Parser;
use anyhow::Result;
use std::path::PathBuf;

mod config;
mod device_manager;

use config::SystemConfig;
use device_manager::DeviceManager;

#[derive(Parser)]
#[command(name = "solar-monitor")]
#[command(about = "Universal Solar Monitoring System")]
struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "system.toml")]
    config: PathBuf,
    
    /// Initialize configuration file
    #[arg(long)]
    init_config: bool,
    
    /// Discover devices and exit
    #[arg(long)]
    discover: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "solar_monitor=info".into())
        )
        .init();
    
    // Handle init-config
    if cli.init_config {
        let config = SystemConfig::default();
        config.save_to_file(cli.config.to_str().unwrap())?;
        println!("Created default configuration: {}", cli.config.display());
        return Ok(());
    }
    
    // Load configuration
    let config = if cli.config.exists() {
        SystemConfig::load_from_file(cli.config.to_str().unwrap())?
    } else {
        tracing::warn!("Configuration file not found, using defaults");
        SystemConfig::default()
    };
    
    tracing::info!("Starting Universal Solar Monitor");
    tracing::info!("Configuration: {}", cli.config.display());
    
    // Initialize storage
    let storage_config = solar_monitor_storage::StorageConfig {
        database_path: config.storage.database_path,
        retention_days: config.storage.retention_days,
        cleanup_interval_hours: config.storage.cleanup_interval_hours,
        enable_aggregation: config.storage.enable_aggregation,
    };
    let storage = solar_monitor_storage::DataStore::new(storage_config).await?;
    
    // Initialize protocol registry
    let protocol_registry = solar_monitor_protocols::create_protocol_registry();
    
    // Handle device discovery
    if cli.discover {
        return run_device_discovery(protocol_registry).await;
    }
    
    // Convert and store device configurations
    let device_configs = convert_device_configs(config.device)?;
    for device_config in &device_configs {
        storage.store_device_config(device_config).await?;
    }
    
    // Start device manager
    let device_manager = DeviceManager::new(protocol_registry.clone(), storage.clone());
    device_manager.start_device_polling(device_configs).await?;
    
    // Start API server
    let api_task = tokio::spawn(async move {
        solar_monitor_api::create_server(storage, protocol_registry).await
    });
    
    // Wait for shutdown signal
    tokio::select! {
        result = api_task => {
            if let Err(e) = result? {
                tracing::error!("API server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Shutdown signal received");
        }
    }
    
    device_manager.stop_all_devices().await;
    tracing::info!("Universal Solar Monitor stopped");
    
    Ok(())
}

async fn run_device_discovery(protocol_registry: solar_monitor_core::ProtocolRegistry) -> Result<()> {
    tracing::info!("Running device discovery...");
    
    let scan_config = solar_monitor_core::ScanConfig {
        serial_ports: vec![
            "/dev/ttyUSB0".to_string(),
            "/dev/ttyUSB1".to_string(),
            "/dev/ttyAMA0".to_string(),
        ],
        can_interfaces: vec!["can0".to_string()],
        timeout_seconds: 10,
    };
    
    let discovered = protocol_registry.discover_all_devices(&scan_config).await?;
    
    if discovered.is_empty() {
        println!("No devices discovered");
    } else {
        println!("Discovered {} device(s):", discovered.len());
        for device in &discovered {
            println!("  - {} ({:?}) via {} protocol", 
                device.name, device.device_type, device.protocol);
            println!("    Connection: {:?}", device.connection_params);
        }
    }
    
    Ok(())
}

fn convert_device_configs(configs: Vec<config::DeviceConfigFile>) -> Result<Vec<solar_monitor_core::DeviceConfig>> {
    let mut device_configs = Vec::new();
    
    for config in configs {
        let device_type = match config.device_type.as_str() {
            "SolarInverter" => solar_monitor_core::DeviceType::SolarInverter,
            "BatterySystem" => solar_monitor_core::DeviceType::BatterySystem,
            "ChargeController" => solar_monitor_core::DeviceType::ChargeController,
            "EnergyMeter" => solar_monitor_core::DeviceType::EnergyMeter,
            _ => return Err(anyhow::anyhow!("Unknown device type: {}", config.device_type)),
        };
        
        device_configs.push(solar_monitor_core::DeviceConfig {
            id: config.id,
            name: config.name,
            device_type,
            protocol: config.protocol,
            connection_params: config.connection_params,
            enabled: config.enabled,
            poll_interval_seconds: config.poll_interval_seconds,
        });
    }
    
    Ok(device_configs)
}
```

#### 6.5 Example Configuration File

**File: `system.toml`**
```toml
[system]
bind_address = "0.0.0.0"
api_port = 8080
# WebSocket served on same port as API
log_level = "info"

[storage]
database_path = "/var/lib/solar-monitor/data.db"
retention_days = 90
cleanup_interval_hours = 24
enable_aggregation = true

[api]
enable_cors = true
max_request_size_mb = 10
rate_limit_requests_per_minute = 100

# EG4 6000XP via RS485 adapter
[[device]]
id = "main-inverter"
name = "EG4 6000XP Inverter"
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

# Future device example (commented out)
# [[device]]
# id = "battery-bank-1"
# name = "LiFePO4 Battery Bank"
# protocol = "modbus-rtu"
# device_type = "BatterySystem"
# enabled = true
# poll_interval_seconds = 60
# 
# [device.connection_params]
# serial_port = "/dev/ttyUSB1"
# baud_rate = "9600"
# unit_id = "1"
```

### Phase 7: Testing and Deployment (Week 4)

#### 7.1 Testing Commands

```bash
# Development setup
cargo build

# Run with discovery to find EG4 device
cargo run -- --discover

# Initialize default config
cargo run -- --init-config

# Run with custom config
cargo run --config /path/to/system.toml

# Frontend development
cd web && npm run dev

# Test EG4 protocol specifically
cargo test -p solar-monitor-protocols eg4

# Integration tests
cargo test --workspace
```

#### 7.2 Production Deployment

```bash
# Build release binary
cargo build --release

# Install systemd service
sudo cp target/release/solar-monitor /usr/local/bin/
sudo cp docs/solar-monitor.service /etc/systemd/system/
sudo systemctl enable solar-monitor
sudo systemctl start solar-monitor
```

This comprehensive implementation breakdown provides a complete path from universal architecture to working EG4 RS485 monitoring system, with clear extensibility for additional protocols and device types.
