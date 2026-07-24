//! Scaena core. Deterministic app-screen capture: devices, capture, seed, flows, framing, export.
//! Zero AI, zero keys, zero network. Everything here shells out to local tools (adb) or touches the
//! local filesystem only.

use std::process::Command;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// An Android device/emulator as reported by `adb devices`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub serial: String,
    /// e.g. "device", "offline", "unauthorized".
    pub state: String,
}

impl Device {
    pub fn is_ready(&self) -> bool {
        self.state == "device"
    }
}

/// Resolve the adb binary: `$ADB` if set, else the standard macOS SDK location, else bare `adb`.
pub fn adb_path() -> String {
    if let Ok(p) = std::env::var("ADB") {
        return p;
    }
    if let Ok(home) = std::env::var("HOME") {
        let sdk = format!("{home}/Library/Android/sdk/platform-tools/adb");
        if std::path::Path::new(&sdk).exists() {
            return sdk;
        }
    }
    "adb".to_string()
}

/// List connected Android devices via `adb devices`. Returns an empty vec if adb is missing or
/// unreachable — never panics, never touches the network.
pub fn adb_devices(adb: &str) -> Vec<Device> {
    let out = match Command::new(adb).arg("devices").output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .skip(1) // "List of devices attached"
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            match (it.next(), it.next()) {
                (Some(serial), Some(state)) => Some(Device {
                    serial: serial.to_string(),
                    state: state.to_string(),
                }),
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nothing_when_adb_missing() {
        // A definitely-absent binary yields an empty list rather than a panic.
        assert!(adb_devices("/nonexistent/adb/binary").is_empty());
    }

    #[test]
    fn device_readiness() {
        let d = Device { serial: "emulator-5554".into(), state: "device".into() };
        assert!(d.is_ready());
        let off = Device { serial: "x".into(), state: "offline".into() };
        assert!(!off.is_ready());
    }
}
