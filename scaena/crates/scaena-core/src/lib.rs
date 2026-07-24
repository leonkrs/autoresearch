//! Scaena core. Deterministic app-screen capture: devices, capture, seed, flows, framing, export.
//! Zero AI, zero keys, zero network. Everything here shells out to local tools (adb, xcrun simctl) or
//! touches the local filesystem only. Two device backends behind one abstraction: Android + iOS sim.

use std::io;
use std::path::Path;
use std::process::Command;

pub mod flow;
pub mod render;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Android,
    Ios,
}

impl Platform {
    pub fn tag(&self) -> &'static str {
        match self {
            Platform::Android => "android",
            Platform::Ios => "ios",
        }
    }
}

/// A device or simulator, from either backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub serial: String,     // adb serial, or iOS simulator UDID
    pub state: String,      // raw backend state ("device"/"offline", or "Booted"/"Shutdown")
    pub platform: Platform,
    pub name: Option<String>, // human name (iOS gives one; adb does not)
}

impl Device {
    /// True when the device can actually be driven right now.
    pub fn is_ready(&self) -> bool {
        match self.platform {
            Platform::Android => self.state == "device",
            Platform::Ios => self.state.eq_ignore_ascii_case("booted"),
        }
    }
}

// ---------------- Android (adb) ----------------

/// Resolve the adb binary: `$ADB`, else the standard macOS SDK location, else bare `adb`.
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

/// Parse `adb devices` stdout into devices. Split out for testing without adb present.
pub fn parse_adb_devices(stdout: &str) -> Vec<Device> {
    stdout
        .lines()
        .skip(1) // "List of devices attached"
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            match (it.next(), it.next()) {
                (Some(serial), Some(state)) => Some(Device {
                    serial: serial.to_string(),
                    state: state.to_string(),
                    platform: Platform::Android,
                    name: None,
                }),
                _ => None,
            }
        })
        .collect()
}

/// List Android devices via `adb devices`. Empty vec if adb is missing. Never panics, never networks.
pub fn adb_devices(adb: &str) -> Vec<Device> {
    match Command::new(adb).arg("devices").output() {
        Ok(o) => parse_adb_devices(&String::from_utf8_lossy(&o.stdout)),
        Err(_) => Vec::new(),
    }
}

// ---------------- iOS (xcrun simctl) ----------------

/// Parse one `xcrun simctl list devices` line, e.g.
/// `    iPhone 15 Pro (A1B2C3D4-1111-2222-3333-444455556666) (Booted)`.
/// Returns None for headers, blank lines, or lines whose UDID does not look like a simulator UDID.
pub fn parse_simctl_line(line: &str) -> Option<Device> {
    let l = line.trim();
    if !l.ends_with(')') {
        return None;
    }
    let state_open = l.rfind('(')?;
    let state = l[state_open + 1..l.len() - 1].trim().to_string();
    let before = l[..state_open].trim_end();
    let udid_open = before.rfind('(')?;
    let udid = before[udid_open + 1..before.len() - 1].trim().to_string();
    // Simulator UDIDs are canonical 36-char UUIDs (8-4-4-4-12 with dashes).
    if udid.len() != 36 || udid.matches('-').count() != 4 {
        return None;
    }
    let name = before[..udid_open].trim().to_string();
    Some(Device {
        serial: udid,
        state,
        platform: Platform::Ios,
        name: if name.is_empty() { None } else { Some(name) },
    })
}

/// Parse full `xcrun simctl list devices` stdout.
pub fn parse_simctl_devices(stdout: &str) -> Vec<Device> {
    stdout.lines().filter_map(parse_simctl_line).collect()
}

/// List iOS simulators via `xcrun simctl list devices`. Empty vec if simctl / Xcode is missing.
pub fn simctl_devices() -> Vec<Device> {
    match Command::new("xcrun")
        .args(["simctl", "list", "devices"])
        .output()
    {
        Ok(o) if o.status.success() => parse_simctl_devices(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

/// Every device across both backends. Missing backends contribute nothing.
pub fn all_devices() -> Vec<Device> {
    let mut v = adb_devices(&adb_path());
    v.extend(simctl_devices());
    v
}

// ---------------- capture ----------------

/// Capture a device/simulator screen to `out` as PNG. Android via `adb exec-out screencap -p`
/// (binary-safe, no host shell redirection). iOS via `xcrun simctl io <udid> screenshot`.
/// Zero network. Returns the PNG bytes written, for immediate validation.
pub fn capture(device: &Device, adb: &str, out: &Path) -> io::Result<Vec<u8>> {
    match device.platform {
        Platform::Android => {
            let output = Command::new(adb)
                .args(["-s", &device.serial, "exec-out", "screencap", "-p"])
                .output()?;
            if !output.status.success() || output.stdout.is_empty() {
                return Err(io::Error::other("adb screencap produced no data"));
            }
            std::fs::write(out, &output.stdout)?;
            Ok(output.stdout)
        }
        Platform::Ios => {
            let status = Command::new("xcrun")
                .args(["simctl", "io", &device.serial, "screenshot", &out.to_string_lossy()])
                .status()?;
            if !status.success() {
                return Err(io::Error::other("simctl screenshot failed"));
            }
            std::fs::read(out)
        }
    }
}

/// Capture the macOS host screen (or a region) via `screencapture -x`. Used for desktop apps or the
/// emulator window; unlike device captures, the result may carry the orange screen-recording dot, so
/// callers should run [`render::scrub_orange_dot`] on it. Returns the raw PNG bytes.
pub fn capture_host(region: Option<(u32, u32, u32, u32)>, out: &Path) -> io::Result<Vec<u8>> {
    let mut cmd = Command::new("screencapture");
    cmd.arg("-x");
    if let Some((x, y, w, h)) = region {
        cmd.args(["-R", &format!("{x},{y},{w},{h}")]);
    }
    cmd.arg(out);
    if !cmd.status()?.success() {
        return Err(io::Error::other("screencapture failed"));
    }
    std::fs::read(out)
}

/// Read (width, height) from a PNG's IHDR without any image crate. None if the bytes are not a PNG.
/// Used to prove a capture is a real, non-empty image.
pub fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if bytes.len() < 24 || bytes[..8] != SIG || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    Some((w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_absent_yields_empty() {
        assert!(adb_devices("/nonexistent/adb").is_empty());
    }

    #[test]
    fn parses_adb_output() {
        let s = "List of devices attached\nemulator-5554\tdevice\nZY1234\toffline\n";
        let d = parse_adb_devices(s);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].serial, "emulator-5554");
        assert!(d[0].is_ready());
        assert_eq!(d[0].platform, Platform::Android);
        assert!(!d[1].is_ready());
    }

    #[test]
    fn parses_simctl_output() {
        let s = "== Devices ==\n\
                 -- iOS 17.5 --\n\
                 \x20\x20\x20\x20iPhone 15 (A1B2C3D4-1111-2222-3333-444455556666) (Booted)\n\
                 \x20\x20\x20\x20iPhone 15 Pro (B2C3D4E5-1111-2222-3333-444455556666) (Shutdown)\n\
                 -- tvOS 17.5 --\n\
                 \x20\x20\x20\x20Apple TV (C3D4E5F6-1111-2222-3333-444455556666) (Shutdown)\n";
        let d = parse_simctl_devices(s);
        assert_eq!(d.len(), 3, "three parenthesised device lines");
        assert_eq!(d[0].name.as_deref(), Some("iPhone 15"));
        assert_eq!(d[0].platform, Platform::Ios);
        assert!(d[0].is_ready(), "Booted sim is ready");
        assert!(!d[1].is_ready(), "Shutdown sim is not ready");
    }

    #[test]
    fn simctl_ignores_headers() {
        assert!(parse_simctl_line("== Devices ==").is_none());
        assert!(parse_simctl_line("-- iOS 17.5 --").is_none());
        assert!(parse_simctl_line("    Unavailable: com.apple.CoreSimulator").is_none());
    }

    #[test]
    fn png_dimensions_from_ihdr() {
        // Minimal PNG signature + IHDR declaring 1080x2400.
        let mut b = vec![137, 80, 78, 71, 13, 10, 26, 10];
        b.extend_from_slice(&[0, 0, 0, 13]); // IHDR length
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&1080u32.to_be_bytes());
        b.extend_from_slice(&2400u32.to_be_bytes());
        assert_eq!(png_dimensions(&b), Some((1080, 2400)));
        assert_eq!(png_dimensions(b"not a png"), None);
    }
}
