fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Specta export is done via a helper bin: `cargo run -p contracts --bin export_types`.
    // Keep build script as a no-op to avoid cyclical deps while tracking changes.
    println!("cargo:rerun-if-changed=src/lib.rs");
    Ok(())
}
