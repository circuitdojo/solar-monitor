use axum::{http::Request, Router};
use http_body_util::BodyExt;
use tower::ServiceExt; // for `oneshot` // for `collect`

use async_trait::async_trait;
use axum::body::Body;
use axum::http::StatusCode;
use contracts as dto;
use serde_json::json;
use solar_monitor_api as api;
use solar_monitor_core as core;
use std::sync::Arc;

async fn test_state(_db_path: &str) -> Arc<api::AppState> {
    let registry = Arc::new(solar_monitor_protocols::create_registry());
    let store = Arc::new(
        solar_monitor_storage::DataStore::new(":memory:")
            .await
            .unwrap(),
    );
    let (tx, _rx) = tokio::sync::broadcast::channel::<contracts::DeviceData>(16);
    Arc::new(api::AppState {
        registry,
        store,
        tasks: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        devices: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        tx,
        started_at: chrono::Utc::now(),
    })
}

async fn test_app(db_path: &str) -> Router {
    let state = test_state(db_path).await;
    api::router(state)
}

async fn test_state_with_mock() -> Arc<api::AppState> {
    // Build a registry with a mock protocol that does not require serial ports
    struct MockProto;
    struct MockConn {
        n: u64,
    }
    #[async_trait]
    impl core::DeviceProtocol for MockProto {
        fn protocol_name(&self) -> &'static str {
            "mock-proto"
        }
        fn metadata(&self) -> core::ProtocolMetadata {
            core::ProtocolMetadata {
                name: "Mock",
                version: "0.0.1",
                description: "Test mock",
                supported_device_types: &[dto::DeviceType::SolarInverter],
                capabilities: core::ProtocolCapabilities {
                    supports_discovery: false,
                    supports_commands: true,
                    supports_real_time: true,
                    max_concurrent_connections: Some(10),
                },
            }
        }
        fn supported_device_types(&self) -> Vec<dto::DeviceType> {
            vec![dto::DeviceType::SolarInverter]
        }
        async fn discover_devices(
            &self,
            _scan: &core::ScanConfig,
        ) -> anyhow::Result<Vec<core::DiscoveredDevice>> {
            Ok(vec![])
        }
        async fn connect(
            &self,
            _config: &core::DeviceConfig,
        ) -> anyhow::Result<Box<dyn core::DeviceConnection>> {
            Ok(Box::new(MockConn { n: 0 }))
        }
    }
    #[async_trait]
    impl core::DeviceConnection for MockConn {
        async fn read_data(&mut self) -> anyhow::Result<dto::DeviceData> {
            self.n += 1;
            Ok(dto::DeviceData {
                device_id: "devX".into(),
                timestamp: chrono::Utc::now(),
                device_type: dto::DeviceType::SolarInverter,
                metrics: dto::DeviceMetrics::default(),
                status: dto::DeviceStatus {
                    is_connected: true,
                    last_seen: chrono::Utc::now(),
                    health: dto::HealthStatus::Healthy,
                    error_message: None,
                },
                raw_data: None,
            })
        }
        async fn send_command(&mut self, _command: &str) -> anyhow::Result<String> {
            Ok("OK".into())
        }
        fn is_connected(&self) -> bool {
            true
        }
        async fn health_check(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let mut reg = core::ProtocolRegistry::new();
    reg.register_protocol(Arc::new(MockProto));
    let registry = Arc::new(reg);
    let store = Arc::new(
        solar_monitor_storage::DataStore::new(":memory:")
            .await
            .unwrap(),
    );
    let (tx, _rx) = tokio::sync::broadcast::channel::<contracts::DeviceData>(16);
    Arc::new(api::AppState {
        registry,
        store,
        tasks: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        devices: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        tx,
        started_at: chrono::Utc::now(),
    })
}

#[tokio::test]
async fn health_status_protocols_and_devices_list() {
    let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-output");
    std::fs::create_dir_all(&base).unwrap();
    let db_path = base.join(format!(
        "api-{}.sqlite",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let app = test_app(&db_path.to_string_lossy()).await;

    // Health
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["status"], "healthy");

    // Status
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/status")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("uptimeSeconds").is_some());
    assert!(v.get("version").is_some());

    // Protocols
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/protocols")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v["protocols"].is_array());
    assert!(v["protocols"].as_array().unwrap().len() >= 1);

    // Devices (empty)
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/devices")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.is_array());
    assert_eq!(v.as_array().unwrap().len(), 0);
}

async fn post_json(app: &Router, uri: &str, body: serde_json::Value) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn put_json(app: &Router, uri: &str, body: serde_json::Value) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn delete(app: &Router, uri: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn get(app: &Router, uri: &str) -> axum::response::Response {
    app.clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn devices_crud_and_errors() {
    let app = test_app(":memory:").await;

    // Not found returns ApiError JSON
    let res = get(&app, "/api/v1/devices/nonexistent").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("error").is_some());

    // Create device via PUT upsert (disabled)
    let create_body = json!({
        "id": "dev1",
        "name": "Inverter A",
        "deviceType": "solarInverter",
        "protocolName": "eg4-6000xp-modbus",
        "enabled": false,
        "pollIntervalSeconds": 30,
        "connectionParams": {"serial_port": "/dev/ttyS1", "baud_rate": "9600"}
    });
    let res = put_json(&app, "/api/v1/devices/dev1", create_body).await;
    assert!(res.status().is_success());

    // Get device
    let res = get(&app, "/api/v1/devices/dev1").await;
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["id"], "dev1");
    assert_eq!(v["enabled"], false);

    // Update device (rename, keep disabled to avoid polling)
    let put_body = json!({
        "id": "dev1",
        "name": "Inverter A2",
        "deviceType": "solarInverter",
        "protocolName": "eg4-6000xp-modbus",
        "enabled": false,
        "pollIntervalSeconds": 60,
        "connectionParams": {"serial_port": "/dev/ttyS1", "baud_rate": "9600"}
    });
    let res = put_json(&app, "/api/v1/devices/dev1", put_body).await;
    assert!(res.status().is_success());

    let res = get(&app, "/api/v1/devices/dev1").await;
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["name"], "Inverter A2");
    assert_eq!(v["pollIntervalSeconds"], 60);

    // Latest data (none)
    let res = get(&app, "/api/v1/devices/dev1/data/latest").await;
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.is_null());

    // Range (empty)
    let res = get(&app, "/api/v1/devices/dev1/data").await;
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.as_array().unwrap().is_empty());

    // Dashboard (empty)
    let res = get(&app, "/api/v1/data/dashboard").await;
    assert!(res.status().is_success());

    // Export (has one)
    let res = get(&app, "/api/v1/devices/export").await;
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1);

    // Delete device
    let res = delete(&app, "/api/v1/devices/dev1").await;
    assert!(res.status().is_success());
    let res = get(&app, "/api/v1/devices/dev1").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn device_command_and_range_validation() {
    let app = test_app(":memory:").await;

    // Upsert disabled device so command attempts won't spawn polling
    let create_body = json!({
        "id": "dev2",
        "name": "Inverter B",
        "deviceType": "solarInverter",
        "protocolName": "eg4-pi30-rs485",
        "enabled": false,
        "pollIntervalSeconds": 30,
        "connectionParams": {"serial_port": "/dev/ttyS9", "baud_rate": "9600"}
    });
    let res = put_json(&app, "/api/v1/devices/dev2", create_body).await;
    assert!(res.status().is_success());

    // Command endpoint should error (no real serial), returning JSON error
    let res = post_json(
        &app,
        "/api/v1/devices/dev2/command",
        json!({"command": "QID"}),
    )
    .await;
    assert!(res.status().is_client_error() || res.status().is_server_error());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("error").is_some());
    assert!(v.get("details").is_some());

    // Range validation: bad timestamp should return 400 with error JSON
    let res = get(&app, "/api/v1/devices/dev2/data?start=not-a-time").await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("error").is_some());
}

#[tokio::test]
async fn command_404_and_discovery_empty() {
    let app = test_app(":memory:").await;

    // Command to non-existent device should be 404 with error JSON
    let res = post_json(
        &app,
        "/api/v1/devices/does-not-exist/command",
        json!({"command": "QID"}),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("error").is_some());

    // Discovery with empty serialPorts should return an empty array
    let res = post_json(
        &app,
        "/api/v1/protocols/discovery",
        json!({
            "serialPorts": [],
            "timeoutSeconds": 1
        }),
    )
    .await;
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(v.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn import_export_round_trip() {
    let app = test_app(":memory:").await;

    // Import two disabled devices
    let body = json!([
        {
            "id": "devA",
            "name": "Alpha",
            "deviceType": "solarInverter",
            "protocolName": "eg4-6000xp-modbus",
            "enabled": false,
            "pollIntervalSeconds": 15,
            "connectionParams": {"serial_port": "/dev/ttyS10", "baud_rate": "9600"}
        },
        {
            "id": "devB",
            "name": "Beta",
            "deviceType": "solarInverter",
            "protocolName": "eg4-6000xp-modbus",
            "enabled": false,
            "pollIntervalSeconds": 30,
            "connectionParams": {"serial_port": "/dev/ttyS11", "baud_rate": "9600"}
        }
    ]);
    let res = post_json(&app, "/api/v1/devices/import", body).await;
    assert!(res.status().is_success());

    // Export and verify both are present with expected fields
    let res = get(&app, "/api/v1/devices/export").await;
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);

    let mut ids: Vec<String> = arr
        .iter()
        .map(|x| x["id"].as_str().unwrap().to_string())
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["devA", "devB"]);

    // Check one entry's shape
    let a = arr.iter().find(|x| x["id"] == "devA").unwrap();
    assert_eq!(a["name"], "Alpha");
    assert_eq!(a["deviceType"], "solarInverter");
    assert_eq!(a["protocolName"], "eg4-6000xp-modbus");
    assert_eq!(a["enabled"], false);
    assert_eq!(a["pollIntervalSeconds"], 15);
    assert!(a["connectionParams"].is_object());
}

#[tokio::test]
async fn enabling_creates_task_and_delete_aborts() {
    let state = test_state_with_mock().await;
    let app = api::router(state.clone());

    // Enable device with mock-proto
    let body = json!({
        "id": "devX",
        "name": "Mock",
        "deviceType": "solarInverter",
        "protocolName": "mock-proto",
        "enabled": true,
        "pollIntervalSeconds": 5,
        "connectionParams": {}
    });
    let res = put_json(&app, "/api/v1/devices/devX", body).await;
    assert!(res.status().is_success());

    // Task should appear
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(state.tasks.lock().await.len(), 1);

    // Delete device and ensure task is removed
    let res = delete(&app, "/api/v1/devices/devX").await;
    assert!(res.status().is_success());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(state.tasks.lock().await.len(), 0);
}

#[tokio::test]
async fn mock_command_returns_ok() {
    let state = test_state_with_mock().await;
    let app = api::router(state.clone());

    // Create device with mock protocol (disabled)
    let body = json!({
        "id": "devCmd",
        "name": "MockCmd",
        "deviceType": "solarInverter",
        "protocolName": "mock-proto",
        "enabled": false,
        "pollIntervalSeconds": 5,
        "connectionParams": {}
    });
    let res = put_json(&app, "/api/v1/devices/devCmd", body).await;
    assert!(res.status().is_success());

    // Send command and expect OK
    let res = post_json(
        &app,
        "/api/v1/devices/devCmd/command",
        json!({"command": "PING"}),
    )
    .await;
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["response"], "OK");
}

#[tokio::test]
async fn dashboard_includes_latest_data() {
    let state = test_state_with_mock().await;
    let app = api::router(state.clone());

    // Ensure device config exists
    let cfg = core::DeviceConfig {
        id: "devDash".into(),
        name: "DashDev".into(),
        device_type: dto::DeviceType::SolarInverter,
        protocol: "mock-proto".into(),
        connection_params: std::collections::HashMap::new(),
        enabled: false,
        poll_interval_seconds: 30,
    };
    state.store.upsert_device_config(&cfg).await.unwrap();

    // Insert two data points; latest should be returned
    let t1 = chrono::Utc::now() - chrono::Duration::seconds(60);
    let t2 = chrono::Utc::now();
    let d1 = dto::DeviceData {
        device_id: cfg.id.clone(),
        timestamp: t1,
        device_type: dto::DeviceType::SolarInverter,
        metrics: dto::DeviceMetrics {
            pv_voltage: Some(50.0),
            ..Default::default()
        },
        status: dto::DeviceStatus {
            is_connected: true,
            last_seen: t1,
            health: dto::HealthStatus::Healthy,
            error_message: None,
        },
        raw_data: None,
    };
    let mut d2 = d1.clone();
    d2.timestamp = t2;
    d2.metrics.pv_voltage = Some(75.0);
    state.store.store_device_data(&d1).await.unwrap();
    state.store.store_device_data(&d2).await.unwrap();

    let res = get(&app, "/api/v1/data/dashboard").await;
    assert!(res.status().is_success());
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = v.as_array().unwrap();
    assert!(!arr.is_empty());
    let first = arr.iter().find(|x| x["deviceId"] == cfg.id).unwrap();
    assert_eq!(first["deviceId"], cfg.id);
}
