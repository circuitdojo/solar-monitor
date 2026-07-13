# Changelog

All notable changes to Solar Monitor are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project adheres to [Semantic Versioning](https://semver.org/).

## [0.4.0] - 2026-07-13

### Fixed

- Serial port recovery after a USB replug. The Modbus port actor previously
  opened its serial fd once and held it forever, so an unplugged cable left
  every poll failing with `Input/output error (os error 5)` until the service
  was restarted. The actor now opens the port lazily and drops it on
  transport errors, reopening on the next poll; Modbus exceptions and
  timeouts do not churn the port.
- Requests arriving while the port is unavailable get an error reply instead
  of being silently dropped.

### Changed

- Serial ports are identified by a stable spec: `usb-serial:<SN>` when the
  adapter reports a USB serial number, the raw device path otherwise. Port
  enumeration returns specs, device configs store them, and the actor
  re-resolves the spec to the current device node on every reopen — so
  recovery works even when the kernel renumbers the port (`ttyUSB0` →
  `ttyUSB1`). Existing configs holding a raw path keep working but do not
  survive renumbering; re-select the port in the device editor to adopt the
  spec.
- `api` and `bin` no longer depend on `serialport` directly; enumeration
  goes through the `tokio-serial` re-exports in the protocols crate.

## [0.3.0] - 2026-07-10

### Added

- Hourly downsampling with configurable retention (`--retention-days`,
  default 30): full-resolution rows fold into avg/min/max hourly buckets,
  and data queries transparently merge both tables.

### Changed

- Workspace migrated to Rust edition 2024.

## [0.2.0] - 2026-07-10

### Added

- App version in the web UI footer.
- Release workflow: pushing a version tag builds and publishes binaries.

## [0.1.0] - 2026-07-10

Initial release: EG4 6000XP polling over RS485 Modbus RTU (LuxPower register
map), SQLite storage, REST + WebSocket API with embedded Preact UI, validated
inverter settings (including generator charging), notification engine with
ntfy/email/Pushover/webhook channels, discovery, systemd install, and
Raspberry Pi / FreeBSD cross-compilation.
