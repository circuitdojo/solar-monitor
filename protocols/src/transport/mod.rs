//! Shared bus transports. One actor owns each physical serial port and
//! serializes all requests, so multiple devices (unit IDs) can share a bus.

pub mod modbus_rtu;
