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

/// Get the actor for a physical port, spawning one if needed. Actors are
/// keyed by port spec (device path or `usb-serial:<SN>`, see
/// [`super::ports`]); a port has exactly one actor at one baud rate, and
/// requesting a different baud for an already-open port is a configuration
/// conflict that fails loudly.
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

/// Minimum quiet time between transactions on the bus. The inverter MCU
/// services the dongle UART at low priority; firing the next request the
/// instant the previous response is parsed makes it occasionally drop one.
const INTER_REQUEST_GAP: tokio::time::Duration = tokio::time::Duration::from_millis(50);

/// The serial context plus the reliability policy around it: lazy open with
/// spec re-resolution, an enforced inter-request gap, one retry per request,
/// and drop-to-flush on both transport errors and timeouts. Dropping after a
/// timeout matters for correctness, not just recovery: a late response has
/// no address field and would otherwise be consumed as the answer to the
/// *next* request, silently misattributing register data.
type BoxedCall<'a, T> = std::pin::Pin<
    Box<
        dyn Future<Output = Result<Result<T, tokio_modbus::ExceptionCode>, tokio_modbus::Error>>
            + Send
            + 'a,
    >,
>;

struct PortIo {
    spec: String,
    baud: u32,
    timeout_secs: u64,
    ctx: Option<tokio_modbus::client::Context>,
    last_io: tokio::time::Instant,
}

impl PortIo {
    fn new(spec: String, baud: u32, timeout_secs: u64) -> Self {
        Self {
            spec,
            baud,
            timeout_secs,
            ctx: None,
            last_io: tokio::time::Instant::now() - INTER_REQUEST_GAP,
        }
    }

    // Re-resolve the spec on every open: a `usb-serial:<SN>` spec finds the
    // adapter's current device node even after a replug renumbered it.
    fn open(&self) -> anyhow::Result<tokio_modbus::client::Context> {
        use tokio_serial::{DataBits, Parity, SerialStream, StopBits};
        let path = super::ports::resolve_port_spec(&self.spec)?;
        let builder = tokio_serial::new(&path, self.baud)
            .data_bits(DataBits::Eight)
            .parity(Parity::None)
            .stop_bits(StopBits::One)
            .timeout(std::time::Duration::from_secs(self.timeout_secs));
        let port = SerialStream::open(&builder)?;
        tracing::info!(
            "opened serial port {path} ({}) at {} baud",
            self.spec,
            self.baud
        );
        Ok(tokio_modbus::prelude::rtu::attach(port))
    }

    async fn transact<T>(
        &mut self,
        unit_id: u8,
        // A plain `AsyncFnMut` bound trips "implementation is not general
        // enough" here because the actor future must be Send; box explicitly.
        mut call: impl for<'a> FnMut(&'a mut tokio_modbus::client::Context) -> BoxedCall<'a, T>,
    ) -> anyhow::Result<T> {
        use tokio_modbus::prelude::*;

        let mut retried = false;
        loop {
            let ctx = match &mut self.ctx {
                Some(c) => c,
                None => {
                    let c = self
                        .open()
                        .map_err(|e| anyhow::anyhow!("failed to open {}: {e}", self.spec))?;
                    self.ctx.insert(c)
                }
            };
            tokio::time::sleep_until(self.last_io + INTER_REQUEST_GAP).await;
            ctx.set_slave(Slave(unit_id));
            let out = tokio::time::timeout(
                tokio::time::Duration::from_secs(self.timeout_secs),
                call(ctx),
            )
            .await;
            self.last_io = tokio::time::Instant::now();
            match out {
                // Modbus-level exceptions come back through the inner Result;
                // the device answered, so the port is healthy.
                Ok(Ok(inner)) => return inner.map_err(|e| anyhow::anyhow!(e)),
                Ok(Err(e)) => {
                    tracing::warn!(
                        "serial port {} transport error; reopening on next request",
                        self.spec
                    );
                    self.ctx = None;
                    return Err(anyhow::anyhow!(e));
                }
                Err(_) => {
                    // Drop the port to flush the response if it arrives late.
                    self.ctx = None;
                    if retried {
                        return Err(anyhow::anyhow!("timeout"));
                    }
                    retried = true;
                    tracing::debug!("request on {} timed out; retrying once", self.spec);
                }
            }
        }
    }
}

async fn run_port_actor(
    path: String,
    baud: u32,
    timeout_secs: u64,
    mut rx: tokio::sync::mpsc::Receiver<PortRequest>,
) {
    use tokio_modbus::client::{Reader, Writer};

    let mut io = PortIo::new(path, baud, timeout_secs);

    while let Some(msg) = rx.recv().await {
        match msg {
            PortRequest::ReadInput {
                unit_id,
                addr,
                qty,
                resp,
            } => {
                let res = io
                    .transact(unit_id, |ctx| Box::pin(ctx.read_input_registers(addr, qty)))
                    .await;
                let _ = resp.send(res);
            }
            PortRequest::ReadHolding {
                unit_id,
                addr,
                qty,
                resp,
            } => {
                let res = io
                    .transact(unit_id, |ctx| {
                        Box::pin(ctx.read_holding_registers(addr, qty))
                    })
                    .await;
                let _ = resp.send(res);
            }
            PortRequest::WriteSingleRegister {
                unit_id,
                addr,
                value,
                resp,
            } => {
                // Retrying a timed-out write is safe: writing the same value
                // to the same register is idempotent.
                let res = io
                    .transact(unit_id, |ctx| {
                        Box::pin(ctx.write_single_register(addr, value))
                    })
                    .await;
                let _ = resp.send(res.map(|_| ()));
            }
        }
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
