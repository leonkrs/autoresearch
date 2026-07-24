//! Scaena CLI. Thin shell over scaena-core. Same capabilities the MCP server and GUI will expose.

use scaena_core::flow::{parse_flow, run_flow};
use scaena_core::{adb_path, all_devices, capture, png_dimensions, Device, VERSION};
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("devices") => cmd_devices(),
        Some("capture") => cmd_capture(&args[1..]),
        Some("flow") => cmd_flow(&args[1..]),
        Some("version") | Some("--version") | Some("-V") => println!("scaena {VERSION}"),
        _ => {
            eprintln!("scaena {VERSION}");
            eprintln!("usage: scaena <command>");
            eprintln!("  devices                 list Android devices (adb) and iOS simulators (simctl)");
            eprintln!("  capture [--device S] [--out P]   screenshot a device to PNG");
            eprintln!("  flow <file> [--out DIR] [--device S]   run a screen flow (seed + capture)");
            eprintln!("  version                 print version");
            std::process::exit(2);
        }
    }
}

fn cmd_flow(args: &[String]) {
    let Some(file) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: scaena flow <file> [--out DIR] [--device S]");
        std::process::exit(2);
    };
    let out_dir = flag(args, "--out").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("scaena-out"));
    let serial = flag(args, "--device");

    let device = pick_device(serial.as_deref());
    let Some(device) = device else {
        eprintln!("no ready device (try: scaena devices)");
        std::process::exit(1);
    };
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => { eprintln!("cannot read flow {file}: {e}"); std::process::exit(1); }
    };
    let steps = match parse_flow(&text) {
        Ok(s) => s,
        Err(e) => { eprintln!("flow parse error: {e}"); std::process::exit(1); }
    };
    let base_dir = Path::new(file).parent().unwrap_or(Path::new("."));
    match run_flow(&adb_path(), &device, &steps, base_dir, &out_dir) {
        Ok(shots) => {
            println!("flow ok — {} capture(s) on {}:", shots.len(), device.serial);
            for p in shots {
                let dims = std::fs::read(&p)
                    .ok()
                    .and_then(|b| png_dimensions(&b))
                    .map(|(w, h)| format!("{w}x{h}"))
                    .unwrap_or_else(|| "?".into());
                println!("  {} ({})", p.display(), dims);
            }
        }
        Err(e) => { eprintln!("flow failed: {e}"); std::process::exit(1); }
    }
}

fn pick_device(serial: Option<&str>) -> Option<Device> {
    let devices = all_devices();
    match serial {
        Some(s) => devices.into_iter().find(|d| d.serial == s),
        None => devices.into_iter().find(Device::is_ready),
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
