Contracts crate
===============

Purpose
- Source of truth for canonical Rust types (DeviceType, HealthStatus, DeviceStatus, DeviceMetrics, DeviceData, DeviceConfigDto).
- Generates matching TypeScript types for the frontend via Typeshare.

Generate TypeScript types
1) Ensure Rust toolchain is installed and Typeshare dependency resolves.
2) From repository root, run:

   cargo build --manifest-path contracts/Cargo.toml --release

This triggers contracts/build.rs, which writes TS types to ../types/ts/ relative to the contracts crate (i.e. types/ts at repo root).

Output
- types/ts/ (committable) contains .ts files for the exported types.

Integration notes
- You can point your frontend to import from types/ts/.
- If you later add DTOs, place them in contracts/src/lib.rs with #[typeshare] and rebuild.
