//! Scaena CLI. Thin shell over scaena-core. Same capabilities the MCP server and GUI will expose.

use scaena_core::{adb_devices, adb_path, VERSION};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("devices") => cmd_devices(),
        Some("version") | Some("--version") | Some("-V") => println!("scaena {VERSION}"),
        _ => {
            eprintln!("scaena {VERSION}");
            eprintln!("usage: scaena <command>");
            eprintln!("  devices     list connected Android devices/emulators");
            eprintln!("  version     print version");
            std::process::exit(2);
        }
    }
}

fn cmd_devices() {
    let devices = adb_devices(&adb_path());
    if devices.is_empty() {
        println!("no devices (is an emulator booted? try: adb devices)");
        return;
    }
    for d in devices {
        let ready = if d.is_ready() { "ready" } else { &d.state };
        println!("{}\t{}", d.serial, ready);
    }
}
