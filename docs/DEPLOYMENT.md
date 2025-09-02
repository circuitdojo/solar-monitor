# Deployment & Operations - Edge Device Specification

## Overview

Single-binary deployment specification optimized for Raspberry Pi and Nucbox edge devices. Focuses on simplicity, reliability, and minimal operational overhead.

## Single Binary Architecture

### 1. Single Binary Architecture

```rust
// All-in-one binary with embedded assets
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
#[prefix = "static/"]
pub struct StaticAssets;

#[derive(RustEmbed)]
#[folder = "config/defaults/"]
pub struct DefaultConfigs;

#[derive(RustEmbed)]
#[folder = "migrations/"]
pub struct EmbeddedMigrations;

pub struct SolarMonitor {
    /// Simple device manager
    device_manager: DeviceManager,

    /// Web server (API + static files)
    web_server: Arc<WebServer>,

    /// Local SQLite database
    database: Arc<DataStore>,

    /// System configuration
    config: SystemConfig,
}

impl SolarMonitor {
    pub async fn new() -> Result<Self> {
        // Initialize with embedded defaults
        let config = Self::load_or_create_config().await?;
        let database = Arc::new(SqliteDatabase::new(&config.database_path).await?);

        // Run embedded migrations
        Self::run_migrations(&database).await?;

        // Initialize all components
        let engine = Arc::new(CoreEngine::new(config.clone()).await?);
        let web_server = Arc::new(WebServer::new(config.clone(), engine.clone()).await?);

        Ok(Self {
            engine,
            web_server,
            database,
            static_plugins: Self::load_builtin_plugins(),
            config_manager: Arc::new(ConfigManager::new(config)),
        })
    }

    pub async fn run(&self) -> Result<()> {
        tracing::info!("Starting Solar Monitor v{}", env!("CARGO_PKG_VERSION"));

        // Start all components
        let engine_handle = self.start_engine();
        let web_handle = self.start_web_server();
        let health_handle = self.start_health_monitoring();

        // Wait for shutdown signal
        tokio::select! {
            _ = engine_handle => tracing::info!("Engine stopped"),
            _ = web_handle => tracing::info!("Web server stopped"),
            _ = health_handle => tracing::info!("Health monitor stopped"),
            _ = Self::wait_for_shutdown() => {
                tracing::info!("Shutdown signal received");
                self.graceful_shutdown().await?;
            }
        }

        Ok(())
    }

    async fn load_or_create_config() -> Result<SystemConfig> {
        let config_path = "./config/system.toml";

        if !std::path::Path::new(config_path).exists() {
            // Create config directory and default config
            tokio::fs::create_dir_all("./config").await?;

            // Extract default config from embedded assets
            let default_config = DefaultConfigs::get("system.toml")
                .ok_or(DeploymentError::MissingEmbeddedAsset)?;

            tokio::fs::write(config_path, default_config.data.as_ref()).await?;
            tracing::info!("Created default configuration at {}", config_path);
        }

        let config_str = tokio::fs::read_to_string(config_path).await?;
        let config: SystemConfig = toml::from_str(&config_str)?;

        Ok(config)
    }

    async fn run_migrations(database: &SqliteDatabase) -> Result<()> {
        for migration_file in DatabaseMigrations::iter() {
            let migration = DatabaseMigrations::get(&migration_file)
                .ok_or(DeploymentError::MissingMigration(migration_file.to_string()))?;

            let sql = std::str::from_utf8(migration.data.as_ref())?;
            database.execute_migration(&migration_file, sql).await?;
        }

        tracing::info!("Database migrations completed");
        Ok(())
    }
}
```

### 2. Build System Configuration

```toml
# Cargo.toml - Single binary configuration
[package]
name = "solar-monitor"
version = "1.0.0"
edition = "2021"

[[bin]]
name = "solar-monitor"
path = "src/main.rs"

[dependencies]
# Core functionality
tokio = { version = "1.0", features = ["full"] }
axum = { version = "0.8", features = ["ws", "macros"] }
tower = "0.4"
tower-http = { version = "0.4", features = ["fs", "cors", "compression"] }

# Database (embedded SQLite)
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "sqlite", "chrono", "uuid"] }

# Configuration and serialization
serde = { version = "1.0", features = ["derive"] }
toml = "0.8"
specta = "1.0"

# Static asset embedding
rust-embed = { version = "8.0", features = ["compression"] }

# Plugins (static compilation)
inventory = "0.3"

# System interaction
sysinfo = "0.29"

[build-dependencies]
# Build script for asset preparation (Specta export handled by a bin)
specta = "1.0"

[profile.release]
# Optimize for size and performance on edge devices
lto = true              # Link-time optimization
codegen-units = 1       # Single codegen unit for better optimization
panic = "abort"         # Smaller binary size
strip = "symbols"       # Remove debug symbols
opt-level = "z"         # Optimize for size
```

### 3. Build Script for Asset Integration

```rust
// build.rs - Prepare assets and generate types
use std::env;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    // Generate TypeScript types from Rust
    generate_typescript_types()?;

    // Build frontend if in release mode
    if env::var("PROFILE")? == "release" {
        build_frontend()?;
    }

    // Generate embedded migration files
    generate_migration_metadata()?;

    println!("cargo:rerun-if-changed=frontend/");
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=database/migrations/");

    Ok(())
}

fn generate_typescript_types() -> Result<(), Box<dyn std::error::Error>> {
    // Use: cargo run -p contracts --bin export_types
    // to regenerate TypeScript types into types/ts/
    Ok(())
}

fn build_frontend() -> Result<(), Box<dyn std::error::Error>> {
    use std::process::Command;

    // Build frontend with Vite
    let output = Command::new("npm")
        .args(&["run", "build"])
        .current_dir("frontend")
        .output()?;

    if !output.status.success() {
        return Err(format!("Frontend build failed: {}",
                          String::from_utf8_lossy(&output.stderr)).into());
    }

    println!("Frontend build completed");
    Ok(())
}
```

## Deployment Methods

### 1. Direct Binary Deployment

```bash
#!/bin/bash
# deploy.sh - Simple deployment script

set -e

# Configuration
BINARY_NAME="solar-monitor"
SERVICE_NAME="solar-monitor"
INSTALL_DIR="/opt/solar-monitor"
CONFIG_DIR="/etc/solar-monitor"
DATA_DIR="/var/lib/solar-monitor"
LOG_DIR="/var/log/solar-monitor"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if running as root
check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root"
        exit 1
    fi
}

# Create necessary directories
create_directories() {
    log_info "Creating directories..."
    mkdir -p "$INSTALL_DIR" "$CONFIG_DIR" "$DATA_DIR" "$LOG_DIR"

    # Set proper permissions
    chown -R solar-monitor:solar-monitor "$DATA_DIR" "$LOG_DIR" 2>/dev/null || true
    chmod 755 "$INSTALL_DIR" "$CONFIG_DIR"
    chmod 750 "$DATA_DIR" "$LOG_DIR"
}

# Create service user
create_user() {
    if ! id -u solar-monitor >/dev/null 2>&1; then
        log_info "Creating solar-monitor user..."
        useradd --system --home-dir "$DATA_DIR" --shell /bin/false solar-monitor
    fi
}

# Install binary
install_binary() {
    log_info "Installing binary..."

    if [ ! -f "./$BINARY_NAME" ]; then
        log_error "Binary '$BINARY_NAME' not found in current directory"
        exit 1
    fi

    cp "./$BINARY_NAME" "$INSTALL_DIR/"
    chmod 755 "$INSTALL_DIR/$BINARY_NAME"
    chown root:root "$INSTALL_DIR/$BINARY_NAME"
}

# Create systemd service
create_service() {
    log_info "Creating systemd service..."

    cat > "/etc/systemd/system/$SERVICE_NAME.service" << EOF
[Unit]
Description=Solar Monitor - Universal Solar Inverter Monitoring
Documentation=https://github.com/your-org/solar-monitor
After=network.target
Wants=network.target

[Service]
Type=simple
User=solar-monitor
Group=solar-monitor
WorkingDirectory=$DATA_DIR
ExecStart=$INSTALL_DIR/$BINARY_NAME
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal
SyslogIdentifier=solar-monitor

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$DATA_DIR $LOG_DIR $CONFIG_DIR
CapabilityBoundingSet=
AmbientCapabilities=

# Resource limits (for edge devices)
MemoryMax=1G
CPUQuota=80%

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
}

# Main installation function
install() {
    log_info "Starting Solar Monitor installation..."

    check_root
    create_user
    create_directories
    install_binary
    create_service

    log_info "Installation completed!"
    log_info "To start the service: systemctl start $SERVICE_NAME"
    log_info "To enable auto-start: systemctl enable $SERVICE_NAME"
    log_info "Bootstrap config (optional): $CONFIG_DIR/system.toml"
    log_info "Data directory: $DATA_DIR"
    log_info "Logs: journalctl -u $SERVICE_NAME -f"
}

# Uninstall function
uninstall() {
    log_info "Uninstalling Solar Monitor..."

    check_root

    # Stop and disable service
    systemctl stop "$SERVICE_NAME" 2>/dev/null || true
    systemctl disable "$SERVICE_NAME" 2>/dev/null || true
    rm -f "/etc/systemd/system/$SERVICE_NAME.service"
    systemctl daemon-reload

    # Remove files
    rm -rf "$INSTALL_DIR"

    log_warn "Configuration and data preserved in $CONFIG_DIR and $DATA_DIR"
    log_warn "To remove completely: rm -rf $CONFIG_DIR $DATA_DIR $LOG_DIR"
    log_info "Uninstallation completed!"
}

# Update function
update() {
    log_info "Updating Solar Monitor..."

    check_root

    # Stop service
    systemctl stop "$SERVICE_NAME"

    # Backup current binary
    cp "$INSTALL_DIR/$BINARY_NAME" "$INSTALL_DIR/${BINARY_NAME}.backup"

    # Install new binary
    install_binary

    # Start service
    systemctl start "$SERVICE_NAME"

    log_info "Update completed!"
}

# Main script logic
case "${1:-install}" in
    install)
        install
        ;;
    uninstall)
        uninstall
        ;;
    update)
        update
        ;;
    *)
        echo "Usage: $0 {install|uninstall|update}"
        exit 1
        ;;
esac
```

### 2. Container Deployment (Optional)

```dockerfile
# Dockerfile - Multi-stage build for minimal container
FROM rust:1.75-slim as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy source code
COPY . .

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd --create-home --shell /bin/bash --uid 1000 solar-monitor

# Copy binary from builder
COPY --from=builder /app/target/release/solar-monitor /usr/local/bin/

# Create directories
RUN mkdir -p /etc/solar-monitor /var/lib/solar-monitor /var/log/solar-monitor \
    && chown -R solar-monitor:solar-monitor /var/lib/solar-monitor /var/log/solar-monitor

# Switch to non-root user
USER solar-monitor
WORKDIR /var/lib/solar-monitor

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/api/v1/health || exit 1

EXPOSE 8080

CMD ["solar-monitor"]
```

### 3. Embedded Device Deployment

```bash
#!/bin/bash
# raspberry-pi-deploy.sh - Optimized for Raspberry Pi

set -e

# Raspberry Pi specific settings
RPI_MODEL=$(cat /proc/cpuinfo | grep "Revision" | cut -d':' -f2 | xargs)
MEMORY_MB=$(free -m | awk 'NR==2{printf "%.0f", $2}')

log_info() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] [INFO] $1"
}

# Detect Raspberry Pi model and optimize accordingly
optimize_for_rpi() {
    log_info "Detected Raspberry Pi (Revision: $RPI_MODEL)"
    log_info "Available memory: ${MEMORY_MB}MB"

    # Set resource limits based on available memory
    if [ "$MEMORY_MB" -lt 2048 ]; then
        MEMORY_LIMIT="512M"
        CPU_QUOTA="60%"
        log_info "Low memory mode: Memory limit $MEMORY_LIMIT, CPU quota $CPU_QUOTA"
    else
        MEMORY_LIMIT="1G"
        CPU_QUOTA="80%"
        log_info "Standard mode: Memory limit $MEMORY_LIMIT, CPU quota $CPU_QUOTA"
    fi

    # Enable SD card optimizations
    echo "# Solar Monitor SD card optimizations" >> /etc/fstab
    echo "tmpfs /tmp tmpfs defaults,noatime,nosuid,size=100m 0 0" >> /etc/fstab
    echo "tmpfs /var/log tmpfs defaults,noatime,nosuid,mode=0755,size=100m 0 0" >> /etc/fstab

    # Configure log rotation for limited storage
    cat > "/etc/logrotate.d/solar-monitor" << EOF
/var/log/solar-monitor/*.log {
    daily
    rotate 7
    compress
    delaycompress
    missingok
    notifempty
    create 644 solar-monitor solar-monitor
}
EOF
}

# Enable hardware acceleration if available
enable_hardware_acceleration() {
    # Check for hardware crypto acceleration on Raspberry Pi 4
    if grep -q "crypto" /proc/cpuinfo; then
        log_info "Hardware crypto acceleration available"
        echo "CRYPTO_ACCELERATION=true" >> /etc/solar-monitor/environment
    fi

    # Enable GPU memory split for better performance
    if [ -f /boot/config.txt ]; then
        if ! grep -q "gpu_mem" /boot/config.txt; then
            echo "gpu_mem=64" >> /boot/config.txt
            log_info "Set GPU memory split to 64MB (reboot required)"
        fi
    fi
}

# Configure for minimal power consumption
configure_power_management() {
    # Disable unnecessary services
    systemctl disable bluetooth hciuart 2>/dev/null || true

    # Configure CPU governor for power efficiency
    echo 'GOVERNOR="ondemand"' > /etc/default/cpufrequtils

    # Disable HDMI if no display needed (saves ~25mA)
    if [ ! -f /etc/solar-monitor/keep-hdmi ]; then
        echo "/opt/vc/bin/tvservice -o" >> /etc/rc.local
        log_info "HDMI output disabled for power saving"
    fi
}

# Main deployment for Raspberry Pi
deploy_rpi() {
    log_info "Starting Raspberry Pi optimized deployment..."

    optimize_for_rpi
    enable_hardware_acceleration
    configure_power_management

    # Run standard installation
    install

    # Update systemd service with Pi-specific settings
    cat >> "/etc/systemd/system/solar-monitor.service" << EOF

# Raspberry Pi specific settings
MemoryMax=$MEMORY_LIMIT
CPUQuota=$CPU_QUOTA
Nice=5

# Environment variables
Environment=RUST_LOG=info
Environment=CRYPTO_ACCELERATION=auto
Environment=DEVICE_TYPE=raspberry_pi
EOF

    systemctl daemon-reload

    log_info "Raspberry Pi deployment completed!"
    log_info "Optimizations applied for $MEMORY_MB MB memory"

    # Offer to enable service
    read -p "Enable solar-monitor service to start on boot? [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        systemctl enable solar-monitor
        systemctl start solar-monitor
        log_info "Service enabled and started"
    fi
}

deploy_rpi
```

## Operations and Maintenance

### 1. Health Monitoring and Alerts

```rust
pub struct HealthMonitor {
    checks: Vec<Box<dyn HealthCheck>>,
    alert_manager: Arc<AlertManager>,
    config: HealthConfig,
}

#[derive(Debug, Clone)]
pub struct HealthConfig {
    pub check_interval: Duration,
    pub alert_thresholds: AlertThresholds,
    pub notification_methods: Vec<NotificationMethod>,
}

#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub memory_usage_percent: f64,
    pub cpu_usage_percent: f64,
    pub disk_usage_percent: f64,
    pub device_offline_minutes: u64,
    pub error_rate_percent: f64,
}

#[async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check(&self) -> HealthCheckResult;
    fn name(&self) -> &str;
}

pub struct SystemResourceCheck;

#[async_trait]
impl HealthCheck for SystemResourceCheck {
    async fn check(&self) -> HealthCheckResult {
        let system_info = collect_system_info().await?;

        let mut issues = Vec::new();

        if system_info.memory_usage_percent > 85.0 {
            issues.push(HealthIssue::HighMemoryUsage(system_info.memory_usage_percent));
        }

        if system_info.disk_usage_percent > 90.0 {
            issues.push(HealthIssue::HighDiskUsage(system_info.disk_usage_percent));
        }

        if system_info.cpu_temperature_celsius > 75.0 {
            issues.push(HealthIssue::HighTemperature(system_info.cpu_temperature_celsius));
        }

        if issues.is_empty() {
            HealthCheckResult::Healthy
        } else {
            HealthCheckResult::Unhealthy { issues }
        }
    }

    fn name(&self) -> &str { "system_resources" }
}
```

### 2. Configuration Management

```toml
# config/system.toml - Production configuration
[system]
# Basic system settings
bind_address = "0.0.0.0"
api_port = 8080
# WebSocket served on same port as HTTP
data_directory = "/var/lib/solar-monitor"
log_level = "info"

# Database settings (embedded SQLite)
[database]
path = "/var/lib/solar-monitor/solar-monitor.db"
connection_pool_size = 10
query_timeout_seconds = 30

# Performance settings for edge devices
[performance]
max_memory_mb = 1024
max_cpu_percent = 75
adaptive_polling = true
thermal_management = true

# Security settings
[security]
authentication_enabled = true
tls_enabled = true
auto_generate_cert = true

# Plugin settings
[plugins]
static_plugins_only = true
plugin_timeout_seconds = 30

# Monitoring and alerts
[monitoring]
health_check_interval_seconds = 60
metrics_retention_hours = 24
enable_alerts = true

[alerts]
memory_threshold_percent = 85
cpu_threshold_percent = 80
disk_threshold_percent = 90
device_offline_threshold_minutes = 10

# Device-specific optimizations
[device]
type = "auto_detect"
sd_card_optimization = true
power_management = true

# Data retention for edge deployment
[data_retention]
raw_data_days = 7
hourly_aggregates_days = 30
daily_aggregates_days = 365
```

### 3. Backup and Recovery

```bash
#!/bin/bash
# backup.sh - Backup and recovery for solar monitor

BACKUP_DIR="/var/backups/solar-monitor"
DATA_DIR="/var/lib/solar-monitor"
CONFIG_DIR="/etc/solar-monitor"
BACKUP_RETENTION_DAYS=30

create_backup() {
    local backup_name="solar-monitor-backup-$(date +%Y%m%d-%H%M%S)"
    local backup_path="$BACKUP_DIR/$backup_name"

    mkdir -p "$backup_path"

    # Stop service for consistent backup
    systemctl stop solar-monitor

    # Backup database
    cp "$DATA_DIR/solar-monitor.db" "$backup_path/"

    # Backup configuration
    cp -r "$CONFIG_DIR" "$backup_path/"

    # Compress backup
    tar -czf "$backup_path.tar.gz" -C "$BACKUP_DIR" "$backup_name"
    rm -rf "$backup_path"

    # Restart service
    systemctl start solar-monitor

    echo "Backup created: $backup_path.tar.gz"

    # Cleanup old backups
    find "$BACKUP_DIR" -name "*.tar.gz" -mtime +$BACKUP_RETENTION_DAYS -delete
}

restore_backup() {
    local backup_file="$1"

    if [ ! -f "$backup_file" ]; then
        echo "Backup file not found: $backup_file"
        exit 1
    fi

    # Stop service
    systemctl stop solar-monitor

    # Extract backup
    tar -xzf "$backup_file" -C "/tmp/"
    local extracted_dir="/tmp/$(basename "$backup_file" .tar.gz)"

    # Restore database
    cp "$extracted_dir/solar-monitor.db" "$DATA_DIR/"

    # Restore configuration
    cp -r "$extracted_dir/solar-monitor"/* "$CONFIG_DIR/"

    # Fix permissions
    chown -R solar-monitor:solar-monitor "$DATA_DIR"

    # Start service
    systemctl start solar-monitor

    echo "Backup restored from: $backup_file"
}

# Set up automated backup via cron
install_cron_backup() {
    cat > "/etc/cron.d/solar-monitor-backup" << EOF
# Solar Monitor automated backup
0 2 * * * root /opt/solar-monitor/backup.sh create_backup >/dev/null 2>&1
EOF

    echo "Automated backup installed (daily at 2 AM)"
}

case "${1:-create_backup}" in
    create_backup)
        create_backup
        ;;
    restore)
        restore_backup "$2"
        ;;
    install_cron)
        install_cron_backup
        ;;
    *)
        echo "Usage: $0 {create_backup|restore <file>|install_cron}"
        exit 1
        ;;
esac
```

### 4. Update and Maintenance

```bash
#!/bin/bash
# update.sh - In-place binary updates

INSTALL_DIR="/opt/solar-monitor"
BINARY_NAME="solar-monitor"
UPDATE_URL="https://github.com/your-org/solar-monitor/releases/latest/download"

check_for_updates() {
    local current_version=$(solar-monitor --version 2>/dev/null | cut -d' ' -f2 || echo "unknown")
    local latest_version=$(curl -s https://api.github.com/repos/your-org/solar-monitor/releases/latest | jq -r .tag_name)

    echo "Current version: $current_version"
    echo "Latest version: $latest_version"

    if [ "$current_version" != "$latest_version" ]; then
        echo "Update available!"
        return 0
    else
        echo "Already up to date"
        return 1
    fi
}

download_update() {
    local arch=$(uname -m)
    local download_url="$UPDATE_URL/solar-monitor-$arch"
    local temp_binary="/tmp/solar-monitor-new"

    echo "Downloading update..."
    curl -L "$download_url" -o "$temp_binary"
    chmod +x "$temp_binary"

    # Verify binary
    if ! "$temp_binary" --version >/dev/null 2>&1; then
        echo "Downloaded binary is invalid"
        rm -f "$temp_binary"
        return 1
    fi

    echo "Update downloaded successfully"
    echo "$temp_binary"
}

apply_update() {
    local new_binary="$1"

    if [ ! -f "$new_binary" ]; then
        echo "Update binary not found: $new_binary"
        return 1
    fi

    echo "Applying update..."

    # Create backup
    cp "$INSTALL_DIR/$BINARY_NAME" "$INSTALL_DIR/${BINARY_NAME}.backup"

    # Stop service
    systemctl stop solar-monitor

    # Replace binary
    cp "$new_binary" "$INSTALL_DIR/$BINARY_NAME"

    # Start service
    systemctl start solar-monitor

    # Verify service is running
    sleep 5
    if systemctl is-active --quiet solar-monitor; then
        echo "Update applied successfully"
        rm -f "$INSTALL_DIR/${BINARY_NAME}.backup"
        rm -f "$new_binary"
    else
        echo "Service failed to start, rolling back..."
        cp "$INSTALL_DIR/${BINARY_NAME}.backup" "$INSTALL_DIR/$BINARY_NAME"
        systemctl start solar-monitor
        return 1
    fi
}

auto_update() {
    if check_for_updates; then
        local new_binary=$(download_update)
        if [ $? -eq 0 ]; then
            apply_update "$new_binary"
        fi
    fi
}

case "${1:-check}" in
    check)
        check_for_updates
        ;;
    download)
        download_update
        ;;
    apply)
        apply_update "$2"
        ;;
    auto)
        auto_update
        ;;
    *)
        echo "Usage: $0 {check|download|apply <binary>|auto}"
        exit 1
        ;;
esac
```

## Single Binary Advantages

### Deployment Benefits

- **Zero Dependencies**: No need for runtime dependencies or package managers
- **Simple Installation**: Single file copy and configuration
- **Atomic Updates**: Replace entire binary in one operation
- **Consistent Environment**: Same behavior across all deployment targets
- **Minimal Attack Surface**: Fewer components to secure and maintain

### Edge Device Optimization

- **Resource Efficiency**: All components optimized together
- **Startup Speed**: No dynamic loading overhead
- **Storage Efficiency**: Single file vs multiple packages
- **Network Efficiency**: Single download for updates
- **Offline Capability**: Fully functional without internet connectivity

This deployment specification ensures the solar monitoring solution can be easily deployed and maintained on edge devices with minimal operational overhead while providing enterprise-grade reliability and monitoring capabilities.
