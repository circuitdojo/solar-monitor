//! SQLite storage implementation for universal device data

use anyhow::Result;
use chrono::{DateTime, Utc};
use contracts::{DeviceData, DeviceMetrics, DeviceStatus, DeviceType, HealthStatus};
use solar_monitor_core::DeviceConfig;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

pub struct DataStore {
    pool: SqlitePool,
}

impl DataStore {
    pub async fn new(database_path: &str) -> Result<Self> {
        // Ensure parent directory exists (best-effort)
        if let Some(parent) = std::path::Path::new(database_path).parent()
            && !parent.as_os_str().is_empty()
        {
            let _ = std::fs::create_dir_all(parent);
        }

        // Build connection URL.
        // Special-case in-memory to ensure correct behavior with pools.
        let (url, max_conns) = if database_path == ":memory:" {
            // Shared-cache memory so all pooled connections see same DB.
            ("sqlite::memory:?cache=shared".to_string(), 1u32)
        } else {
            // Best-effort: pre-create the SQLite file to avoid implicit creation issues.
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(false)
                .open(database_path);
            (format!("sqlite://{}?mode=rwc", database_path), 5u32)
        };

        let pool = SqlitePoolOptions::new()
            .max_connections(max_conns)
            .connect(&url)
            .await?;

        // Run embedded migrations from the workspace migrations directory
        // Path is relative to this crate at compile time
        sqlx::migrate!("../migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn store_device_data(&self, data: &DeviceData) -> Result<()> {
        let metrics_json = serde_json::to_string(&data.metrics)?;
        let status_json = serde_json::to_string(&data.status)?;
        let device_type_str = device_type_to_str(data.device_type.clone());

        sqlx::query(
            r#"
            INSERT INTO device_data (
                device_id, timestamp, device_type, metrics, status, raw_data
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&data.device_id)
        .bind(data.timestamp)
        .bind(&device_type_str)
        .bind(&metrics_json)
        .bind(&status_json)
        .bind(&data.raw_data)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_latest_device_data(&self, device_id: &str) -> Result<Option<DeviceData>> {
        let row = sqlx::query(
            "SELECT * FROM device_data WHERE device_id = ? ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(row_to_device_data))
    }

    /// Range query over full-resolution rows merged with hourly aggregates
    /// (older data lives only in device_data_hourly after downsampling; its
    /// points carry the hour's average, timestamped at the bucket start).
    pub async fn get_device_data_range(
        &self,
        device_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceData>> {
        let limit = limit.unwrap_or(1000).min(10_000) as usize;

        let raw = sqlx::query(
            r#"
            SELECT * FROM device_data
            WHERE device_id = ? AND timestamp BETWEEN ? AND ?
            ORDER BY timestamp ASC
            LIMIT ?
            "#,
        )
        .bind(device_id)
        .bind(start_time)
        .bind(end_time)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let hourly = sqlx::query(
            r#"
            SELECT device_id, hour_start, device_type, metrics_avg
            FROM device_data_hourly
            WHERE device_id = ? AND hour_start BETWEEN ? AND ?
            ORDER BY hour_start ASC
            LIMIT ?
            "#,
        )
        .bind(device_id)
        .bind(start_time)
        .bind(end_time)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        use sqlx::Row;
        let mut out: Vec<DeviceData> = hourly
            .into_iter()
            .map(|r| {
                let timestamp: DateTime<Utc> = r.get("hour_start");
                let metrics_json: String = r.get("metrics_avg");
                DeviceData {
                    device_id: r.get("device_id"),
                    timestamp,
                    device_type: str_to_device_type(&r.get::<String, _>("device_type")),
                    metrics: serde_json::from_str(&metrics_json).unwrap_or_default(),
                    status: DeviceStatus {
                        is_connected: true,
                        last_seen: timestamp,
                        health: HealthStatus::Healthy,
                        error_message: None,
                    },
                    raw_data: None,
                }
            })
            .collect();
        out.extend(raw.into_iter().map(row_to_device_data));
        out.sort_by_key(|d| d.timestamp);
        out.truncate(limit);
        Ok(out)
    }

    /// Fold full-resolution rows older than `keep_full_days` into hourly
    /// avg/min/max aggregates, then delete them. Only complete hour buckets
    /// (entirely before the hour-aligned cutoff) are touched. Returns
    /// (raw rows pruned, hourly rows written).
    pub async fn downsample_and_prune(&self, keep_full_days: u32) -> Result<(u64, u64)> {
        use chrono::DurationRound;
        use sqlx::Row;

        let cutoff = (Utc::now() - chrono::Duration::days(keep_full_days as i64))
            .duration_trunc(chrono::Duration::hours(1))?;

        let mut rows_pruned = 0u64;
        let mut hours_written = 0u64;
        loop {
            // Oldest remaining raw row before the cutoff picks the next bucket
            let oldest = sqlx::query(
                "SELECT device_id, timestamp, device_type FROM device_data
                 WHERE timestamp < ? ORDER BY timestamp ASC LIMIT 1",
            )
            .bind(cutoff)
            .fetch_optional(&self.pool)
            .await?;
            let Some(r) = oldest else { break };
            let device_id: String = r.get("device_id");
            let ts: DateTime<Utc> = r.get("timestamp");
            let device_type: String = r.get("device_type");
            let hour_start = ts.duration_trunc(chrono::Duration::hours(1))?;
            let hour_end = hour_start + chrono::Duration::hours(1);

            let samples = sqlx::query(
                "SELECT metrics FROM device_data
                 WHERE device_id = ? AND timestamp >= ? AND timestamp < ?",
            )
            .bind(&device_id)
            .bind(hour_start)
            .bind(hour_end)
            .fetch_all(&self.pool)
            .await?;
            let values: Vec<serde_json::Value> = samples
                .iter()
                .filter_map(|r| serde_json::from_str(&r.get::<String, _>("metrics")).ok())
                .collect();
            let count = values.len() as i64;
            let (avg, min, max) = aggregate_json(&values);

            let mut tx = self.pool.begin().await?;
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO device_data_hourly
                    (device_id, hour_start, device_type, metrics_avg, metrics_min, metrics_max, sample_count)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&device_id)
            .bind(hour_start)
            .bind(&device_type)
            .bind(serde_json::to_string(&avg)?)
            .bind(serde_json::to_string(&min)?)
            .bind(serde_json::to_string(&max)?)
            .bind(count)
            .execute(&mut *tx)
            .await?;
            let deleted = sqlx::query(
                "DELETE FROM device_data
                 WHERE device_id = ? AND timestamp >= ? AND timestamp < ?",
            )
            .bind(&device_id)
            .bind(hour_start)
            .bind(hour_end)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;

            rows_pruned += deleted.rows_affected();
            hours_written += 1;
        }
        Ok((rows_pruned, hours_written))
    }

    // Device config persistence
    pub async fn upsert_device_config(&self, cfg: &DeviceConfig) -> Result<()> {
        let dtype = device_type_to_str(cfg.device_type.clone());
        let params = serde_json::to_string(&cfg.connection_params)?;
        sqlx::query(
            r#"
            INSERT INTO devices (id, name, device_type, protocol, connection_params, enabled, poll_interval_seconds)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                device_type=excluded.device_type,
                protocol=excluded.protocol,
                connection_params=excluded.connection_params,
                enabled=excluded.enabled,
                poll_interval_seconds=excluded.poll_interval_seconds
            "#,
        )
        .bind(&cfg.id)
        .bind(&cfg.name)
        .bind(&dtype)
        .bind(&cfg.protocol)
        .bind(&params)
        .bind(cfg.enabled as i32)
        .bind(cfg.poll_interval_seconds as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_device_config(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM devices WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_device_configs(&self) -> Result<Vec<DeviceConfig>> {
        let rows = sqlx::query("SELECT * FROM devices ORDER BY created_at ASC")
            .fetch_all(&self.pool)
            .await?;
        use sqlx::Row;
        let mut out = Vec::new();
        for r in rows {
            let id: String = r.get("id");
            let name: String = r.get("name");
            let dt: String = r.get("device_type");
            let protocol: String = r.get("protocol");
            let params: String = r.get("connection_params");
            let enabled: i64 = r.get("enabled");
            let poll: i64 = r.get("poll_interval_seconds");
            let device_type = match dt.as_str() {
                "solarinverter" => DeviceType::SolarInverter,
                "batterysystem" => DeviceType::BatterySystem,
                "chargecontroller" => DeviceType::ChargeController,
                "energymeter" => DeviceType::EnergyMeter,
                _ => DeviceType::SolarInverter,
            };
            let connection_params: std::collections::HashMap<String, String> =
                serde_json::from_str(&params).unwrap_or_default();
            out.push(DeviceConfig {
                id,
                name,
                device_type,
                protocol,
                connection_params,
                enabled: enabled != 0,
                poll_interval_seconds: poll as u32,
            });
        }
        Ok(out)
    }

    pub async fn get_device_config(&self, id: &str) -> Result<Option<DeviceConfig>> {
        let row = sqlx::query("SELECT * FROM devices WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| {
            use sqlx::Row;
            let id: String = r.get("id");
            let name: String = r.get("name");
            let dt: String = r.get("device_type");
            let protocol: String = r.get("protocol");
            let params: String = r.get("connection_params");
            let enabled: i64 = r.get("enabled");
            let poll: i64 = r.get("poll_interval_seconds");
            let device_type = match dt.as_str() {
                "solarinverter" => DeviceType::SolarInverter,
                "batterysystem" => DeviceType::BatterySystem,
                "chargecontroller" => DeviceType::ChargeController,
                "energymeter" => DeviceType::EnergyMeter,
                _ => DeviceType::SolarInverter,
            };
            let connection_params: std::collections::HashMap<String, String> =
                serde_json::from_str(&params).unwrap_or_default();
            DeviceConfig {
                id,
                name,
                device_type,
                protocol,
                connection_params,
                enabled: enabled != 0,
                poll_interval_seconds: poll as u32,
            }
        }))
    }
}

impl DataStore {
    // Notification channels

    pub async fn upsert_notification_channel(
        &self,
        ch: &contracts::NotificationChannelDto,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO notification_channels (id, name, kind, config, enabled)
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                kind=excluded.kind,
                config=excluded.config,
                enabled=excluded.enabled
            "#,
        )
        .bind(&ch.id)
        .bind(&ch.name)
        .bind(enum_str(&ch.kind)?)
        .bind(serde_json::to_string(&ch.config)?)
        .bind(ch.enabled as i32)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_notification_channels(
        &self,
    ) -> Result<Vec<contracts::NotificationChannelDto>> {
        use sqlx::Row;
        let rows = sqlx::query("SELECT * FROM notification_channels ORDER BY created_at ASC")
            .fetch_all(&self.pool)
            .await?;
        let mut out = Vec::new();
        for r in rows {
            let kind_s: String = r.get("kind");
            let config_s: String = r.get("config");
            out.push(contracts::NotificationChannelDto {
                id: r.get("id"),
                name: r.get("name"),
                kind: enum_from_str(&kind_s)?,
                config: serde_json::from_str(&config_s)?,
                enabled: r.get::<i64, _>("enabled") != 0,
            });
        }
        Ok(out)
    }

    pub async fn delete_notification_channel(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM notification_channels WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // Notification rules

    pub async fn upsert_notification_rule(
        &self,
        rule: &contracts::NotificationRuleDto,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO notification_rules
                (id, name, event, device_id, params, channel_ids, enabled, cooldown_seconds)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                event=excluded.event,
                device_id=excluded.device_id,
                params=excluded.params,
                channel_ids=excluded.channel_ids,
                enabled=excluded.enabled,
                cooldown_seconds=excluded.cooldown_seconds
            "#,
        )
        .bind(&rule.id)
        .bind(&rule.name)
        .bind(enum_str(&rule.event)?)
        .bind(&rule.device_id)
        .bind(serde_json::to_string(&rule.params)?)
        .bind(serde_json::to_string(&rule.channel_ids)?)
        .bind(rule.enabled as i32)
        .bind(rule.cooldown_seconds as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_notification_rules(&self) -> Result<Vec<contracts::NotificationRuleDto>> {
        use sqlx::Row;
        let rows = sqlx::query("SELECT * FROM notification_rules ORDER BY created_at ASC")
            .fetch_all(&self.pool)
            .await?;
        let mut out = Vec::new();
        for r in rows {
            let event_s: String = r.get("event");
            let params_s: String = r.get("params");
            let channels_s: String = r.get("channel_ids");
            out.push(contracts::NotificationRuleDto {
                id: r.get("id"),
                name: r.get("name"),
                event: enum_from_str(&event_s)?,
                device_id: r.get("device_id"),
                params: serde_json::from_str(&params_s)?,
                channel_ids: serde_json::from_str(&channels_s)?,
                enabled: r.get::<i64, _>("enabled") != 0,
                cooldown_seconds: r.get::<i64, _>("cooldown_seconds") as u32,
            });
        }
        Ok(out)
    }

    pub async fn delete_notification_rule(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM notification_rules WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // Notification delivery log

    /// Retention cap: newest rows kept, oldest pruned on insert.
    const NOTIFICATION_LOG_CAP: i64 = 1000;

    pub async fn append_notification_log(
        &self,
        entry: &contracts::NotificationLogEntryDto,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO notification_log
                (timestamp, rule_id, rule_name, device_id, title, body,
                 channel_id, channel_name, ok, error)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(entry.timestamp)
        .bind(&entry.rule_id)
        .bind(&entry.rule_name)
        .bind(&entry.device_id)
        .bind(&entry.title)
        .bind(&entry.body)
        .bind(&entry.channel_id)
        .bind(&entry.channel_name)
        .bind(entry.ok as i32)
        .bind(&entry.error)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "DELETE FROM notification_log WHERE id NOT IN
             (SELECT id FROM notification_log ORDER BY id DESC LIMIT ?)",
        )
        .bind(Self::NOTIFICATION_LOG_CAP)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_notification_log(
        &self,
        limit: u32,
    ) -> Result<Vec<contracts::NotificationLogEntryDto>> {
        use sqlx::Row;
        let rows = sqlx::query("SELECT * FROM notification_log ORDER BY id DESC LIMIT ?")
            .bind(limit.min(1000) as i64)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| contracts::NotificationLogEntryDto {
                id: r.get("id"),
                timestamp: r.get("timestamp"),
                rule_id: r.get("rule_id"),
                rule_name: r.get("rule_name"),
                device_id: r.get("device_id"),
                title: r.get("title"),
                body: r.get("body"),
                channel_id: r.get("channel_id"),
                channel_name: r.get("channel_name"),
                ok: r.get::<i64, _>("ok") != 0,
                error: r.get("error"),
            })
            .collect())
    }
}

/// Store serde-renamed enums as their bare JSON string (e.g. "gridState").
fn enum_str<T: serde::Serialize>(v: &T) -> Result<String> {
    Ok(serde_json::to_string(v)?.trim_matches('"').to_string())
}

fn enum_from_str<T: serde::de::DeserializeOwned>(s: &str) -> Result<T> {
    Ok(serde_json::from_str(&format!("\"{}\"", s))?)
}

fn row_to_device_data(r: sqlx::sqlite::SqliteRow) -> DeviceData {
    // Use column getters by name
    // We must map the row struct; since query! macro returns a struct, but here we used dynamic Row for flexibility
    use sqlx::Row;

    let device_id: String = r.get("device_id");
    let timestamp: DateTime<Utc> = r.get("timestamp");
    let device_type: String = r.get("device_type");
    let metrics_json: String = r.get("metrics");
    let status_json: String = r.get("status");
    let raw_data: Option<String> = r.try_get("raw_data").ok();

    let device_type = match device_type.as_str() {
        "solarinverter" => DeviceType::SolarInverter,
        "batterysystem" => DeviceType::BatterySystem,
        "chargecontroller" => DeviceType::ChargeController,
        "energymeter" => DeviceType::EnergyMeter,
        _ => DeviceType::SolarInverter,
    };

    let metrics: DeviceMetrics = serde_json::from_str(&metrics_json).unwrap_or_default();
    let status: DeviceStatus = serde_json::from_str(&status_json).unwrap_or(DeviceStatus {
        is_connected: false,
        last_seen: timestamp,
        health: HealthStatus::Offline,
        error_message: Some("unavailable".to_string()),
    });

    DeviceData {
        device_id,
        timestamp,
        device_type,
        metrics,
        status,
        raw_data,
    }
}

fn str_to_device_type(s: &str) -> DeviceType {
    match s {
        "solarinverter" => DeviceType::SolarInverter,
        "batterysystem" => DeviceType::BatterySystem,
        "chargecontroller" => DeviceType::ChargeController,
        "energymeter" => DeviceType::EnergyMeter,
        _ => DeviceType::SolarInverter,
    }
}

/// Recursively aggregate JSON metric samples into (avg, min, max), keeping
/// the DeviceMetrics shape: numeric leaves are aggregated; objects recurse
/// over the union of keys (so intermittent customMetrics like gen_power
/// still aggregate); anything else (strings, arrays, bools) carries the
/// last non-null sample in all three outputs.
fn aggregate_json(
    samples: &[serde_json::Value],
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    use serde_json::Value;

    let present: Vec<&Value> = samples.iter().filter(|v| !v.is_null()).collect();
    let Some(last) = present.last() else {
        return (Value::Null, Value::Null, Value::Null);
    };

    if present.iter().all(|v| v.is_object()) {
        let mut keys: Vec<&String> = present
            .iter()
            .flat_map(|v| v.as_object().unwrap().keys())
            .collect();
        keys.sort();
        keys.dedup();
        let mut avg = serde_json::Map::new();
        let mut min = serde_json::Map::new();
        let mut max = serde_json::Map::new();
        for key in keys {
            let children: Vec<Value> = present
                .iter()
                .filter_map(|v| v.as_object().unwrap().get(key).cloned())
                .collect();
            let (a, lo, hi) = aggregate_json(&children);
            avg.insert(key.clone(), a);
            min.insert(key.clone(), lo);
            max.insert(key.clone(), hi);
        }
        return (Value::Object(avg), Value::Object(min), Value::Object(max));
    }

    let nums: Vec<f64> = present.iter().filter_map(|v| v.as_f64()).collect();
    if !nums.is_empty() {
        let n = |f: f64| {
            serde_json::Number::from_f64(f)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        };
        let avg = nums.iter().sum::<f64>() / nums.len() as f64;
        let lo = nums.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = nums.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        return (n(avg), n(lo), n(hi));
    }

    ((*last).clone(), (*last).clone(), (*last).clone())
}

fn device_type_to_str(dt: DeviceType) -> String {
    match dt {
        DeviceType::SolarInverter => "solarinverter",
        DeviceType::BatterySystem => "batterysystem",
        DeviceType::ChargeController => "chargecontroller",
        DeviceType::EnergyMeter => "energymeter",
    }
    .to_string()
}
