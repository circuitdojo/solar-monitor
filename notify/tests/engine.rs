//! End-to-end engine test: a grid-lost transition must dispatch to the
//! configured channel and record the delivery outcome in the log.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use contracts::{
    DeviceData, DeviceMetrics, DeviceStatus, DeviceType, HealthStatus, NotificationChannelDto,
    NotificationChannelKind, NotificationEvent, NotificationRuleDto,
};
use solar_monitor_notify::Notifier;
use solar_monitor_storage::DataStore;

fn sample(grid_v: f64) -> DeviceData {
    DeviceData {
        device_id: "inv1".into(),
        timestamp: chrono::Utc::now(),
        device_type: DeviceType::SolarInverter,
        metrics: DeviceMetrics {
            grid_voltage: Some(grid_v),
            ..Default::default()
        },
        status: DeviceStatus {
            is_connected: true,
            last_seen: chrono::Utc::now(),
            health: HealthStatus::Healthy,
            error_message: None,
        },
        raw_data: None,
    }
}

#[tokio::test]
async fn grid_loss_fires_and_logs_delivery_outcome() {
    let store = Arc::new(DataStore::new(":memory:").await.unwrap());

    // Webhook to a port nothing listens on: dispatch happens, delivery fails
    store
        .upsert_notification_channel(&NotificationChannelDto {
            id: "hook".into(),
            name: "Dead hook".into(),
            kind: NotificationChannelKind::Webhook,
            config: HashMap::from([("url".to_string(), "http://127.0.0.1:1/x".to_string())]),
            enabled: true,
        })
        .await
        .unwrap();
    store
        .upsert_notification_rule(&NotificationRuleDto {
            id: "grid".into(),
            name: "Grid watch".into(),
            event: NotificationEvent::GridState,
            device_id: None,
            params: HashMap::new(),
            channel_ids: vec!["hook".into()],
            enabled: true,
            cooldown_seconds: 0,
        })
        .await
        .unwrap();

    let notifier = Notifier::new(store.clone()).await.unwrap();
    let (tx, rx) = tokio::sync::broadcast::channel::<DeviceData>(16);
    notifier.spawn(rx);

    // First sample sets the baseline (no alert), second flips the state
    tx.send(sample(240.0)).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    tx.send(sample(0.0)).unwrap();

    // Delivery to a refused port fails fast; poll for the log row
    let mut entries = Vec::new();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        entries = store.list_notification_log(10).await.unwrap();
        if !entries.is_empty() {
            break;
        }
    }
    assert_eq!(entries.len(), 1, "expected exactly one delivery attempt");
    let e = &entries[0];
    assert_eq!(e.rule_id, "grid");
    assert_eq!(e.channel_id, "hook");
    assert_eq!(e.device_id.as_deref(), Some("inv1"));
    assert!(e.title.starts_with("Grid lost"));
    assert!(!e.ok);
    assert!(e.error.is_some());

    // Restoration fires the opposite transition
    tx.send(sample(240.0)).unwrap();
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        entries = store.list_notification_log(10).await.unwrap();
        if entries.len() >= 2 {
            break;
        }
    }
    assert_eq!(entries.len(), 2);
    assert!(entries[0].title.starts_with("Grid restored"));
}
