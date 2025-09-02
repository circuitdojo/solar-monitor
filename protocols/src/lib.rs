//! Protocols registry and EG4 RS485 stub

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use solar_monitor_core as core;
use solar_monitor_core::{
    DeviceConnection, DeviceConfig, DeviceData, DeviceMetrics, DeviceProtocol, DeviceStatus,
    DeviceType, DiscoveredDevice, HealthStatus, ProtocolCapabilities, ProtocolMetadata, ScanConfig,
};

pub fn registered() -> &'static [&'static str] { &["eg4-pi30-rs485"] }

pub fn create_registry() -> core::ProtocolRegistry {
    let mut reg = core::ProtocolRegistry::new();
    reg.register_protocol(Arc::new(Eg4Pi30Rs485));
    reg
}

pub struct Eg4Pi30Rs485;

#[async_trait]
impl DeviceProtocol for Eg4Pi30Rs485 {
    fn protocol_name(&self) -> &'static str { "eg4-pi30-rs485" }

    fn metadata(&self) -> ProtocolMetadata {
        ProtocolMetadata {
            name: "EG4/PI30 RS485",
            version: "0.1.0",
            description: "EG4 6000XP inverter via RS485 using PI30",
            supported_device_types: &[DeviceType::SolarInverter],
            capabilities: ProtocolCapabilities {
                supports_discovery: true,
                supports_commands: true,
                supports_real_time: true,
                max_concurrent_connections: Some(1),
            },
        }
    }

    fn supported_device_types(&self) -> Vec<DeviceType> { vec![DeviceType::SolarInverter] }

    async fn discover_devices(&self, scan: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        let mut out = Vec::new();
        for port in &scan.serial_ports {
            if let Ok((id, name)) = try_qid(port, scan.timeout_seconds).await {
                out.push(DiscoveredDevice {
                    id,
                    name,
                    device_type: DeviceType::SolarInverter,
                    protocol: "eg4-pi30-rs485".to_string(),
                    connection_params: HashMap::from([
                        ("serial_port".to_string(), port.clone()),
                        ("baud_rate".to_string(), "9600".to_string()),
                        ("data_bits".to_string(), "8".to_string()),
                        ("parity".to_string(), "none".to_string()),
                        ("stop_bits".to_string(), "1".to_string()),
                    ]),
                });
            }
        }
        Ok(out)
    }

    async fn connect(&self, config: &DeviceConfig) -> Result<Box<dyn DeviceConnection>> {
        let serial_port = config
            .connection_params
            .get("serial_port")
            .ok_or_else(|| anyhow!("Missing serial_port parameter"))?;
        let baud: u32 = config
            .connection_params
            .get("baud_rate")
            .and_then(|s| s.parse().ok())
            .unwrap_or(9600);
        let data_bits = config
            .connection_params
            .get("data_bits")
            .map(|s| s.as_str())
            .unwrap_or("8");
        let parity = config
            .connection_params
            .get("parity")
            .map(|s| s.as_str())
            .unwrap_or("none");
        let stop_bits = config
            .connection_params
            .get("stop_bits")
            .map(|s| s.as_str())
            .unwrap_or("1");
        let timeout_secs: u64 = config
            .connection_params
            .get("timeout_seconds")
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

        let conn = Eg4Conn::open(serial_port, baud, data_bits, parity, stop_bits, timeout_secs).await?;
        Ok(Box::new(conn))
    }
}

struct Eg4Conn {
    port: tokio_serial::SerialStream,
}

#[async_trait]
impl DeviceConnection for Eg4Conn {
    async fn read_data(&mut self) -> Result<DeviceData> {
        let raw = self.send_pi30("QPIGS").await?;
        let metrics = parse_qpigs_to_metrics(&raw)?;
        let now = chrono::Utc::now();
        let status = DeviceStatus { is_connected: true, last_seen: now, health: HealthStatus::Healthy, error_message: None };
        Ok(DeviceData { device_id: "eg4".into(), timestamp: now, device_type: DeviceType::SolarInverter, metrics, status, raw_data: Some(raw) })
    }

    async fn send_command(&mut self, command: &str) -> Result<String> { self.send_pi30(command).await }
    fn is_connected(&self) -> bool { true }
    async fn health_check(&mut self) -> Result<()> { let _ = self.send_pi30("QID").await?; Ok(()) }
}

impl Eg4Conn {
    async fn open(path: &str, baud: u32, data_bits: &str, parity: &str, stop_bits: &str, timeout_secs: u64) -> Result<Self> {
        use tokio_serial::{DataBits, Parity, SerialPortBuilderExt, StopBits};

        let mut builder = tokio_serial::new(path, baud);
        builder = builder
            .data_bits(match data_bits { "7" => DataBits::Seven, _ => DataBits::Eight })
            .parity(match parity { "odd" => Parity::Odd, "even" => Parity::Even, _ => Parity::None })
            .stop_bits(match stop_bits { "2" => StopBits::Two, _ => StopBits::One })
            .timeout(std::time::Duration::from_secs(timeout_secs));
        let port = builder.open_native_async()?;
        Ok(Self { port })
    }

    async fn send_pi30(&mut self, cmd: &str) -> Result<String> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::time::{timeout, Duration};

        let full = build_pi30_command(cmd);
        self.port.write_all(full.as_bytes()).await?;

        // Read response with a bounded timeout
        let mut buf = vec![0u8; 1024];
        let n = timeout(Duration::from_secs(3), self.port.read(&mut buf)).await??;
        buf.truncate(n);
        let s = String::from_utf8_lossy(&buf).to_string();
        if !s.starts_with('(') {
            return Err(anyhow!("Invalid PI30 response: missing '('"));
        }
        Ok(s)
    }
}

fn build_pi30_command(cmd: &str) -> String {
    let crc = crc16_modbus(cmd.as_bytes());
    format!("{}{:04X}\r", cmd, crc)
}

fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            if (crc & 1) != 0 { crc = (crc >> 1) ^ 0xA001; } else { crc >>= 1; }
        }
    }
    crc
}

fn parse_qpigs_to_metrics(resp: &str) -> Result<DeviceMetrics> {
    // Expect format: ( ..fields.. \r
    if !resp.starts_with('(') { return Err(anyhow!("Invalid QPIGS format")); }
    // Trim leading '(' and trailing CR if present
    let body = resp.trim_start_matches('(').trim_end_matches(['\r', '\n']);
    let parts: Vec<&str> = body.split_whitespace().collect();
    // We expect at least 16 fields per the EG4 mapping
    if parts.len() < 16 { return Err(anyhow!("QPIGS: insufficient fields")); }

    let parse_f = |i: usize| -> Option<f64> { parts.get(i).and_then(|s| s.parse::<f64>().ok()) };

    let grid_voltage = parse_f(0);
    let grid_frequency = parse_f(1);
    let output_apparent_power = parse_f(4);
    let output_active_power = parse_f(5);
    let output_load_percent = parse_f(6);
    let bus_voltage = parse_f(7);
    let battery_voltage = parse_f(8);
    let battery_charging_current = parse_f(9);
    let battery_capacity = parse_f(10);
    let inverter_temperature = parse_f(11);
    let pv_current = parse_f(12);
    let pv_voltage = parse_f(13);
    let battery_scc_voltage = parse_f(14);
    let battery_discharge_current = parse_f(15);

    let pv_power = pv_voltage.zip(pv_current).map(|(v, i)| v * i);
    let battery_current = battery_charging_current.zip(battery_discharge_current).map(|(c, d)| c - d);

    let mut custom = HashMap::new();
    if let Some(v) = bus_voltage { custom.insert("bus_voltage".to_string(), v); }
    if let Some(v) = battery_scc_voltage { custom.insert("scc_voltage".to_string(), v); }
    if let Some(v) = output_apparent_power { custom.insert("apparent_power".to_string(), v); }

    Ok(DeviceMetrics {
        input_power_watts: pv_power,
        output_power_watts: output_active_power,
        load_percentage: output_load_percent,
        battery_voltage,
        battery_current,
        battery_soc_percentage: battery_capacity,
        battery_temperature_celsius: None,
        pv_voltage,
        pv_current,
        pv_power_watts: pv_power,
        grid_voltage,
        grid_frequency,
        grid_power_watts: None,
        device_temperature_celsius: inverter_temperature,
        efficiency_percentage: None,
        fault_codes: vec![],
        operating_mode: Some("Normal".to_string()),
        custom_metrics: custom,
    })
}

async fn try_qid(port: &str, timeout_secs: u32) -> Result<(String, String)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{timeout, Duration};
    use tokio_serial::{SerialPortBuilderExt};

    let builder = tokio_serial::new(port, 9600)
        .data_bits(tokio_serial::DataBits::Eight)
        .parity(tokio_serial::Parity::None)
        .stop_bits(tokio_serial::StopBits::One)
        .timeout(Duration::from_secs(timeout_secs as u64));
    let mut s = builder.open_native_async()?;

    let cmd = build_pi30_command("QID");
    s.write_all(cmd.as_bytes()).await?;
    let mut buf = vec![0u8; 256];
    let n = timeout(Duration::from_secs(timeout_secs as u64), s.read(&mut buf)).await??;
    buf.truncate(n);
    let resp = String::from_utf8_lossy(&buf).to_string();
    if !resp.starts_with('(') { return Err(anyhow!("invalid QID resp")); }
    // Derive a simple id from first few chars and a friendly name
    let id = format!("eg4-{}", port);
    let name = format!("EG4 6000XP on {}", port);
    Ok((id, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_qpigs_basic() {
        // indices:
        // 0 grid_voltage, 1 grid_frequency, 4 apparent, 5 active, 6 load%, 7 bus_v, 8 batt_v,
        // 9 batt_charge_a, 10 batt_capacity, 11 inverter_temp, 12 pv_current, 13 pv_voltage,
        // 14 scc_voltage, 15 batt_discharge_a
        let resp = "(230.0 50.0 230.0 50.0 500 450 12 400.0 52.5 10.0 85.0 25.0 5.0 100.0 54.0 2.0\r";
        let m = parse_qpigs_to_metrics(resp).expect("parse qpigs");
        assert_eq!(m.grid_voltage, Some(230.0));
        assert_eq!(m.grid_frequency, Some(50.0));
        assert_eq!(m.output_power_watts, Some(450.0));
        assert_eq!(m.load_percentage, Some(12.0));
        assert_eq!(m.battery_voltage, Some(52.5));
        assert_eq!(m.pv_voltage, Some(100.0));
        assert_eq!(m.pv_current, Some(5.0));
        // pv_power should be voltage * current
        assert_eq!(m.pv_power_watts, Some(500.0));
        // derived battery current = charge - discharge = 10 - 2 = 8
        assert_eq!(m.battery_current, Some(8.0));
        // custom metrics should include bus_voltage and scc_voltage
        assert_eq!(m.custom_metrics.get("bus_voltage"), Some(&400.0));
        assert_eq!(m.custom_metrics.get("scc_voltage"), Some(&54.0));
    }
}
