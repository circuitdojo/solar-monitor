//! Combined view of inverters wired in parallel.
//!
//! Every enabled device is a member of the system. Members are polled
//! independently, so their readings never share a timestamp; the
//! [`CombinedAccumulator`] keeps the latest reading per member and emits one
//! combined reading per *round* (every member has reported since the last
//! emit). The same accumulator drives the live WebSocket stream and the
//! history endpoint, so both produce identical points. Nothing is persisted.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use solar_monitor_core::{
    self as core, DeviceData, DeviceStatus, DeviceType, HealthStatus, ParallelCombiner,
};

/// `device_id` carried by combined readings.
pub const COMBINED_ID: &str = "combined";

/// The current set of members and how to combine them.
pub struct CombinedSystem {
    /// Member id → poll interval (seconds), used for the staleness bound.
    pub poll_secs: HashMap<String, u32>,
    pub device_type: DeviceType,
    pub combine: ParallelCombiner,
}

/// Resolve the members from the device registry, or explain why the enabled
/// devices can't be combined.
pub fn resolve(
    devices: &HashMap<String, core::DeviceConfig>,
    registry: &core::ProtocolRegistry,
) -> Result<CombinedSystem, String> {
    let enabled: Vec<&core::DeviceConfig> = devices.values().filter(|d| d.enabled).collect();
    if enabled.len() < 2 {
        return Err("a combined view needs at least two enabled devices".into());
    }
    let protocol = &enabled[0].protocol;
    if enabled.iter().any(|d| &d.protocol != protocol) {
        return Err("enabled devices use different protocols and can't be combined".into());
    }
    let combine = registry
        .get_protocol(protocol)
        .ok_or_else(|| format!("protocol '{protocol}' is not available"))?
        .parallel_combiner()
        .ok_or_else(|| format!("protocol '{protocol}' does not support a combined view"))?;
    Ok(CombinedSystem {
        poll_secs: enabled
            .iter()
            .map(|d| (d.id.clone(), d.poll_interval_seconds.max(5)))
            .collect(),
        device_type: enabled[0].device_type.clone(),
        combine,
    })
}

#[derive(Default)]
pub struct CombinedAccumulator {
    latest: HashMap<String, DeviceData>,
    last_emit: Option<DateTime<Utc>>,
}

impl CombinedAccumulator {
    /// Feed one member reading (in timestamp order). Returns a combined
    /// reading when this completes a round. A round in which some member's
    /// reading is more than two of its poll intervals older than the newest
    /// is dropped: a missing inverter must never yield a silent partial total.
    pub fn push(&mut self, data: DeviceData, sys: &CombinedSystem) -> Option<DeviceData> {
        if !sys.poll_secs.contains_key(&data.device_id) {
            return None;
        }
        self.latest.retain(|id, _| sys.poll_secs.contains_key(id));
        self.latest.insert(data.device_id.clone(), data);

        let readings: Vec<&DeviceData> = sys
            .poll_secs
            .keys()
            .map(|id| self.latest.get(id))
            .collect::<Option<_>>()?;
        if let Some(t) = self.last_emit
            && readings.iter().any(|d| d.timestamp <= t)
        {
            return None;
        }
        let newest = readings.iter().map(|d| d.timestamp).max()?;
        let stale = readings.iter().any(|d| {
            newest - d.timestamp > Duration::seconds(2 * sys.poll_secs[&d.device_id] as i64)
        });
        if stale {
            return None;
        }
        self.last_emit = Some(newest);

        Some(DeviceData {
            device_id: COMBINED_ID.into(),
            timestamp: newest,
            device_type: sys.device_type.clone(),
            metrics: (sys.combine)(&readings),
            status: DeviceStatus {
                is_connected: readings.iter().all(|d| d.status.is_connected),
                last_seen: newest,
                health: if readings
                    .iter()
                    .all(|d| d.status.health == HealthStatus::Healthy)
                {
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Warning
                },
                error_message: None,
            },
            raw_data: None,
        })
    }
}

/// Live combiner: turns member readings from the device broadcast into
/// combined readings on `AppState::combined_tx`. Membership is re-resolved
/// per reading, so adding/removing/disabling devices takes effect at once.
pub fn spawn(state: Arc<crate::AppState>) {
    let mut rx = state.tx.subscribe();
    tokio::spawn(async move {
        let mut acc = CombinedAccumulator::default();
        loop {
            match rx.recv().await {
                Ok(data) => {
                    let sys = resolve(&*state.devices.lock().await, &state.registry);
                    if let Ok(sys) = sys
                        && let Some(c) = acc.push(data, &sys)
                    {
                        let _ = state.combined_tx.send(c);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_monitor_core::DeviceMetrics;

    fn combine_sum_pv(members: &[&DeviceData]) -> DeviceMetrics {
        DeviceMetrics {
            pv_power_watts: Some(
                members
                    .iter()
                    .filter_map(|d| d.metrics.pv_power_watts)
                    .sum(),
            ),
            ..Default::default()
        }
    }

    fn sys(ids: &[&str]) -> CombinedSystem {
        CombinedSystem {
            poll_secs: ids.iter().map(|id| (id.to_string(), 10)).collect(),
            device_type: DeviceType::SolarInverter,
            combine: combine_sum_pv,
        }
    }

    fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn reading(id: &str, secs: i64, pv: f64) -> DeviceData {
        let ts = t0() + Duration::seconds(secs);
        DeviceData {
            device_id: id.into(),
            timestamp: ts,
            device_type: DeviceType::SolarInverter,
            metrics: DeviceMetrics {
                pv_power_watts: Some(pv),
                ..Default::default()
            },
            status: DeviceStatus {
                is_connected: true,
                last_seen: ts,
                health: HealthStatus::Healthy,
                error_message: None,
            },
            raw_data: None,
        }
    }

    #[test]
    fn emits_once_per_round() {
        let s = sys(&["a", "b"]);
        let mut acc = CombinedAccumulator::default();
        assert!(acc.push(reading("a", 0, 100.0), &s).is_none());
        let c = acc.push(reading("b", 2, 50.0), &s).unwrap();
        assert_eq!(c.device_id, COMBINED_ID);
        assert_eq!(c.timestamp, t0() + Duration::seconds(2));
        assert_eq!(c.metrics.pv_power_watts, Some(150.0));
        // a reports again: b hasn't, so no new round yet
        assert!(acc.push(reading("a", 10, 200.0), &s).is_none());
        let c = acc.push(reading("b", 12, 60.0), &s).unwrap();
        assert_eq!(c.metrics.pv_power_watts, Some(260.0));
    }

    #[test]
    fn stale_member_blocks_output() {
        let s = sys(&["a", "b"]);
        let mut acc = CombinedAccumulator::default();
        acc.push(reading("a", 0, 100.0), &s);
        // b's first reading arrives long after a's: a is stale (> 2 × 10 s)
        assert!(acc.push(reading("b", 60, 50.0), &s).is_none());
        // a recovers: round completes with fresh readings
        let c = acc.push(reading("a", 62, 100.0), &s).unwrap();
        assert_eq!(c.metrics.pv_power_watts, Some(150.0));
    }

    #[test]
    fn ignores_non_members() {
        let s = sys(&["a", "b"]);
        let mut acc = CombinedAccumulator::default();
        acc.push(reading("a", 0, 100.0), &s);
        assert!(acc.push(reading("z", 1, 999.0), &s).is_none());
        let c = acc.push(reading("b", 2, 50.0), &s).unwrap();
        assert_eq!(c.metrics.pv_power_watts, Some(150.0));
    }

    #[test]
    fn interleaved_history_combines_per_round() {
        let s = sys(&["a", "b"]);
        let mut rows = vec![
            reading("a", 0, 1.0),
            reading("a", 10, 2.0),
            reading("a", 20, 3.0),
            reading("b", 5, 10.0),
            reading("b", 15, 20.0),
            reading("b", 25, 30.0),
        ];
        rows.sort_by_key(|d| d.timestamp);
        let mut acc = CombinedAccumulator::default();
        let out: Vec<f64> = rows
            .into_iter()
            .filter_map(|d| acc.push(d, &s))
            .map(|c| c.metrics.pv_power_watts.unwrap())
            .collect();
        assert_eq!(out, vec![11.0, 22.0, 33.0]);
    }

    fn cfg(id: &str, protocol: &str, enabled: bool) -> core::DeviceConfig {
        core::DeviceConfig {
            id: id.into(),
            name: id.into(),
            device_type: DeviceType::SolarInverter,
            protocol: protocol.into(),
            connection_params: HashMap::new(),
            enabled,
            poll_interval_seconds: 10,
        }
    }

    #[test]
    fn resolve_requires_two_enabled_same_protocol_supported() {
        let registry = solar_monitor_protocols::create_registry();
        let proto = registry.protocols().next().unwrap().protocol_name();
        let devs = |list: Vec<core::DeviceConfig>| -> HashMap<String, core::DeviceConfig> {
            list.into_iter().map(|d| (d.id.clone(), d)).collect()
        };

        let one = devs(vec![cfg("a", proto, true), cfg("b", proto, false)]);
        assert!(resolve(&one, &registry).is_err());

        let mixed = devs(vec![cfg("a", proto, true), cfg("b", "other", true)]);
        assert!(resolve(&mixed, &registry).is_err());

        let unknown = devs(vec![cfg("a", "other", true), cfg("b", "other", true)]);
        assert!(resolve(&unknown, &registry).is_err());

        let ok = devs(vec![
            cfg("a", proto, true),
            cfg("b", proto, true),
            cfg("c", proto, false),
        ]);
        let sys = resolve(&ok, &registry).unwrap();
        let mut ids: Vec<_> = sys.poll_secs.keys().cloned().collect();
        ids.sort();
        assert_eq!(ids, vec!["a", "b"]);
    }
}
