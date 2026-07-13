//! EG4 6000XP: LuxPower register map, sourced from the official vendor
//! document "6kXP-Modbus updated on 2023.10.28" (Table 8, hold registers).
//! The PDF is vendor-copyrighted and not redistributed — obtain it from
//! EG4/LuxPower and place it at `docs/6kXP-Modbus-2023.10.28.pdf`
//! (gitignored). Every setting cites a documented register and range;
//! registers without a documented range are deliberately excluded.
//!
//! The Modbus RTU interface is the dongle port: 19200 baud 8N1, unit 1,
//! plain standard Modbus RTU. See CLAUDE.md for the hard-won bus notes.

use solar_monitor_core::DeviceMetrics;

use crate::luxpower::settings::{Kind, SettingDef};
use crate::luxpower::{InputBlocks, ModelDef};

/// Function-enable bit field (hold reg 21).
const FUNC_EN_REG: u16 = 21;

pub static EG4_6000XP: ModelDef = ModelDef {
    protocol_name: "eg4-6000xp-modbus",
    display_name: "EG4 6000XP Modbus RTU",
    description: "EG4 6000XP inverter via Modbus RTU",
    default_baud: 19200,
    // The 6000XP Modbus interface (dongle port) runs at 19200; sweep 9600 as fallback
    discovery_bauds: &[19200, 9600],
    settings: SETTINGS,
    // Disruptive or hardware-risky writes — the UI must confirm first.
    confirm_keys: &[
        (
            "inverter_state",
            "Switching the inverter state cuts output power.",
        ),
        (
            "eps_enabled",
            "Toggling backup output cuts power to backup loads.",
        ),
        (
            "eps_voltage",
            "Changing the backup output voltage can damage connected loads. \
             Verify every backup load supports the new voltage.",
        ),
        (
            "eps_frequency",
            "Changing the backup output frequency can damage connected loads. \
             Verify every backup load supports the new frequency.",
        ),
        (
            "charge_voltage",
            "Set strictly per your battery's documentation — too high can \
             permanently damage batteries. Ignored for closed-loop lithium.",
        ),
        (
            "equalization_voltage",
            "Equalization is for flooded lead-acid only — running it on \
             lithium or sealed batteries can permanently damage them.",
        ),
    ],
    decode_metrics,
};

const SETTINGS: &[SettingDef] = &[
    // System (FuncEn reg 21; FunctionEn1 reg 110; OutputPrioConfig 145; LineMode 146)
    SettingDef {
        key: "inverter_state",
        label: "Inverter state",
        group: "System",
        kind: Kind::BitChoice {
            reg: FUNC_EN_REG,
            bit: 9,
            labels: ["Standby", "Powered on"],
        },
    },
    SettingDef {
        key: "output_priority",
        label: "Output priority",
        group: "System",
        kind: Kind::Choice {
            reg: 145,
            options: &[0, 1, 2],
            labels: Some(&["Battery first", "PV first", "AC first"]),
            unit: "",
        },
    },
    SettingDef {
        key: "line_mode",
        label: "AC input range",
        group: "System",
        kind: Kind::Choice {
            reg: 146,
            options: &[0, 1, 2],
            labels: Some(&[
                "APL (90-280 V, 20 ms)",
                "UPS (170-280 V, 10 ms)",
                "GEN (90-280 V, 20 ms)",
            ]),
            unit: "",
        },
    },
    SettingDef {
        key: "buzzer_enabled",
        label: "Buzzer (off-grid)",
        group: "System",
        kind: Kind::Bit { reg: 110, bit: 7 },
    },
    SettingDef {
        key: "eco_mode",
        label: "Eco mode",
        group: "System",
        kind: Kind::Bit { reg: 110, bit: 15 },
    },
    SettingDef {
        key: "green_mode",
        label: "Green mode",
        group: "System",
        kind: Kind::Bit { reg: 110, bit: 14 },
    },
    SettingDef {
        key: "ongrid_working_mode",
        label: "On-grid working mode",
        group: "System",
        kind: Kind::BitChoice {
            reg: 110,
            bit: 11,
            labels: ["Self consumption", "Charge first"],
        },
    },
    SettingDef {
        key: "ac_first_window_1",
        label: "AC-first window 1",
        group: "System",
        kind: Kind::TimeWindow {
            start_reg: 152,
            end_reg: 153,
        },
    },
    SettingDef {
        key: "ac_first_window_2",
        label: "AC-first window 2",
        group: "System",
        kind: Kind::TimeWindow {
            start_reg: 154,
            end_reg: 155,
        },
    },
    SettingDef {
        key: "ac_first_window_3",
        label: "AC-first window 3",
        group: "System",
        kind: Kind::TimeWindow {
            start_reg: 156,
            end_reg: 157,
        },
    },
    // Charging
    SettingDef {
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
    SettingDef {
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
    SettingDef {
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
    SettingDef {
        key: "float_charge_voltage",
        label: "Float charge voltage",
        group: "Charging",
        kind: Kind::Number {
            reg: 144,
            scale: 0.1,
            min: 50.0,
            max: 56.0,
            step: 0.1,
            unit: "V",
        },
    },
    // AC charge
    SettingDef {
        key: "ac_charge_enabled",
        label: "AC charge (from grid)",
        group: "AC charge",
        kind: Kind::Bit {
            reg: FUNC_EN_REG,
            bit: 7,
        },
    },
    SettingDef {
        key: "ac_charge_type",
        label: "AC charge control",
        group: "AC charge",
        kind: Kind::Bits {
            reg: 120,
            shift: 1,
            width: 3,
            options: &[0, 1, 2, 3, 4, 5],
            labels: &[
                "Disabled",
                "By time",
                "By voltage",
                "By SOC",
                "Voltage + time",
                "SOC + time",
            ],
        },
    },
    SettingDef {
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
    SettingDef {
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
    SettingDef {
        key: "ac_charge_window_1",
        label: "AC charge window 1",
        group: "AC charge",
        kind: Kind::TimeWindow {
            start_reg: 68,
            end_reg: 69,
        },
    },
    SettingDef {
        key: "ac_charge_window_2",
        label: "AC charge window 2",
        group: "AC charge",
        kind: Kind::TimeWindow {
            start_reg: 70,
            end_reg: 71,
        },
    },
    SettingDef {
        key: "ac_charge_window_3",
        label: "AC charge window 3",
        group: "AC charge",
        kind: Kind::TimeWindow {
            start_reg: 72,
            end_reg: 73,
        },
    },
    SettingDef {
        key: "ac_charge_current",
        label: "AC charge current limit",
        group: "AC charge",
        kind: Kind::Number {
            reg: 168,
            scale: 1.0,
            min: 0.0,
            max: 140.0,
            step: 1.0,
            unit: "A",
        },
    },
    SettingDef {
        key: "ac_charge_start_voltage",
        label: "AC charge start voltage",
        group: "AC charge",
        kind: Kind::Number {
            reg: 158,
            scale: 0.1,
            min: 38.5,
            max: 52.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "ac_charge_end_voltage",
        label: "AC charge end voltage",
        group: "AC charge",
        kind: Kind::Number {
            reg: 159,
            scale: 0.1,
            min: 48.0,
            max: 59.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "ac_charge_start_soc",
        label: "AC charge start SOC",
        group: "AC charge",
        kind: Kind::Number {
            reg: 160,
            scale: 1.0,
            min: 0.0,
            max: 90.0,
            step: 1.0,
            unit: "%",
        },
    },
    // Charge priority (forced charge; FuncEn bit 11, regs 74-81, 201)
    SettingDef {
        key: "charge_priority_enabled",
        label: "Charge priority",
        group: "Charge priority",
        kind: Kind::Bit {
            reg: FUNC_EN_REG,
            bit: 11,
        },
    },
    SettingDef {
        key: "charge_priority_power_percent",
        label: "Charge priority power",
        group: "Charge priority",
        kind: Kind::Number {
            reg: 74,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "charge_priority_soc_limit",
        label: "Charge priority SOC limit",
        group: "Charge priority",
        kind: Kind::Number {
            reg: 75,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "charge_priority_end_voltage",
        label: "Charge priority voltage limit",
        group: "Charge priority",
        kind: Kind::Number {
            reg: 201,
            scale: 0.1,
            min: 48.0,
            max: 59.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "charge_priority_window_1",
        label: "Charge priority window 1",
        group: "Charge priority",
        kind: Kind::TimeWindow {
            start_reg: 76,
            end_reg: 77,
        },
    },
    SettingDef {
        key: "charge_priority_window_2",
        label: "Charge priority window 2",
        group: "Charge priority",
        kind: Kind::TimeWindow {
            start_reg: 78,
            end_reg: 79,
        },
    },
    SettingDef {
        key: "charge_priority_window_3",
        label: "Charge priority window 3",
        group: "Charge priority",
        kind: Kind::TimeWindow {
            start_reg: 80,
            end_reg: 81,
        },
    },
    // Discharging
    SettingDef {
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
    SettingDef {
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
    SettingDef {
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
    SettingDef {
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
    SettingDef {
        key: "discharge_ctrl_type",
        label: "Discharge control",
        group: "Discharging",
        kind: Kind::Bits {
            reg: 120,
            shift: 4,
            width: 2,
            options: &[0, 1, 2],
            labels: &["By voltage", "By SOC", "Both"],
        },
    },
    SettingDef {
        key: "ongrid_eod_type",
        label: "On-grid end of discharge by",
        group: "Discharging",
        kind: Kind::BitChoice {
            reg: 120,
            bit: 6,
            labels: ["By voltage", "By SOC"],
        },
    },
    SettingDef {
        key: "ongrid_eod_voltage",
        label: "On-grid end-of-discharge voltage",
        group: "Discharging",
        kind: Kind::Number {
            reg: 169,
            scale: 0.1,
            min: 40.0,
            max: 56.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "eps_discharge_soc_low",
        label: "EPS discharge SOC floor",
        group: "Discharging",
        kind: Kind::Number {
            reg: 125,
            scale: 1.0,
            min: 0.0,
            max: 90.0,
            step: 1.0,
            unit: "%",
        },
    },
    // Forced discharge (FuncEn bit 10, regs 82-89, 202)
    SettingDef {
        key: "forced_discharge_enabled",
        label: "Forced discharge",
        group: "Forced discharge",
        kind: Kind::Bit {
            reg: FUNC_EN_REG,
            bit: 10,
        },
    },
    SettingDef {
        key: "forced_discharge_power_percent",
        label: "Forced discharge power",
        group: "Forced discharge",
        kind: Kind::Number {
            reg: 82,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "forced_discharge_soc_limit",
        label: "Forced discharge SOC limit",
        group: "Forced discharge",
        kind: Kind::Number {
            reg: 83,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "forced_discharge_end_voltage",
        label: "Forced discharge voltage limit",
        group: "Forced discharge",
        kind: Kind::Number {
            reg: 202,
            scale: 0.1,
            min: 40.0,
            max: 56.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "forced_discharge_window_1",
        label: "Forced discharge window 1",
        group: "Forced discharge",
        kind: Kind::TimeWindow {
            start_reg: 84,
            end_reg: 85,
        },
    },
    SettingDef {
        key: "forced_discharge_window_2",
        label: "Forced discharge window 2",
        group: "Forced discharge",
        kind: Kind::TimeWindow {
            start_reg: 86,
            end_reg: 87,
        },
    },
    SettingDef {
        key: "forced_discharge_window_3",
        label: "Forced discharge window 3",
        group: "Forced discharge",
        kind: Kind::TimeWindow {
            start_reg: 88,
            end_reg: 89,
        },
    },
    // Battery protection (valid per discharge control type; regs 162-167)
    SettingDef {
        key: "bat_low_voltage",
        label: "Battery low alarm voltage",
        group: "Battery protection",
        kind: Kind::Number {
            reg: 162,
            scale: 0.1,
            min: 40.0,
            max: 50.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "bat_low_back_voltage",
        label: "Battery low recovery voltage",
        group: "Battery protection",
        kind: Kind::Number {
            reg: 163,
            scale: 0.1,
            min: 42.0,
            max: 52.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "bat_low_soc",
        label: "Battery low alarm SOC",
        group: "Battery protection",
        kind: Kind::Number {
            reg: 164,
            scale: 1.0,
            min: 0.0,
            max: 90.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "bat_low_back_soc",
        label: "Battery low recovery SOC",
        group: "Battery protection",
        kind: Kind::Number {
            reg: 165,
            scale: 1.0,
            min: 20.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "bat_low_to_utility_voltage",
        label: "Switch to grid voltage",
        group: "Battery protection",
        kind: Kind::Number {
            reg: 166,
            scale: 0.1,
            min: 44.4,
            max: 51.4,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "bat_low_to_utility_soc",
        label: "Switch to grid SOC",
        group: "Battery protection",
        kind: Kind::Number {
            reg: 167,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    // Battery (lead-acid; regs 147-151)
    SettingDef {
        key: "battery_capacity",
        label: "Battery capacity (unmatched)",
        group: "Battery (lead-acid)",
        kind: Kind::Number {
            reg: 147,
            scale: 1.0,
            min: 0.0,
            max: 10000.0,
            step: 1.0,
            unit: "Ah",
        },
    },
    SettingDef {
        key: "battery_nominal_voltage",
        label: "Battery nominal voltage (unmatched)",
        group: "Battery (lead-acid)",
        kind: Kind::Number {
            reg: 148,
            scale: 0.1,
            min: 40.0,
            max: 59.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "equalization_voltage",
        label: "Equalization voltage",
        group: "Battery (lead-acid)",
        kind: Kind::Number {
            reg: 149,
            scale: 0.1,
            min: 50.0,
            max: 59.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "equalization_interval",
        label: "Equalization interval",
        group: "Battery (lead-acid)",
        kind: Kind::Number {
            reg: 150,
            scale: 1.0,
            min: 0.0,
            max: 365.0,
            step: 1.0,
            unit: "days",
        },
    },
    SettingDef {
        key: "equalization_duration",
        label: "Equalization duration",
        group: "Battery (lead-acid)",
        kind: Kind::Number {
            reg: 151,
            scale: 1.0,
            min: 0.0,
            max: 24.0,
            step: 1.0,
            unit: "h",
        },
    },
    // Export to grid (FuncEn bit 15, MaxBackFlow 103, FunctionEn1 bit 1)
    SettingDef {
        key: "feed_in_grid_enabled",
        label: "Export to grid",
        group: "Export to grid",
        kind: Kind::Bit {
            reg: FUNC_EN_REG,
            bit: 15,
        },
    },
    SettingDef {
        key: "max_feed_in_percent",
        label: "Export power limit",
        group: "Export to grid",
        kind: Kind::Number {
            reg: 103,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "fast_zero_export",
        label: "Fast zero export",
        group: "Export to grid",
        kind: Kind::Bit { reg: 110, bit: 1 },
    },
    // Generator charging (registers valid on units with a generator on the GEN/AC input)
    SettingDef {
        key: "gen_port_mode",
        label: "GEN port mode",
        group: "Generator",
        kind: Kind::BitChoice {
            reg: 179,
            bit: 13,
            labels: ["Generator", "Smart load"],
        },
    },
    SettingDef {
        key: "gen_charge_type",
        label: "Generator charge control",
        group: "Generator",
        kind: Kind::BitChoice {
            reg: 120,
            bit: 7,
            labels: ["By voltage", "By SOC"],
        },
    },
    SettingDef {
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
    SettingDef {
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
    SettingDef {
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
    SettingDef {
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
    SettingDef {
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
    SettingDef {
        key: "gen_peak_shaving_enabled",
        label: "Generator peak shaving",
        group: "Generator",
        kind: Kind::Bit { reg: 179, bit: 8 },
    },
    // Backup output
    SettingDef {
        key: "eps_enabled",
        label: "Off-grid output (EPS)",
        group: "Backup output",
        kind: Kind::Bit {
            reg: FUNC_EN_REG,
            bit: 0,
        },
    },
    SettingDef {
        key: "seamless_eps_switch",
        label: "Seamless off-grid switching",
        group: "Backup output",
        kind: Kind::Bit {
            reg: FUNC_EN_REG,
            bit: 8,
        },
    },
    SettingDef {
        key: "micro_grid_enabled",
        label: "Micro-grid",
        group: "Backup output",
        kind: Kind::Bit { reg: 110, bit: 2 },
    },
    SettingDef {
        key: "eps_voltage",
        label: "Backup output voltage",
        group: "Backup output",
        kind: Kind::Choice {
            reg: 90,
            options: &[208, 220, 230, 240, 277],
            labels: None,
            unit: "V",
        },
    },
    SettingDef {
        key: "eps_frequency",
        label: "Backup output frequency",
        group: "Backup output",
        kind: Kind::Choice {
            reg: 91,
            options: &[50, 60],
            labels: None,
            unit: "Hz",
        },
    },
    // Grid peak shaving (uFunctionEn2 bit 7, regs 206-212, 218-219)
    SettingDef {
        key: "grid_peak_shaving_enabled",
        label: "Grid peak shaving",
        group: "Grid peak shaving",
        kind: Kind::Bit { reg: 179, bit: 7 },
    },
    SettingDef {
        key: "grid_peak_shaving_power",
        label: "Peak shaving power",
        group: "Grid peak shaving",
        kind: Kind::Number {
            reg: 206,
            scale: 0.1,
            min: 0.0,
            max: 25.5,
            step: 0.1,
            unit: "kW",
        },
    },
    SettingDef {
        key: "grid_peak_shaving_soc",
        label: "Peak shaving SOC",
        group: "Grid peak shaving",
        kind: Kind::Number {
            reg: 207,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "grid_peak_shaving_voltage",
        label: "Peak shaving voltage",
        group: "Grid peak shaving",
        kind: Kind::Number {
            reg: 208,
            scale: 0.1,
            min: 48.0,
            max: 59.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "grid_peak_shaving_soc_2",
        label: "Peak shaving SOC 2",
        group: "Grid peak shaving",
        kind: Kind::Number {
            reg: 218,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "grid_peak_shaving_voltage_2",
        label: "Peak shaving voltage 2",
        group: "Grid peak shaving",
        kind: Kind::Number {
            reg: 219,
            scale: 0.1,
            min: 48.0,
            max: 59.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "peak_shaving_window_1",
        label: "Peak shaving window 1",
        group: "Grid peak shaving",
        kind: Kind::TimeWindow {
            start_reg: 209,
            end_reg: 210,
        },
    },
    SettingDef {
        key: "peak_shaving_window_2",
        label: "Peak shaving window 2",
        group: "Grid peak shaving",
        kind: Kind::TimeWindow {
            start_reg: 211,
            end_reg: 212,
        },
    },
    // Smart load (GEN port in smart-load mode; regs 213-217)
    SettingDef {
        key: "smart_load_on_voltage",
        label: "Smart load on voltage",
        group: "Smart load (GEN port)",
        kind: Kind::Number {
            reg: 213,
            scale: 0.1,
            min: 48.0,
            max: 59.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "smart_load_off_voltage",
        label: "Smart load off voltage",
        group: "Smart load (GEN port)",
        kind: Kind::Number {
            reg: 214,
            scale: 0.1,
            min: 40.0,
            max: 52.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "smart_load_on_soc",
        label: "Smart load on SOC",
        group: "Smart load (GEN port)",
        kind: Kind::Number {
            reg: 215,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "smart_load_off_soc",
        label: "Smart load off SOC",
        group: "Smart load (GEN port)",
        kind: Kind::Number {
            reg: 216,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "smart_load_start_pv_power",
        label: "Smart load minimum PV power",
        group: "Smart load (GEN port)",
        kind: Kind::Number {
            reg: 217,
            scale: 0.1,
            min: 0.0,
            max: 12.0,
            step: 0.1,
            unit: "kW",
        },
    },
    // AC coupling (uFunctionEn2 bit 11, regs 220-223)
    SettingDef {
        key: "ac_couple_enabled",
        label: "AC coupling",
        group: "AC coupling (GEN port)",
        kind: Kind::Bit { reg: 179, bit: 11 },
    },
    SettingDef {
        key: "ac_couple_ctrl_type",
        label: "AC couple control",
        group: "AC coupling (GEN port)",
        // uFunctionEn2.ubBatChgControl (reg 179 bit 9) selects which pair
        // below is live — the inverter ignores the other one, same as
        // gen_charge_type gating the generator start/end pair.
        kind: Kind::BitChoice {
            reg: 179,
            bit: 9,
            labels: ["By SOC", "By voltage"],
        },
    },
    SettingDef {
        key: "ac_couple_start_soc",
        label: "AC couple start SOC",
        group: "AC coupling (GEN port)",
        kind: Kind::Number {
            reg: 220,
            scale: 1.0,
            min: 0.0,
            max: 80.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "ac_couple_end_soc",
        label: "AC couple end SOC",
        group: "AC coupling (GEN port)",
        kind: Kind::Number {
            reg: 221,
            scale: 1.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
            unit: "%",
        },
    },
    SettingDef {
        key: "ac_couple_start_voltage",
        label: "AC couple start voltage",
        group: "AC coupling (GEN port)",
        kind: Kind::Number {
            reg: 222,
            scale: 0.1,
            min: 40.0,
            max: 52.0,
            step: 0.1,
            unit: "V",
        },
    },
    SettingDef {
        key: "ac_couple_end_voltage",
        label: "AC couple end voltage",
        group: "AC coupling (GEN port)",
        kind: Kind::Number {
            reg: 223,
            scale: 0.1,
            min: 40.0,
            max: 56.0,
            step: 0.1,
            unit: "V",
        },
    },
];

fn decode_metrics(blocks: &InputBlocks) -> DeviceMetrics {
    let [b1, b2, b3, b4, b5] = blocks;
    let u16le = |s: &[u8]| -> u16 { u16::from_le_bytes([s[0], s[1]]) };
    // Input reg 77 bit0 = ACInputType: 0 grid, 1 generator (Table 7). The gen
    // input registers (121-126) hold junk unless a generator is configured.
    let ac_input_is_gen = u16le(&b2[74..76]) & 1 == 1;
    let i16le = |s: &[u8]| -> i16 { i16::from_le_bytes([s[0], s[1]]) };
    // b1 indices in bytes
    let vpv1 = u16le(&b1[2..4]) as f64 / 10.0;
    let vbat = u16le(&b1[8..10]) as f64 / 10.0;
    // reg 5 packs SOC (low byte) and SOH (high byte)
    let soc_soh = u16le(&b1[10..12]);
    let soc = (soc_soh & 0xFF) as f64;
    let soh = (soc_soh >> 8) as f64;
    let vac_r = u16le(&b1[24..26]) as f64 / 10.0;
    let fac = u16le(&b1[30..32]) as f64 / 100.0;
    let pinv = u16le(&b1[32..34]) as f64; // W
    let ppv1 = u16le(&b1[14..16]) as f64;
    let ppv2 = u16le(&b1[16..18]) as f64;
    let ppv3 = u16le(&b1[18..20]) as f64;
    let pv_power = ppv1 + ppv2 + ppv3;
    // b5 contains Pload (reg 170 -> bytes index 20..22)
    let pload = u16le(&b5[20..22]) as f64;
    // b3 contains BatCurrentBMS at reg 98 -> data3[36..38]
    // skipping extra read: approximate battery current from Pcharge/Pdischarge and vbat
    let pcharge = u16le(&b1[20..22]) as f64;
    let pdis = u16le(&b1[22..24]) as f64;
    let batt_cur = if vbat > 0.1 {
        (pcharge - pdis) / vbat
    } else {
        0.0
    };

    // Additional metrics from further blocks
    let vpv2 = u16le(&b1[4..6]) as f64 / 10.0;
    let vpv3 = u16le(&b1[6..8]) as f64 / 10.0;
    let vac_s = u16le(&b1[26..28]) as f64 / 10.0;
    let vac_t = u16le(&b1[28..30]) as f64 / 10.0;
    let linv_rms = u16le(&b1[36..38]) as f64 / 100.0;
    let pf = u16le(&b1[38..40]) as f64 / 1000.0;
    let veps_r = u16le(&b1[40..42]) as f64 / 10.0;
    let veps_s = u16le(&b1[42..44]) as f64 / 10.0;
    let veps_t = u16le(&b1[44..46]) as f64 / 10.0;
    let feps = u16le(&b1[46..48]) as f64 / 100.0;
    let peps = u16le(&b1[48..50]) as f64;
    let seps = u16le(&b1[50..52]) as f64;
    let ptogrid = u16le(&b1[52..54]) as f64;
    let ptouser = u16le(&b1[54..56]) as f64;
    let epv1_day = u16le(&b1[56..58]) as f64 / 10.0;
    let epv2_day = u16le(&b1[58..60]) as f64 / 10.0;
    let epv3_day = u16le(&b1[60..62]) as f64 / 10.0;
    let einv_day = u16le(&b1[62..64]) as f64 / 10.0;
    let erec_day = u16le(&b1[64..66]) as f64 / 10.0;
    let echg_day = u16le(&b1[66..68]) as f64 / 10.0;
    let edischg_day = u16le(&b1[68..70]) as f64 / 10.0;
    let eeps_day = u16le(&b1[70..72]) as f64 / 10.0;
    let etogrid_day = u16le(&b1[72..74]) as f64 / 10.0;
    let etouser_day = u16le(&b1[74..76]) as f64 / 10.0;
    let vbus1 = u16le(&b1[76..78]) as f64 / 10.0;
    let vbus2 = u16le(&b1[78..80]) as f64 / 10.0;

    // Totals and temps (b2)
    let tinner = u16le(&b2[48..50]) as f64;
    let tradiator1 = u16le(&b2[50..52]) as f64;
    let tradiator2 = u16le(&b2[52..54]) as f64;
    let tbat = u16le(&b2[54..56]) as f64;
    let epv1_all = (u16le(&b2[0..2]) as u32 | ((u16le(&b2[2..4]) as u32) << 16)) as f64 / 10.0;
    let epv2_all = (u16le(&b2[4..6]) as u32 | ((u16le(&b2[6..8]) as u32) << 16)) as f64 / 10.0;
    let epv3_all = (u16le(&b2[8..10]) as u32 | ((u16le(&b2[10..12]) as u32) << 16)) as f64 / 10.0;
    let einv_all = (u16le(&b2[12..14]) as u32 | ((u16le(&b2[14..16]) as u32) << 16)) as f64 / 10.0;
    let erec_all = (u16le(&b2[16..18]) as u32 | ((u16le(&b2[18..20]) as u32) << 16)) as f64 / 10.0;
    let echg_all = (u16le(&b2[20..22]) as u32 | ((u16le(&b2[22..24]) as u32) << 16)) as f64 / 10.0;
    let edischg_all =
        (u16le(&b2[24..26]) as u32 | ((u16le(&b2[26..28]) as u32) << 16)) as f64 / 10.0;
    let eeps_all = (u16le(&b2[28..30]) as u32 | ((u16le(&b2[30..32]) as u32) << 16)) as f64 / 10.0;
    let etogrid_all =
        (u16le(&b2[32..34]) as u32 | ((u16le(&b2[34..36]) as u32) << 16)) as f64 / 10.0;
    let etouser_all =
        (u16le(&b2[36..38]) as u32 | ((u16le(&b2[38..40]) as u32) << 16)) as f64 / 10.0;

    // BMS current and cell stats (b3)
    let bat_current_bms = i16le(&b3[36..38]) as f64 / 100.0;
    let max_cell_v = u16le(&b3[42..44]) as f64 / 1000.0;
    let min_cell_v = u16le(&b3[44..46]) as f64 / 1000.0;
    let max_cell_t = i16le(&b3[46..48]) as f64 / 10.0;
    let min_cell_t = i16le(&b3[48..50]) as f64 / 10.0;
    let cycles_bms = u16le(&b3[52..54]) as f64;

    // Generator and EPS (b4)
    let gen_v = u16le(&b4[2..4]) as f64 / 10.0;
    let gen_f = u16le(&b4[4..6]) as f64 / 100.0;
    let gen_p = u16le(&b4[6..8]) as f64;
    let egen_day = u16le(&b4[8..10]) as f64 / 10.0;
    let egen_all = (u16le(&b4[10..12]) as u32 | ((u16le(&b4[12..14]) as u32) << 16)) as f64 / 10.0;
    let eps_v_l1n = u16le(&b4[14..16]) as f64 / 10.0;
    let eps_v_l2n = u16le(&b4[16..18]) as f64 / 10.0;
    let peps_l1n = u16le(&b4[18..20]) as f64;
    let peps_l2n = u16le(&b4[20..22]) as f64;

    // Per-phase and load energy (b5)
    let eload_day = u16le(&b5[22..24]) as f64 / 10.0;
    let pinv_s = u16le(&b5[40..42]) as f64;
    let pinv_t = u16le(&b5[42..44]) as f64;
    let ptogrid_s = u16le(&b5[48..50]) as f64;
    let ptogrid_t = u16le(&b5[50..52]) as f64;
    let ptouser_s = u16le(&b5[52..54]) as f64;
    let ptouser_t = u16le(&b5[54..56]) as f64;

    let mut metrics = DeviceMetrics {
        pv_voltage: Some(vpv1),
        battery_voltage: Some(vbat),
        battery_current: Some(if bat_current_bms.abs() > 0.0 {
            bat_current_bms
        } else {
            batt_cur
        }),
        battery_soc_percentage: Some(soc),
        grid_voltage: Some(vac_r),
        grid_frequency: Some(fac),
        output_power_watts: Some(if pload > 0.0 { pload } else { pinv }),
        pv_power_watts: Some(pv_power),
        device_temperature_celsius: Some(tinner),
        ..Default::default()
    };
    metrics.custom_metrics.insert("pv1_power".into(), ppv1);
    metrics.custom_metrics.insert("pv2_power".into(), ppv2);
    metrics.custom_metrics.insert("pv3_power".into(), ppv3);
    metrics.custom_metrics.insert("pv1_voltage".into(), vpv1);
    metrics.custom_metrics.insert("pv2_voltage".into(), vpv2);
    metrics.custom_metrics.insert("pv3_voltage".into(), vpv3);
    metrics
        .custom_metrics
        .insert("grid_voltage_s".into(), vac_s);
    metrics
        .custom_metrics
        .insert("grid_voltage_t".into(), vac_t);
    metrics
        .custom_metrics
        .insert("inverter_rms_current".into(), linv_rms);
    metrics.custom_metrics.insert("power_factor".into(), pf);
    metrics
        .custom_metrics
        .insert("offgrid_voltage_r".into(), veps_r);
    metrics
        .custom_metrics
        .insert("offgrid_voltage_s".into(), veps_s);
    metrics
        .custom_metrics
        .insert("offgrid_voltage_t".into(), veps_t);
    metrics
        .custom_metrics
        .insert("offgrid_frequency".into(), feps);
    metrics
        .custom_metrics
        .insert("offgrid_power_active".into(), peps);
    metrics
        .custom_metrics
        .insert("offgrid_power_apparent".into(), seps);
    metrics
        .custom_metrics
        .insert("export_power".into(), ptogrid);
    metrics
        .custom_metrics
        .insert("import_power".into(), ptouser);
    metrics
        .custom_metrics
        .insert("pv1_day_kwh".into(), epv1_day);
    metrics
        .custom_metrics
        .insert("pv2_day_kwh".into(), epv2_day);
    metrics
        .custom_metrics
        .insert("pv3_day_kwh".into(), epv3_day);
    metrics
        .custom_metrics
        .insert("inverter_day_kwh".into(), einv_day);
    metrics
        .custom_metrics
        .insert("ac_charge_day_kwh".into(), erec_day);
    metrics
        .custom_metrics
        .insert("charge_day_kwh".into(), echg_day);
    metrics
        .custom_metrics
        .insert("discharge_day_kwh".into(), edischg_day);
    metrics
        .custom_metrics
        .insert("offgrid_day_kwh".into(), eeps_day);
    metrics
        .custom_metrics
        .insert("export_day_kwh".into(), etogrid_day);
    metrics
        .custom_metrics
        .insert("import_day_kwh".into(), etouser_day);
    metrics.custom_metrics.insert("bus1_voltage".into(), vbus1);
    metrics.custom_metrics.insert("bus2_voltage".into(), vbus2);
    metrics
        .custom_metrics
        .insert("pv1_total_kwh".into(), epv1_all);
    metrics
        .custom_metrics
        .insert("pv2_total_kwh".into(), epv2_all);
    metrics
        .custom_metrics
        .insert("pv3_total_kwh".into(), epv3_all);
    metrics
        .custom_metrics
        .insert("inverter_total_kwh".into(), einv_all);
    metrics
        .custom_metrics
        .insert("ac_charge_total_kwh".into(), erec_all);
    metrics
        .custom_metrics
        .insert("charge_total_kwh".into(), echg_all);
    metrics
        .custom_metrics
        .insert("discharge_total_kwh".into(), edischg_all);
    metrics
        .custom_metrics
        .insert("offgrid_total_kwh".into(), eeps_all);
    metrics
        .custom_metrics
        .insert("export_total_kwh".into(), etogrid_all);
    metrics
        .custom_metrics
        .insert("import_total_kwh".into(), etouser_all);
    metrics.custom_metrics.insert("battery_temp_c".into(), tbat);
    metrics
        .custom_metrics
        .insert("heatsink_temp1_c".into(), tradiator1);
    metrics
        .custom_metrics
        .insert("heatsink_temp2_c".into(), tradiator2);
    metrics
        .custom_metrics
        .insert("bms_max_cell_v".into(), max_cell_v);
    metrics
        .custom_metrics
        .insert("bms_min_cell_v".into(), min_cell_v);
    metrics
        .custom_metrics
        .insert("bms_max_cell_t_c".into(), max_cell_t);
    metrics
        .custom_metrics
        .insert("bms_min_cell_t_c".into(), min_cell_t);
    metrics
        .custom_metrics
        .insert("bms_cycles".into(), cycles_bms);
    metrics.custom_metrics.insert("battery_soh".into(), soh);
    metrics.custom_metrics.insert(
        "ac_input_is_generator".into(),
        if ac_input_is_gen { 1.0 } else { 0.0 },
    );
    if ac_input_is_gen {
        metrics.custom_metrics.insert("gen_voltage".into(), gen_v);
        metrics.custom_metrics.insert("gen_frequency".into(), gen_f);
        metrics.custom_metrics.insert("gen_power".into(), gen_p);
        metrics
            .custom_metrics
            .insert("gen_day_kwh".into(), egen_day);
        metrics
            .custom_metrics
            .insert("gen_total_kwh".into(), egen_all);
    }
    metrics
        .custom_metrics
        .insert("eps_voltage_l1n".into(), eps_v_l1n);
    metrics
        .custom_metrics
        .insert("eps_voltage_l2n".into(), eps_v_l2n);
    metrics
        .custom_metrics
        .insert("eps_power_l1n".into(), peps_l1n);
    metrics
        .custom_metrics
        .insert("eps_power_l2n".into(), peps_l2n);
    metrics
        .custom_metrics
        .insert("load_day_kwh".into(), eload_day);
    metrics
        .custom_metrics
        .insert("inverter_power_s".into(), pinv_s);
    metrics
        .custom_metrics
        .insert("inverter_power_t".into(), pinv_t);
    metrics
        .custom_metrics
        .insert("export_power_s".into(), ptogrid_s);
    metrics
        .custom_metrics
        .insert("export_power_t".into(), ptogrid_t);
    metrics
        .custom_metrics
        .insert("import_power_s".into(), ptouser_s);
    metrics
        .custom_metrics
        .insert("import_power_t".into(), ptouser_t);

    metrics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_keys_exist_in_table() {
        for (key, msg) in EG4_6000XP.confirm_keys {
            assert!(
                SETTINGS.iter().any(|d| d.key == *key),
                "confirm key '{}' not in settings table",
                key
            );
            assert!(
                !msg.is_empty(),
                "confirm key '{}' has an empty warning",
                key
            );
        }
    }

    #[test]
    fn settings_table_is_sane() {
        let mut keys = std::collections::HashSet::new();
        for def in SETTINGS {
            assert!(keys.insert(def.key), "duplicate setting key {}", def.key);
            match &def.kind {
                Kind::Number {
                    min, max, scale, ..
                } => {
                    assert!(min < max, "{}: min !< max", def.key);
                    assert!(*scale > 0.0, "{}: bad scale", def.key);
                }
                Kind::Bits {
                    shift,
                    width,
                    options,
                    labels,
                    ..
                } => {
                    assert!(*width >= 1 && shift + width <= 16, "{}: bad field", def.key);
                    assert_eq!(options.len(), labels.len(), "{}: labels mismatch", def.key);
                    let max_field = (1u16 << width) - 1;
                    for o in *options {
                        assert!(*o <= max_field, "{}: option {} exceeds width", def.key, o);
                    }
                }
                Kind::Choice {
                    options, labels, ..
                } => {
                    assert!(!options.is_empty(), "{}: empty options", def.key);
                    if let Some(ls) = labels {
                        assert_eq!(options.len(), ls.len(), "{}: labels mismatch", def.key);
                    }
                }
                Kind::Bit { bit, .. } | Kind::BitChoice { bit, .. } => {
                    assert!(*bit < 16, "{}: bad bit", def.key);
                }
                Kind::TimeWindow { start_reg, end_reg } => {
                    assert_ne!(start_reg, end_reg, "{}: same reg twice", def.key);
                }
            }
        }
    }

    fn block_from_regs(regs: &[(u16, u16)]) -> Vec<u8> {
        // 40 registers as LE bytes; (offset-within-block, value) pairs
        let mut b = vec![0u8; 80];
        for &(off, val) in regs {
            let bytes = val.to_le_bytes();
            b[(off * 2) as usize] = bytes[0];
            b[(off * 2 + 1) as usize] = bytes[1];
        }
        b
    }

    #[test]
    fn decode_basic_registers() {
        let b1 = block_from_regs(&[
            (1, 3502),   // vpv1 = 350.2 V
            (4, 532),    // vbat = 53.2 V
            (5, 0x6247), // SOC 0x47=71, SOH 0x62=98
            (7, 1200),   // ppv1 = 1200 W
            (12, 2401),  // vac_r = 240.1 V
            (15, 5998),  // fac = 59.98 Hz
            (16, 800),   // pinv = 800 W
        ]);
        let b2 = block_from_regs(&[
            (24, 45), // tinner = 45 C
            (37, 0),  // reg 77 bit0 = 0: grid, not generator
        ]);
        let b3 = block_from_regs(&[(18, 250)]); // BMS current = 2.5 A
        let b4 = block_from_regs(&[]);
        let b5 = block_from_regs(&[(10, 650)]); // pload = 650 W

        let blocks: InputBlocks = [b1, b2, b3, b4, b5];
        let m = decode_metrics(&blocks);

        assert_eq!(m.pv_voltage, Some(350.2));
        assert_eq!(m.battery_voltage, Some(53.2));
        assert_eq!(m.battery_soc_percentage, Some(71.0));
        assert_eq!(m.custom_metrics.get("battery_soh"), Some(&98.0));
        assert_eq!(m.grid_voltage, Some(240.1));
        assert_eq!(m.grid_frequency, Some(59.98));
        assert_eq!(m.pv_power_watts, Some(1200.0));
        assert_eq!(m.output_power_watts, Some(650.0));
        assert_eq!(m.battery_current, Some(2.5));
        assert_eq!(m.device_temperature_celsius, Some(45.0));
        // Grid input: generator metrics must be gated off
        assert_eq!(m.custom_metrics.get("ac_input_is_generator"), Some(&0.0));
        assert!(!m.custom_metrics.contains_key("gen_voltage"));
    }

    #[test]
    fn decode_generator_gating() {
        let b2 = block_from_regs(&[(37, 1)]); // reg 77 bit0 = 1: generator input
        let b4 = block_from_regs(&[(1, 2400), (2, 6000), (3, 3000)]);
        let blocks: InputBlocks = [
            block_from_regs(&[]),
            b2,
            block_from_regs(&[]),
            b4,
            block_from_regs(&[]),
        ];
        let m = decode_metrics(&blocks);
        assert_eq!(m.custom_metrics.get("ac_input_is_generator"), Some(&1.0));
        assert_eq!(m.custom_metrics.get("gen_voltage"), Some(&240.0));
        assert_eq!(m.custom_metrics.get("gen_frequency"), Some(&60.0));
        assert_eq!(m.custom_metrics.get("gen_power"), Some(&3000.0));
    }
}
