//! Scaena CLI. Thin shell over scaena-core. Same capabilities the MCP server and GUI will expose.

use scaena_core::flow::{parse_flow, restore, run_flow, snapshot};
use scaena_core::render::{export_store, frame_png, preset};
use scaena_core::{adb_path, all_devices, capture, png_dimensions, Device, VERSION};
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("devices") => cmd_devices(),
        Some("capture") => cmd_capture(&args[1..]),
        Some("flow") => cmd_flow(&args[1..]),
        Some("snapshot") => cmd_snapshot(&args[1..]),
        Some("restore") => cmd_restore(&args[1..]),
        Some("frame") => cmd_frame(&args[1..]),
        Some("export") => cmd_export(&args[1..]),
        Some("version") | Some("--version") | Some("-V") => println!("scaena {VERSION}"),
        _ => {
            eprintln!("scaena {VERSION}");
            eprintln!("usage: scaena <command>");
            eprintln!("  devices                 list Android devices (adb) and iOS simulators (simctl)");
            eprintln!("  capture [--device S] [--out P]   screenshot a device to PNG");
            eprintln!("  flow <file> [--out DIR] [--device S]   run a screen flow (seed + capture)");
            eprintln!("  snapshot <pkg> [--out F] [--device S]  save an app's private state to a tar");
            eprintln!("  restore <pkg> <tar> [--device S]       replay a snapshot into the app");
            eprintln!("  version                 print version");
            std::process::exit(2);
        }
    }
}

fn cmd_snapshot(args: &[String]) {
    let Some(pkg) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: scaena snapshot <pkg> [--out F]"); std::process::exit(2);
    };
    let Some(device) = pick_device(flag(args, "--device").as_deref()) else {
        eprintln!("no ready device (try: scaena devices)"); std::process::exit(1);
    };
    let out = flag(args, "--out").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(format!("{pkg}.snapshot.tar")));
    match snapshot(&adb_path(), &device.serial, pkg, &out) {
        Ok(n) => println!("snapshot {} ({} bytes) from {}", out.display(), n, device.serial),
        Err(e) => { eprintln!("snapshot failed: {e}"); std::process::exit(1); }
    }
}

fn cmd_restore(args: &[String]) {
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let (Some(pkg), Some(tar)) = (pos.first(), pos.get(1)) else {
        eprintln!("usage: scaena restore <pkg> <tar>"); std::process::exit(2);
    };
    let Some(device) = pick_device(flag(args, "--device").as_deref()) else {
        eprintln!("no ready device (try: scaena devices)"); std::process::exit(1);
    };
    match restore(&adb_path(), &device.serial, pkg, Path::new(tar)) {
        Ok(()) => println!("restored {tar} into {pkg} on {}", device.serial),
        Err(e) => { eprintln!("restore failed: {e}"); std::process::exit(1); }
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

fn cmd_frame(args: &[String]) {
    let Some(src) = args.first().filter(|a| !a.starts_with("--")) else {
        eprintln!("usage: scaena frame <png> [--out F] [--pad N] [--radius N]"); std::process::exit(2);
    };
    let pad: u32 = flag(args, "--pad").and_then(|s| s.parse().ok()).unwrap_or(64);
    let radius: u32 = flag(args, "--radius").and_then(|s| s.parse().ok()).unwrap_or(48);
    let out = flag(args, "--out").unwrap_or_else(|| format!("{src}.framed.png"));
    let bytes = match std::fs::read(src) {
        Ok(b) => b,
        Err(e) => { eprintln!("read {src}: {e}"); std::process::exit(1); }
    };
    match frame_png(&bytes, pad, [14, 13, 16, 255], radius) {
        Ok(png) => {
            let _ = std::fs::write(&out, &png);
            let (w, h) = png_dimensions(&png).unwrap_or((0, 0));
            println!("framed -> {out} ({w}x{h})");
        }
        Err(e) => { eprintln!("frame failed: {e}"); std::process::exit(1); }
    }
}

fn cmd_export(args: &[String]) {
    let name = flag(args, "--store").unwrap_or_else(|| "play".into());
    let out_dir = PathBuf::from(flag(args, "--out").unwrap_or_else(|| "scaena-store".into()));
    let Some(p) = preset(&name) else {
        eprintln!("unknown store '{name}' (try: play, appstore)"); std::process::exit(2);
    };
    let srcs = positionals(args);
    if srcs.is_empty() {
        eprintln!("usage: scaena export <png...> [--store play|appstore] [--out DIR]"); std::process::exit(2);
    }
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("mkdir {}: {e}", out_dir.display()); std::process::exit(1);
    }
    let mut n = 0;
    for src in srcs {
        let bytes = match std::fs::read(src) { Ok(b) => b, Err(e) => { eprintln!("skip {src}: {e}"); continue; } };
        match export_store(&bytes, &p) {
            Ok((png, w, h)) => {
                let stem = Path::new(src).file_stem().and_then(|s| s.to_str()).unwrap_or("shot");
                let dest = out_dir.join(format!("{stem}.png"));
                let _ = std::fs::write(&dest, &png);
                println!("  {} -> {} ({w}x{h})", src, dest.display());
                n += 1;
            }
            Err(e) => eprintln!("  {src}: {e}"),
        }
    }
    println!("exported {n} screenshot(s) for {} into {}", p.name, out_dir.display());
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

const VALUE_FLAGS: &[&str] = &["--out", "--store", "--pad", "--radius", "--device"];

/// Positional args, excluding flags AND the values that follow value-taking flags. Without this, a
/// `--out DIR` value was mistaken for a positional input (it tried to read the output dir as a source).
fn positionals(args: &[String]) -> Vec<&String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a.starts_with("--") {
            if VALUE_FLAGS.contains(&a.as_str()) {
                i += 1; // skip its value
            }
        } else {
            out.push(a);
        }
        i += 1;
    }
    out
}
