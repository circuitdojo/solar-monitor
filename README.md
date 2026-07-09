# Solar Monitor

Monitoring service for EG4 solar inverters (6000XP via Modbus RTU, PI30 over RS485). Single Rust binary serving a REST/WebSocket API and an embedded Preact web UI, with SQLite storage. Runs on a Raspberry Pi next to the inverter.

## Local Development

The frontend must be built before the server can serve it (the dev server reads `web/dist/` from disk; there is no Vite proxy setup):

```sh
cd web && npm install && npm run build
```

Then run the server from anywhere in the repo:

```sh
cargo run -p solar-monitor -- --serve --port 8090
```

Open http://localhost:8090. The SQLite DB defaults to `./data/solar.db` (created automatically, relative to the current directory; override with `--db`).

After changing frontend code, re-run `npm run build`. TypeScript types for API DTOs are generated from the Rust contracts — regenerate with `cargo run -p contracts --bin export_types` (output in `types/ts/`) rather than editing them by hand.

## Building for the Raspberry Pi (aarch64)

One-time setup on the build machine:

```sh
rustup target add aarch64-unknown-linux-gnu
# Arch: pacman -S aarch64-linux-gnu-gcc
# Debian/Ubuntu: apt install gcc-aarch64-linux-gnu
```

Build a self-contained release binary with the web UI embedded (`embed-frontend`). Build the frontend first — its `dist/` gets compiled into the binary:

```sh
cd web && npm run build && cd ..
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --release --target aarch64-unknown-linux-gnu \
  -p solar-monitor --features solar-monitor-api/embed-frontend
```

No sysroot or `pkg-config` setup is needed: the `serialport` dependency is used with `default-features = false`, which drops the libudev native dependency (port enumeration falls back to sysfs on Linux — you lose only USB VID/PID metadata in listings). Keep it that way; re-enabling default features breaks cross-compilation.

## Deploying

The production Pi is `pi@solar-pi.local` (Debian Bookworm, aarch64, key-only SSH — note the username). The RS485 adapter is `/dev/ttyUSB0`.

```sh
scp target/aarch64-unknown-linux-gnu/release/solar-monitor pi@solar-pi.local:/tmp/
ssh pi@solar-pi.local 'sudo install -m755 /tmp/solar-monitor /usr/local/bin/ && sudo systemctl restart solar-monitor'
```

The service is defined in `/etc/systemd/system/solar-monitor.service`:

- `ExecStart=/usr/local/bin/solar-monitor --serve --bind 0.0.0.0 --port 8080 --db /var/lib/solar-monitor/solar.db`
- `User=inverter` with `SupplementaryGroups=dialout` (serial port access)
- `StateDirectory=solar-monitor` provides the DB directory
- Hardening: `ProtectHome=true`, `ProtectSystem=full`, `PrivateTmp=true`. The binary must live outside `/home` (e.g. `/usr/local/bin`) or `ProtectHome` prevents systemd from executing it — this has bitten before.

Check on it with:

```sh
ssh pi@solar-pi.local systemctl status solar-monitor
curl http://solar-pi.local:8080/api/v1/health
```

Dashboard: http://solar-pi.local:8080

For a first-time install on a new host, the binary can generate and install the unit itself: `solar-monitor --install` (see `--help` for `--user`, `--data-dir`, `--service-name`).

## Device Discovery

Probe for inverters on the Pi (stop the service first if it is actively polling the same port):

```sh
ssh pi@solar-pi.local '/usr/local/bin/solar-monitor --discover --serial-ports /dev/ttyUSB0 --timeout 5'
```

Discovery probes Modbus RTU unit IDs 1–3 at 19200 then 9600 baud. Devices can also be added manually in the web UI (Devices → Add).

Wiring notes (learned the hard way):

- The 6000XP's Modbus RTU interface is the **dongle port** (4-pin connector where the EG4 WiFi dongle plugs in; black/white wires are A/B) at **19200 baud 8N1, unit ID 1**. It serves the LuxPower register map via function 0x04 (read input registers). See [jsharkey/lxp-esphome](https://github.com/jsharkey/lxp-esphome) for a community register reference.
- The **battery comms port** is a different bus: an EG4-LL/LifePower4 BMS answers there at 9600 baud, function 0x03 only, and truncates any response longer than ~12 bytes. Plugging the adapter into that port will never reach the inverter.

## Workspace Layout

| Crate | Purpose |
|---|---|
| `contracts` | Shared DTOs; exports TypeScript types via specta |
| `core` | Core engine types (scan config, versioning) |
| `protocols` | Protocol drivers: EG4 6000XP Modbus RTU, PI30 RS485 |
| `storage` | SQLite persistence (sqlx), migrations in `migrations/` |
| `api` | Axum router, WebSocket, static/embedded frontend serving |
| `bin` | `solar-monitor` binary: CLI, service install, composition |
| `web` | Preact + Vite + Tailwind frontend (not a cargo crate) |

## Linting

```sh
cargo fmt
cargo clippy --workspace --all-features
```
