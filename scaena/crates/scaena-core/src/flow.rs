//! Declarative screen flows: launch an app, seed its private state, wait, capture. The differentiator
//! that reaches screens behind a wall WITHOUT authenticating — by writing app-private files via
//! `run-as` (Android). Zero AI, zero network. Android only for now (iOS uses container FS, later).

use crate::{capture, Device, Platform};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn adb_ok(adb: &str, serial: &str, args: &[&str]) -> io::Result<()> {
    let o = Command::new(adb).args(["-s", serial]).args(args).output()?;
    if o.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "adb {args:?} failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )))
    }
}

/// Seed a file into an app's private `files/<rel>` via `run-as` (never authenticates). Pushes the host
/// source to a world-readable tmp, then has the app uid copy it into its own sandbox.
pub fn seed_app_file(adb: &str, serial: &str, pkg: &str, rel: &str, host_src: &Path) -> io::Result<()> {
    if !host_src.exists() {
        return Err(io::Error::other(format!("seed source not found: {}", host_src.display())));
    }
    let tmp = "/data/local/tmp/scaena_seed";
    adb_ok(adb, serial, &["push", &host_src.to_string_lossy(), tmp])?;
    // Ensure the parent dir exists, then copy in. All DIRECT run-as commands (no `sh -c`, no `>`):
    // adb joins argv into one string, so a `>` redirection would run in the OUTER shell (cwd `/`), not
    // the app sandbox. `cp` needs no redirection, and run-as chdir's to the app data dir, so `files/`
    // resolves correctly. `rel` may nest.
    let dir = Path::new(rel).parent().and_then(|p| p.to_str()).filter(|s| !s.is_empty());
    let mkdir_arg = dir.map(|d| format!("files/{d}")).unwrap_or_else(|| "files".to_string());
    let _ = Command::new(adb)
        .args(["-s", serial, "shell", "run-as", pkg, "mkdir", "-p", &mkdir_arg])
        .output();
    let dest = format!("files/{rel}");
    adb_ok(adb, serial, &["shell", "run-as", pkg, "cp", tmp, &dest])
}

/// Snapshot an app's private `files/` dir to a host tar (via `run-as tar -c`, streamed through
/// `exec-out` so it is binary-safe with no shell redirection). This is the basis of session-replay:
/// a human signs in ONCE, we snapshot the resulting state, and `restore` replays it forever. Scaena
/// itself never authenticates. Returns the tar byte length.
pub fn snapshot(adb: &str, serial: &str, pkg: &str, out: &Path) -> io::Result<u64> {
    let output = Command::new(adb)
        .args(["-s", serial, "exec-out", "run-as", pkg, "tar", "-c", "files"])
        .output()?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err(io::Error::other(format!(
            "snapshot failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    std::fs::write(out, &output.stdout)?;
    Ok(output.stdout.len() as u64)
}

/// Restore a snapshot tar into an app's private dir (`run-as tar -x`, cwd = app data dir). Direct
/// command, no redirection. Replays previously-captured state (including a signed-in session) with no
/// authentication.
pub fn restore(adb: &str, serial: &str, pkg: &str, tar: &Path) -> io::Result<()> {
    if !tar.exists() {
        return Err(io::Error::other(format!("snapshot not found: {}", tar.display())));
    }
    let tmp = "/data/local/tmp/scaena_snap.tar";
    adb_ok(adb, serial, &["push", &tar.to_string_lossy(), tmp])?;
    adb_ok(adb, serial, &["shell", "run-as", pkg, "tar", "-x", "-f", tmp])
}

pub fn launch(adb: &str, serial: &str, pkg: &str, activity: &str) -> io::Result<()> {
    adb_ok(adb, serial, &["shell", "am", "start", "-n", &format!("{pkg}/{activity}")])
}

pub fn force_stop(adb: &str, serial: &str, pkg: &str) -> io::Result<()> {
    adb_ok(adb, serial, &["shell", "am", "force-stop", pkg])
}

/// One parsed flow step. Kept public + testable so the parser is verifiable without a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Launch { pkg: String, activity: String },
    Stop { pkg: String },
    Seed { rel: String, src: String },
    Wait { ms: u64 },
    Capture { name: String },
}

/// Parse a flow file (line DSL). `#` comments and blank lines ignored. Verbs:
/// `launch <pkg> <activity>` · `stop [pkg]` · `seed <rel> <hostfile>` · `wait <ms>` · `capture <name>`.
pub fn parse_flow(text: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let verb = it.next().unwrap();
        let a: Vec<&str> = it.collect();
        let err = |m: &str| format!("line {}: {m}", i + 1);
        let step = match verb {
            "launch" => Step::Launch {
                pkg: a.first().ok_or_else(|| err("launch <pkg> <activity>"))?.to_string(),
                activity: a.get(1).ok_or_else(|| err("launch <pkg> <activity>"))?.to_string(),
            },
            "stop" => Step::Stop { pkg: a.first().unwrap_or(&"").to_string() },
            "seed" => Step::Seed {
                rel: a.first().ok_or_else(|| err("seed <rel> <hostfile>"))?.to_string(),
                src: a.get(1).ok_or_else(|| err("seed <rel> <hostfile>"))?.to_string(),
            },
            "wait" => Step::Wait {
                ms: a.first().and_then(|s| s.parse().ok()).ok_or_else(|| err("wait <ms>"))?,
            },
            "capture" => Step::Capture {
                name: a.first().ok_or_else(|| err("capture <name>"))?.to_string(),
            },
            other => return Err(err(&format!("unknown verb '{other}'"))),
        };
        steps.push(step);
    }
    Ok(steps)
}

/// Run a parsed flow on an Android device. `base_dir` = flow file's dir (seed sources resolve against
/// it). Returns the captured PNG paths, written under `out_dir`.
pub fn run_flow(
    adb: &str,
    device: &Device,
    steps: &[Step],
    base_dir: &Path,
    out_dir: &Path,
) -> io::Result<Vec<PathBuf>> {
    if device.platform != Platform::Android {
        return Err(io::Error::other("flows currently support Android only"));
    }
    std::fs::create_dir_all(out_dir)?;
    let serial = &device.serial;
    let mut cur_pkg = String::new();
    let mut shots = Vec::new();
    for step in steps {
        match step {
            Step::Launch { pkg, activity } => {
                cur_pkg = pkg.clone();
                launch(adb, serial, pkg, activity)?;
            }
            Step::Stop { pkg } => {
                let p = if pkg.is_empty() { &cur_pkg } else { pkg };
                force_stop(adb, serial, p)?;
            }
            Step::Seed { rel, src } => {
                if cur_pkg.is_empty() {
                    return Err(io::Error::other("seed before launch: no package context"));
                }
                seed_app_file(adb, serial, &cur_pkg, rel, &base_dir.join(src))?;
            }
            Step::Wait { ms } => std::thread::sleep(Duration::from_millis(*ms)),
            Step::Capture { name } => {
                let out = out_dir.join(format!("{name}.png"));
                capture(device, adb, &out)?;
                shots.push(out);
            }
        }
    }
    Ok(shots)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_flow() {
        let text = "# demo\nlaunch com.x.y com.x.y.Main\nwait 3000\nseed data.json fixt.json\ncapture home\nstop\n";
        let steps = parse_flow(text).unwrap();
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0], Step::Launch { pkg: "com.x.y".into(), activity: "com.x.y.Main".into() });
        assert_eq!(steps[1], Step::Wait { ms: 3000 });
        assert_eq!(steps[2], Step::Seed { rel: "data.json".into(), src: "fixt.json".into() });
        assert_eq!(steps[3], Step::Capture { name: "home".into() });
        assert_eq!(steps[4], Step::Stop { pkg: "".into() });
    }

    #[test]
    fn rejects_unknown_verb() {
        assert!(parse_flow("frobnicate now").is_err());
        assert!(parse_flow("wait notanumber").is_err());
    }
}
