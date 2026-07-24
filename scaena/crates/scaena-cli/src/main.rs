//! Scaena CLI. Thin shell over scaena-core. Same capabilities the MCP server and GUI will expose.

use scaena_core::{adb_path, all_devices, capture, png_dimensions, Device, VERSION};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("devices") => cmd_devices(),
        Some("capture") => cmd_capture(&args[1..]),
        Some("version") | Some("--version") | Some("-V") => println!("scaena {VERSION}"),
        _ => {
            eprintln!("scaena {VERSION}");
            eprintln!("usage: scaena <command>");
            eprintln!("  devices                 list Android devices (adb) and iOS simulators (simctl)");
            eprintln!("  capture [--device S] [--out P]   screenshot a device to PNG");
            eprintln!("  version                 print version");
            std::process::exit(2);
        }
    }
}

fn cmd_devices() {
    let devices = all_devices();
    if devices.is_empty() {
        println!("no devices (boot an Android emulator or iOS simulator, then retry)");
        return;
    }
    for d in devices {
        let ready = if d.is_ready() { "ready" } else { d.state.as_str() };
        let name = d.name.as_deref().unwrap_or("-");
        println!("{}\t{}\t{}\t{}", d.platform.tag(), d.serial, ready, name);
    }
}

fn cmd_capture(args: &[String]) {
    let serial = flag(args, "--device");
    let out = flag(args, "--out").map(PathBuf::from);

    let devices = all_devices();
    let device: Option<Device> = match &serial {
        Some(s) => devices.into_iter().find(|d| &d.serial == s),
        None => devices.into_iter().find(Device::is_ready),
    };
    let Some(device) = device else {
        eprintln!("no ready device to capture (try: scaena devices)");
        std::process::exit(1);
    };

    let out = out.unwrap_or_else(|| PathBuf::from(format!("scaena-{}.png", device.serial)));
    match capture(&device, &adb_path(), &out) {
        Ok(bytes) => {
            let dims = png_dimensions(&bytes)
                .map(|(w, h)| format!("{w}x{h}"))
                .unwrap_or_else(|| "unknown size".into());
            println!("saved {} ({}, {} bytes)", out.display(), dims, bytes.len());
        }
        Err(e) => {
            eprintln!("capture failed: {e}");
            std::process::exit(1);
        }
    }
}

/// Value of `--name value` in args, if present.
fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}
