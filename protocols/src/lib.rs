//! Protocols registry and EG4 RS485 stub

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use once_cell::sync::Lazy;
use solar_monitor_core as core;
use solar_monitor_core::{
    DeviceConfig, DeviceConnection, DeviceData, DeviceMetrics, DeviceProtocol, DeviceStatus,
    DeviceType, DiscoveredDevice, HealthStatus, ProtocolCapabilities, ProtocolMetadata, ScanConfig,
};

pub fn registered() -> &'static [&'static str] {
    &["eg4-6000xp-modbus", "eg4-pi30-rs485"]
}

pub fn create_registry() -> core::ProtocolRegistry {
    let mut reg = core::ProtocolRegistry::new();
    reg.register_protocol(Arc::new(Eg4_6000xpModbus));
    reg.register_protocol(Arc::new(Eg4Pi30Rs485));
    reg
}

pub struct Eg4Pi30Rs485;

#[async_trait]
impl DeviceProtocol for Eg4Pi30Rs485 {
    fn protocol_name(&self) -> &'static str {
        "eg4-pi30-rs485"
    }

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

    fn supported_device_types(&self) -> Vec<DeviceType> {
        vec![DeviceType::SolarInverter]
    }

    async fn discover_devices(&self, scan: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        let mut out = Vec::new();
        // Try common PI30 baud rates to improve discovery odds
        let bauds: &[u32] = &[2400, 9600];
        for port in &scan.serial_ports {
            let mut identified: Option<(String, String, u32)> = None;
            for &baud in bauds {
                if let Ok((id, name)) = try_qid_baud(port, baud, scan.timeout_seconds).await {
                    identified = Some((id, name, baud));
                    break;
                }
            }
            if let Some((id, name, baud)) = identified {
                out.push(DiscoveredDevice {
                    id,
                    name,
                    device_type: DeviceType::SolarInverter,
                    protocol: "eg4-pi30-rs485".to_string(),
                    connection_params: HashMap::from([
                        ("serial_port".to_string(), port.clone()),
                        ("baud_rate".to_string(), baud.to_string()),
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

        let conn = Eg4Conn::open(
            serial_port,
            baud,
            data_bits,
            parity,
            stop_bits,
            timeout_secs,
        )
        .await?;
        Ok(Box::new(conn))
    }
}

// --- EG4 6000XP via Modbus RTU ---

pub struct Eg4_6000xpModbus;

#[async_trait]
impl DeviceProtocol for Eg4_6000xpModbus {
    fn protocol_name(&self) -> &'static str {
        "eg4-6000xp-modbus"
    }

    fn metadata(&self) -> ProtocolMetadata {
        ProtocolMetadata {
            name: "EG4 6000XP Modbus RTU",
            version: "0.1.0",
            description: "EG4 6000XP inverter via Modbus RTU",
            supported_device_types: &[DeviceType::SolarInverter],
            capabilities: ProtocolCapabilities {
                supports_discovery: true,
                supports_commands: false,
                supports_real_time: true,
                max_concurrent_connections: Some(1),
            },
        }
    }

    fn supported_device_types(&self) -> Vec<DeviceType> {
        vec![DeviceType::SolarInverter]
    }

    async fn discover_devices(&self, scan: &ScanConfig) -> Result<Vec<DiscoveredDevice>> {
        let mut out = Vec::new();
        for port in &scan.serial_ports {
            for unit_id in 1u8..=3u8 {
                if let Ok(_) =
                    try_read_basic_modbus(port, 9600, unit_id, scan.timeout_seconds as u64).await
                {
                    out.push(DiscoveredDevice {
                        id: format!("eg4-6000xp-{}-{}", port.replace('/', "_"), unit_id),
                        name: format!("EG4 6000XP on {} (id {})", port, unit_id),
                        device_type: DeviceType::SolarInverter,
                        protocol: "eg4-6000xp-modbus".to_string(),
                        connection_params: HashMap::from([
                            ("serial_port".to_string(), port.clone()),
                            ("baud_rate".to_string(), "9600".to_string()),
                            ("data_bits".to_string(), "8".to_string()),
                            ("parity".to_string(), "none".to_string()),
                            ("stop_bits".to_string(), "1".to_string()),
                            ("unit_id".to_string(), unit_id.to_string()),
                        ]),
                    });
                    break;
                }
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
        let timeout_secs: u64 = config
            .connection_params
            .get("timeout_seconds")
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);
        let unit_id: u8 = config
            .connection_params
            .get("unit_id")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1u8);

        let conn = Eg4ModbusConn::open(serial_port, baud, timeout_secs, unit_id).await?;
        Ok(Box::new(conn))
    }
}

struct Eg4ModbusConn {
    path: String,
    baud: u32,
    unit_id: u8,
    timeout_secs: u64,
    handle: std::sync::Arc<PortHandle>,
}

// Port actor machinery
#[derive(Clone)]
struct PortHandle {
    tx: tokio::sync::mpsc::Sender<PortRequest>,
}

enum PortRequest {
    ReadInput {
        unit_id: u8,
        addr: u16,
        qty: u16,
        resp: tokio::sync::oneshot::Sender<anyhow::Result<Vec<u16>>>,
    },
}

impl PortHandle {
    async fn read_input_registers(&self, unit_id: u8, addr: u16, qty: u16) -> Result<Vec<u16>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PortRequest::ReadInput {
                unit_id,
                addr,
                qty,
                resp: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("port actor unavailable"))?;
        rx.await.map_err(|_| anyhow::anyhow!("port actor dropped"))?
    }
}

static PORT_ACTORS: Lazy<
    std::sync::Mutex<std::collections::HashMap<String, std::sync::Arc<PortHandle>>>,
> = Lazy::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

async fn get_or_spawn_port_actor(
    path: &str,
    baud: u32,
    timeout_secs: u64,
) -> std::sync::Arc<PortHandle> {
    let key = format!("{}@{}", path, baud);
    if let Some(h) = PORT_ACTORS.lock().unwrap().get(&key).cloned() {
        return h;
    }
    let (tx, rx) = tokio::sync::mpsc::channel::<PortRequest>(32);
    let handle = std::sync::Arc::new(PortHandle { tx });
    PORT_ACTORS
        .lock()
        .unwrap()
        .insert(key.clone(), handle.clone());
    let path_s = path.to_string();
    tokio::spawn(async move {
        run_port_actor(path_s, baud, timeout_secs, rx).await;
        // on exit, remove?
        let _ = PORT_ACTORS.lock().unwrap().remove(&key);
    });
    handle
}

async fn run_port_actor(
    path: String,
    baud: u32,
    timeout_secs: u64,
    mut rx: tokio::sync::mpsc::Receiver<PortRequest>,
) {
    use tokio_modbus::prelude::*;
    use tokio_modbus::prelude::rtu;
    use tokio_serial::{DataBits, Parity, SerialPortBuilderExt, SerialStream, StopBits};
    use tokio::time::{timeout, Duration};

    loop {
        // open port and attach RTU context without fixed slave
        let builder = tokio_serial::new(&path, baud)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One)
            .timeout(std::time::Duration::from_secs(timeout_secs));
        let port = match SerialStream::open(&builder) {
            Ok(p) => p,
            Err(_e) => {
                let _ = tokio::time::sleep(Duration::from_millis(500)).await;
                if rx.recv().await.is_none() { break; }
                continue;
            }
        };
        let mut ctx = rtu::attach(port);

        while let Some(msg) = rx.recv().await {
            let res: anyhow::Result<Vec<u16>> = match msg {
                PortRequest::ReadInput { unit_id, addr, qty, .. } => {
                    // Switch slave then read
                    ctx.set_slave(Slave(unit_id));
                    match timeout(Duration::from_secs(timeout_secs), ctx.read_input_registers(addr, qty)).await {
                        Ok(Ok(regs)) => regs.map_err(|e| anyhow::anyhow!(e)),
                        Ok(Err(e)) => Err(anyhow::anyhow!(e)),
                        Err(_) => Err(anyhow::anyhow!("timeout")),
                    }
                }
            };
            // send response back to requester
            match msg { PortRequest::ReadInput { resp, .. } => { let _ = resp.send(res); } }
        }
        // channel closed, exit loop
        break;
    }
}

async fn read_full(port: &mut tokio_serial::SerialStream, mut buf: &mut [u8]) -> std::io::Result<()> {
    use tokio::io::AsyncReadExt;
    while !buf.is_empty() {
        let n = port.read(buf).await?;
        if n == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "eof"));
        }
        let tmp = buf;
        buf = &mut tmp[n..];
    }
    Ok(())
}

impl Eg4ModbusConn {
    async fn open(path: &str, baud: u32, timeout_secs: u64, unit_id: u8) -> Result<Self> {
        let handle = get_or_spawn_port_actor(path, baud, timeout_secs).await;
        Ok(Self {
            path: path.to_string(),
            baud,
            unit_id,
            timeout_secs,
            handle,
        })
    }

    async fn read_input_registers(&mut self, addr: u16, qty: u16) -> Result<Vec<u8>> {
        let regs = self
            .handle
            .read_input_registers(self.unit_id, addr, qty)
            .await?;
        let mut out = Vec::with_capacity(regs.len() * 2);
        for r in regs { out.extend_from_slice(&r.to_le_bytes()); }
        Ok(out)
    }
}

#[async_trait]
impl DeviceConnection for Eg4ModbusConn {
    async fn read_data(&mut self) -> Result<DeviceData> {
        // Read 5 blocks of 40 input registers each as per shared mapping
        let b1 = self.read_input_registers(0, 40).await?;
        let b2 = self.read_input_registers(40, 40).await?;
        let b3 = self.read_input_registers(80, 40).await?;
        let b4 = self.read_input_registers(120, 40).await?;
        let b5 = self.read_input_registers(160, 40).await?;

        let u16le = |s: &[u8]| -> u16 { u16::from_le_bytes([s[0], s[1]]) };
        let i16le = |s: &[u8]| -> i16 { i16::from_le_bytes([s[0], s[1]]) };
        // b1 indices in bytes
        let vpv1 = u16le(&b1[2..4]) as f64 / 10.0;
        let vbat = u16le(&b1[8..10]) as f64 / 10.0;
        let soc = u16le(&b1[10..12]) as f64;
        let vac_r = u16le(&b1[24..26]) as f64 / 10.0;
        let fac = u16le(&b1[30..32]) as f64 / 100.0;
        let pinv = u16le(&b1[32..34]) as f64; // W
        let ppv1 = u16le(&b1[14..16]) as f64;
        let ppv2 = u16le(&b1[16..18]) as f64;
        let ppv3 = u16le(&b1[18..20]) as f64;
        let pv_power = ppv1 + ppv2 + ppv3;
        // b5 contains Pload (reg 170 -> bytes index 20..22)
        let pload = u16le(&b5[20..22]) as f64;
        // b3 contains BatCurrentBMS at reg 98 -> data3[36..38]
        // skipping extra read: approximate battery current from Pcharge/Pdischarge and vbat
        let pcharge = u16le(&b1[20..22]) as f64;
        let pdis = u16le(&b1[22..24]) as f64;
        let batt_cur = if vbat > 0.1 {
            (pcharge - pdis) / vbat
        } else {
            0.0
        };

        // Additional metrics from further blocks
        let vpv2 = u16le(&b1[4..6]) as f64 / 10.0;
        let vpv3 = u16le(&b1[6..8]) as f64 / 10.0;
        let vac_s = u16le(&b1[26..28]) as f64 / 10.0;
        let vac_t = u16le(&b1[28..30]) as f64 / 10.0;
        let linv_rms = u16le(&b1[36..38]) as f64 / 100.0;
        let pf = u16le(&b1[38..40]) as f64 / 1000.0;
        let veps_r = u16le(&b1[40..42]) as f64 / 10.0;
        let veps_s = u16le(&b1[42..44]) as f64 / 10.0;
        let veps_t = u16le(&b1[44..46]) as f64 / 10.0;
        let feps = u16le(&b1[46..48]) as f64 / 100.0;
        let peps = u16le(&b1[48..50]) as f64;
        let seps = u16le(&b1[50..52]) as f64;
        let ptogrid = u16le(&b1[52..54]) as f64;
        let ptouser = u16le(&b1[54..56]) as f64;
        let epv1_day = u16le(&b1[56..58]) as f64 / 10.0;
        let epv2_day = u16le(&b1[58..60]) as f64 / 10.0;
        let epv3_day = u16le(&b1[60..62]) as f64 / 10.0;
        let einv_day = u16le(&b1[62..64]) as f64 / 10.0;
        let erec_day = u16le(&b1[64..66]) as f64 / 10.0;
        let echg_day = u16le(&b1[66..68]) as f64 / 10.0;
        let edischg_day = u16le(&b1[68..70]) as f64 / 10.0;
        let eeps_day = u16le(&b1[70..72]) as f64 / 10.0;
        let etogrid_day = u16le(&b1[72..74]) as f64 / 10.0;
        let etouser_day = u16le(&b1[74..76]) as f64 / 10.0;
        let vbus1 = u16le(&b1[76..78]) as f64 / 10.0;
        let vbus2 = u16le(&b1[78..80]) as f64 / 10.0;

        // Totals and temps (b2)
        let tinner = u16le(&b2[48..50]) as f64;
        let tradiator1 = u16le(&b2[50..52]) as f64;
        let tradiator2 = u16le(&b2[52..54]) as f64;
        let tbat = u16le(&b2[54..56]) as f64;
        let epv1_all = (u16le(&b2[0..2]) as u32 | ((u16le(&b2[2..4]) as u32) << 16)) as f64 / 10.0;
        let epv2_all = (u16le(&b2[4..6]) as u32 | ((u16le(&b2[6..8]) as u32) << 16)) as f64 / 10.0;
        let epv3_all = (u16le(&b2[8..10]) as u32 | ((u16le(&b2[10..12]) as u32) << 16)) as f64 / 10.0;
        let einv_all = (u16le(&b2[12..14]) as u32 | ((u16le(&b2[14..16]) as u32) << 16)) as f64 / 10.0;
        let erec_all = (u16le(&b2[16..18]) as u32 | ((u16le(&b2[18..20]) as u32) << 16)) as f64 / 10.0;
        let echg_all = (u16le(&b2[20..22]) as u32 | ((u16le(&b2[22..24]) as u32) << 16)) as f64 / 10.0;
        let edischg_all = (u16le(&b2[24..26]) as u32 | ((u16le(&b2[26..28]) as u32) << 16)) as f64 / 10.0;
        let eeps_all = (u16le(&b2[28..30]) as u32 | ((u16le(&b2[30..32]) as u32) << 16)) as f64 / 10.0;
        let etogrid_all = (u16le(&b2[32..34]) as u32 | ((u16le(&b2[34..36]) as u32) << 16)) as f64 / 10.0;
        let etouser_all = (u16le(&b2[36..38]) as u32 | ((u16le(&b2[38..40]) as u32) << 16)) as f64 / 10.0;

        // BMS current and cell stats (b3)
        let bat_current_bms = i16le(&b3[36..38]) as f64 / 100.0;
        let max_cell_v = u16le(&b3[42..44]) as f64 / 1000.0;
        let min_cell_v = u16le(&b3[44..46]) as f64 / 1000.0;
        let max_cell_t = i16le(&b3[46..48]) as f64 / 10.0;
        let min_cell_t = i16le(&b3[48..50]) as f64 / 10.0;
        let cycles_bms = u16le(&b3[52..54]) as f64;

        // Generator and EPS (b4)
        let gen_v = u16le(&b4[2..4]) as f64 / 10.0;
        let gen_f = u16le(&b4[4..6]) as f64 / 100.0;
        let gen_p = u16le(&b4[6..8]) as f64;
        let eps_v_l1n = u16le(&b4[14..16]) as f64 / 10.0;
        let eps_v_l2n = u16le(&b4[16..18]) as f64 / 10.0;
        let peps_l1n = u16le(&b4[18..20]) as f64;
        let peps_l2n = u16le(&b4[20..22]) as f64;

        // Per-phase and load energy (b5)
        let eload_day = u16le(&b5[22..24]) as f64 / 10.0;
        let pinv_s = u16le(&b5[40..42]) as f64;
        let pinv_t = u16le(&b5[42..44]) as f64;
        let ptogrid_s = u16le(&b5[48..50]) as f64;
        let ptogrid_t = u16le(&b5[50..52]) as f64;
        let ptouser_s = u16le(&b5[52..54]) as f64;
        let ptouser_t = u16le(&b5[54..56]) as f64;

        let now = chrono::Utc::now();
        let mut metrics = DeviceMetrics::default();
        metrics.pv_voltage = Some(vpv1);
        metrics.battery_voltage = Some(vbat);
        metrics.battery_current = Some(if bat_current_bms.abs() > 0.0 { bat_current_bms } else { batt_cur });
        metrics.battery_soc_percentage = Some(soc);
        metrics.grid_voltage = Some(vac_r);
        metrics.grid_frequency = Some(fac);
        metrics.output_power_watts = Some(if pload > 0.0 { pload } else { pinv });
        metrics.pv_power_watts = Some(pv_power);
        metrics.device_temperature_celsius = Some(tinner);
        metrics.custom_metrics.insert("pv1_power".into(), ppv1);
        metrics.custom_metrics.insert("pv2_power".into(), ppv2);
        metrics.custom_metrics.insert("pv3_power".into(), ppv3);
        metrics.custom_metrics.insert("pv1_voltage".into(), vpv1);
        metrics.custom_metrics.insert("pv2_voltage".into(), vpv2);
        metrics.custom_metrics.insert("pv3_voltage".into(), vpv3);
        metrics.custom_metrics.insert("grid_voltage_s".into(), vac_s);
        metrics.custom_metrics.insert("grid_voltage_t".into(), vac_t);
        metrics.custom_metrics.insert("inverter_rms_current".into(), linv_rms);
        metrics.custom_metrics.insert("power_factor".into(), pf);
        metrics.custom_metrics.insert("offgrid_voltage_r".into(), veps_r);
        metrics.custom_metrics.insert("offgrid_voltage_s".into(), veps_s);
        metrics.custom_metrics.insert("offgrid_voltage_t".into(), veps_t);
        metrics.custom_metrics.insert("offgrid_frequency".into(), feps);
        metrics.custom_metrics.insert("offgrid_power_active".into(), peps);
        metrics.custom_metrics.insert("offgrid_power_apparent".into(), seps);
        metrics.custom_metrics.insert("export_power".into(), ptogrid);
        metrics.custom_metrics.insert("import_power".into(), ptouser);
        metrics.custom_metrics.insert("pv1_day_kwh".into(), epv1_day);
        metrics.custom_metrics.insert("pv2_day_kwh".into(), epv2_day);
        metrics.custom_metrics.insert("pv3_day_kwh".into(), epv3_day);
        metrics.custom_metrics.insert("inverter_day_kwh".into(), einv_day);
        metrics.custom_metrics.insert("ac_charge_day_kwh".into(), erec_day);
        metrics.custom_metrics.insert("charge_day_kwh".into(), echg_day);
        metrics.custom_metrics.insert("discharge_day_kwh".into(), edischg_day);
        metrics.custom_metrics.insert("offgrid_day_kwh".into(), eeps_day);
        metrics.custom_metrics.insert("export_day_kwh".into(), etogrid_day);
        metrics.custom_metrics.insert("import_day_kwh".into(), etouser_day);
        metrics.custom_metrics.insert("bus1_voltage".into(), vbus1);
        metrics.custom_metrics.insert("bus2_voltage".into(), vbus2);
        metrics.custom_metrics.insert("pv1_total_kwh".into(), epv1_all);
        metrics.custom_metrics.insert("pv2_total_kwh".into(), epv2_all);
        metrics.custom_metrics.insert("pv3_total_kwh".into(), epv3_all);
        metrics.custom_metrics.insert("inverter_total_kwh".into(), einv_all);
        metrics.custom_metrics.insert("ac_charge_total_kwh".into(), erec_all);
        metrics.custom_metrics.insert("charge_total_kwh".into(), echg_all);
        metrics.custom_metrics.insert("discharge_total_kwh".into(), edischg_all);
        metrics.custom_metrics.insert("offgrid_total_kwh".into(), eeps_all);
        metrics.custom_metrics.insert("export_total_kwh".into(), etogrid_all);
        metrics.custom_metrics.insert("import_total_kwh".into(), etouser_all);
        metrics.custom_metrics.insert("battery_temp_c".into(), tbat);
        metrics.custom_metrics.insert("heatsink_temp1_c".into(), tradiator1);
        metrics.custom_metrics.insert("heatsink_temp2_c".into(), tradiator2);
        metrics.custom_metrics.insert("bms_max_cell_v".into(), max_cell_v);
        metrics.custom_metrics.insert("bms_min_cell_v".into(), min_cell_v);
        metrics.custom_metrics.insert("bms_max_cell_t_c".into(), max_cell_t);
        metrics.custom_metrics.insert("bms_min_cell_t_c".into(), min_cell_t);
        metrics.custom_metrics.insert("bms_cycles".into(), cycles_bms);
        metrics.custom_metrics.insert("gen_voltage".into(), gen_v);
        metrics.custom_metrics.insert("gen_frequency".into(), gen_f);
        metrics.custom_metrics.insert("gen_power".into(), gen_p);
        metrics.custom_metrics.insert("eps_voltage_l1n".into(), eps_v_l1n);
        metrics.custom_metrics.insert("eps_voltage_l2n".into(), eps_v_l2n);
        metrics.custom_metrics.insert("eps_power_l1n".into(), peps_l1n);
        metrics.custom_metrics.insert("eps_power_l2n".into(), peps_l2n);
        metrics.custom_metrics.insert("load_day_kwh".into(), eload_day);
        metrics.custom_metrics.insert("inverter_power_s".into(), pinv_s);
        metrics.custom_metrics.insert("inverter_power_t".into(), pinv_t);
        metrics.custom_metrics.insert("export_power_s".into(), ptogrid_s);
        metrics.custom_metrics.insert("export_power_t".into(), ptogrid_t);
        metrics.custom_metrics.insert("import_power_s".into(), ptouser_s);
        metrics.custom_metrics.insert("import_power_t".into(), ptouser_t);

        let status = DeviceStatus {
            is_connected: true,
            last_seen: now,
            health: HealthStatus::Healthy,
            error_message: None,
        };
        Ok(DeviceData {
            device_id: "eg4-6000xp".into(),
            timestamp: now,
            device_type: DeviceType::SolarInverter,
            metrics,
            status,
            raw_data: None,
        })
    }

    async fn send_command(&mut self, _command: &str) -> Result<String> {
        Err(anyhow!("not supported"))
    }
    fn is_connected(&self) -> bool {
        true
    }
    async fn health_check(&mut self) -> Result<()> {
        Ok(())
    }
}

async fn try_read_basic_modbus(
    path: &str,
    baud: u32,
    unit_id: u8,
    timeout_secs: u64,
) -> Result<()> {
    let handle = get_or_spawn_port_actor(path, baud, timeout_secs).await;
    let _ = handle.read_input_registers(unit_id, 0, 2).await?;
    Ok(())
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
        let status = DeviceStatus {
            is_connected: true,
            last_seen: now,
            health: HealthStatus::Healthy,
            error_message: None,
        };
        Ok(DeviceData {
            device_id: "eg4".into(),
            timestamp: now,
            device_type: DeviceType::SolarInverter,
            metrics,
            status,
            raw_data: Some(raw),
        })
    }

    async fn send_command(&mut self, command: &str) -> Result<String> {
        self.send_pi30(command).await
    }
    fn is_connected(&self) -> bool {
        true
    }
    async fn health_check(&mut self) -> Result<()> {
        let _ = self.send_pi30("QID").await?;
        Ok(())
    }
}

impl Eg4Conn {
    async fn open(
        path: &str,
        baud: u32,
        data_bits: &str,
        parity: &str,
        stop_bits: &str,
        timeout_secs: u64,
    ) -> Result<Self> {
        use tokio_serial::{DataBits, Parity, SerialPortBuilderExt, StopBits};

        let mut builder = tokio_serial::new(path, baud);
        builder = builder
            .data_bits(match data_bits {
                "7" => DataBits::Seven,
                _ => DataBits::Eight,
            })
            .parity(match parity {
                "odd" => Parity::Odd,
                "even" => Parity::Even,
                _ => Parity::None,
            })
            .stop_bits(match stop_bits {
                "2" => StopBits::Two,
                _ => StopBits::One,
            })
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
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

fn parse_qpigs_to_metrics(resp: &str) -> Result<DeviceMetrics> {
    // Expect format: ( ..fields.. \r
    if !resp.starts_with('(') {
        return Err(anyhow!("Invalid QPIGS format"));
    }
    // Trim leading '(' and trailing CR if present
    let body = resp.trim_start_matches('(').trim_end_matches(['\r', '\n']);
    let parts: Vec<&str> = body.split_whitespace().collect();
    // We expect at least 16 fields per the EG4 mapping
    if parts.len() < 16 {
        return Err(anyhow!("QPIGS: insufficient fields"));
    }

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
    let battery_current = battery_charging_current
        .zip(battery_discharge_current)
        .map(|(c, d)| c - d);

    let mut custom = HashMap::new();
    if let Some(v) = bus_voltage {
        custom.insert("bus_voltage".to_string(), v);
    }
    if let Some(v) = battery_scc_voltage {
        custom.insert("scc_voltage".to_string(), v);
    }
    if let Some(v) = output_apparent_power {
        custom.insert("apparent_power".to_string(), v);
    }

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

async fn try_qid_baud(port: &str, baud: u32, timeout_secs: u32) -> Result<(String, String)> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{timeout, Duration};
    use tokio_serial::SerialPortBuilderExt;

    let builder = tokio_serial::new(port, baud)
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
    if !resp.starts_with('(') {
        return Err(anyhow!("invalid QID resp"));
    }
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
        let resp =
            "(230.0 50.0 230.0 50.0 500 450 12 400.0 52.5 10.0 85.0 25.0 5.0 100.0 54.0 2.0\r";
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
