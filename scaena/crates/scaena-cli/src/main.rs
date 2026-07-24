//! Scaena CLI. Thin shell over scaena-core. Same capabilities the MCP server and GUI will expose.

use scaena_core::{all_devices, VERSION};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("devices") => cmd_devices(),
        Some("version") | Some("--version") | Some("-V") => println!("scaena {VERSION}"),
        _ => {
            eprintln!("scaena {VERSION}");
            eprintln!("usage: scaena <command>");
            eprintln!("  devices     list Android devices (adb) and iOS simulators (simctl)");
            eprintln!("  version     print version");
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
