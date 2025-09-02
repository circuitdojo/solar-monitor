//! SQLite storage implementation for universal device data

use anyhow::Result;
use chrono::{DateTime, Utc};
use contracts::{DeviceData, DeviceMetrics, DeviceStatus, DeviceType, HealthStatus};
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

        let url = format!("sqlite:{}", database_path);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
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
        let device_type_str = device_type_to_str(data.device_type);

        sqlx::query!(
            r#"
            INSERT INTO device_data (
                device_id, timestamp, device_type, metrics, status, raw_data
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
            data.device_id,
            data.timestamp,
            device_type_str,
            metrics_json,
            status_json,
            data.raw_data
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_latest_device_data(&self, device_id: &str) -> Result<Option<DeviceData>> {
        let row = sqlx::query!(
            "SELECT * FROM device_data WHERE device_id = ? ORDER BY timestamp DESC LIMIT 1",
            device_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| row_to_device_data(r)))
    }

    pub async fn get_device_data_range(
        &self,
        device_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceData>> {
        let limit = limit.unwrap_or(1000).min(10_000);

        let rows = sqlx::query!(
            r#"
            SELECT * FROM device_data
            WHERE device_id = ? AND timestamp BETWEEN ? AND ?
            ORDER BY timestamp ASC
            LIMIT ?
            "#,
            device_id,
            start_time,
            end_time,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(row_to_device_data).collect())
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

