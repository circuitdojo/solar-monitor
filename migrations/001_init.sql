PRAGMA foreign_keys=ON;

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
    name TEXT NOT NULL UNIQUE,          -- Human-friendly unique name
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
