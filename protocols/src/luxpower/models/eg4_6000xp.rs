//! EG4 6000XP: LuxPower register map ("6kXP-Modbus updated on 2023.10.28").
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
    decode_metrics,
};

const SETTINGS: &[SettingDef] = &[
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
    // Generator charging (registers valid on units with a generator on the GEN/AC input)
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
    // Backup output
    SettingDef {
        key: "eps_voltage",
        label: "Backup output voltage",
        group: "Backup output",
        kind: Kind::Choice {
            reg: 90,
            options: &[208, 220, 230, 240, 277],
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
            unit: "Hz",
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
