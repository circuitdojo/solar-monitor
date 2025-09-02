# Performance Requirements - Edge Device Specification

## Overview
Performance targets and resource limits for edge device deployment on Raspberry Pi and Nucbox systems. Optimized for minimal resource usage while maintaining responsive operation.

## Hardware Target Specifications

### Primary Target: Raspberry Pi 4B
```yaml
cpu: ARM Cortex-A72 quad-core @ 1.5GHz
memory: 4GB LPDDR4-3200
storage: MicroSD Class 10 (32GB minimum)
network: Gigabit Ethernet + Wi-Fi
power: 5V 3A USB-C (15W typical)
operating_temp: 0°C to 85°C
```

### Secondary Target: Intel Nucbox (N100)
```yaml
cpu: Intel N100 quad-core @ 3.4GHz boost
memory: 8GB DDR4-3200
storage: 128GB eMMC or NVMe SSD
network: Gigabit Ethernet + Wi-Fi 6
power: 12V 2A (24W typical)
operating_temp: 0°C to 70°C
```

### Minimum Requirements
```yaml
cpu_cores: 2
cpu_frequency: 1.0GHz minimum
memory: 2GB RAM minimum
storage: 16GB available space
network: 100Mbps Ethernet or Wi-Fi
```

## Performance Targets

### 1. Edge Device Resource Limits
```rust
#[derive(Debug, Clone)]
pub struct EdgePerformanceTargets {
    /// Conservative CPU usage for thermal management
    pub max_cpu_usage_percent: f64,
    
    /// Memory usage accounting for OS overhead
    pub max_memory_usage_mb: u32,
    
    /// SD card friendly I/O limits
    pub max_storage_iops: u32,
    
    /// Local network bandwidth
    pub max_network_mbps: f64,
    
    /// Response targets for dashboard
    pub api_response_time_ms: ResponseTimeTargets,
    
    /// Realistic throughput for 1-5 devices
    pub throughput: EdgeThroughputTargets,
}

#[derive(Debug, Clone)]
pub struct ResponseTimeTargets {
    pub p50_ms: u64,  // 50th percentile
    pub p95_ms: u64,  // 95th percentile
    pub p99_ms: u64,  // 99th percentile
    pub max_ms: u64,  // Absolute maximum
}

#[derive(Debug, Clone)]
pub struct ThroughputTargets {
    /// Device data points processed per second
    pub data_points_per_second: u32,
    
    /// Concurrent WebSocket connections
    pub max_websocket_connections: u32,
    
    /// API requests per second
    pub api_requests_per_second: u32,
    
    /// Maximum concurrent devices
    pub max_concurrent_devices: u32,
}

// Realistic targets for edge deployment (RPi4 with 4GB)
pub const EDGE_PERFORMANCE_TARGETS: EdgePerformanceTargets = EdgePerformanceTargets {
    max_cpu_usage_percent: 50.0,  // Conservative for thermal management
    max_memory_usage_mb: 512,     // 12% of 4GB, 25% of 2GB
    max_storage_iops: 50,         // SD card friendly
    max_network_mbps: 5.0,        // Local network only
    
    api_response_time_ms: ResponseTimeTargets {
        p50_ms: 25,   // Very fast for local
        p95_ms: 100,  // Still responsive
        p99_ms: 250,  // Acceptable worst case
        max_ms: 1000, // Reasonable timeout
    },
    
    throughput: ThroughputTargets {
        data_points_per_second: 10,     // ~5 devices @ 30s intervals
        max_websocket_connections: 5,   // Family dashboard clients
        api_requests_per_second: 10,    // Normal browsing
        max_concurrent_devices: 5,      // Residential scale
    },
};
```

### 2. Performance Monitoring System
```rust
use prometheus::{Counter, Histogram, Gauge, Registry};
use std::sync::Arc;
use tokio::time::{interval, Duration};

pub struct PerformanceMonitor {
    registry: Arc<Registry>,
    metrics: PerformanceMetrics,
    targets: PerformanceTargets,
    alerts: Vec<PerformanceAlert>,
}

#[derive(Clone)]
pub struct PerformanceMetrics {
    // System metrics
    pub cpu_usage: Gauge,
    pub memory_usage: Gauge,
    pub disk_usage: Gauge,
    pub network_io: Counter,
    
    // Application metrics
    pub active_devices: Gauge,
    pub data_points_processed: Counter,
    pub api_request_duration: Histogram,
    pub websocket_connections: Gauge,
    pub plugin_errors: Counter,
    
    // Database metrics
    pub db_query_duration: Histogram,
    pub db_connections_active: Gauge,
    pub db_operations_total: Counter,
    
    // Cache metrics (for edge optimization)
    pub cache_hits: Counter,
    pub cache_misses: Counter,
    pub cache_size_bytes: Gauge,
}

impl PerformanceMonitor {
    pub async fn new(targets: PerformanceTargets) -> Result<Self> {
        let registry = Arc::new(Registry::new());
        let metrics = Self::create_metrics(&registry)?;
        
        let monitor = Self {
            registry,
            metrics,
            targets,
            alerts: Vec::new(),
        };
        
        // Start background monitoring
        monitor.start_system_monitoring().await;
        
        Ok(monitor)
    }

    async fn start_system_monitoring(&self) {
        let metrics = self.metrics.clone();
        let targets = self.targets.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                // Collect system metrics
                if let Ok(sys_info) = Self::collect_system_info().await {
                    metrics.cpu_usage.set(sys_info.cpu_usage);
                    metrics.memory_usage.set(sys_info.memory_usage_mb as f64);
                    metrics.disk_usage.set(sys_info.disk_usage_percent);
                    
                    // Check for performance violations
                    Self::check_performance_violations(&sys_info, &targets).await;
                }
            }
        });
    }

    async fn collect_system_info() -> Result<SystemInfo> {
        #[cfg(target_os = "linux")]
        {
            use sysinfo::{System, SystemExt, CpuExt, DiskExt};
            
            let mut system = System::new_all();
            system.refresh_all();
            
            let cpu_usage = system.global_cpu_info().cpu_usage();
            let memory_usage_mb = (system.used_memory() / 1024 / 1024) as u32;
            let total_memory_mb = (system.total_memory() / 1024 / 1024) as u32;
            
            // Get disk usage for root partition
            let disk_usage_percent = system.disks()
                .iter()
                .find(|disk| disk.mount_point() == std::path::Path::new("/"))
                .map(|disk| {
                    let used = disk.total_space() - disk.available_space();
                    (used as f64 / disk.total_space() as f64) * 100.0
                })
                .unwrap_or(0.0);
            
            Ok(SystemInfo {
                cpu_usage: cpu_usage as f64,
                memory_usage_mb,
                total_memory_mb,
                disk_usage_percent,
                load_average: system.load_average().one,
            })
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            // Fallback for other platforms
            Ok(SystemInfo {
                cpu_usage: 0.0,
                memory_usage_mb: 0,
                total_memory_mb: 0,
                disk_usage_percent: 0.0,
                load_average: 0.0,
            })
        }
    }

    pub fn record_api_request(&self, duration: Duration, path: &str, status: u16) {
        self.metrics.api_request_duration
            .with_label_values(&[path, &status.to_string()])
            .observe(duration.as_secs_f64());
    }

    pub fn record_data_point_processed(&self, device_id: &str, plugin: &str) {
        self.metrics.data_points_processed
            .with_label_values(&[device_id, plugin])
            .inc();
    }
}

#[derive(Debug)]
pub struct SystemInfo {
    pub cpu_usage: f64,
    pub memory_usage_mb: u32,
    pub total_memory_mb: u32,
    pub disk_usage_percent: f64,
    pub load_average: f64,
}
```

### 3. Resource Management
```rust
pub struct ResourceManager {
    limits: ResourceLimits,
    current_usage: Arc<RwLock<ResourceUsage>>,
    scaling_config: ScalingConfig,
}

#[derive(Debug, Clone)]
pub struct ResourceLimits {
    /// Maximum memory before triggering cleanup
    pub memory_limit_mb: u32,
    
    /// Maximum CPU usage before throttling
    pub cpu_limit_percent: f64,
    
    /// Maximum disk usage before cleanup
    pub disk_limit_percent: f64,
    
    /// Connection limits to prevent resource exhaustion
    pub max_database_connections: u32,
    pub max_websocket_connections: u32,
    pub max_device_connections: u32,
}

#[derive(Debug, Clone)]
pub struct ScalingConfig {
    /// Reduce polling frequency under high load
    pub adaptive_polling: bool,
    
    /// Compress historical data more aggressively
    pub adaptive_compression: bool,
    
    /// Reduce WebSocket update frequency
    pub adaptive_websocket_frequency: bool,
    
    /// Temporarily disable non-essential features
    pub graceful_degradation: bool,
}

impl ResourceManager {
    pub async fn check_resource_pressure(&self) -> ResourcePressure {
        let usage = self.current_usage.read().await;
        let limits = &self.limits;
        
        let memory_pressure = (usage.memory_usage_mb as f64 / limits.memory_limit_mb as f64) * 100.0;
        let cpu_pressure = usage.cpu_usage;
        let disk_pressure = usage.disk_usage_percent;
        
        match (memory_pressure, cpu_pressure, disk_pressure) {
            (m, c, d) if m > 90.0 || c > 85.0 || d > 90.0 => ResourcePressure::Critical,
            (m, c, d) if m > 75.0 || c > 70.0 || d > 75.0 => ResourcePressure::High,
            (m, c, d) if m > 60.0 || c > 55.0 || d > 60.0 => ResourcePressure::Medium,
            _ => ResourcePressure::Low,
        }
    }

    pub async fn apply_resource_optimization(&self, pressure: ResourcePressure) -> Result<()> {
        match pressure {
            ResourcePressure::Critical => {
                tracing::warn!("Critical resource pressure detected, applying emergency optimizations");
                
                // Emergency measures
                self.reduce_polling_frequency(0.5).await?;
                self.limit_websocket_updates(Duration::from_secs(5)).await?;
                self.trigger_aggressive_cleanup().await?;
                self.disable_non_essential_features().await?;
            },
            ResourcePressure::High => {
                tracing::info!("High resource pressure, applying optimizations");
                
                self.reduce_polling_frequency(0.75).await?;
                self.limit_websocket_updates(Duration::from_secs(2)).await?;
                self.trigger_cleanup().await?;
            },
            ResourcePressure::Medium => {
                tracing::debug!("Medium resource pressure, minor optimizations");
                
                self.optimize_caching().await?;
                self.compress_old_data().await?;
            },
            ResourcePressure::Low => {
                // Normal operation, potentially restore full functionality
                self.restore_normal_operation().await?;
            },
        }
        
        Ok(())
    }

    async fn reduce_polling_frequency(&self, factor: f64) -> Result<()> {
        // Reduce device polling frequency by the given factor
        // Implementation would adjust polling intervals for all devices
        tracing::info!("Reduced polling frequency by factor {}", factor);
        Ok(())
    }

    async fn trigger_aggressive_cleanup(&self) -> Result<()> {
        // Emergency cleanup procedures
        // - Clear all caches
        // - Force garbage collection
        // - Close idle connections
        // - Temporarily pause non-critical background tasks
        
        tracing::warn!("Performing aggressive resource cleanup");
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResourcePressure {
    Low,
    Medium,
    High,
    Critical,
}
```

### 4. Edge Device Optimizations
```rust
pub struct EdgeOptimizations {
    device_type: EdgeDeviceType,
    optimizations: Vec<OptimizationType>,
}

#[derive(Debug, Clone)]
pub enum EdgeDeviceType {
    RaspberryPi { model: String, memory_mb: u32 },
    NucBox { cpu_model: String, memory_mb: u32 },
    Generic { cpu_arch: String, memory_mb: u32 },
}

#[derive(Debug, Clone)]
pub enum OptimizationType {
    /// Use faster crypto algorithms for ARM
    ArmCryptoOptimization,
    
    /// Optimize for SD card storage characteristics
    SdCardOptimization,
    
    /// Reduce memory fragmentation
    MemoryPooling,
    
    /// Use efficient data structures for limited RAM
    CompactDataStructures,
    
    /// Optimize for thermal throttling
    ThermalManagement,
    
    /// Battery/power management for portable deployments
    PowerManagement,
}

impl EdgeOptimizations {
    pub fn detect_device_type() -> EdgeDeviceType {
        // Detect device type from /proc/cpuinfo and /proc/meminfo
        #[cfg(target_os = "linux")]
        {
            if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
                if cpuinfo.contains("BCM") && cpuinfo.contains("ARM") {
                    // Raspberry Pi detection
                    let memory = Self::get_total_memory_mb();
                    return EdgeDeviceType::RaspberryPi {
                        model: Self::detect_rpi_model(&cpuinfo),
                        memory_mb: memory,
                    };
                } else if cpuinfo.contains("Intel") && cpuinfo.contains("N100") {
                    let memory = Self::get_total_memory_mb();
                    return EdgeDeviceType::NucBox {
                        cpu_model: "N100".to_string(),
                        memory_mb: memory,
                    };
                }
            }
        }
        
        EdgeDeviceType::Generic {
            cpu_arch: std::env::consts::ARCH.to_string(),
            memory_mb: Self::get_total_memory_mb(),
        }
    }

    pub fn apply_optimizations(&self) -> Result<()> {
        for optimization in &self.optimizations {
            match optimization {
                OptimizationType::ArmCryptoOptimization => {
                    // Use ChaCha20-Poly1305 instead of AES-GCM for ARM without crypto extensions
                    tracing::info!("Applying ARM crypto optimizations");
                },
                OptimizationType::SdCardOptimization => {
                    // Reduce write frequency, use write batching
                    tracing::info!("Applying SD card optimizations");
                },
                OptimizationType::MemoryPooling => {
                    // Pre-allocate memory pools to reduce fragmentation
                    tracing::info!("Applying memory pooling optimizations");
                },
                OptimizationType::CompactDataStructures => {
                    // Use more compact representations for data
                    tracing::info!("Applying compact data structure optimizations");
                },
                OptimizationType::ThermalManagement => {
                    // Monitor CPU temperature and throttle if needed
                    self.setup_thermal_monitoring()?;
                },
                OptimizationType::PowerManagement => {
                    // Optimize for power consumption
                    tracing::info!("Applying power management optimizations");
                },
            }
        }
        
        Ok(())
    }

    fn setup_thermal_monitoring(&self) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            tokio::spawn(async {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                
                loop {
                    interval.tick().await;
                    
                    if let Ok(temp_str) = tokio::fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").await {
                        if let Ok(temp_milli) = temp_str.trim().parse::<i32>() {
                            let temp_celsius = temp_milli / 1000;
                            
                            if temp_celsius > 70 {
                                tracing::warn!("High CPU temperature: {}°C, applying thermal throttling", temp_celsius);
                                // Implement thermal throttling
                                // - Reduce CPU-intensive operations
                                // - Increase polling intervals
                                // - Pause non-essential background tasks
                            }
                        }
                    }
                }
            });
        }
        
        Ok(())
    }
}
```

### 5. Performance Testing Framework
```rust
#[cfg(test)]
mod performance_tests {
    use super::*;
    use criterion::{criterion_group, criterion_main, Criterion};
    use tokio::runtime::Runtime;

    fn benchmark_data_processing(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();
        
        c.bench_function("process_device_data", |b| {
            b.to_async(&rt).iter(|| async {
                let pipeline = create_test_pipeline().await;
                let test_data = generate_test_data();
                
                pipeline.process_device_data(Uuid::new_v4(), test_data).await.unwrap();
            });
        });
    }

    fn benchmark_api_endpoints(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();
        
        c.bench_function("list_devices_api", |b| {
            b.to_async(&rt).iter(|| async {
                let app = create_test_app().await;
                let response = app.get("/api/v1/devices").await;
                assert!(response.status().is_success());
            });
        });
    }

    fn stress_test_concurrent_connections(c: &mut Criterion) {
        let rt = Runtime::new().unwrap();
        
        c.bench_function("concurrent_websockets", |b| {
            b.to_async(&rt).iter(|| async {
                let handles: Vec<_> = (0..20).map(|_| {
                    tokio::spawn(async {
                        // Simulate WebSocket connection and data flow
                        simulate_websocket_client().await
                    })
                }).collect();
                
                futures::future::join_all(handles).await;
            });
        });
    }

    criterion_group!(
        performance_benches,
        benchmark_data_processing,
        benchmark_api_endpoints,
        stress_test_concurrent_connections
    );
    criterion_main!(performance_benches);
}
```

## Configuration for Performance Tuning

### Edge Device Configuration
```toml
[performance]
device_type = "auto_detect" # or "raspberry_pi", "nucbox", "generic"

# Resource limits
[performance.limits]
max_memory_mb = 1536
max_cpu_percent = 60
max_disk_percent = 75
max_device_connections = 50
max_websocket_connections = 20

# Adaptive scaling
[performance.scaling]
adaptive_polling = true
adaptive_compression = true
adaptive_websocket_frequency = true
graceful_degradation = true

# Optimization flags
[performance.optimizations]
arm_crypto_optimization = true
sd_card_optimization = true
memory_pooling = true
compact_data_structures = true
thermal_management = true

# Monitoring
[performance.monitoring]
enable_metrics = true
metrics_retention_hours = 24
performance_alerts = true
resource_check_interval_seconds = 10

# Caching for edge devices
[performance.caching]
enable_response_caching = true
cache_size_mb = 64
cache_ttl_seconds = 300
```

## Scalability Limits and Recommendations

### Single Device Limits (Raspberry Pi 4)
- **Devices**: 10-50 concurrent devices depending on poll frequency
- **Data throughput**: 100-500 data points per second
- **WebSocket clients**: 10-20 concurrent connections
- **Historical data**: 1-3 months of raw data, 1+ years aggregated
- **API requests**: 50-100 requests per second

### Scaling Beyond Single Device
- **Horizontal scaling**: Deploy multiple edge devices for larger installations
- **Data aggregation**: Roll up data at edge, send summaries to central system
- **Load balancing**: Distribute devices across multiple monitoring instances
- **Centralized storage**: Use edge devices for real-time monitoring, central system for long-term storage

This performance specification ensures the solar monitoring solution runs efficiently on constrained edge devices while providing clear scaling paths for larger deployments.
