//! LuxPower-map Modbus RTU inverters (EG4 6000XP family).
//!
//! The transport (Modbus RTU port actor) is shared; each model contributes a
//! [`ModelDef`] — protocol id, bauds, curated settings table, and a live-data
//! decode function. Adding another LuxPower-map model (18kPV, 12000XP, …) is
//! a new file in `models/` plus one line in `create_registry()`.
//!
//! Note: LuxPower models are wire-identical during discovery; once more than
//! one model is registered, discovery will need to read a model-identifying
//! holding register to tell them apart.

pub mod models;
pub mod settings;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use solar_monitor_core::{
    DeviceConfig, DeviceConnection, DeviceData, DeviceMetrics, DeviceProtocol, DeviceStatus,
    DeviceType, DiscoveredDevice, HealthStatus, ProtocolCapabilities, ProtocolMetadata, ScanConfig,
    SettingsAccess,
};

use crate::transport::modbus_rtu::{PortHandle, get_or_spawn_port_actor};
use settings::{LuxPowerSettings, SettingDef};

/// Five aligned 40-register input blocks (regs 0-199), as little-endian bytes
/// per register — the layout the decode functions were written against.
pub type InputBlocks = [Vec<u8>; 5];

pub struct ModelDef {
    /// Stable protocol id used in device configs (e.g. "eg4-6000xp-modbus").
    pub protocol_name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub default_baud: u32,
    pub discovery_bauds: &'static [u32],
    pub settings: &'static [SettingDef],
    /// Setting keys that are disruptive to write (e.g. cut output power);
    /// surfaced to the UI as a required confirmation.
    pub confirm_keys: &'static [&'static str],
    pub decode_metrics: fn(&InputBlocks) -> DeviceMetrics,
}

pub struct LuxPowerProtocol {
    pub model: &'static ModelDef,
}

/// Connection parameters shared by connect/discovery/settings so all three
/// agree on defaults.
struct PortParams {
    path: String,
    baud: u32,
    timeout_secs: u64,
    unit_id: u8,
}

fn port_params(config: &DeviceConfig, model: &ModelDef) -> Result<PortParams> {
    let path = config
        .connection_params
        .get("serial_port")
        .ok_or_else(|| anyhow!("Missing serial_port parameter"))?
        .clone();
    let baud = config
        .connection_params
        .get("baud_rate")
        .and_then(|s| s.parse().ok())
        .unwrap_or(model.default_baud);
    let timeout_secs = config
        .connection_params
        .get("timeout_seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let unit_id = config
        .connection_params
        .get("unit_id")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    Ok(PortParams {
        path,
        baud,
        timeout_secs,
        unit_id,
    })
}

#[async_trait]
impl DeviceProtocol for LuxPowerProtocol {
    fn protocol_name(&self) -> &'static str {
        self.model.protocol_name
    }

    fn metadata(&self) -> ProtocolMetadata {
        ProtocolMetadata {
            protocol_name: self.model.protocol_name,
            name: self.model.display_name,
            version: "0.1.0",
            description: self.model.description,
            supported_device_types: &[DeviceType::SolarInverter],
            capabilities: ProtocolCapabilities {
                supports_discovery: true,
                supports_settings: true,
                supports_real_time: true,
                max_concurrent_connections: Some(1),
            },
        }
    }

    fn supported_device_types(&self) -> Vec<DeviceType> {
        vec![DeviceType::SolarInverter]
    }

    async fn discover_devices(&self, scan: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        let mut out = Vec::new();
        for port in &scan.serial_ports {
            'port: for &baud in self.model.discovery_bauds {
                for unit_id in 1u8..=3u8 {
                    if try_read_basic_modbus(port, baud, unit_id, scan.timeout_seconds as u64)
                        .await
                        .is_ok()
                    {
                        out.push(DiscoveredDevice {
                            id: format!(
                                "{}-{}-{}",
                                self.model.protocol_name,
                                port.replace('/', "_"),
                                unit_id
                            ),
                            name: format!(
                                "{} on {} (id {})",
                                self.model.display_name, port, unit_id
                            ),
                            device_type: DeviceType::SolarInverter,
                            protocol: self.model.protocol_name.to_string(),
                            connection_params: HashMap::from([
                                ("serial_port".to_string(), port.clone()),
                                ("baud_rate".to_string(), baud.to_string()),
                                ("data_bits".to_string(), "8".to_string()),
                                ("parity".to_string(), "none".to_string()),
                                ("stop_bits".to_string(), "1".to_string()),
                                ("unit_id".to_string(), unit_id.to_string()),
                            ]),
                        });
                        break 'port;
                    }
                }
            }
        }
        Ok(out)
    }

    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>> {
        let p = port_params(config, self.model)?;
        let handle = get_or_spawn_port_actor(&p.path, p.baud, p.timeout_secs).await?;
        Ok(Box::new(LuxPowerConn {
            device_id: config.id.clone(),
            unit_id: p.unit_id,
            handle,
            model: self.model,
        }))
    }

    async fn settings(&self, config: &DeviceConfig) -> Result<Option<Box<dyn SettingsAccess>>> {
        let p = port_params(config, self.model)?;
        let handle = get_or_spawn_port_actor(&p.path, p.baud, p.timeout_secs).await?;
        Ok(Some(Box::new(LuxPowerSettings {
            handle,
            unit_id: p.unit_id,
            table: self.model.settings,
            confirm_keys: self.model.confirm_keys,
        })))
    }
}

async fn try_read_basic_modbus(
    path: &str,
    baud: u32,
    unit_id: u8,
    timeout_secs: u64,
) -> Result<()> {
    let handle = get_or_spawn_port_actor(path, baud, timeout_secs).await?;
    let _ = handle.read_input_registers(unit_id, 0, 2).await?;
    Ok(())
}

struct LuxPowerConn {
    device_id: String,
    unit_id: u8,
    handle: Arc<PortHandle>,
    model: &'static ModelDef,
}

impl LuxPowerConn {
    async fn read_input_block(&self, addr: u16, qty: u16) -> Result<Vec<u8>> {
        let regs = self
            .handle
            .read_input_registers(self.unit_id, addr, qty)
            .await?;
        let mut out = Vec::with_capacity(regs.len() * 2);
        for r in regs {
            out.extend_from_slice(&r.to_le_bytes());
        }
        Ok(out)
    }
}

#[async_trait]
impl DeviceConnection for LuxPowerConn {
    async fn read_data(&mut self) -> Result<DeviceData> {
        let blocks: InputBlocks = [
            self.read_input_block(0, 40).await?,
            self.read_input_block(40, 40).await?,
            self.read_input_block(80, 40).await?,
            self.read_input_block(120, 40).await?,
            self.read_input_block(160, 40).await?,
        ];
        let metrics = (self.model.decode_metrics)(&blocks);

        let now = chrono::Utc::now();
        let status = DeviceStatus {
            is_connected: true,
            last_seen: now,
            health: HealthStatus::Healthy,
            error_message: None,
        };
        Ok(DeviceData {
            device_id: self.device_id.clone(),
            timestamp: now,
            device_type: DeviceType::SolarInverter,
            metrics,
            status,
            raw_data: None,
        })
    }

    fn is_connected(&self) -> bool {
        true
    }

    async fn health_check(&mut self) -> Result<()> {
        Ok(())
    }
}
