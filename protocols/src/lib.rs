//! Protocols registry and EG4 RS485 stub

use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use solar_monitor_core as core;
use solar_monitor_core::{
    DeviceConnection, DeviceConfig, DeviceData, DeviceMetrics, DeviceProtocol, DeviceStatus,
    DeviceType, DiscoveredDevice, HealthStatus, ProtocolCapabilities, ProtocolMetadata, ScanConfig,
};

pub fn registered() -> &'static [&'static str] { &["eg4-pi30-rs485"] }

pub fn create_registry() -> core::ProtocolRegistry {
    let mut reg = core::ProtocolRegistry::new();
    reg.register_protocol(Arc::new(Eg4Pi30Rs485));
    reg
}

pub struct Eg4Pi30Rs485;

#[async_trait]
impl DeviceProtocol for Eg4Pi30Rs485 {
    fn protocol_name(&self) -> &'static str { "eg4-pi30-rs485" }

    fn metadata(&self) -> ProtocolMetadata {
        ProtocolMetadata {
            name: "EG4/PI30 RS485",
            version: "0.1.0",
            description: "EG4 6000XP inverter via RS485 using PI30",
            supported_device_types: &[DeviceType::SolarInverter],
            capabilities: ProtocolCapabilities {
                supports_discovery: true,
                supports_commands: true,
                supports_real_time: true,
                max_concurrent_connections: Some(1),
            },
        }
    }

    fn supported_device_types(&self) -> Vec<DeviceType> { vec![DeviceType::SolarInverter] }

    async fn discover_devices(&self, scan: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        // Stub: return empty or a placeholder if desired
        let mut out = Vec::new();
        for port in &scan.serial_ports {
            // Placeholder discovered entry (connectivity test would go here)
            out.push(DiscoveredDevice {
                id: format!("eg4-{}", port),
                name: format!("EG4 6000XP on {}", port),
                device_type: DeviceType::SolarInverter,
                protocol: "eg4-pi30-rs485".to_string(),
                connection_params: HashMap::from([
                    ("serial_port".to_string(), port.clone()),
                    ("baud_rate".to_string(), "2400".to_string()),
                ]),
            });
        }
        Ok(out)
    }

    async fn connect(&self, _config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>> {
        Ok(Box::new(Eg4Conn { connected: true }))
    }
}

struct Eg4Conn { connected: bool }

#[async_trait]
impl DeviceConnection for Eg4Conn {
    async fn read_data(&mut self) -> Result<DeviceData> {
        // Minimal, fake data point for scaffolding
        let now = chrono::Utc::now();
        Ok(DeviceData {
            device_id: "stub".to_string(),
            timestamp: now,
            device_type: DeviceType::SolarInverter,
            metrics: DeviceMetrics { pv_power_watts: Some(0.0), ..Default::default() },
            status: DeviceStatus { is_connected: self.connected, last_seen: now, health: HealthStatus::Healthy, error_message: None },
            raw_data: None,
        })
    }

    async fn send_command(&mut self, _command: &str) -> Result<String> { Ok("OK".to_string()) }
    fn is_connected(&self) -> bool { self.connected }
    async fn health_check(&mut self) -> Result<()> { Ok(()) }
}

