# Solar Monitor - Codebase Architecture

## Overview

A Rust workspace producing a single `solar-monitor` binary that polls EG4 solar inverters over RS485 serial (Modbus RTU, LuxPower register map), stores readings in SQLite, and serves a REST + WebSocket API with an embedded Preact web UI. It deploys to a Raspberry Pi sitting next to the inverter — see README.md for verified build, cross-compile, and deployment instructions.

```
Browser ↔ HTTP/WS (axum, port 8080) ↔ solar-monitor ↔ /dev/ttyUSB0 (RS485) ↔ EG4 inverter
                                          ↕
                                    SQLite (sqlx)
```

## Workspace Layout

| Crate | Package name | Purpose |
|---|---|---|
| `contracts/` | `contracts` | API DTOs (serde camelCase); exports TypeScript bindings via specta (`src/bin/export_types.rs` → `types/ts/index.ts`) |
| `core/` | `solar-monitor-core` | `DeviceProtocol`/`DeviceConnection`/`SettingsAccess` traits, `ProtocolRegistry`, `DeviceConfig`, `ScanConfig` |
| `protocols/` | `solar-monitor-protocols` | Layered drivers: `transport/modbus_rtu.rs` (per-port actor with `set_slave`), `luxpower/` (protocol family: connection, settings engine), `luxpower/models/` (per-model `ModelDef`: register decode + settings table, e.g. `eg4_6000xp.rs`) |
| `storage/` | `solar-monitor-storage` | `DataStore` over sqlx/SQLite; schema in `migrations/*.sql` |
| `notify/` | `solar-monitor-notify` | Notification engine: subscribes to the live-data broadcast, edge-triggered event detectors (grid lost/restored, battery low, device offline, generator) with hysteresis + per-transition cooldown; channel senders for ntfy/email/Pushover/webhook (rustls only — native-tls breaks cross-builds) |
| `api/` | `solar-monitor-api` | Axum `Router`, `AppState`, polling tasks, WebSocket broadcast, frontend serving |
| `bin/` | `solar-monitor` | CLI (clap): `--serve`, `--discover`, `--install`/`--uninstall` (systemd), state composition |
| `web/` | (npm, not cargo) | Preact + Vite + Tailwind UI; wouter routing; built output in `web/dist/` |

Dependency direction: `bin` → `api` → {`protocols`, `storage`, `notify`} → {`core`, `contracts`}.

## Key Mechanics

- **API surface**: all routes under `/api/v1/` (health, status, devices CRUD, discovery, test-params, data ranges, dashboard, device settings, `/api/v1/ws` WebSocket). Live `DeviceData` fans out through a `tokio::sync::broadcast` channel in `AppState`.
- **Inverter settings**: `GET/PUT /api/v1/devices/{id}/settings[/{key}]` is protocol-agnostic — the API resolves `DeviceProtocol::settings(&cfg)` to an optional `SettingsAccess` trait object (`None` ⇒ 400). The LuxPower implementation (`protocols/src/luxpower/settings.rs`) is generic over each model's curated table (`luxpower/models/*.rs`: register, scale, documented range per setting; reads sweep aligned 40-reg holding blocks covering every register the table references, last block trimmed). The 6000XP table is sourced from the official register map PDF ("6kXP-Modbus updated on 2023.10.28"; gitignored at `docs/6kXP-Modbus-2023.10.28.pdf` — vendor doc, not redistributed) — new settings must cite a documented register and range from it; registers with no documented range stay out. Writes are range-checked and read back from the inverter; bit settings (`Kind::Bit`/`Kind::BitChoice`, e.g. FuncEn reg 21, generator charge mode reg 120 bit7) use read-modify-write. Extend by adding table entries — never expose raw register writes (the old command endpoint was removed; settings are the only write surface).
- Generator charge settings (hold regs 194–198) are mode-dependent: SOC pair active when reg 120 bit7 = 1 ("By SOC"), voltage pair when 0 — the inverter ignores the inactive pair. Both are always shown/editable.
- **Polling**: `solar_monitor_api::start_polling` spawns a task per enabled device; task handles live in `AppState.tasks`. On startup, `bin/src/main.rs` auto-starts polling for persisted enabled devices; a device whose protocol isn't in the registry (e.g. an old `eg4-pi30-rs485` row) logs a warning, shows as Stopped, and can be edited/removed in the UI — it is never deleted automatically.
- **Serial access**: one actor per physical serial port keyed by path (multiple Modbus unit IDs share one RS485 bus; each request carries its unit id). Requesting a port that is already open at a different baud is a hard error, not a second actor. Default baud comes from the model (`ModelDef::default_baud`, 19200 for the 6000XP). Discovery iterates every registered protocol; the LuxPower sweep probes unit IDs 1–3 per baud.
- **Notifications**: channels and rules persist in SQLite (`002_notifications.sql`) and are edited on the Notifications page (`/api/v1/notifications/*`; config mutations call `Notifier::reload`). The engine baselines silently on the first sample per (rule, device) — no alert storm at startup — then fires only on state transitions; the offline detector runs on a 15 s ticker keyed off last data arrival. Channel `config` maps are kind-specific string maps (see `notify/src/channels.rs`); secrets live in the DB in plaintext, same trust model as the rest of the LAN-only app.
- **Frontend serving**: without features, `ServeDir` from `web/dist` (path resolved via `CARGO_MANIFEST_DIR`, works from any CWD in dev). With `--features solar-monitor-api/embed-frontend`, `rust_embed` compiles `web/dist` into the binary — this is how production builds ship. Build `web/` before the cargo build in that case.
- **Generated types**: never hand-edit `types/ts/index.ts`; run `cargo run -p contracts --bin export_types`. Contracts use camelCase serde renames — enum values like `DeviceType` are `"solarInverter"` etc.; the frontend must match exactly (they cross the wire).

## Gotchas (hard-won)

- The 6000XP's Modbus RTU interface is the **dongle port** (4-pin, black/white = A/B): 19200 baud 8N1, unit 1, **plain standard Modbus RTU** (the SN-framed LE dialect in the official PDF never appears on the wire), LuxPower register map. The **battery comms port** instead reaches an EG4-LL BMS: 9600 baud, FC 0x03 only, responses truncated at ~12 bytes — do not confuse the two buses.
- The PDF has **two register tables**: input (FC4, live data) and holding (FC3, config) — the same number means different things in each. `ACInputType` is *input* reg 77 bit0; holding 77 is a charge-priority hour. Generator input regs (121–126) hold junk unless the AC input type is Generator — gate on the bit, never on plausibility.
- The per-port actor caches its serial fd forever; after a USB replug the service must be restarted or every read fails silently. A **defective USB-RS485 adapter** is indistinguishable from wrong wiring — if two computers see a dead bus, swap the adapter.
- Time-window registers pack hour in the **low byte**, minute in the high byte. Input reg 5 packs SOC (low) | SOH (high).

- `serialport` is depended on with `default-features = false` in `api/` and `bin/` to avoid libudev, which breaks aarch64 cross-compilation. Do not re-enable default features. Port enumeration works via sysfs regardless.
- The production Pi host/user lives in the gitignored `.env` (`SOLAR_PI`); login is key-only and the username is *not* `pi` or `root`.
- The systemd unit uses `ProtectHome=true`; the binary must live outside `/home` (installed at `/usr/local/bin/solar-monitor`).
- Something else already listens on port 8080 on the dev machine — use another port (e.g. 8090) for local `--serve`.

## Development Commands

Common tasks are in the `justfile` (`just --list`): `just serve` (web build + local serve on 8090), `just test`, `just lint` (fmt + clippy, before every commit), `just types` (TS regen), `just build-pi` / `just deploy` (embed-frontend cross-build → Pi; `build-pi` fails if the frontend didn't get embedded), `just build-freebsd`, `just pi-logs` / `just pi-status`. Underlying commands:

```sh
cd web && npm run build            # required before serving locally
cargo run -p solar-monitor -- --serve --port 8090
cargo test --workspace
cargo fmt && cargo clippy --workspace --all-features   # before every commit
```

Cross-compile + deploy: `just deploy`, or see README.md ("Building for the Raspberry Pi", "Deploying"). Never ship a Pi/FreeBSD binary built without `--features solar-monitor-api/embed-frontend` — the UI 404s while the API works (this has bitten before; the justfile guards against it).

## Conventions

- Rust edition 2021 across crates; async via tokio throughout; errors via anyhow at binary level, `Result` with typed errors in libraries.
- DTOs live in `contracts` and are the single source of truth for the wire format; internal domain types live in `core`.
- New LuxPower-map EG4 models (18kPV, 12000XP, …) are a new `ModelDef` in `protocols/src/luxpower/models/` plus one `register_protocol` line in `protocols::create_registry()`. Other vendors implement `core::DeviceProtocol` (and return a `SettingsAccess` from `settings()` if configurable) and get registered the same way; the API layer and web UI are capability-driven and need no changes. Note: LuxPower models are wire-identical during discovery — once a second model exists, discovery needs a model-identifying holding register read.
- `docs/` holds only local reference material (the gitignored vendor register-map PDF). The original aspirational design specs were removed 2026-07-10 — they never matched the implementation; recover from git history if ever needed. README.md and this file are the documentation.
