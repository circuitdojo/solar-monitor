//! Modbus RTU port actor.
//!
//! One actor task per physical serial port owns the `SerialStream` and
//! serializes all Modbus requests through an mpsc channel. Each request
//! carries its own unit id (`set_slave` before every operation), so multiple
//! devices on one RS485 bus multiplex correctly through a single actor.

use anyhow::Result;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct PortHandle {
    tx: tokio::sync::mpsc::Sender<PortRequest>,
}

enum PortRequest {
    ReadInput {
        unit_id: u8,
        addr: u16,
        qty: u16,
        resp: tokio::sync::oneshot::Sender<anyhow::Result<Vec<u16>>>,
    },
    ReadHolding {
        unit_id: u8,
        addr: u16,
        qty: u16,
        resp: tokio::sync::oneshot::Sender<anyhow::Result<Vec<u16>>>,
    },
    WriteSingleRegister {
        unit_id: u8,
        addr: u16,
        value: u16,
        resp: tokio::sync::oneshot::Sender<anyhow::Result<()>>,
    },
}

impl PortHandle {
    pub async fn read_input_registers(&self, unit_id: u8, addr: u16, qty: u16) -> Result<Vec<u16>> {
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
        rx.await
            .map_err(|_| anyhow::anyhow!("port actor dropped"))?
    }

    pub async fn read_holding_registers(
        &self,
        unit_id: u8,
        addr: u16,
        qty: u16,
    ) -> Result<Vec<u16>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PortRequest::ReadHolding {
                unit_id,
                addr,
                qty,
                resp: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("port actor unavailable"))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("port actor dropped"))?
    }

    pub async fn write_single_register(&self, unit_id: u8, addr: u16, value: u16) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(PortRequest::WriteSingleRegister {
                unit_id,
                addr,
                value,
                resp: tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("port actor unavailable"))?;
        rx.await
            .map_err(|_| anyhow::anyhow!("port actor dropped"))?
    }
}

struct PortEntry {
    handle: Arc<PortHandle>,
    baud: u32,
}

static PORT_ACTORS: Lazy<Mutex<HashMap<String, PortEntry>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Get the actor for a physical port, spawning one if needed. A port has
/// exactly one actor at one baud rate; requesting a different baud for an
/// already-open port is a configuration conflict and fails loudly.
pub async fn get_or_spawn_port_actor(
    path: &str,
    baud: u32,
    timeout_secs: u64,
) -> Result<Arc<PortHandle>> {
    {
        let actors = PORT_ACTORS.lock().unwrap();
        if let Some(entry) = actors.get(path) {
            if entry.baud != baud {
                return Err(anyhow::anyhow!(
                    "port {} already open at {} baud, requested {}",
                    path,
                    entry.baud,
                    baud
                ));
            }
            return Ok(entry.handle.clone());
        }
    }
    let (tx, rx) = tokio::sync::mpsc::channel::<PortRequest>(32);
    let handle = Arc::new(PortHandle { tx });
    PORT_ACTORS.lock().unwrap().insert(
        path.to_string(),
        PortEntry {
            handle: handle.clone(),
            baud,
        },
    );
    let path_s = path.to_string();
    tokio::spawn(async move {
        run_port_actor(path_s.clone(), baud, timeout_secs, rx).await;
        let _ = PORT_ACTORS.lock().unwrap().remove(&path_s);
    });
    Ok(handle)
}

async fn run_port_actor(
    path: String,
    baud: u32,
    timeout_secs: u64,
    mut rx: tokio::sync::mpsc::Receiver<PortRequest>,
) {
    use tokio::time::{Duration, timeout};
    use tokio_modbus::prelude::rtu;
    use tokio_modbus::prelude::*;
    use tokio_serial::{DataBits, Parity, SerialStream, StopBits};

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
                if rx.recv().await.is_none() {
                    break;
                }
                continue;
            }
        };
        let mut ctx = rtu::attach(port);

        while let Some(msg) = rx.recv().await {
            match msg {
                PortRequest::ReadInput {
                    unit_id,
                    addr,
                    qty,
                    resp,
                } => {
                    // Switch slave then read
                    ctx.set_slave(Slave(unit_id));
                    let res = match timeout(
                        Duration::from_secs(timeout_secs),
                        ctx.read_input_registers(addr, qty),
                    )
                    .await
                    {
                        Ok(Ok(regs)) => regs.map_err(|e| anyhow::anyhow!(e)),
                        Ok(Err(e)) => Err(anyhow::anyhow!(e)),
                        Err(_) => Err(anyhow::anyhow!("timeout")),
                    };
                    let _ = resp.send(res);
                }
                PortRequest::ReadHolding {
                    unit_id,
                    addr,
                    qty,
                    resp,
                } => {
                    ctx.set_slave(Slave(unit_id));
                    let res = match timeout(
                        Duration::from_secs(timeout_secs),
                        ctx.read_holding_registers(addr, qty),
                    )
                    .await
                    {
                        Ok(Ok(regs)) => regs.map_err(|e| anyhow::anyhow!(e)),
                        Ok(Err(e)) => Err(anyhow::anyhow!(e)),
                        Err(_) => Err(anyhow::anyhow!("timeout")),
                    };
                    let _ = resp.send(res);
                }
                PortRequest::WriteSingleRegister {
                    unit_id,
                    addr,
                    value,
                    resp,
                } => {
                    ctx.set_slave(Slave(unit_id));
                    let res = match timeout(
                        Duration::from_secs(timeout_secs),
                        ctx.write_single_register(addr, value),
                    )
                    .await
                    {
                        Ok(Ok(result)) => result.map_err(|e| anyhow::anyhow!(e)).map(|_| ()),
                        Ok(Err(e)) => Err(anyhow::anyhow!(e)),
                        Err(_) => Err(anyhow::anyhow!("timeout")),
                    };
                    let _ = resp.send(res);
                }
            };
        }
        // channel closed, exit loop
        break;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn baud_mismatch_is_rejected() {
        let path = "/dev/ttyNONEXISTENT-test";
        let first = get_or_spawn_port_actor(path, 19200, 1).await;
        assert!(first.is_ok());
        let err = match get_or_spawn_port_actor(path, 9600, 1).await {
            Ok(_) => panic!("different baud on same port must fail"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("already open at 19200"));
        // Same baud reuses the existing actor
        assert!(get_or_spawn_port_actor(path, 19200, 1).await.is_ok());
    }
}
