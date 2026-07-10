-- Hourly downsampling of device_data. Rows older than the retention window
-- are aggregated into one row per (device, hour) — avg for charting, min/max
-- so peaks survive (max PV power, SOC lows) — then deleted from device_data.

CREATE TABLE IF NOT EXISTS device_data_hourly (
    device_id TEXT NOT NULL,
    hour_start DATETIME NOT NULL,        -- UTC hour bucket start
    device_type TEXT NOT NULL,
    metrics_avg TEXT NOT NULL,           -- JSON, same shape as DeviceMetrics
    metrics_min TEXT NOT NULL,
    metrics_max TEXT NOT NULL,
    sample_count INTEGER NOT NULL,
    PRIMARY KEY (device_id, hour_start)
);

CREATE INDEX IF NOT EXISTS idx_device_data_hourly_time
    ON device_data_hourly(hour_start);
