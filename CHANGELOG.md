# Changelog

All notable changes to Solar Monitor are documented here. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project adheres to [Semantic Versioning](https://semver.org/).

## [0.5.0-rc.1] - 2026-11-18

### Added

- Combined view for inverters wired in parallel. With two or more enabled
  inverters, the dashboard defaults to a "System (N inverters)" view: PV,
  load, battery charge/discharge, grid import/export and daily energy are
  summed across units; battery SOC, voltages and grid frequency are
  averaged; temperatures show the hottest unit. The view only updates once
  every inverter has reported, so a missing inverter stalls it instead of
  showing a partial total. Live over the WebSocket (`combined_data`) and as
  history from `GET /api/v1/combined/data`. Experimental: whether load and
  grid import/export are per-unit on a real parallel install is unconfirmed.
- The 6000XP reports its parallel role and inverter count (input register
  113) as the `parallel_role` and `parallel_count` custom metrics.
- Dev-only `mock` feature (`just serve-mock`): a synthetic inverter protocol,
  including settings, for UI work without hardware. Never in release builds.

### Security

- Dependency advisories fixed: lettre, quinn, rustls, rustls-webpki and
  webpki-roots updated within semver; sqlx 0.8 → 0.9.1 so the bundled SQLite
  is 3.54 (RUSTSEC-2026-0459). Existing databases open unchanged. sqlx no
  longer compiles the MySQL driver, so the `rsa` audit ignore is gone.

### Changed

- The dashboard has a fixed "Dashboard" title with a device dropdown, and
  the Settings title no longer repeats the device name. Dashboard and
  Settings remember their device selection independently.

## [0.4.6] - 2026-07-29

### Fixed

- Pending notification retries are superseded by newer transitions for the
  same rule, device, and channel, preventing a delayed grid-lost alert from
  arriving after grid restoration. Retries also resolve the latest channel
  configuration before each attempt and stop when their rule or channel is
  disabled, deleted, or unlinked.
- The dashboard falls back to battery voltage × current for historical data
  and protocols that do not provide the direct `Pcharge`/`Pdischarge` custom
  metrics, rather than incorrectly showing the battery as idle.

## [0.4.5] - 2026-07-29

### Fixed

- Battery power was reported ten times too low. `BatCurrent_BMS` (input
  register 98) was decoded at 0.01 A/LSB instead of 0.1, so a 530 W charge
  after a grid outage showed as 56 W. Caught on live hardware: with grid
  and PV at zero and ~490 W on the EPS output, the battery — the only
  possible source — read 1.06 A instead of ~10.6 A.

### Changed

- The dashboard battery flow reads `Pcharge`/`Pdischarge` (input registers
  10 and 11), which the inverter reports in watts, instead of inferring
  power from battery voltage × current. Both are exposed as the
  `battery_charge_power` and `battery_discharge_power` custom metrics.

## [0.4.4] - 2026-07-29

### Fixed

- Notifications that fail for a transient reason are now retried instead of
  dropped. A grid outage takes the router down with it, so the grid-lost
  alert — the one that matters most — was the one that could not leave the
  house, while the later grid-restored alert sailed through. Delivery
  failures are now classified: transport errors (DNS, connect, TLS,
  timeout), HTTP 408/429/5xx, and non-permanent SMTP errors retry on a
  30 s / 1 m / 2 m / 5 m / 10 m / 10 m / 10 m backoff (~38 min); config
  errors and other 4xx responses (bad token, missing key) are permanent and
  are not retried.
- Notification log entries record the full error cause chain. A failure that
  never reached the service read as `pushover request failed` with no
  indication that the network, not Pushover, was at fault.

### Changed

- The notification log shows the outcome of a retried delivery: the first
  failure appears immediately, then the entry flips to success with
  `delivered on attempt N`, or to `<error> (gave up after retrying)` once
  the backoff is exhausted.

## [0.4.3] - 2026-07-13

### Changed

- Hardware-risky settings now carry their own confirmation warning instead
  of one hardcoded UI message. `eps_voltage`, `eps_frequency`,
  `charge_voltage`, and `equalization_voltage` join `inverter_state` and
  `eps_enabled` as confirm-gated writes. `DeviceSettingDto.requiresConfirm`
  (bool) is replaced by `confirm` (warning string, or null) — an
  API/TypeScript wire change.

## [0.4.2] - 2026-07-13

### Changed

- Device ids are server-minted UUIDs. The Add Device form no longer asks for
  an ID — `POST /api/v1/devices` generates one and returns it, and
  `PUT /api/v1/devices/{id}` is update-only (404 for unknown ids) instead of
  upserting, so a create can no longer silently overwrite an existing device.
  `AddDeviceRequestDto` loses its `id` field (API/TypeScript wire change).
  Existing devices keep their ids; export/import still preserves ids.

## [0.4.1] - 2026-07-13

### Fixed

- Transient Modbus timeouts no longer drop poll samples. The 6000XP
  occasionally skips a request even on a healthy bus; the port actor now
  enforces a 50 ms quiet gap between transactions, retries each request once
  on timeout, and reopens the port between attempts so a late-arriving
  response can't be misattributed to the next request (RTU responses carry
  no address field). A `WARN` poll failure now means both attempts timed
  out; single timeouts log at `debug`.

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
