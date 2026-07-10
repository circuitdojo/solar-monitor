-- Delivery history: one row per (notification, channel) attempt

CREATE TABLE IF NOT EXISTS notification_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TIMESTAMP NOT NULL,
    rule_id TEXT NOT NULL,
    rule_name TEXT NOT NULL,
    device_id TEXT,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    channel_name TEXT NOT NULL,
    ok INTEGER NOT NULL,
    error TEXT
);

CREATE INDEX IF NOT EXISTS idx_notification_log_time
    ON notification_log (timestamp DESC);
