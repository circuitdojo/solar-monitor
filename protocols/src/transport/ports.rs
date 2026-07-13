//! Stable serial-port identity.
//!
//! Kernel device names (`/dev/ttyUSB0`) are assigned at plug-in time and can
//! change across replugs — the old name stays taken while any stale fd holds
//! it. A port *spec* is therefore either a raw device path or
//! `usb-serial:<SN>`, where `<SN>` is the adapter's USB serial number as
//! reported by port enumeration. Specs are what enumeration hands out, what
//! device configs store, and what the port actor re-resolves to a concrete
//! device node on every (re)open, so a replugged adapter is found wherever
//! the kernel put it.

use anyhow::{Result, anyhow};
use tokio_serial::SerialPortType;

const USB_SERIAL_PREFIX: &str = "usb-serial:";

/// Enumerate serial ports as specs: `usb-serial:<SN>` for USB adapters that
/// report a serial number, the raw device path otherwise (built-in UARTs,
/// platforms whose enumeration lacks USB metadata such as FreeBSD, and
/// adapters without a programmed serial — common on CH340 clones).
pub fn list_port_specs() -> Vec<String> {
    let Ok(ports) = tokio_serial::available_ports() else {
        return Vec::new();
    };
    ports
        .into_iter()
        .map(|p| match p.port_type {
            SerialPortType::UsbPort(usb) => match usb.serial_number {
                Some(sn) if !sn.is_empty() => format!("{USB_SERIAL_PREFIX}{sn}"),
                _ => p.port_name,
            },
            _ => p.port_name,
        })
        .collect()
}

/// Resolve a port spec to the device path it currently points at. Raw paths
/// pass through untouched; `usb-serial:<SN>` scans enumeration for the
/// adapter with that USB serial number (first match wins if two adapters
/// report the same serial).
pub fn resolve_port_spec(spec: &str) -> Result<String> {
    let Some(sn) = spec.strip_prefix(USB_SERIAL_PREFIX) else {
        return Ok(spec.to_string());
    };
    for p in tokio_serial::available_ports()? {
        if let SerialPortType::UsbPort(usb) = &p.port_type
            && usb.serial_number.as_deref() == Some(sn)
        {
            return Ok(p.port_name);
        }
    }
    Err(anyhow!("no serial port with USB serial number {sn}"))
}
