//! Combining readings from LuxPower-map inverters wired in parallel.
//!
//! The input register map has no system-total registers: each inverter
//! reports only its own share of PV, inverter output and battery flow,
//! while the battery bank and the grid are shared and read the same on
//! every unit. Combining is therefore a per-metric rule: sum the per-unit
//! shares, average the shared readings, take the max of temperatures.
//!
//! Every key is classified explicitly; anything unlisted (string voltages,
//! bus voltages, power factor, parallel role, …) is per-unit detail with no
//! meaningful system value and is dropped rather than guessed at.
//!
//! Open question: whether Pload (input reg 170) and the CT-derived grid
//! import/export are per unit or already system-wide on a parallel install
//! is not stated in the register map. They are summed here; if a real
//! parallel system shows them doubled, move them to `AVG`.

use solar_monitor_core::{DeviceData, DeviceMetrics};

/// Per-unit shares: the system value is the sum.
const SUM: &[&str] = &[
    "pv1_power",
    "pv2_power",
    "pv3_power",
    "pv1_day_kwh",
    "pv2_day_kwh",
    "pv3_day_kwh",
    "pv1_total_kwh",
    "pv2_total_kwh",
    "pv3_total_kwh",
    "inverter_power_s",
    "inverter_power_t",
    "inverter_day_kwh",
    "inverter_total_kwh",
    "ac_charge_day_kwh",
    "ac_charge_total_kwh",
    "battery_charge_power",
    "battery_discharge_power",
    "charge_day_kwh",
    "charge_total_kwh",
    "discharge_day_kwh",
    "discharge_total_kwh",
    "offgrid_power_active",
    "offgrid_power_apparent",
    "offgrid_day_kwh",
    "offgrid_total_kwh",
    "eps_power_l1n",
    "eps_power_l2n",
    "import_power",
    "import_power_s",
    "import_power_t",
    "export_power",
    "export_power_s",
    "export_power_t",
    "import_day_kwh",
    "import_total_kwh",
    "export_day_kwh",
    "export_total_kwh",
    "load_day_kwh",
    "gen_power",
    "gen_day_kwh",
    "gen_total_kwh",
];

/// Shared battery bank / grid / generator readings: the system value is the
/// average (members should agree; averaging smooths sampling skew).
const AVG: &[&str] = &[
    "battery_soh",
    "bms_max_cell_v",
    "bms_min_cell_v",
    "bms_cycles",
    "grid_voltage_s",
    "grid_voltage_t",
    "offgrid_voltage_r",
    "offgrid_voltage_s",
    "offgrid_voltage_t",
    "offgrid_frequency",
    "eps_voltage_l1n",
    "eps_voltage_l2n",
    "gen_voltage",
    "gen_frequency",
];

/// Temperatures (the hottest unit is what matters) and flags (set if any
/// member reports it).
const MAX: &[&str] = &[
    "battery_temp_c",
    "heatsink_temp1_c",
    "heatsink_temp2_c",
    "bms_max_cell_t_c",
    "ac_input_is_generator",
];

/// Coldest cell across the bank.
const MIN: &[&str] = &["bms_min_cell_t_c"];

#[derive(Clone, Copy)]
enum Rule {
    Sum,
    Avg,
    Max,
    Min,
}

/// Fold one value per member. `None` unless *every* member has the value:
/// a sum over a subset would be shown as a system total, and the same
/// all-or-nothing rule keeps the other classes consistent with it.
fn fold(values: impl Iterator<Item = Option<f64>>, rule: Rule) -> Option<f64> {
    let vals: Option<Vec<f64>> = values.collect();
    let vals = vals.filter(|v| !v.is_empty())?;
    Some(match rule {
        Rule::Sum => vals.iter().sum(),
        Rule::Avg => vals.iter().sum::<f64>() / vals.len() as f64,
        Rule::Max => vals.iter().cloned().fold(f64::MIN, f64::max),
        Rule::Min => vals.iter().cloned().fold(f64::MAX, f64::min),
    })
}

/// [`solar_monitor_core::ParallelCombiner`] for the LuxPower family.
pub fn combine(members: &[&DeviceData]) -> DeviceMetrics {
    let typed = |pick: fn(&DeviceMetrics) -> Option<f64>, rule| {
        fold(members.iter().map(|d| pick(&d.metrics)), rule)
    };
    let custom = |key: &str, rule| {
        fold(
            members
                .iter()
                .map(|d| d.metrics.custom_metrics.get(key).copied()),
            rule,
        )
    };

    let mut out = DeviceMetrics {
        pv_power_watts: typed(|m| m.pv_power_watts, Rule::Sum),
        output_power_watts: typed(|m| m.output_power_watts, Rule::Sum),
        battery_voltage: typed(|m| m.battery_voltage, Rule::Avg),
        battery_soc_percentage: typed(|m| m.battery_soc_percentage, Rule::Avg),
        grid_voltage: typed(|m| m.grid_voltage, Rule::Avg),
        grid_frequency: typed(|m| m.grid_frequency, Rule::Avg),
        device_temperature_celsius: typed(|m| m.device_temperature_celsius, Rule::Max),
        ..Default::default()
    };

    for (keys, rule) in [
        (SUM, Rule::Sum),
        (AVG, Rule::Avg),
        (MAX, Rule::Max),
        (MIN, Rule::Min),
    ] {
        for key in keys {
            if let Some(v) = custom(key, rule) {
                out.custom_metrics.insert((*key).to_string(), v);
            }
        }
    }

    // Battery current from the summed charge/discharge power over the shared
    // bank voltage: one formula regardless of whether each unit's own
    // current came from its BMS or its power-based fallback.
    let charge = out.custom_metrics.get("battery_charge_power");
    let discharge = out.custom_metrics.get("battery_discharge_power");
    if let (Some(c), Some(d), Some(v)) = (charge, discharge, out.battery_voltage)
        && v > 0.1
    {
        out.battery_current = Some((c - d) / v);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_monitor_core::{DeviceStatus, DeviceType, HealthStatus};

    fn reading(custom: &[(&str, f64)], pv: f64, soc: f64, vbat: f64, temp: f64) -> DeviceData {
        let now = chrono::Utc::now();
        DeviceData {
            device_id: "x".into(),
            timestamp: now,
            device_type: DeviceType::SolarInverter,
            metrics: DeviceMetrics {
                pv_power_watts: Some(pv),
                output_power_watts: Some(pv / 2.0),
                battery_soc_percentage: Some(soc),
                battery_voltage: Some(vbat),
                battery_current: Some(99.0),
                pv_voltage: Some(300.0),
                device_temperature_celsius: Some(temp),
                custom_metrics: custom.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
                ..Default::default()
            },
            status: DeviceStatus {
                is_connected: true,
                last_seen: now,
                health: HealthStatus::Healthy,
                error_message: None,
            },
            raw_data: None,
        }
    }

    #[test]
    fn sums_shares_averages_shared_maxes_temps() {
        let a = reading(
            &[
                ("pv1_day_kwh", 10.0),
                ("battery_soh", 98.0),
                ("heatsink_temp1_c", 40.0),
                ("bms_min_cell_t_c", 18.0),
            ],
            3000.0,
            80.0,
            52.0,
            35.0,
        );
        let b = reading(
            &[
                ("pv1_day_kwh", 12.0),
                ("battery_soh", 100.0),
                ("heatsink_temp1_c", 45.0),
                ("bms_min_cell_t_c", 17.0),
            ],
            1000.0,
            82.0,
            54.0,
            30.0,
        );
        let m = combine(&[&a, &b]);
        assert_eq!(m.pv_power_watts, Some(4000.0));
        assert_eq!(m.output_power_watts, Some(2000.0));
        assert_eq!(m.battery_soc_percentage, Some(81.0));
        assert_eq!(m.battery_voltage, Some(53.0));
        assert_eq!(m.device_temperature_celsius, Some(35.0));
        assert_eq!(m.custom_metrics["pv1_day_kwh"], 22.0);
        assert_eq!(m.custom_metrics["battery_soh"], 99.0);
        assert_eq!(m.custom_metrics["heatsink_temp1_c"], 45.0);
        assert_eq!(m.custom_metrics["bms_min_cell_t_c"], 17.0);
    }

    #[test]
    fn drops_per_unit_and_unclassified_keys() {
        let a = reading(
            &[("pv1_voltage", 300.0), ("parallel_role", 1.0)],
            0.0,
            50.0,
            52.0,
            30.0,
        );
        let b = reading(
            &[("pv1_voltage", 310.0), ("parallel_role", 2.0)],
            0.0,
            50.0,
            52.0,
            30.0,
        );
        let m = combine(&[&a, &b]);
        assert!(m.pv_voltage.is_none());
        assert!(!m.custom_metrics.contains_key("pv1_voltage"));
        assert!(!m.custom_metrics.contains_key("parallel_role"));
    }

    #[test]
    fn omits_key_any_member_lacks() {
        // gen_power is only decoded when the AC input is a generator; a sum
        // over one unit must not be presented as the system total.
        let a = reading(&[("gen_power", 2000.0)], 0.0, 50.0, 52.0, 30.0);
        let b = reading(&[], 0.0, 50.0, 52.0, 30.0);
        let m = combine(&[&a, &b]);
        assert!(!m.custom_metrics.contains_key("gen_power"));
    }

    #[test]
    fn battery_current_derived_from_summed_power() {
        let a = reading(
            &[
                ("battery_charge_power", 1000.0),
                ("battery_discharge_power", 0.0),
            ],
            0.0,
            50.0,
            50.0,
            30.0,
        );
        let b = reading(
            &[
                ("battery_charge_power", 0.0),
                ("battery_discharge_power", 3500.0),
            ],
            0.0,
            50.0,
            50.0,
            30.0,
        );
        let m = combine(&[&a, &b]);
        assert_eq!(m.battery_current, Some(-50.0));
    }
}
