//! SQLite storage implementation for universal device data

use anyhow::Result;
use chrono::{DateTime, Utc};
use contracts::{DeviceData, DeviceMetrics, DeviceStatus, DeviceType, HealthStatus};
use solar_monitor_core::DeviceConfig;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

pub struct DataStore {
    pool: SqlitePool,
}

impl DataStore {
    pub async fn new(database_path: &str) -> Result<Self> {
        // Ensure parent directory exists (best-effort)
        if let Some(parent) = std::path::Path::new(database_path).parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
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
        let row = sqlx::query("SELECT * FROM device_data WHERE device_id = ? ORDER BY timestamp DESC LIMIT 1")
            .bind(device_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(row_to_device_data))
    }

    pub async fn get_device_data_range(
        &self,
        device_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceData>> {
        let limit = limit.unwrap_or(1000).min(10_000);

        let rows = sqlx::query(
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

        Ok(rows.into_iter().map(row_to_device_data).collect())
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
            let connection_params: std::collections::HashMap<String, String> = serde_json::from_str(&params).unwrap_or_default();
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
            let connection_params: std::collections::HashMap<String, String> = serde_json::from_str(&params).unwrap_or_default();
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

fn device_type_to_str(dt: DeviceType) -> String {
    match dt {
        DeviceType::SolarInverter => "solarinverter",
        DeviceType::BatterySystem => "batterysystem",
        DeviceType::ChargeController => "chargecontroller",
        DeviceType::EnergyMeter => "energymeter",
    }
    .to_string()
}
