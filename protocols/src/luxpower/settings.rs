//! Curated, validated settings backed by the LuxPower hold-register map.
//!
//! Each model contributes a static table of [`SettingDef`]s (register, scale,
//! documented range per setting). Every write is range-checked against the
//! documented limits and read back to confirm. Raw register writes are never
//! exposed.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::sync::Arc;

use crate::transport::modbus_rtu::PortHandle;

pub enum Kind {
    /// Whole-register numeric value: raw * scale, clamped to [min, max] (scaled units).
    Number {
        reg: u16,
        scale: f64,
        min: f64,
        max: f64,
        step: f64,
        unit: &'static str,
    },
    /// Single bit of a bit-field register (read-modify-write), shown as a toggle.
    Bit { reg: u16, bit: u8 },
    /// Single bit of a bit-field register, shown as a labeled 0/1 choice.
    BitChoice {
        reg: u16,
        bit: u8,
        labels: [&'static str; 2],
    },
    /// Register restricted to an enumerated set of raw values.
    Choice {
        reg: u16,
        options: &'static [u16],
        unit: &'static str,
    },
    /// Two registers, each packing hour (low byte) and minute (high byte).
    TimeWindow { start_reg: u16, end_reg: u16 },
}

pub struct SettingDef {
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub kind: Kind,
}

fn fmt_time(reg_val: u16) -> String {
    format!("{:02}:{:02}", reg_val & 0xFF, reg_val >> 8)
}

fn parse_time(s: &str) -> Result<u16> {
    let (h, m) = s
        .split_once(':')
        .ok_or_else(|| anyhow!("time must be HH:MM"))?;
    let h: u16 = h.trim().parse().map_err(|_| anyhow!("bad hour"))?;
    let m: u16 = m.trim().parse().map_err(|_| anyhow!("bad minute"))?;
    if h > 23 || m > 59 {
        return Err(anyhow!("time out of range"));
    }
    Ok(h | (m << 8))
}

fn dto(def: &SettingDef, regs: &dyn Fn(u16) -> u16) -> contracts::DeviceSettingDto {
    let setting = match &def.kind {
        Kind::Number {
            reg,
            scale,
            min,
            max,
            step,
            unit,
        } => contracts::SettingValueDto::Number {
            value: (regs(*reg) as f64 * scale * 1000.0).round() / 1000.0,
            min: *min,
            max: *max,
            step: *step,
            unit: Some(unit.to_string()),
        },
        Kind::Bit { reg, bit } => contracts::SettingValueDto::Toggle {
            enabled: regs(*reg) & (1 << bit) != 0,
        },
        Kind::BitChoice { reg, bit, labels } => contracts::SettingValueDto::Choice {
            value: (regs(*reg) >> bit) & 1,
            options: vec![0, 1],
            labels: Some(labels.iter().map(|s| s.to_string()).collect()),
            unit: None,
        },
        Kind::Choice { reg, options, unit } => contracts::SettingValueDto::Choice {
            value: regs(*reg),
            options: options.to_vec(),
            labels: None,
            unit: Some(unit.to_string()),
        },
        Kind::TimeWindow { start_reg, end_reg } => contracts::SettingValueDto::TimeWindow {
            start: fmt_time(regs(*start_reg)),
            end: fmt_time(regs(*end_reg)),
        },
    };
    contracts::DeviceSettingDto {
        key: def.key.to_string(),
        label: def.label.to_string(),
        group: def.group.to_string(),
        setting,
    }
}

/// Settings access for one LuxPower-map device, routed through the shared
/// per-port actor so it never contends with polling on the same bus.
pub struct LuxPowerSettings {
    pub(crate) handle: Arc<PortHandle>,
    pub(crate) unit_id: u8,
    pub(crate) table: &'static [SettingDef],
}

#[async_trait]
impl solar_monitor_core::SettingsAccess for LuxPowerSettings {
    /// Read all curated settings (five aligned 40-register holding blocks, regs 0-199).
    async fn read_settings(&self) -> Result<Vec<contracts::DeviceSettingDto>> {
        let mut blocks: Vec<Vec<u16>> = Vec::with_capacity(5);
        for start in [0u16, 40, 80, 120, 160] {
            blocks.push(
                self.handle
                    .read_holding_registers(self.unit_id, start, 40)
                    .await?,
            );
        }
        let regs = move |addr: u16| -> u16 {
            blocks
                .get((addr / 40) as usize)
                .map(|b| b[(addr % 40) as usize])
                .unwrap_or(0)
        };
        Ok(self.table.iter().map(|d| dto(d, &regs)).collect())
    }

    /// Write one setting, range-checked, then read back and return the stored value.
    async fn write_setting(&self, key: &str, value: &str) -> Result<contracts::DeviceSettingDto> {
        let def = self
            .table
            .iter()
            .find(|d| d.key == key)
            .ok_or_else(|| anyhow!("unknown setting '{}'", key))?;

        match &def.kind {
            Kind::Number {
                reg,
                scale,
                min,
                max,
                ..
            } => {
                let v: f64 = value
                    .trim()
                    .parse()
                    .map_err(|_| anyhow!("expected a number"))?;
                if v < *min || v > *max {
                    return Err(anyhow!("{} out of range {}..{}", v, min, max));
                }
                let raw = (v / scale).round() as u16;
                self.handle
                    .write_single_register(self.unit_id, *reg, raw)
                    .await?;
            }
            Kind::Bit { reg, bit } | Kind::BitChoice { reg, bit, .. } => {
                let on: bool = match value.trim() {
                    "0" => false,
                    "1" => true,
                    other => other
                        .parse()
                        .map_err(|_| anyhow!("expected true/false or 0/1"))?,
                };
                let cur = self
                    .handle
                    .read_holding_registers(self.unit_id, *reg, 1)
                    .await?[0];
                let new = if on {
                    cur | (1 << bit)
                } else {
                    cur & !(1 << bit)
                };
                if new != cur {
                    self.handle
                        .write_single_register(self.unit_id, *reg, new)
                        .await?;
                }
            }
            Kind::Choice { reg, options, .. } => {
                let v: u16 = value
                    .trim()
                    .parse()
                    .map_err(|_| anyhow!("expected a number"))?;
                if !options.contains(&v) {
                    return Err(anyhow!("{} is not one of {:?}", v, options));
                }
                self.handle
                    .write_single_register(self.unit_id, *reg, v)
                    .await?;
            }
            Kind::TimeWindow { start_reg, end_reg } => {
                let (start, end) = value
                    .split_once('-')
                    .ok_or_else(|| anyhow!("expected HH:MM-HH:MM"))?;
                let start = parse_time(start)?;
                let end = parse_time(end)?;
                self.handle
                    .write_single_register(self.unit_id, *start_reg, start)
                    .await?;
                self.handle
                    .write_single_register(self.unit_id, *end_reg, end)
                    .await?;
            }
        }

        // Read back the affected registers and return what the inverter actually stored
        let mut vals: std::collections::HashMap<u16, u16> = std::collections::HashMap::new();
        let addrs: Vec<u16> = match &def.kind {
            Kind::Number { reg, .. }
            | Kind::Choice { reg, .. }
            | Kind::Bit { reg, .. }
            | Kind::BitChoice { reg, .. } => vec![*reg],
            Kind::TimeWindow { start_reg, end_reg } => vec![*start_reg, *end_reg],
        };
        for a in addrs {
            let v = self
                .handle
                .read_holding_registers(self.unit_id, a, 1)
                .await?[0];
            vals.insert(a, v);
        }
        let regs = move |addr: u16| -> u16 { *vals.get(&addr).unwrap_or(&0) };
        Ok(dto(def, &regs))
    }
}
