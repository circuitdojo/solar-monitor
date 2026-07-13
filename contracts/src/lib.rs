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

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceListItemDto {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub protocol_name: String,
    pub enabled: bool,
    pub poll_interval_seconds: u32,
    pub connection_params: std::collections::HashMap<String, String>,
    pub is_polling: bool,
    pub supports_settings: bool,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolCapabilitiesDto {
    pub supports_discovery: bool,
    pub supports_settings: bool,
    pub supports_real_time: bool,
    pub max_concurrent_connections: Option<u32>,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolInfoDto {
    /// Stable protocol id used in device configs (e.g. "eg4-6000xp-modbus").
    pub protocol_name: String,
    /// Human-readable display name.
    pub name: String,
    pub version: String,
    pub description: String,
    pub supported_device_types: Vec<DeviceType>,
    pub capabilities: ProtocolCapabilitiesDto,
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
/// Create/update payload for a device. The device id is never client-chosen:
/// POST mints a UUID and returns it; PUT takes the id from the URL path.
pub struct AddDeviceRequestDto {
    pub name: String,
    pub device_type: DeviceType,
    pub protocol_name: String,
    pub enabled: bool,
    pub poll_interval_seconds: u32,
    pub connection_params: HashMap<String, String>,
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

// Notifications

#[derive(Type, Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NotificationChannelKind {
    Ntfy,
    Email,
    Pushover,
    Webhook,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationChannelDto {
    pub id: String,
    pub name: String,
    pub kind: NotificationChannelKind,
    /// Kind-specific settings. ntfy: serverUrl, topic, token (optional);
    /// email: smtpHost, smtpPort, username, password, from, to;
    /// pushover: userKey, appToken; webhook: url.
    pub config: HashMap<String, String>,
    pub enabled: bool,
}

#[derive(Type, Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NotificationEvent {
    /// Grid voltage collapse/return (params: lostBelow, restoredAbove, volts)
    GridState,
    /// Battery SOC threshold (params: lowBelow, recoveredAbove, percent)
    BatteryLow,
    /// No data from the device (params: offlineAfterSeconds)
    DeviceOffline,
    /// Generator start/stop (params: startAbove, stopBelow, watts)
    Generator,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationRuleDto {
    pub id: String,
    pub name: String,
    pub event: NotificationEvent,
    /// None = applies to every device
    pub device_id: Option<String>,
    /// Event-specific numeric parameters; missing keys use defaults
    pub params: HashMap<String, f64>,
    pub channel_ids: Vec<String>,
    pub enabled: bool,
    /// Minimum seconds between repeated firings of the same transition
    pub cooldown_seconds: u32,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationLogEntryDto {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub rule_id: String,
    pub rule_name: String,
    pub device_id: Option<String>,
    pub title: String,
    pub body: String,
    pub channel_id: String,
    pub channel_name: String,
    pub ok: bool,
    pub error: Option<String>,
}

// Device settings (typed, validated register-backed configuration)

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SettingValueDto {
    #[serde(rename_all = "camelCase")]
    Number {
        value: f64,
        min: f64,
        max: f64,
        step: f64,
        unit: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Toggle { enabled: bool },
    #[serde(rename_all = "camelCase")]
    Choice {
        value: u16,
        options: Vec<u16>,
        /// Optional display labels, parallel to `options`
        labels: Option<Vec<String>>,
        unit: Option<String>,
    },
    /// Times as "HH:MM"
    #[serde(rename_all = "camelCase")]
    TimeWindow { start: String, end: String },
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSettingDto {
    pub key: String,
    pub label: String,
    pub group: String,
    /// UI must ask for confirmation before writing (disruptive settings,
    /// e.g. those that cut output power).
    pub requires_confirm: bool,
    pub setting: SettingValueDto,
}

#[derive(Type, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteSettingRequestDto {
    /// New value, stringly typed: a number for Number/Choice, "true"/"false"
    /// for Toggle, "HH:MM-HH:MM" for TimeWindow.
    pub value: String,
}
