# Protocol System - Device-Agnostic Edge Specification

## Overview
The protocol system provides a device-agnostic architecture for supporting multiple solar device types (inverters, batteries, charge controllers) through compile-time protocol modules. Designed for edge device deployment with minimal resource usage and zero dynamic loading overhead.

## Core Architecture

### 1. Protocol Registry (Compile-Time)
```rust
use std::collections::HashMap;
use std::sync::Arc;
use async_trait::async_trait;

pub struct ProtocolRegistry {
    /// Compile-time registered protocols (no dynamic loading)
    protocols: HashMap<&'static str, Arc<dyn DeviceProtocol>>,
    
    /// Protocol metadata and capabilities
    metadata: HashMap<&'static str, ProtocolMetadata>,
    
    /// Device discovery cache
    discovered_devices: Arc<RwLock<HashMap<String, DiscoveredDevice>>>,
}

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

impl ProtocolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            protocols: HashMap::new(),
            metadata: HashMap::new(),
            discovered_devices: Arc::new(RwLock::new(HashMap::new())),
        };
        
        // Register compile-time protocols
        registry.register_protocol(Arc::new(EG4Protocol::new()));
        // registry.register_protocol(Arc::new(ModbusProtocol::new()));
        // registry.register_protocol(Arc::new(CANProtocol::new()));
        
        registry
    }
    
    fn register_protocol(&mut self, protocol: Arc<dyn DeviceProtocol>) {
        let name = protocol.protocol_name();
        let metadata = protocol.metadata();
        
        self.protocols.insert(name, protocol);
        self.metadata.insert(name, metadata);
        
        tracing::info!("Registered protocol: {}", name);
    }

    pub fn get_protocol(&self, name: &str) -> Result<&dyn DeviceProtocol> {
        self.protocols.get(name)
            .map(|p| p.as_ref())
            .ok_or_else(|| ProtocolError::NotFound(name.to_string()))
    }
    
    pub fn list_protocols(&self) -> Vec<&ProtocolMetadata> {
        self.metadata.values().collect()
    }
    
    pub async fn discover_all_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        let mut all_devices = Vec::new();
        
        for protocol in self.protocols.values() {
            match protocol.discover_devices(scan_config).await {
                Ok(mut devices) => all_devices.append(&mut devices),
                Err(e) => tracing::warn!("Discovery failed for {}: {}", protocol.protocol_name(), e),
            }
        }
        
        // Cache discovered devices
        let mut cache = self.discovered_devices.write().await;
        for device in &all_devices {
            cache.insert(device.id.clone(), device.clone());
        }
        
        Ok(all_devices)
    }

}
```

### 2. Device-Agnostic Protocol Interface
```rust
// Universal device protocol interface (matches Core Engine)
#[async_trait]
pub trait DeviceProtocol: Send + Sync {
    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>>;
    async fn discover_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>>;
    fn supported_device_types(&self) -> Vec<DeviceType>;
    fn protocol_name(&self) -> &'static str;
    fn metadata(&self) -> ProtocolMetadata;
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

// Data types used by protocols
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub protocol: String,
    pub connection_params: HashMap<String, String>,
}
```

### 3. EG4/PI30 Protocol Implementation (RS485 Reference)
```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

pub struct EG4Protocol;

impl EG4Protocol { pub fn new() -> Self { Self } }

#[async_trait]
impl DeviceProtocol for EG4Protocol {
    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>> {
        let serial_port = config.connection_params.get("serial_port")
            .ok_or_else(|| anyhow::anyhow!("Missing serial_port parameter"))?;
        let baud_rate: u32 = config.connection_params.get("baud_rate")
            .unwrap_or(&"2400".to_string()).parse().unwrap_or(2400);
        let connection = EG4SerialConnection::new(config.clone(), serial_port, baud_rate).await?;
        Ok(Box::new(connection))
    }

    fn supported_device_types(&self) -> Vec<DeviceType> { vec![DeviceType::SolarInverter] }
    fn protocol_name(&self) -> &'static str { "eg4-pi30-rs485" }
    fn metadata(&self) -> ProtocolMetadata { /* as above */ unimplemented!() }

    async fn discover_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        let mut devices = Vec::new();
        for port in &scan_config.serial_ports {
            if let Ok(device) = self.test_eg4_serial_device(port).await { devices.push(device); }
        }
        Ok(devices)
    }
}
```

### 4. Future Protocol Examples (Device-Agnostic Framework)

#### ModBus TCP Protocol (for Battery Systems)
```rust
pub struct ModbusProtocol;

#[async_trait]
impl DeviceProtocol for ModbusProtocol {
    fn supported_device_types(&self) -> Vec<DeviceType> {
        vec![DeviceType::BatterySystem, DeviceType::EnergyMeter]
    }
    
    fn protocol_name(&self) -> &'static str {
        "modbus-tcp"
    }
    
    fn metadata(&self) -> ProtocolMetadata {
        ProtocolMetadata {
            name: "Modbus TCP",
            version: "1.0.0",
            description: "Generic Modbus TCP for battery systems and energy meters",
            supported_device_types: &[DeviceType::BatterySystem, DeviceType::EnergyMeter],
            capabilities: ProtocolCapabilities {
                supports_discovery: true,
                supports_commands: true, 
                supports_real_time: true,
                max_concurrent_connections: Some(50),
            },
        }
    }
    
    async fn discover_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        // Scan for Modbus devices (batteries, energy meters)
        let mut devices = Vec::new();
        
        for ip_range in &scan_config.ip_ranges {
            for port in &[502, 1502, 4196] { // Common Modbus ports
                if let Ok(device) = self.test_modbus_device(ip_range, *port).await {
                    devices.push(device);
                }
            }
        }
        
        Ok(devices)
    }
    
    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>> {
        let host = config.connection_params.get("host").unwrap();
        let port: u16 = config.connection_params.get("port").unwrap().parse()?;
        let unit_id: u8 = config.connection_params.get("unit_id")
            .unwrap_or(&"1".to_string()).parse()?;
            
        let connection = ModbusConnection::new(config.clone(), host.clone(), port, unit_id).await?;
        Ok(Box::new(connection))
    }
}

// ModbusConnection would implement DeviceConnection to read battery/meter data
// and map it to the universal DeviceMetrics structure
```

#### CAN Bus Protocol (for Advanced Battery Systems)  
```rust
pub struct CANProtocol;

#[async_trait]
impl DeviceProtocol for CANProtocol {
    fn supported_device_types(&self) -> Vec<DeviceType> {
        vec![DeviceType::BatterySystem, DeviceType::ChargeController]
    }
    
    fn protocol_name(&self) -> &'static str {
        "can-bus"
    }
    
    async fn discover_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        // Scan CAN interfaces for battery management systems
        let mut devices = Vec::new();
        
        for interface in &scan_config.can_interfaces {
            if let Ok(mut discovered) = self.scan_can_interface(interface).await {
                devices.append(&mut discovered);
            }
        }
        
        Ok(devices)
    }
    
    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>> {
        let interface = config.connection_params.get("interface").unwrap();
        let can_id: u32 = config.connection_params.get("can_id").unwrap().parse()?;
        
        let connection = CANConnection::new(config.clone(), interface.clone(), can_id).await?;
        Ok(Box::new(connection))
    }
}
```

### 5. Device-Agnostic Configuration Examples

#### Multi-Device Configuration
```toml
# system.toml - Device-agnostic configuration showing EG4 + future battery system

[system]
bind_address = "0.0.0.0"
api_port = 8080
# WebSocket served on same port as API

# Device configurations (device-agnostic)
[[device]]
id = "main-inverter"
name = "Main EG4 6000XP Inverter"
protocol = "eg4-pi30-rs485"
device_type = "SolarInverter"
enabled = true
poll_interval_seconds = 30

[device.connection_params]
serial_port = "/dev/ttyUSB0"
baud_rate = "2400"

[[device]]
id = "battery-bank-1"
name = "LiFePO4 Battery Bank"
protocol = "modbus-tcp"  # Future addition
device_type = "BatterySystem"
enabled = true
poll_interval_seconds = 60

[device.connection_params]
host = "192.168.1.101"
port = "502"
unit_id = "1"

[[device]]
id = "solar-charge-controller"
name = "MPPT Charge Controller"
protocol = "can-bus"    # Future addition
device_type = "ChargeController"  
enabled = true
poll_interval_seconds = 45

[device.connection_params]
interface = "can0"
can_id = "0x123"

[[device]]
id = "energy-meter"
name = "Grid Tie Energy Meter"
protocol = "modbus-tcp"  # Future addition
device_type = "EnergyMeter"
enabled = true
poll_interval_seconds = 30

[device.connection_params]
host = "192.168.1.102"
port = "502"
unit_id = "2"
```

### 6. Universal Device Discovery
```rust
// Example showing device-agnostic discovery across all protocols
pub async fn discover_all_solar_devices() -> Result<Vec<DiscoveredDevice>> {
    let registry = ProtocolRegistry::new();
    
    let scan_config = ScanConfig {
        ip_ranges: vec!["192.168.1.0/24".to_string()],
        can_interfaces: vec!["can0".to_string()],
        serial_ports: vec!["/dev/ttyUSB0".to_string()],
        timeout_seconds: 10,
    };
    
    // Discover devices across ALL protocols
    let discovered = registry.discover_all_devices(&scan_config).await?;
    
    // Results include mixed device types:
    // - EG4 6000XP inverters (via PI30)
    // - Battery systems (via Modbus)  
    // - Charge controllers (via CAN)
    // - Energy meters (via Modbus)
    
    for device in &discovered {
        println!("Found {:?} - {} using {} protocol", 
                 device.device_type, device.name, device.protocol);
    }
    
    Ok(discovered)
}
```

### 7. Protocol Implementation Template
```rust
// Template for adding new protocols to the system
pub struct NewProtocol;

impl NewProtocol {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DeviceProtocol for NewProtocol {
    fn protocol_name(&self) -> &'static str {
        "new-protocol"
    }
    
    fn supported_device_types(&self) -> Vec<DeviceType> {
        vec![DeviceType::SolarInverter] // or other types
    }
    
    fn metadata(&self) -> ProtocolMetadata {
        ProtocolMetadata {
            name: "New Protocol",
            version: "1.0.0",
            description: "Description of protocol support",
            supported_device_types: &[DeviceType::SolarInverter],
            capabilities: ProtocolCapabilities {
                supports_discovery: true,
                supports_commands: false,
                supports_real_time: true,
                max_concurrent_connections: Some(20),
            },
        }
    }
    
    async fn discover_devices(&self, scan_config: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        // Implement device discovery logic
        let devices = Vec::new();
        Ok(devices)
    }
    
    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>> {
        // Create protocol-specific connection
        let connection = NewProtocolConnection::new(config).await?;
        Ok(Box::new(connection))
    }
}

// To register: Add Arc::new(NewProtocol::new()) to ProtocolRegistry::new()
```

This protocol system specification provides a device-agnostic, extensible architecture for supporting multiple solar device types while maintaining edge device constraints and compile-time safety. The EG4 6000XP support serves as the reference implementation, with clear patterns for adding Modbus, CAN, and other protocols to support batteries, charge controllers, and energy meters.
