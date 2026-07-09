//! Core engine: traits, registry, and types re-exports

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

pub use contracts::{DeviceData, DeviceMetrics, DeviceStatus, DeviceType, HealthStatus};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
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

#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub serial_ports: Vec<String>,
    pub timeout_seconds: u32,
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
pub struct DeviceConfig {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub protocol: String,
    pub connection_params: HashMap<String, String>,
    pub enabled: bool,
    pub poll_interval_seconds: u32,
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
    async fn send_command(&mut self, command: &str) -> Result<String>;
    fn is_connected(&self) -> bool;
    async fn health_check(&mut self) -> Result<()>;
}

pub struct ProtocolRegistry {
    protocols: HashMap<&'static str, Arc<dyn DeviceProtocol>>,
    metadata: HashMap<&'static str, ProtocolMetadata>,
}

impl ProtocolRegistry {
    pub fn new() -> Self {
        Self {
            protocols: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn register_protocol(&mut self, proto: Arc<dyn DeviceProtocol>) {
        let name = proto.protocol_name();
        self.metadata.insert(name, proto.metadata());
        self.protocols.insert(name, proto);
    }

    pub fn get_protocol(&self, name: &str) -> Option<&dyn DeviceProtocol> {
        self.protocols.get(name).map(|p| p.as_ref())
    }

    pub fn list_protocols(&self) -> Vec<&ProtocolMetadata> {
        self.metadata.values().collect()
    }
}
