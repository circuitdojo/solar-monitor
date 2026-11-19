//! Dev-only synthetic inverter (`--features mock`), for working on the UI —
//! notably the combined parallel view — without hardware. Never part of a
//! release or Pi build.
//!
//! Readings are a deterministic function of wall-clock time and the
//! `unit_id` connection param, shaped like the 6000XP decoder's output (same
//! metric keys) so the dashboard renders it as a real inverter. Several mock
//! devices model one parallel system: PV differs per unit, the load is
//! split evenly, and the battery bank and grid are shared (same SOC, voltage
//! and frequency on every unit). Reuses the LuxPower parallel combiner.
//!
//! Settings run the real 6000XP table and settings engine against an
//! in-memory holding-register bank per device (seeded to in-range values,
//! lost on restart), so the Settings page behaves as it would on hardware.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use chrono::{Timelike, Utc};

use crate::luxpower::models::EG4_6000XP;
use crate::luxpower::settings::{HoldingRegisters, Kind, LuxPowerSettings};

use solar_monitor_core::{
    DeviceConfig, DeviceConnection, DeviceData, DeviceMetrics, DeviceProtocol, DeviceStatus,
    DeviceType, DiscoveredDevice, HealthStatus, ParallelCombiner, ProtocolCapabilities,
    ProtocolMetadata, ScanConfig, SettingsAccess,
};

pub const PROTOCOL_NAME: &str = "mock-6000xp";

#[derive(Default)]
pub struct MockProtocol {
    /// Device id → its simulated holding registers.
    banks: Mutex<HashMap<String, Arc<MemRegisters>>>,
}

/// In-memory holding registers, seeded so every table setting reads back
/// in range: numbers at the midpoint of their documented range, choices at
/// their first option, bits clear, time windows 00:00.
struct MemRegisters(Mutex<HashMap<u16, u16>>);

impl MemRegisters {
    fn seeded() -> Self {
        let mut regs: HashMap<u16, u16> = HashMap::new();
        for def in EG4_6000XP.settings {
            match &def.kind {
                Kind::Number {
                    reg,
                    scale,
                    min,
                    max,
                    step,
                    ..
                } => {
                    let mid = ((min + max) / 2.0 / step).round() * step;
                    regs.insert(*reg, (mid / scale).round() as u16);
                }
                Kind::Choice { reg, options, .. } => {
                    regs.insert(*reg, options[0]);
                }
                Kind::Bits {
                    reg,
                    shift,
                    options,
                    ..
                } => {
                    *regs.entry(*reg).or_default() |= options[0] << shift;
                }
                Kind::Bit { .. } | Kind::BitChoice { .. } | Kind::TimeWindow { .. } => {}
            }
        }
        Self(Mutex::new(regs))
    }
}

#[async_trait]
impl HoldingRegisters for MemRegisters {
    async fn read(&self, start: u16, qty: u16) -> Result<Vec<u16>> {
        let regs = self.0.lock().unwrap();
        Ok((start..start + qty)
            .map(|r| regs.get(&r).copied().unwrap_or(0))
            .collect())
    }
    async fn write(&self, reg: u16, value: u16) -> Result<()> {
        self.0.lock().unwrap().insert(reg, value);
        Ok(())
    }
}

#[async_trait]
impl DeviceProtocol for MockProtocol {
    fn protocol_name(&self) -> &'static str {
        PROTOCOL_NAME
    }

    fn metadata(&self) -> ProtocolMetadata {
        ProtocolMetadata {
            protocol_name: PROTOCOL_NAME,
            name: "Mock 6000XP (dev)",
            version: "1.0",
            description: "Synthetic readings for UI development; unit_id selects the unit",
            supported_device_types: &[DeviceType::SolarInverter],
            capabilities: ProtocolCapabilities {
                supports_discovery: false,
                supports_settings: true,
                supports_real_time: true,
                max_concurrent_connections: None,
            },
        }
    }

    fn supported_device_types(&self) -> Vec<DeviceType> {
        vec![DeviceType::SolarInverter]
    }

    async fn discover_devices(&self, _scan: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        Ok(Vec::new())
    }

    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>> {
        let unit: u8 = match config.connection_params.get("unit_id") {
            Some(s) => s.parse()?,
            None => 1,
        };
        Ok(Box::new(MockConn {
            device_id: config.id.clone(),
            unit,
        }))
    }

    async fn settings(&self, config: &DeviceConfig) -> Result<Option<Box<dyn SettingsAccess>>> {
        let bank = self
            .banks
            .lock()
            .unwrap()
            .entry(config.id.clone())
            .or_insert_with(|| Arc::new(MemRegisters::seeded()))
            .clone();
        Ok(Some(Box::new(LuxPowerSettings {
            regs: bank,
            table: EG4_6000XP.settings,
            confirm_keys: EG4_6000XP.confirm_keys,
        })))
    }

    fn parallel_combiner(&self) -> Option<ParallelCombiner> {
        Some(crate::luxpower::parallel::combine)
    }
}

struct MockConn {
    device_id: String,
    unit: u8,
}

/// Units in the simulated parallel system (`parallel_count`); load is split
/// across this many.
const UNITS: f64 = 2.0;

fn metrics(unit: u8, secs: f64) -> DeviceMetrics {
    let tau = std::f64::consts::TAU;
    // Fast cycles so changes are visible within minutes of watching.
    let wave = |period: f64, phase: f64| (tau * secs / period + phase).sin();
    let u = unit as f64;

    // PV: each unit has its own array (different size and a little cloud jitter).
    let pv_peak = 2500.0 + 1500.0 * (u - 1.0);
    let pv = (pv_peak * (0.6 + 0.4 * wave(600.0, u))).max(0.0).round();
    let pv1 = (pv * 0.55).round();
    let pv2 = pv - pv1;

    // Shared house load, split evenly across units.
    let load_total = 3000.0 + 1500.0 * wave(240.0, 0.0);
    let load = (load_total / UNITS).round();

    // Battery takes this unit's surplus, grid covers what's left (no export).
    let net = pv - load;
    let (charge, discharge) = if net >= 0.0 { (net, 0.0) } else { (0.0, -net) };
    let import = (load_total / UNITS * 0.1 * (1.0 + wave(300.0, 1.0))).round();

    // Shared bank and grid: identical on every unit.
    let soc = (60.0 + 25.0 * wave(1800.0, 0.0)).round();
    let vbat = 51.2 + soc / 100.0 * 2.4;
    let grid_v = 240.0 + 2.0 * wave(90.0, 0.0);
    let grid_f = 60.0 + 0.02 * wave(45.0, 0.0);

    // Daily energy: ramps from midnight UTC, per-unit share.
    let day_h = (secs % 86_400.0) / 3600.0;
    let day = |kw_avg: f64| (kw_avg * day_h * 10.0).round() / 10.0;

    let mut m = DeviceMetrics {
        pv_voltage: Some(380.0 + 5.0 * u),
        pv_power_watts: Some(pv),
        output_power_watts: Some(load),
        battery_voltage: Some(vbat),
        battery_current: Some((charge - discharge) / vbat),
        battery_soc_percentage: Some(soc),
        grid_voltage: Some(grid_v),
        grid_frequency: Some(grid_f),
        device_temperature_celsius: Some(38.0 + 2.0 * u),
        ..Default::default()
    };
    for (k, v) in [
        ("pv1_power", pv1),
        ("pv2_power", pv2),
        ("pv3_power", 0.0),
        ("pv1_voltage", 380.0 + 5.0 * u),
        ("pv2_voltage", 370.0 + 5.0 * u),
        ("battery_charge_power", charge),
        ("battery_discharge_power", discharge),
        ("import_power", import),
        ("export_power", 0.0),
        ("pv1_day_kwh", day(pv_peak * 0.55 / 3000.0)),
        ("pv2_day_kwh", day(pv_peak * 0.45 / 3000.0)),
        ("pv3_day_kwh", 0.0),
        ("load_day_kwh", day(3.0 / UNITS)),
        ("charge_day_kwh", day(0.4 * u)),
        ("discharge_day_kwh", day(0.3)),
        ("import_day_kwh", day(0.15)),
        ("export_day_kwh", 0.0),
        ("battery_soh", 99.0),
        ("battery_temp_c", 24.0),
        ("heatsink_temp1_c", 41.0 + 3.0 * u),
        ("heatsink_temp2_c", 39.0 + 3.0 * u),
        ("ac_input_is_generator", 0.0),
        ("parallel_role", if unit == 1 { 1.0 } else { 2.0 }),
        ("parallel_count", UNITS),
    ] {
        m.custom_metrics.insert(k.into(), v);
    }
    m
}

#[async_trait]
impl DeviceConnection for MockConn {
    async fn read_data(&mut self) -> Result<DeviceData> {
        let now = Utc::now();
        let secs = now.timestamp() as f64 + now.nanosecond() as f64 / 1e9;
        Ok(DeviceData {
            device_id: self.device_id.clone(),
            timestamp: now,
            device_type: DeviceType::SolarInverter,
            metrics: metrics(self.unit, secs),
            status: DeviceStatus {
                is_connected: true,
                last_seen: now,
                health: HealthStatus::Healthy,
                error_message: None,
            },
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

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::SettingValueDto;

    fn cfg(id: &str) -> DeviceConfig {
        DeviceConfig {
            id: id.into(),
            name: id.into(),
            device_type: DeviceType::SolarInverter,
            protocol: PROTOCOL_NAME.into(),
            connection_params: HashMap::new(),
            enabled: true,
            poll_interval_seconds: 5,
        }
    }

    #[tokio::test]
    async fn seeded_settings_in_range_and_writes_persist_per_device() {
        let proto = MockProtocol::default();
        let s = proto.settings(&cfg("a")).await.unwrap().unwrap();
        let all = s.read_settings().await.unwrap();
        assert_eq!(all.len(), EG4_6000XP.settings.len());
        let mut number_key = None;
        for d in &all {
            match &d.setting {
                SettingValueDto::Number {
                    value, min, max, ..
                } => {
                    assert!(value >= min && value <= max, "{} = {value}", d.key);
                    number_key.get_or_insert((d.key.clone(), *min));
                }
                SettingValueDto::Choice { value, options, .. } => {
                    assert!(options.contains(value), "{} = {value}", d.key);
                }
                _ => {}
            }
        }

        // A write sticks across settings() calls for the same device only.
        let (key, min) = number_key.unwrap();
        s.write_setting(&key, &min.to_string()).await.unwrap();
        let value_of = |all: Vec<contracts::DeviceSettingDto>| match all
            .into_iter()
            .find(|d| d.key == key)
            .unwrap()
            .setting
        {
            SettingValueDto::Number { value, .. } => value,
            _ => unreachable!(),
        };
        let again = proto.settings(&cfg("a")).await.unwrap().unwrap();
        assert_eq!(value_of(again.read_settings().await.unwrap()), min);
        let other = proto.settings(&cfg("b")).await.unwrap().unwrap();
        assert_ne!(value_of(other.read_settings().await.unwrap()), min);
    }
}
