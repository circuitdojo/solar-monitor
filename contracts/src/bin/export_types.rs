use specta::ts;
use std::fs;
use std::path::PathBuf;

use contracts::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("types")
        .join("ts");
    fs::create_dir_all(&out_dir)?;

    let mut conf = ts::ExportConfiguration::default();
    conf = conf.bigint(ts::BigIntExportBehavior::Number);

    let mut out = String::new();
    out.push_str(&ts::export::<DeviceType>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<HealthStatus>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<DeviceStatus>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<DeviceMetrics>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<DeviceData>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<DeviceConfigDto>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<AddDeviceRequestDto>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<DeviceListItemDto>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<TestConnectionParamsDto>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<TestConnectionResponseDto>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<ResourceUsageDto>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<StorageUsageDto>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<SystemStatusDto>(&conf)?);
    out.push_str("\n\n");
    out.push_str(&ts::export::<ErrorResponseDto>(&conf)?);

    let path = out_dir.join("index.ts");
    fs::write(&path, out)?;
    println!("Exported Specta TypeScript bindings to {}", path.display());
    Ok(())
}
