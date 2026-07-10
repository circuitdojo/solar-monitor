//! Minimal entrypoint with CLI flags

use anyhow::Result;
use clap::Parser;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "solar-monitor", about = "Solar Monitor service and tools")]
struct Cli {
    /// Bind address for the HTTP server
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,

    /// Port for the HTTP server
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// Path to SQLite database
    #[arg(long, default_value = "./data/solar.db")]
    db: String,

    /// Start the HTTP server
    #[arg(long)]
    serve: bool,

    /// Run device discovery and print results, then exit
    #[arg(long)]
    discover: bool,

    /// Comma-separated serial port list for discovery (e.g., /dev/ttyUSB0,/dev/ttyUSB1)
    #[arg(long)]
    serial_ports: Option<String>,

    /// Discovery timeout seconds
    #[arg(long, default_value_t = 3)]
    timeout: u32,

    /// Install a systemd service and exit
    #[arg(long)]
    install: bool,

    /// Uninstall the systemd service and exit
    #[arg(long)]
    uninstall: bool,

    /// Systemd service name
    #[arg(long, default_value = "solar-monitor")]
    service_name: String,

    /// Run service as this user (optional)
    #[arg(long)]
    user: Option<String>,

    /// Data directory for persistent storage (defaults to /var/lib/solar-monitor when installing)
    #[arg(long)]
    data_dir: Option<String>,

    /// Config directory (unused by default, reserved for future)
    #[arg(long)]
    config_dir: Option<String>,

    /// Days of full-resolution history to keep; older readings are
    /// downsampled to hourly avg/min/max. 0 disables downsampling.
    #[arg(long, default_value_t = 30)]
    retention_days: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let cli = Cli::parse();
    if cli.uninstall {
        uninstall_service(&cli)?;
        return Ok(());
    }
    if cli.install {
        install_service(&cli)?;
        return Ok(());
    }
    // Compose state
    let registry = solar_monitor_protocols::create_registry();
    let store = Arc::new(solar_monitor_storage::DataStore::new(&cli.db).await?);
    let (tx, _rx) = tokio::sync::broadcast::channel::<contracts::DeviceData>(100);
    let notifier = solar_monitor_notify::Notifier::new(store.clone()).await?;
    notifier.clone().spawn(tx.subscribe());
    let state = Arc::new(solar_monitor_api::AppState {
        registry: Arc::new(registry),
        store,
        tasks: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        devices: tokio::sync::Mutex::new(std::collections::HashMap::new()),
        tx,
        notifier,
        started_at: chrono::Utc::now(),
    });

    // Optional: perform discovery and exit
    if cli.discover {
        let ports: Vec<String> = if let Some(p) = &cli.serial_ports {
            p.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            match serialport::available_ports() {
                Ok(p) => p.into_iter().map(|p| p.port_name).collect(),
                Err(_) => Vec::new(),
            }
        };
        if ports.is_empty() {
            println!(
                "No serial ports found. Try --serial-ports or check permissions (dialout group)."
            );
            return Ok(());
        }

        println!("Scanning ports: {}", ports.join(","));
        let scan = solar_monitor_core::ScanConfig {
            serial_ports: ports,
            timeout_seconds: cli.timeout,
        };

        let mut discovered = Vec::new();
        for proto in state.registry.protocols() {
            println!("Probing via {}...", proto.protocol_name());
            match proto.discover_devices(&scan).await {
                Ok(mut found) => {
                    if !found.is_empty() {
                        println!(
                            "Found {} device(s) via {}",
                            found.len(),
                            proto.protocol_name()
                        );
                    }
                    discovered.append(&mut found);
                }
                Err(e) => {
                    println!("Discovery via {} failed: {}", proto.protocol_name(), e);
                }
            }
        }

        if discovered.is_empty() {
            println!("No devices discovered");
        } else {
            println!("Discovered {} device(s):", discovered.len());
            for d in discovered {
                println!("- {} ({:?}) via {}", d.name, d.device_type, d.protocol);
                println!("  id: {}", d.id);
                println!("  params: {:?}", d.connection_params);
            }
        }
        return Ok(());
    }

    // Auto-start polling for persisted, enabled devices
    {
        let configs = state.store.list_device_configs().await.unwrap_or_default();
        for cfg in configs.into_iter().filter(|c| c.enabled) {
            state
                .devices
                .lock()
                .await
                .insert(cfg.id.clone(), cfg.clone());
            let (id, protocol) = (cfg.id.clone(), cfg.protocol.clone());
            if let Err(e) = solar_monitor_api::start_polling(state.clone(), cfg).await {
                tracing::warn!(
                    "device {} (protocol '{}') not polling: {} — edit or remove it on the Devices page",
                    id,
                    protocol,
                    e
                );
            }
        }
    }

    // Periodic downsampling: fold full-resolution rows older than the
    // retention window into hourly aggregates (runs at startup, then every 6h)
    if cli.retention_days > 0 {
        let store = state.store.clone();
        let days = cli.retention_days;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_secs(6 * 3600));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                match store.downsample_and_prune(days).await {
                    Ok((0, _)) => {}
                    Ok((rows, hours)) => tracing::info!(
                        "downsampled {} readings into {} hourly rows (>{} days old)",
                        rows,
                        hours,
                        days
                    ),
                    Err(e) => tracing::warn!("downsampling failed: {}", e),
                }
            }
        });
    }

    let app = {
        #[cfg(feature = "openapi")]
        {
            solar_monitor_api::router_with_openapi(state)
        }
        #[cfg(not(feature = "openapi"))]
        {
            solar_monitor_api::router(state)
        }
    };

    println!(
        "solar-monitor v{} | protocols: {:?}",
        solar_monitor_core::version(),
        solar_monitor_protocols::registered()
    );

    if cli.serve || std::env::var("SERVE").ok().as_deref() == Some("1") {
        let listener = tokio::net::TcpListener::bind((cli.bind.as_str(), cli.port)).await?;
        axum::serve(listener, app).await?;
    }

    Ok(())
}

fn install_service(cli: &Cli) -> Result<()> {
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    // Resolve binary path. The unit sets ProtectHome=true, so a binary under
    // /home can't be executed by the service — install a copy outside it.
    let exe = std::env::current_exe()?;
    let exe_str = if exe.starts_with("/home") {
        let target = format!("/usr/local/bin/{}", cli.service_name);
        match fs::copy(&exe, &target) {
            Ok(_) => {
                println!(
                    "Copied binary to {} (ProtectHome=true would block execution from /home)",
                    target
                );
                target
            }
            Err(e) => {
                anyhow::bail!(
                    "binary is at {} (under /home, blocked by ProtectHome) and copying to {} failed: {}.\nRe-run with sudo, or install the binary outside /home first.",
                    exe.display(),
                    target,
                    e
                );
            }
        }
    } else {
        exe.to_string_lossy().into_owned()
    };

    // DB lives in the systemd StateDirectory unless overridden — systemd
    // creates /var/lib/<service-name> with the right ownership for User=.
    let db_path = if cli.db != "./data/solar.db" {
        if let Some(parent) = PathBuf::from(&cli.db).parent() {
            let _ = fs::create_dir_all(parent);
        }
        cli.db.clone()
    } else {
        let data_dir = cli
            .data_dir
            .clone()
            .unwrap_or_else(|| format!("/var/lib/{}", cli.service_name));
        format!("{}/solar.db", data_dir)
    };
    if let Some(cfg) = &cli.config_dir {
        let _ = fs::create_dir_all(cfg);
    }

    // Compose unit file
    let mut unit = String::new();
    unit.push_str("[Unit]\n");
    unit.push_str("Description=Solar Monitor Service\n");
    unit.push_str("After=network-online.target\n");
    unit.push_str("Wants=network-online.target\n\n");
    unit.push_str("[Service]\n");
    unit.push_str("Type=simple\n");
    unit.push_str("Environment=RUST_LOG=info\n");
    if let Some(user) = &cli.user {
        unit.push_str(&format!("User={}\n", user));
    }
    // Serial (RS485) access for a non-root service user
    unit.push_str("SupplementaryGroups=dialout\n");
    // Owns /var/lib/<service-name> with correct permissions for User=
    unit.push_str(&format!("StateDirectory={}\n", cli.service_name));
    unit.push_str(&format!(
        "ExecStart={} --serve --bind {} --port {} --db {}\n",
        exe_str, cli.bind, cli.port, db_path
    ));
    unit.push_str("Restart=on-failure\n");
    unit.push_str("RestartSec=5\n");
    unit.push_str("NoNewPrivileges=true\n");
    unit.push_str("ProtectSystem=full\n");
    unit.push_str("ProtectHome=true\n");
    unit.push_str("PrivateTmp=true\n\n");
    unit.push_str("[Install]\n");
    unit.push_str("WantedBy=multi-user.target\n");

    // Try writing to systemd directory
    let systemd_path = PathBuf::from(format!("/etc/systemd/system/{}.service", cli.service_name));
    match fs::File::create(&systemd_path) {
        Ok(mut f) => {
            f.write_all(unit.as_bytes())?;
            println!("Installed service at {}", systemd_path.display());
            println!(
                "Next steps:\n  sudo systemctl daemon-reload\n  sudo systemctl enable {}\n  sudo systemctl start {}",
                cli.service_name, cli.service_name
            );
        }
        Err(e) => {
            // Fallback: write to local file for manual install
            let local = PathBuf::from(format!("{}.service", cli.service_name));
            let mut f = fs::File::create(&local)?;
            f.write_all(unit.as_bytes())?;
            println!(
                "Could not write {} ({}). Wrote {} instead.",
                systemd_path.display(),
                e,
                local.display()
            );
            println!(
                "Install manually with:\n  sudo cp {} {}\n  sudo systemctl daemon-reload\n  sudo systemctl enable {}\n  sudo systemctl start {}",
                local.display(),
                systemd_path.display(),
                cli.service_name,
                cli.service_name
            );
        }
    }

    println!("DB: {}", db_path);
    Ok(())
}

fn uninstall_service(cli: &Cli) -> Result<()> {
    use std::fs;
    use std::path::PathBuf;

    let systemd_path = PathBuf::from(format!("/etc/systemd/system/{}.service", cli.service_name));
    match fs::remove_file(&systemd_path) {
        Ok(_) => {
            println!("Removed {}", systemd_path.display());
            println!(
                "Next steps:\n  sudo systemctl daemon-reload\n  sudo systemctl disable {}\n  sudo systemctl stop {}",
                cli.service_name, cli.service_name
            );
        }
        Err(e) => {
            println!("Could not remove {} ({}).", systemd_path.display(), e);
            println!(
                "If the service exists, remove it manually:\n  sudo rm {}\n  sudo systemctl daemon-reload\n  sudo systemctl disable {}\n  sudo systemctl stop {}",
                systemd_path.display(),
                cli.service_name,
                cli.service_name
            );
        }
    }
    Ok(())
}
