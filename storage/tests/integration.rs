use chrono::Utc;
use contracts as dto;
use solar_monitor_core as core;
use solar_monitor_storage::DataStore;

#[tokio::test]
async fn storage_round_trip() {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-output");
    std::fs::create_dir_all(&base).unwrap();
    let db_path = base.join(format!(
        "storage-{}.sqlite",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db_path_str = db_path.to_string_lossy().to_string();

    let store = DataStore::new(&db_path_str).await.expect("init store");

    // Upsert device config
    let cfg = core::DeviceConfig {
        id: "dev1".into(),
        name: "Device One".into(),
        device_type: dto::DeviceType::SolarInverter,
        protocol: "eg4-pi30-rs485".into(),
        connection_params: std::collections::HashMap::from([
            ("serial_port".into(), "/dev/ttyS1".into()),
            ("baud_rate".into(), "9600".into()),
        ]),
        enabled: true,
        poll_interval_seconds: 30,
    };
    store.upsert_device_config(&cfg).await.expect("upsert cfg");

    let list = store.list_device_configs().await.expect("list cfgs");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "dev1");

    // Store a datapoint
    let data = dto::DeviceData {
        device_id: "dev1".into(),
        timestamp: Utc::now(),
        device_type: dto::DeviceType::SolarInverter,
        metrics: dto::DeviceMetrics {
            pv_voltage: Some(100.0),
            pv_current: Some(5.0),
            ..Default::default()
        },
        status: dto::DeviceStatus {
            is_connected: true,
            last_seen: Utc::now(),
            health: dto::HealthStatus::Healthy,
            error_message: None,
        },
        raw_data: Some("(mock)".into()),
    };
    store.store_device_data(&data).await.expect("store data");

    let got = store
        .get_latest_device_data("dev1")
        .await
        .expect("get latest");
    assert!(got.is_some());
    let got = got.unwrap();
    assert_eq!(got.device_id, "dev1");
    assert_eq!(got.metrics.pv_voltage, Some(100.0));
}
