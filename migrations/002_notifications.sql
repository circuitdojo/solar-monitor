-- Notification channels (where to send) and rules (what to watch)

CREATE TABLE IF NOT EXISTS notification_channels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,               -- ntfy | email | pushover | webhook
    config TEXT NOT NULL DEFAULT '{}', -- kind-specific JSON object
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS notification_rules (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    event TEXT NOT NULL,              -- gridState | batteryLow | deviceOffline | generator
    device_id TEXT,                   -- NULL = any device
    params TEXT NOT NULL DEFAULT '{}', -- event-specific JSON object of numbers
    channel_ids TEXT NOT NULL DEFAULT '[]',
    enabled INTEGER NOT NULL DEFAULT 1,
    cooldown_seconds INTEGER NOT NULL DEFAULT 300,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
