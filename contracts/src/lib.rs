use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

#[derive(Type, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeviceType {
    SolarInverter,
    BatterySystem,
    ChargeController,
    EnergyMeter,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Offline,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub is_connected: bool,
    pub last_seen: DateTime<Utc>,
    pub health: HealthStatus,
    pub error_message: Option<String>,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMetrics {
    // Power metrics (all device types)
    pub input_power_watts: Option<f64>,
    pub output_power_watts: Option<f64>,
    pub load_percentage: Option<f64>,

    // Battery metrics
    pub battery_voltage: Option<f64>,
    pub battery_current: Option<f64>,
    pub battery_soc_percentage: Option<f64>,
    pub battery_temperature_celsius: Option<f64>,

    // Solar metrics
    pub pv_voltage: Option<f64>,
    pub pv_current: Option<f64>,
    pub pv_power_watts: Option<f64>,

    // Grid metrics
    pub grid_voltage: Option<f64>,
    pub grid_frequency: Option<f64>,
    pub grid_power_watts: Option<f64>,

    // Device-specific
    pub device_temperature_celsius: Option<f64>,
    pub efficiency_percentage: Option<f64>,
    pub fault_codes: Vec<String>,
    pub operating_mode: Option<String>,

    // Protocol-specific extension
    pub custom_metrics: HashMap<String, f64>,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceData {
    pub device_id: String,
    pub timestamp: DateTime<Utc>,
    pub device_type: DeviceType,
    pub metrics: DeviceMetrics,
    pub status: DeviceStatus,
    pub raw_data: Option<String>,
}

// Optional: Basic config DTO used for persistence/API
#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceConfigDto {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub protocol_name: String,
    pub enabled: bool,
    pub poll_interval_seconds: u32,
    pub connection_params: HashMap<String, String>,
}

// API DTOs commonly used by the web layer

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionParamsDto {
    pub device_type: DeviceType,
    pub protocol_name: String,
    pub connection_params: HashMap<String, String>,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResponseDto {
    pub ok: bool,
    pub message: Option<String>,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceUsageDto {
    pub current: f64,
    pub peak: f64,
    pub average: f64,
    pub unit: String, // "percent", "MB", etc.
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageUsageDto {
    pub used_mb: f64,
    pub total_mb: f64,
    pub percent: f64,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatusDto {
    pub uptime_seconds: u64,
    pub version: String,
    pub active_devices: u32,
    pub active_connections: u32,
    pub active_clients: u32,
    pub data_points_per_second: f64,
    pub memory_usage: ResourceUsageDto,
    pub cpu_usage: ResourceUsageDto,
    pub storage_usage: StorageUsageDto,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorResponseDto {
    pub error: String,
    pub details: String,
    pub timestamp: String,
}
