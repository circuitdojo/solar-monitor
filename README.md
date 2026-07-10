# Solar Monitor

Monitoring service for EG4 solar inverters (6000XP via Modbus RTU). Single Rust binary serving a REST/WebSocket API and an embedded Preact web UI, with SQLite storage. Runs on a Raspberry Pi next to the inverter.

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

## Building for FreeBSD (x86_64)

Unlike the Pi build, FreeBSD cross-linking **does** need a sysroot: rustup only
ships the Rust std rlibs, and the final link pulls in FreeBSD system libraries
(`libexecinfo` for backtraces; `libkvm`/`libmemstat`/`libprocstat`/`libdevstat`
for `sysinfo`) plus FreeBSD's `crt1.o` — none of which a Linux `cc`/`ld` has.

One-time setup (needs `clang` and `lld` on the build machine):

```sh
rustup target add x86_64-unknown-freebsd
# Sysroot from the official base tarball — match the target's major version
mkdir -p ~/toolchains/freebsd15-sysroot
curl -fLO https://download.freebsd.org/releases/amd64/15.0-RELEASE/base.txz
tar -xf base.txz -C ~/toolchains/freebsd15-sysroot ./lib ./usr/lib ./usr/include ./usr/libdata
```

`.cargo/config.toml` points the `x86_64-unknown-freebsd` target at
`clang --sysroot=~/toolchains/freebsd15-sysroot -fuse-ld=lld`, and sets
`CC`/`CFLAGS` for that target so C dependencies (bundled SQLite) compile
against the FreeBSD headers. If the sysroot lives elsewhere, update the paths
there. Then:

```sh
cd web && npm run build && cd ..
cargo build --release --target x86_64-unknown-freebsd \
  -p solar-monitor --features solar-monitor-api/embed-frontend
```

The sysroot's major version must match the machine you deploy to (a 15.0
sysroot produces `for FreeBSD 15.0` binaries; check with `file`).

## Deploying

`just deploy` does all of the below (frontend build → embed cross-build → scp → restart → health check) and refuses to ship a binary without the embedded UI. The manual steps:

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

- The 6000XP's Modbus RTU interface is the **dongle port** (4-pin connector where the EG4 WiFi dongle plugs in; black/white wires are A/B) at **19200 baud 8N1, unit ID 1**. It speaks plain standard Modbus RTU (functions 0x03/0x04/0x06/0x10) over the LuxPower register map — the SN-framed little-endian dialect described in the official LuxPower PDF does not appear on the wire. See [jsharkey/lxp-esphome](https://github.com/jsharkey/lxp-esphome) for a community register reference.
- The **battery comms port** is a different bus: an EG4-LL/LifePower4 BMS answers there at 9600 baud, function 0x03 only, and truncates any response longer than ~12 bytes. Plugging the adapter into that port will never reach the inverter.
- A **defective USB-RS485 adapter** produces exactly the same symptom as wrong wiring or wrong protocol: total silence. A week was lost to one. If wiring, baud, and unit ID check out and the bus stays dead on two different computers, swap the adapter before doubting anything else.
- The service's per-port actor holds the serial device open. If the USB adapter is replugged, restart the service (`systemctl restart solar-monitor`) — the cached file descriptor points at the old device and every read fails silently.

## Web UI

- **Dashboard** (`/`) — live view: stat tiles for load/solar/battery/grid with sparklines, last-hour power chart, battery SOC chart, energy-today bars, power flow, temperatures. Prefills an hour of history, then streams over the WebSocket. A generator area appears automatically when the inverter's AC input type is set to Generator.
- **Devices** (`/devices`) — device list, add/remove, polling status.
- **Settings** (`/settings`) — inverter configuration read from and written to the holding registers (see below).

## Inverter Configuration

`/settings` exposes a curated set of EG4 6000XP settings (charge/discharge power and current limits, charge voltage, AC-charge enable/power/SOC limit/time windows, discharge cut-off SOC and voltage, generator charging, backup output voltage/frequency). Every value shown is read from the inverter; every write is range-checked against the official LuxPower hold-register limits, written with function 0x06, then read back — the UI shows what the inverter actually stored.

Generator charging (hold regs 194–198 plus the mode bit at reg 120 bit7) is configured independently of whether a generator is present: the start/end **SOC** pair applies when charge control is "By SOC", the start/end **voltage** pair when "By voltage" — the inverter ignores the inactive pair. The AC input *type* (grid vs generator, input reg 77 bit0) is reported on the dashboard but intentionally not writable here.

API:

```
GET /api/v1/devices/{id}/settings          # read all settings
PUT /api/v1/devices/{id}/settings/{key}    # body {"value": "..."} — number, "true"/"false", or "HH:MM-HH:MM"
```

The settings table lives in `protocols/src/eg4_settings.rs` — one table entry per setting (register, scale, documented range). Add new settings there; do not open raw register writes to the UI. The inverter standby/power-on bit is deliberately not exposed.

## Workspace Layout

| Crate | Purpose |
|---|---|
| `contracts` | Shared DTOs; exports TypeScript types via specta |
| `core` | Core engine types (scan config, versioning) |
| `protocols` | Protocol drivers: Modbus RTU transport + LuxPower-map models (EG4 6000XP) |
| `storage` | SQLite persistence (sqlx), migrations in `migrations/` |
| `api` | Axum router, WebSocket, static/embedded frontend serving |
| `bin` | `solar-monitor` binary: CLI, service install, composition |
| `web` | Preact + Vite + Tailwind frontend (not a cargo crate) |

## Linting

```sh
cargo fmt
cargo clippy --workspace --all-features
```
