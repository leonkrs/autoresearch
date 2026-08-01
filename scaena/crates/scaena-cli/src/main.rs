//! Scaena CLI. Thin shell over scaena-core. Same capabilities the MCP server and GUI will expose.

mod serve;

use scaena_core::flow::{parse_flow, restore, run_flow, snapshot};
use scaena_core::render::{export_store, frame_png, preset, scrub_orange_dot};
use scaena_core::{adb_path, all_devices, capture, capture_host, png_dimensions, Device, VERSION};
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
        Some("serve") => {
            let port: u16 = flag(&args, "--port").and_then(|s| s.parse().ok()).unwrap_or(7777);
            if args.iter().any(|a| a == "--open") {
                let url = format!("http://127.0.0.1:{port}");
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = std::process::Command::new("open").arg(&url).status();
                });
            }
            if let Err(e) = serve::run(port) { eprintln!("serve failed: {e}"); std::process::exit(1); }
        }
        Some("tokens") => cmd_tokens(&args[1..]),
        Some("contact") => cmd_contact(&args[1..]),
        Some("mock") => cmd_mock(&args[1..]),
        Some("doctor") => cmd_doctor(),
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
    let pos: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let Some(pkg) = pos.first() else {
        eprintln!("usage: scaena snapshot <pkg> [<out>] [--out F] [--full]"); std::process::exit(2);
    };
    let Some(device) = pick_device(flag(args, "--device").as_deref()) else {
        eprintln!("no ready device (try: scaena devices)"); std::process::exit(1);
    };
    // Out path: positional (like `restore`, matching the `snapshot <pkg> <out>` flow verb) or --out,
    // else default. Positional wins so the CLI and the flow DSL share one signature.
    let out = pos.get(1).map(|p| PathBuf::from(p.as_str()))
        .or_else(|| flag(args, "--out").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(format!("{pkg}.snapshot.tar")));
    let full = args.iter().any(|a| a == "--full");
    let res = if full {
        scaena_core::flow::snapshot_dirs(&adb_path(), &device.serial, pkg, &out, &["files", "shared_prefs", "databases"])
    } else {
        snapshot(&adb_path(), &device.serial, pkg, &out)
    };
    match res {
        Ok(n) => println!("snapshot{} {} ({} bytes) from {}", if full { " --full (session)" } else { "" }, out.display(), n, device.serial),
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
    let dry = args.iter().any(|a| a == "--dry");

    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => { eprintln!("cannot read flow {file}: {e}"); std::process::exit(1); }
    };
    let steps = match parse_flow(&text) {
        Ok(s) => s,
        Err(e) => { eprintln!("flow parse error: {e}"); std::process::exit(1); }
    };
    if dry {
        println!("flow {file}: {} step(s) (dry, no device)", steps.len());
        for (i, s) in steps.iter().enumerate() {
            println!("  {}. {s:?}", i + 1);
        }
        return;
    }
    let device = pick_device(serial.as_deref());
    let Some(device) = device else {
        eprintln!("no ready device (try: scaena devices)");
        std::process::exit(1);
    };
    let frame = args.iter().any(|a| a == "--frame");
    let base_dir = Path::new(file).parent().unwrap_or(Path::new("."));
    match run_flow(&adb_path(), &device, &steps, base_dir, &out_dir) {
        Ok(shots) => {
            println!("flow ok: {} capture(s) on {}:", shots.len(), device.serial);
            for p in &shots {
                // Optionally wrap each capture in a device frame, in place.
                if frame {
                    if let Ok(bytes) = std::fs::read(p) {
                        if let Ok(png) = frame_png(&bytes, 60, [14, 13, 16, 255], 44) {
                            let _ = std::fs::write(p, &png);
                        }
                    }
                }
                let dims = std::fs::read(p)
                    .ok()
                    .and_then(|b| png_dimensions(&b))
                    .map(|(w, h)| format!("{w}x{h}"))
                    .unwrap_or_else(|| "?".into());
                println!("  {} ({}{})", p.display(), dims, if frame { ", framed" } else { "" });
            }
        }
        Err(e) => { eprintln!("flow failed: {e}"); std::process::exit(1); }
    }
}

fn cmd_doctor() {
    fn check(label: &str, ok: bool, detail: &str) {
        println!("  {} {label}{}", if ok { "[ok]" } else { "[--]" }, if detail.is_empty() { String::new() } else { format!("  {detail}") });
    }
    println!("scaena doctor {VERSION}");

    let adb = adb_path();
    let adb_ver = std::process::Command::new(&adb).arg("version").output();
    let adb_ok = adb_ver.as_ref().map(|o| o.status.success()).unwrap_or(false);
    check("adb", adb_ok, &adb);

    let devices = all_devices();
    let ready = devices.iter().filter(|d| d.is_ready()).count();
    check("devices", !devices.is_empty(), &format!("{} total, {ready} ready", devices.len()));

    let simctl = std::process::Command::new("xcrun").args(["simctl", "help"]).output().map(|o| o.status.success()).unwrap_or(false);
    check("ios simctl", simctl, if simctl { "" } else { "no Xcode simulator runtime (iOS disabled)" });

    let font = scaena_core::mock::load_font(None).is_ok();
    check("mock font", font, if font { "system font found" } else { "no .ttf (mock text disabled)" });

    let path = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let on_path = path.split(':').any(|p| p == format!("{home}/.local/bin"));
    check("~/.local/bin on PATH", on_path, if on_path { "" } else { "add it to run `scaena` globally" });
}

fn cmd_mock(args: &[String]) {
    let titles = positionals(args);
    if titles.is_empty() {
        eprintln!("usage: scaena mock <card title...> [--header T] [--out F] [--font TTF]");
        std::process::exit(2);
    }
    let header = flag(args, "--header").unwrap_or_else(|| "Today".into());
    let out = flag(args, "--out").unwrap_or_else(|| "scaena-mock.png".into());
    let font = match scaena_core::mock::load_font(flag(args, "--font").as_deref()) {
        Ok(f) => f,
        Err(e) => { eprintln!("{e}"); std::process::exit(1); }
    };
    // Spocken-ish palette; accents cycle per card.
    let accents = [[235, 169, 72], [255, 90, 110], [111, 168, 220], [127, 200, 169], [139, 124, 246]];
    let cards: Vec<scaena_core::mock::Card> = titles
        .iter()
        .enumerate()
        .map(|(i, t)| scaena_core::mock::Card { title: (*t).clone(), accent: accents[i % accents.len()] })
        .collect();
    match scaena_core::mock::render_mock(
        &header, &cards, [14, 13, 16], [25, 23, 28, 255], [245, 243, 239], [235, 169, 72], &font,
    ) {
        Ok(png) => {
            let _ = std::fs::write(&out, &png);
            let (w, h) = png_dimensions(&png).unwrap_or((0, 0));
            println!("mock -> {out} ({w}x{h}, {} cards)", cards.len());
        }
        Err(e) => { eprintln!("mock failed: {e}"); std::process::exit(1); }
    }
}

fn cmd_contact(args: &[String]) {
    let srcs = positionals(args);
    if srcs.is_empty() {
        eprintln!("usage: scaena contact <png...> [--cols N] [--out F]"); std::process::exit(2);
    }
    let cols: u32 = flag(args, "--cols").and_then(|s| s.parse().ok()).unwrap_or(3);
    let out = flag(args, "--out").unwrap_or_else(|| "scaena-contact.png".into());
    let mut imgs = Vec::new();
    for s in &srcs {
        match std::fs::read(s) { Ok(b) => imgs.push(b), Err(e) => eprintln!("skip {s}: {e}") }
    }
    match scaena_core::render::contact_sheet(&imgs, cols, 300, 16, [14, 13, 16, 255]) {
        Ok(png) => {
            let _ = std::fs::write(&out, &png);
            let (w, h) = png_dimensions(&png).unwrap_or((0, 0));
            println!("contact sheet ({} shots) -> {out} ({w}x{h})", imgs.len());
        }
        Err(e) => { eprintln!("contact failed: {e}"); std::process::exit(1); }
    }
}

fn cmd_tokens(args: &[String]) {
    let Some(file) = positionals(args).first().copied() else {
        eprintln!("usage: scaena tokens <Theme.kt | styles.css> [--out json]"); std::process::exit(2);
    };
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => { eprintln!("read {file}: {e}"); std::process::exit(1); }
    };
    let toks = scaena_core::tokens::import(&text, file);
    let json = scaena_core::tokens::to_json(&toks);
    if let Some(out) = flag(args, "--out") {
        let _ = std::fs::write(&out, &json);
        println!("{} tokens -> {out}", toks.len());
    } else {
        println!("{json}");
    }
}

fn cmd_capture_host(args: &[String]) {
    let out = flag(args, "--out").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("scaena-host.png"));
    let region = flag(args, "--region").and_then(|s| {
        let n: Vec<u32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        (n.len() == 4).then(|| (n[0], n[1], n[2], n[3]))
    });
    let raw = match capture_host(region, &out) {
        Ok(b) => b,
        Err(e) => { eprintln!("host capture failed: {e}"); std::process::exit(1); }
    };
    // Scrub the macOS orange screen-recording dot from the top strip.
    match scrub_orange_dot(&raw, 60, [28, 28, 30]) {
        Ok(png) => {
            let _ = std::fs::write(&out, &png);
            let (w, h) = png_dimensions(&png).unwrap_or((0, 0));
            println!("host capture -> {} ({w}x{h}, orange-dot scrubbed)", out.display());
        }
        Err(e) => { eprintln!("scrub failed: {e}"); std::process::exit(1); }
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
    if args.iter().any(|a| a == "--host") {
        return cmd_capture_host(args);
    }
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

const VALUE_FLAGS: &[&str] =
    &["--out", "--store", "--pad", "--radius", "--device", "--header", "--font", "--cols", "--port"];

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
