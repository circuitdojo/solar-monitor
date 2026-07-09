//! EG4 6000XP configuration via holding registers.
//!
//! Curated, validated settings backed by the LuxPower hold-register map
//! ("6kXP-Modbus updated on 2023.10.28"). Every write is range-checked
//! against the documented limits and read back to confirm.

use anyhow::{anyhow, Result};
use solar_monitor_core as core;

use crate::get_or_spawn_port_actor;

/// Function-enable bit field (hold reg 21).
const FUNC_EN_REG: u16 = 21;

enum Kind {
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

struct Def {
    key: &'static str,
    label: &'static str,
    group: &'static str,
    kind: Kind,
}

const SETTINGS: &[Def] = &[
    // Charging
    Def {
        key: "charge_power_percent",
        label: "Charge power",
        group: "Charging",
        kind: Kind::Number {
            reg: 64,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    Def {
        key: "charge_current",
        label: "Charge current limit",
        group: "Charging",
        kind: Kind::Number {
            reg: 101,
            scale: 1.0,
            min: 0.0,
            max: 140.0,
            step: 1.0,
            unit: "A",
        },
    },
    Def {
        key: "charge_voltage",
        label: "Charge voltage (lead-acid)",
        group: "Charging",
        kind: Kind::Number {
            reg: 99,
            scale: 0.1,
            min: 50.0,
            max: 59.0,
            step: 0.1,
            unit: "V",
        },
    },
    // AC charge
    Def {
        key: "ac_charge_enabled",
        label: "AC charge (from grid)",
        group: "AC charge",
        kind: Kind::Bit {
            reg: FUNC_EN_REG,
            bit: 7,
        },
    },
    Def {
        key: "ac_charge_power_percent",
        label: "AC charge power",
        group: "AC charge",
        kind: Kind::Number {
            reg: 66,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    Def {
        key: "ac_charge_soc_limit",
        label: "AC charge SOC limit",
        group: "AC charge",
        kind: Kind::Number {
            reg: 67,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    Def {
        key: "ac_charge_window_1",
        label: "AC charge window 1",
        group: "AC charge",
        kind: Kind::TimeWindow {
            start_reg: 68,
            end_reg: 69,
        },
    },
    Def {
        key: "ac_charge_window_2",
        label: "AC charge window 2",
        group: "AC charge",
        kind: Kind::TimeWindow {
            start_reg: 70,
            end_reg: 71,
        },
    },
    // Discharging
    Def {
        key: "discharge_power_percent",
        label: "Discharge power",
        group: "Discharging",
        kind: Kind::Number {
            reg: 65,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    Def {
        key: "discharge_current",
        label: "Discharge current limit",
        group: "Discharging",
        kind: Kind::Number {
            reg: 102,
            scale: 1.0,
            min: 0.0,
            max: 140.0,
            step: 1.0,
            unit: "A",
        },
    },
    Def {
        key: "discharge_cut_soc",
        label: "Discharge cut-off SOC (EOD)",
        group: "Discharging",
        kind: Kind::Number {
            reg: 105,
            scale: 1.0,
            min: 10.0,
            max: 90.0,
            step: 1.0,
            unit: "%",
        },
    },
    Def {
        key: "discharge_cut_voltage",
        label: "Discharge cut-off voltage (lead-acid)",
        group: "Discharging",
        kind: Kind::Number {
            reg: 100,
            scale: 0.1,
            min: 40.0,
            max: 52.0,
            step: 0.1,
            unit: "V",
        },
    },
    // Generator charging (registers valid on units with a generator on the GEN/AC input)
    Def {
        key: "gen_charge_type",
        label: "Generator charge control",
        group: "Generator",
        kind: Kind::BitChoice {
            reg: 120,
            bit: 7,
            labels: ["By voltage", "By SOC"],
        },
    },
    Def {
        key: "gen_charge_start_soc",
        label: "Generator charge start SOC",
        group: "Generator",
        kind: Kind::Number {
            reg: 196,
            scale: 1.0,
            min: 0.0,
            max: 90.0,
            step: 1.0,
            unit: "%",
        },
    },
    Def {
        key: "gen_charge_end_soc",
        label: "Generator charge end SOC",
        group: "Generator",
        kind: Kind::Number {
            reg: 197,
            scale: 1.0,
            min: 20.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    Def {
        key: "gen_charge_start_voltage",
        label: "Generator charge start voltage",
        group: "Generator",
        kind: Kind::Number {
            reg: 194,
            scale: 0.1,
            min: 38.4,
            max: 52.0,
            step: 0.1,
            unit: "V",
        },
    },
    Def {
        key: "gen_charge_end_voltage",
        label: "Generator charge end voltage",
        group: "Generator",
        kind: Kind::Number {
            reg: 195,
            scale: 0.1,
            min: 48.0,
            max: 59.0,
            step: 0.1,
            unit: "V",
        },
    },
    Def {
        key: "gen_charge_current",
        label: "Generator charge current limit",
        group: "Generator",
        kind: Kind::Number {
            reg: 198,
            scale: 1.0,
            min: 0.0,
            max: 60.0,
            step: 1.0,
            unit: "A",
        },
    },
    // Backup output
    Def {
        key: "eps_voltage",
        label: "Backup output voltage",
        group: "Backup output",
        kind: Kind::Choice {
            reg: 90,
            options: &[208, 220, 230, 240, 277],
            unit: "V",
        },
    },
    Def {
        key: "eps_frequency",
        label: "Backup output frequency",
        group: "Backup output",
        kind: Kind::Choice {
            reg: 91,
            options: &[50, 60],
            unit: "Hz",
        },
    },
];

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

struct Conn {
    handle: std::sync::Arc<crate::PortHandle>,
    unit_id: u8,
}

async fn open(cfg: &core::DeviceConfig) -> Result<Conn> {
    let path = cfg
        .connection_params
        .get("serial_port")
        .ok_or_else(|| anyhow!("missing serial_port"))?;
    let baud = cfg
        .connection_params
        .get("baud_rate")
        .and_then(|s| s.parse().ok())
        .unwrap_or(19200);
    let timeout = cfg
        .connection_params
        .get("timeout_seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let unit_id = cfg
        .connection_params
        .get("unit_id")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    Ok(Conn {
        handle: get_or_spawn_port_actor(path, baud, timeout).await,
        unit_id,
    })
}

fn dto(def: &Def, regs: &dyn Fn(u16) -> u16) -> contracts::DeviceSettingDto {
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

/// Read all curated settings (five aligned 40-register holding blocks, regs 0-199).
pub async fn eg4_6000xp_read_settings(
    cfg: &core::DeviceConfig,
) -> Result<Vec<contracts::DeviceSettingDto>> {
    let conn = open(cfg).await?;
    let mut blocks: Vec<Vec<u16>> = Vec::with_capacity(5);
    for start in [0u16, 40, 80, 120, 160] {
        blocks.push(
            conn.handle
                .read_holding_registers(conn.unit_id, start, 40)
                .await?,
        );
    }
    let regs = move |addr: u16| -> u16 {
        blocks
            .get((addr / 40) as usize)
            .map(|b| b[(addr % 40) as usize])
            .unwrap_or(0)
    };
    Ok(SETTINGS.iter().map(|d| dto(d, &regs)).collect())
}

/// Write one setting, range-checked, then read back and return the stored value.
pub async fn eg4_6000xp_write_setting(
    cfg: &core::DeviceConfig,
    key: &str,
    value: &str,
) -> Result<contracts::DeviceSettingDto> {
    let def = SETTINGS
        .iter()
        .find(|d| d.key == key)
        .ok_or_else(|| anyhow!("unknown setting '{}'", key))?;
    let conn = open(cfg).await?;

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
            conn.handle
                .write_single_register(conn.unit_id, *reg, raw)
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
            let cur = conn
                .handle
                .read_holding_registers(conn.unit_id, *reg, 1)
                .await?[0];
            let new = if on {
                cur | (1 << bit)
            } else {
                cur & !(1 << bit)
            };
            if new != cur {
                conn.handle
                    .write_single_register(conn.unit_id, *reg, new)
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
            conn.handle
                .write_single_register(conn.unit_id, *reg, v)
                .await?;
        }
        Kind::TimeWindow { start_reg, end_reg } => {
            let (start, end) = value
                .split_once('-')
                .ok_or_else(|| anyhow!("expected HH:MM-HH:MM"))?;
            let start = parse_time(start)?;
            let end = parse_time(end)?;
            conn.handle
                .write_single_register(conn.unit_id, *start_reg, start)
                .await?;
            conn.handle
                .write_single_register(conn.unit_id, *end_reg, end)
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
        let v = conn
            .handle
            .read_holding_registers(conn.unit_id, a, 1)
            .await?[0];
        vals.insert(a, v);
    }
    let regs = move |addr: u16| -> u16 { *vals.get(&addr).unwrap_or(&0) };
    Ok(dto(def, &regs))
}
