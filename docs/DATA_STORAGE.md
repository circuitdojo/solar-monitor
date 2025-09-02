# Universal Data Storage - Edge Device Specification

## Overview

Device-agnostic SQLite-based data storage for universal solar monitoring on edge devices. Supports inverters, batteries, charge controllers, and energy meters through a unified schema with JSON blob storage for protocol-specific metrics.

## Architecture Components

### 1. Simple SQLite Storage

```rust
use sqlx::SqlitePool;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub struct DataStore {
    pool: SqlitePool,
    config: StorageConfig,
}

// Universal device data structure supporting all device types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceData {
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub device_type: DeviceType,
    pub metrics: DeviceMetrics,      // Universal normalized metrics 
    pub status: DeviceStatus,        // Device health and operational state
    pub raw_data: Option<String>,    // Protocol-specific raw response
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeviceType {
    SolarInverter,
    BatterySystem,
    ChargeController,
    EnergyMeter,
}

// Device-agnostic metrics that work across all device types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceMetrics {
    // Power metrics (all device types)
    pub input_power_watts: Option<f64>,
    pub output_power_watts: Option<f64>,
    pub load_percentage: Option<f64>,
    
    // Battery metrics (inverters, battery systems)
    pub battery_voltage: Option<f64>,
    pub battery_current: Option<f64>,
    pub battery_soc_percentage: Option<f64>,
    pub battery_temperature_celsius: Option<f64>,
    
    // Solar metrics (inverters, charge controllers)
    pub pv_voltage: Option<f64>,
    pub pv_current: Option<f64>,
    pub pv_power_watts: Option<f64>,
    
    // Grid metrics (inverters, energy meters)
    pub grid_voltage: Option<f64>,
    pub grid_frequency: Option<f64>, 
    pub grid_power_watts: Option<f64>,
    
    // Device health
    pub device_temperature_celsius: Option<f64>,
    pub efficiency_percentage: Option<f64>,
    pub fault_codes: Vec<String>,
    pub operating_mode: Option<String>,
    
    // Protocol-specific extension point
    pub custom_metrics: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub is_connected: bool,
    pub last_seen: DateTime<Utc>,
    pub health: HealthStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Offline,
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub database_path: String,
    pub retention_days: u32,
    pub cleanup_interval_hours: u32,
}

impl DataStore {
    pub async fn new(config: StorageConfig) -> Result<Self> {
        // Create database file and directory if they don't exist
        if let Some(parent) = std::path::Path::new(&config.database_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let pool = SqlitePool::connect(&format!("sqlite:{}", config.database_path)).await?;

        // Run embedded migrations
        sqlx::migrate!("./migrations").run(&pool).await?;

        let store = Self { pool, config };

        // Start cleanup task
        store.start_cleanup_task();

        Ok(store)
    }

    pub async fn store_data(&self, data: &DeviceData) -> Result<()> {
        let metrics_json = serde_json::to_string(&data.metrics)?;
        let status_json = serde_json::to_string(&data.status)?;
        // Store canonical serialized device_type string via serde
        let device_type = serde_json::to_string(&data.device_type)?;
        
        sqlx::query!(
            r#"
            INSERT INTO device_data (
                device_id, timestamp, device_type, metrics, status, raw_data
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
            data.device_id,
            data.timestamp,
            device_type,
            metrics_json,
            status_json,
            data.raw_data
        ).execute(&self.pool).await?;

        Ok(())
    }

    pub async fn get_latest_data(&self, device_id: &str) -> Result<Option<DeviceData>> {
        let row = sqlx::query!(
            "SELECT * FROM device_data WHERE device_id = ? ORDER BY timestamp DESC LIMIT 1",
            device_id
        ).fetch_optional(&self.pool).await?;

        if let Some(r) = row {
            // device_type stored as lowercase string (e.g., "solarinverter")
            let device_type = match r.device_type.as_str() {
                "solarinverter" => DeviceType::SolarInverter,
                "batterysystem" => DeviceType::BatterySystem,
                "chargecontroller" => DeviceType::ChargeController,
                "energymeter" => DeviceType::EnergyMeter,
                _ => DeviceType::SolarInverter,
            };
            
            let metrics: DeviceMetrics = serde_json::from_str(&r.metrics)?;
            let status: DeviceStatus = serde_json::from_str(&r.status)?;
            
            Ok(Some(DeviceData {
                device_id: r.device_id,
                timestamp: r.timestamp,
                device_type,
                metrics,
                status,
                raw_data: r.raw_data,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_historical_data(
        &self,
        device_id: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: Option<u32>,
    ) -> Result<Vec<DeviceData>> {
        let limit = limit.unwrap_or(1000).min(10000); // Cap at 10k for edge devices

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
        ).fetch_all(&self.pool).await?;

        let data: Result<Vec<DeviceData>, _> = rows.into_iter().map(|r| {
            let device_type = match r.device_type.as_str() {
                "solarinverter" => DeviceType::SolarInverter,
                "batterysystem" => DeviceType::BatterySystem,
                "chargecontroller" => DeviceType::ChargeController,
                "energymeter" => DeviceType::EnergyMeter,
                _ => DeviceType::SolarInverter,
            };
            
            let metrics: DeviceMetrics = serde_json::from_str(&r.metrics)?;
            let status: DeviceStatus = serde_json::from_str(&r.status)?;
            
            Ok(DeviceData {
                device_id: r.device_id,
                timestamp: r.timestamp,
                device_type,
                metrics,
                status,
                raw_data: r.raw_data,
            })
        }).collect();
        
        data
    }

    fn start_cleanup_task(&self) {
        let pool = self.pool.clone();
        let retention_days = self.config.retention_days;
        let cleanup_interval = self.config.cleanup_interval_hours;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(
                std::time::Duration::from_secs(cleanup_interval as u64 * 3600)
            );

            loop {
                interval.tick().await;

                let cutoff_date = Utc::now() - chrono::Duration::days(retention_days as i64);

                if let Err(e) = sqlx::query!(
                    "DELETE FROM device_data WHERE timestamp < ?",
                    cutoff_date
                ).execute(&pool).await {
                    tracing::error!("Failed to cleanup old data: {}", e);
                } else {
                    tracing::info!("Cleaned up data older than {} days", retention_days);
                }
            }
        });
    }
}
```

### 2. Universal Database Schema

```sql
-- Universal device data table
CREATE TABLE IF NOT EXISTS device_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    device_type TEXT NOT NULL,           -- lowercase string identifier (e.g., 'solarinverter')
    metrics TEXT NOT NULL,               -- JSON blob of DeviceMetrics
    status TEXT NOT NULL,                -- JSON blob of DeviceStatus
    raw_data TEXT,                       -- Protocol-specific raw response
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Device configuration table for multi-protocol setup
CREATE TABLE IF NOT EXISTS devices (
    id TEXT PRIMARY KEY,                -- UUID (string)
    name TEXT NOT NULL UNIQUE,          -- Human-friendly name
    device_type TEXT NOT NULL,          -- lowercase string identifier (e.g., 'solarinverter')
    protocol TEXT NOT NULL,             -- e.g., "eg4-pi30-rs485"
    connection_params TEXT NOT NULL,    -- JSON blob of connection parameters
    enabled BOOLEAN NOT NULL DEFAULT 1,
    poll_interval_seconds INTEGER NOT NULL DEFAULT 30,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Trigger to maintain updated_at
CREATE TRIGGER IF NOT EXISTS trg_devices_updated_at
AFTER UPDATE ON devices
FOR EACH ROW BEGIN
    UPDATE devices SET updated_at = CURRENT_TIMESTAMP WHERE id = OLD.id;
END;

-- Indexes for efficient querying across device types
CREATE INDEX IF NOT EXISTS idx_device_data_device_time ON device_data(device_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_device_data_timestamp ON device_data(timestamp);
CREATE INDEX IF NOT EXISTS idx_device_data_type_time ON device_data(device_type, timestamp);
CREATE INDEX IF NOT EXISTS idx_devices_type ON devices(device_type);
CREATE INDEX IF NOT EXISTS idx_devices_protocol ON devices(protocol);
```

### 3. Migrations and Upserts

Recommended initial migration (001_init.sql):
```sql
PRAGMA foreign_keys=ON;
-- Create tables and indexes as defined above
```

Device upsert examples:
```sql
-- Insert new device or update on ID conflict
INSERT INTO devices (id, name, device_type, protocol, connection_params, enabled, poll_interval_seconds)
VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(id) DO UPDATE SET
    name=excluded.name,
    device_type=excluded.device_type,
    protocol=excluded.protocol,
    connection_params=excluded.connection_params,
    enabled=excluded.enabled,
    poll_interval_seconds=excluded.poll_interval_seconds;

-- Optionally upsert by unique name when importing (resolve to existing ID)
INSERT INTO devices (id, name, device_type, protocol, connection_params, enabled, poll_interval_seconds)
VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(name) DO UPDATE SET
    device_type=excluded.device_type,
    protocol=excluded.protocol,
    connection_params=excluded.connection_params,
    enabled=excluded.enabled,
    poll_interval_seconds=excluded.poll_interval_seconds;
```

```sql
-- Device-agnostic schema supporting all solar equipment types
CREATE TABLE device_data (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    timestamp DATETIME NOT NULL,
    device_type TEXT NOT NULL,           -- "solarinverter", "batterysystem", etc.
    metrics TEXT NOT NULL,              -- JSON blob of DeviceMetrics
    status TEXT NOT NULL,               -- JSON blob of DeviceStatus
    raw_data TEXT,                      -- Protocol-specific raw response
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Device configuration table for multi-protocol setup
CREATE TABLE devices (
    id TEXT PRIMARY KEY,                -- Device ID
    name TEXT NOT NULL,
    device_type TEXT NOT NULL,
    protocol TEXT NOT NULL,             -- "eg4-pi30-rs485", "modbus-tcp", "can-bus"
    connection_params TEXT NOT NULL,    -- JSON blob of connection parameters
    enabled BOOLEAN NOT NULL DEFAULT 1,
    poll_interval_seconds INTEGER DEFAULT 30,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for efficient querying across device types
CREATE INDEX idx_device_data_device_time ON device_data(device_id, timestamp);
CREATE INDEX idx_device_data_timestamp ON device_data(timestamp);
CREATE INDEX idx_device_data_type_time ON device_data(device_type, timestamp);
CREATE INDEX idx_devices_type ON devices(device_type);
CREATE INDEX idx_devices_protocol ON devices(protocol);
```

## Configuration

### Storage Configuration

```toml
[storage]
database_path = "/var/lib/solar-monitor/data.db"
retention_days = 90
cleanup_interval_hours = 24

# Device-agnostic backup settings
backup_enabled = true
backup_interval_hours = 6
backup_retention_days = 7

# Multi-device performance tuning
max_connections = 10
batch_size = 100          # Batch inserts for multiple devices
json_compression = true   # Compress JSON blobs for storage efficiency
```

### Universal Daily Aggregation (Optional)

For long-term storage efficiency across all device types:

```sql
CREATE TABLE daily_summary (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id TEXT NOT NULL,
    date DATE NOT NULL,
    device_type TEXT NOT NULL,
    
    -- Universal aggregated metrics (device-type agnostic)
    avg_input_power REAL,
    avg_output_power REAL,
    max_power REAL,
    total_energy_kwh REAL,
    
    -- Device-type specific aggregations as JSON
    type_specific_metrics TEXT,  -- JSON blob for device-type specific aggregations
    
    sample_count INTEGER,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX idx_daily_summary_device_date ON daily_summary(device_id, date);
CREATE INDEX idx_daily_summary_type_date ON daily_summary(device_type, date);
```

### Backup and Recovery

```rust
impl DataStore {
    pub async fn create_backup(&self) -> Result<String> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_path = format!("/var/backups/solar-monitor/backup_{}.db", timestamp);

        // Create backup directory
        if let Some(parent) = std::path::Path::new(&backup_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // SQLite backup using VACUUM INTO
        sqlx::query(&format!("VACUUM INTO '{}'", backup_path))
            .execute(&self.pool).await?;
            
        tracing::info!("Created universal database backup: {}", backup_path);

        Ok(backup_path)
    }
    
    // Device-agnostic query methods
    pub async fn get_devices_by_type(&self, device_type: &DeviceType) -> Result<Vec<DeviceData>> {
        let type_str = format!("{:?}", device_type).to_lowercase();
        
        let rows = sqlx::query!(
            "SELECT * FROM device_data WHERE device_type = ? ORDER BY timestamp DESC LIMIT 100",
            type_str
        ).fetch_all(&self.pool).await?;
        
        self.parse_device_data_rows(rows)
    }
    
    pub async fn get_all_device_types(&self) -> Result<Vec<DeviceType>> {
        let rows = sqlx::query!(
            "SELECT DISTINCT device_type FROM device_data"
        ).fetch_all(&self.pool).await?;

        Ok(rows
            .into_iter()
            .filter_map(|r| match r.device_type.as_str() {
                "solarinverter" => Some(DeviceType::SolarInverter),
                "batterysystem" => Some(DeviceType::BatterySystem),
                "chargecontroller" => Some(DeviceType::ChargeController),
                "energymeter" => Some(DeviceType::EnergyMeter),
                _ => None,
            })
            .collect())
    }
    
    pub async fn get_multi_device_summary(&self) -> Result<HashMap<DeviceType, DeviceMetrics>> {
        let mut summary = HashMap::new();
        
        for device_type in self.get_all_device_types().await? {
            if let Ok(devices) = self.get_devices_by_type(&device_type).await {
                if let Some(latest) = devices.first() {
                    summary.insert(device_type, latest.metrics.clone());
                }
            }
        }
        
        Ok(summary)
    }
    
    // Helper method for parsing database rows to DeviceData
    fn parse_device_data_rows(&self, rows: Vec<sqlx::sqlite::SqliteRow>) -> Result<Vec<DeviceData>> {
        // Implementation details for converting rows to DeviceData with JSON parsing
        todo!("Parse rows with JSON deserialization")
    }
}
```

## Resource Usage

### Universal Database Size Estimates

- **Per data point**: ~200-400 bytes (JSON blob storage)
- **Per device per day** (30s intervals): ~600KB
- **10 mixed devices for 90 days**: ~540MB
- **Annual storage**: ~2GB (with all device types)
- **Device config**: ~1KB per device
- **Indexing overhead**: ~10-15% of total size

### Multi-Device Performance Targets

- **Write throughput**: 100+ inserts/second across all protocols
- **Query response**: <100ms for mixed-device dashboard data
- **Device discovery**: <5 seconds for all protocol scans
- **Startup time**: <3 seconds for universal database initialization
- **Memory usage**: <50MB for SQLite cache + JSON processing
- **JSON parsing**: <1ms per device data point
- **Protocol switching**: <10ms per device type change

This simplified storage approach focuses on essential functionality while maintaining excellent performance on edge devices with minimal operational overhead.
