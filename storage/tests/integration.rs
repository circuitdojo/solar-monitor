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

#[tokio::test]
async fn downsampling_aggregates_and_prunes() {
    let store = DataStore::new(":memory:").await.expect("init store");

    let sample = |ts: chrono::DateTime<Utc>, pv: f64, gen_w: Option<f64>| dto::DeviceData {
        device_id: "dev1".into(),
        timestamp: ts,
        device_type: dto::DeviceType::SolarInverter,
        metrics: dto::DeviceMetrics {
            pv_power_watts: Some(pv),
            operating_mode: Some("Normal".into()),
            custom_metrics: gen_w
                .map(|g| std::collections::HashMap::from([("gen_power".to_string(), g)]))
                .unwrap_or_default(),
            ..Default::default()
        },
        status: dto::DeviceStatus {
            is_connected: true,
            last_seen: ts,
            health: dto::HealthStatus::Healthy,
            error_message: None,
        },
        raw_data: None,
    };

    // Three samples in one hour bucket, 3 days old; gen_power intermittent
    let old_hour = (Utc::now() - chrono::Duration::days(3))
        .date_naive()
        .and_hms_opt(10, 0, 0)
        .unwrap()
        .and_utc();
    for (offset_min, pv, gen_w) in [
        (5, 100.0, Some(500.0)),
        (25, 200.0, None),
        (45, 300.0, Some(1500.0)),
    ] {
        store
            .store_device_data(&sample(
                old_hour + chrono::Duration::minutes(offset_min),
                pv,
                gen_w,
            ))
            .await
            .unwrap();
    }
    // A fresh sample that must survive at full resolution
    let fresh_ts = Utc::now();
    store
        .store_device_data(&sample(fresh_ts, 999.0, None))
        .await
        .unwrap();

    let (pruned, hours) = store.downsample_and_prune(1).await.unwrap();
    assert_eq!(pruned, 3, "old rows folded");
    assert_eq!(hours, 1, "one hourly bucket written");

    // Second run is a no-op
    let (pruned2, _) = store.downsample_and_prune(1).await.unwrap();
    assert_eq!(pruned2, 0);

    // Range query merges: one hourly point (avg) + one fresh raw point
    let all = store
        .get_device_data_range(
            "dev1",
            old_hour - chrono::Duration::hours(1),
            Utc::now(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(all.len(), 2);
    let hourly = &all[0];
    assert_eq!(hourly.timestamp, old_hour);
    assert_eq!(
        hourly.metrics.pv_power_watts,
        Some(200.0),
        "avg of 100/200/300"
    );
    assert_eq!(
        hourly.metrics.custom_metrics.get("gen_power"),
        Some(&1000.0),
        "avg over the samples where gen_power was present"
    );
    assert_eq!(hourly.metrics.operating_mode.as_deref(), Some("Normal"));
    let fresh = &all[1];
    assert_eq!(fresh.metrics.pv_power_watts, Some(999.0));
}
