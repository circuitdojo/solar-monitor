//! Event detectors: map a `DeviceData` sample to a boolean condition with
//! hysteresis, plus the human-readable transition messages.

use contracts::{DeviceData, NotificationEvent, NotificationRuleDto};

pub struct Transition {
    /// true = the alert condition became active (e.g. grid lost);
    /// false = it cleared (e.g. grid restored)
    pub triggered: bool,
    pub title: String,
    pub body: String,
}

pub fn param(rule: &NotificationRuleDto, key: &str, default: f64) -> f64 {
    rule.params.get(key).copied().unwrap_or(default)
}

/// Evaluate a rule against a sample. Returns `Some((active, transition))`
/// where `transition` describes the message to send if this sample flips the
/// state; `None` when the metric is unavailable or in the hysteresis band
/// (state unchanged).
pub fn evaluate(rule: &NotificationRuleDto, d: &DeviceData) -> Option<(bool, Option<Transition>)> {
    let m = &d.metrics;
    match rule.event {
        NotificationEvent::GridState => {
            let v = m.grid_voltage?;
            let lost_below = param(rule, "lostBelow", 80.0);
            let restored_above = param(rule, "restoredAbove", 100.0);
            hysteresis(
                v,
                lost_below,
                restored_above,
                Transition {
                    triggered: true,
                    title: format!("Grid lost — {}", d.device_id),
                    body: format!("Grid voltage {:.1} V on {}.", v, d.device_id),
                },
                Transition {
                    triggered: false,
                    title: format!("Grid restored — {}", d.device_id),
                    body: format!("Grid voltage {:.1} V on {}.", v, d.device_id),
                },
            )
        }
        NotificationEvent::BatteryLow => {
            let soc = m.battery_soc_percentage?;
            let low_below = param(rule, "lowBelow", 20.0);
            let recovered_above = param(rule, "recoveredAbove", 30.0);
            hysteresis(
                soc,
                low_below,
                recovered_above,
                Transition {
                    triggered: true,
                    title: format!("Battery low — {}", d.device_id),
                    body: format!("Battery at {:.0}% on {}.", soc, d.device_id),
                },
                Transition {
                    triggered: false,
                    title: format!("Battery recovered — {}", d.device_id),
                    body: format!("Battery back to {:.0}% on {}.", soc, d.device_id),
                },
            )
        }
        NotificationEvent::Generator => {
            // Gen input registers hold junk unless the AC input type is
            // Generator — gate on the flag the driver exposes.
            if m.custom_metrics.get("ac_input_is_generator").copied() != Some(1.0) {
                return None;
            }
            let p = m.custom_metrics.get("gen_power").copied()?;
            let start_above = param(rule, "startAbove", 100.0);
            let stop_below = param(rule, "stopBelow", 50.0);
            // Inverted sense vs. hysteresis(): high value = active
            if p > start_above {
                Some((
                    true,
                    Some(Transition {
                        triggered: true,
                        title: format!("Generator running — {}", d.device_id),
                        body: format!("Generator at {:.0} W on {}.", p, d.device_id),
                    }),
                ))
            } else if p < stop_below {
                Some((
                    false,
                    Some(Transition {
                        triggered: false,
                        title: format!("Generator stopped — {}", d.device_id),
                        body: format!("Generator input idle on {}.", d.device_id),
                    }),
                ))
            } else {
                None
            }
        }
        // Offline detection is time-based, handled by the engine ticker
        NotificationEvent::DeviceOffline => None,
    }
}

/// Low value = alert active, high value = cleared, in-between = no change.
fn hysteresis(
    value: f64,
    active_below: f64,
    clear_above: f64,
    on_active: Transition,
    on_clear: Transition,
) -> Option<(bool, Option<Transition>)> {
    if value < active_below {
        Some((true, Some(on_active)))
    } else if value > clear_above {
        Some((false, Some(on_clear)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn rule(event: NotificationEvent, params: &[(&str, f64)]) -> NotificationRuleDto {
        NotificationRuleDto {
            id: "r1".into(),
            name: "test".into(),
            event,
            device_id: None,
            params: params.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            channel_ids: vec![],
            enabled: true,
            cooldown_seconds: 0,
        }
    }

    fn sample(grid_v: Option<f64>, soc: Option<f64>, custom: &[(&str, f64)]) -> DeviceData {
        DeviceData {
            device_id: "dev".into(),
            timestamp: chrono::Utc::now(),
            device_type: contracts::DeviceType::SolarInverter,
            metrics: contracts::DeviceMetrics {
                grid_voltage: grid_v,
                battery_soc_percentage: soc,
                custom_metrics: custom.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
                ..Default::default()
            },
            status: contracts::DeviceStatus {
                is_connected: true,
                last_seen: chrono::Utc::now(),
                health: contracts::HealthStatus::Healthy,
                error_message: None,
            },
            raw_data: None,
        }
    }

    #[test]
    fn grid_hysteresis() {
        let r = rule(NotificationEvent::GridState, &[]);
        // lost below 80
        let (active, t) = evaluate(&r, &sample(Some(3.0), None, &[])).unwrap();
        assert!(active);
        assert!(t.unwrap().title.starts_with("Grid lost"));
        // dead band: no state
        assert!(evaluate(&r, &sample(Some(90.0), None, &[])).is_none());
        // restored above 100
        let (active, t) = evaluate(&r, &sample(Some(240.0), None, &[])).unwrap();
        assert!(!active);
        assert!(t.unwrap().title.starts_with("Grid restored"));
        // metric missing: no evaluation
        assert!(evaluate(&r, &sample(None, None, &[])).is_none());
    }

    #[test]
    fn battery_thresholds_from_params() {
        let r = rule(
            NotificationEvent::BatteryLow,
            &[("lowBelow", 40.0), ("recoveredAbove", 55.0)],
        );
        assert!(evaluate(&r, &sample(None, Some(35.0), &[])).unwrap().0);
        assert!(evaluate(&r, &sample(None, Some(50.0), &[])).is_none());
        assert!(!evaluate(&r, &sample(None, Some(60.0), &[])).unwrap().0);
    }

    #[test]
    fn generator_gated_on_input_type() {
        let r = rule(NotificationEvent::Generator, &[]);
        // Gen metrics present but AC input is grid: never evaluate
        assert!(evaluate(&r, &sample(None, None, &[("gen_power", 5000.0)])).is_none());
        let on = sample(
            None,
            None,
            &[("ac_input_is_generator", 1.0), ("gen_power", 5000.0)],
        );
        assert!(evaluate(&r, &on).unwrap().0);
        let off = sample(
            None,
            None,
            &[("ac_input_is_generator", 1.0), ("gen_power", 0.0)],
        );
        assert!(!evaluate(&r, &off).unwrap().0);
    }
}
