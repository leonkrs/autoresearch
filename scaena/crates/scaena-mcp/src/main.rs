//! Scaena MCP server over stdio (newline-delimited JSON-RPC 2.0). Pure Rust, single binary, no Node.
//! Exposes Scaena's capabilities so an agent can list devices, capture screens, seed state, run flows,
//! and snapshot/restore, the same core the CLI and GUI use. Zero AI, zero network of its own.

use scaena_core::flow::{parse_flow, restore, run_flow, snapshot, snapshot_dirs};
use scaena_core::render::{export_store, frame_png, preset, scrub_orange_dot};
use scaena_core::tokens;
use scaena_core::{adb_path, all_devices, capture, capture_host, png_dimensions, Device};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

const PROTOCOL: &str = "2025-06-18";

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            Ok(_) => continue,
            Err(_) => break,
        };
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        // Notifications (no id) get no response.
        if id.is_none() {
            continue;
        }
        let result = handle(method, req.get("params"));
        let response = match result {
            Ok(r) => json!({"jsonrpc":"2.0","id":id,"result":r}),
            Err((code, msg)) => json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":msg}}),
        };
        let _ = writeln!(out, "{response}");
        let _ = out.flush();
    }
}

fn handle(method: &str, params: Option<&Value>) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "scaena", "version": env!("CARGO_PKG_VERSION")}
        })),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => call_tool(params),
        "ping" => Ok(json!({})),
        _ => Err((-32601, format!("method not found: {method}"))),
    }
}

fn tools() -> Value {
    let obj = |props: Value, req: Value| json!({"type":"object","properties":props,"required":req});
    json!([
        {"name":"devices","description":"List connected Android devices and iOS simulators.",
         "inputSchema": obj(json!({}), json!([]))},
        {"name":"capture","description":"Screenshot a device and return the PNG image.",
         "inputSchema": obj(json!({"device":{"type":"string","description":"serial/udid; default first ready"}}), json!([]))},
        {"name":"seed","description":"Write a host file into an app's private files/<rel> via run-as (no sign-in).",
         "inputSchema": obj(json!({"pkg":{"type":"string"},"rel":{"type":"string"},"src":{"type":"string"}}), json!(["pkg","rel","src"]))},
        {"name":"snapshot","description":"Save an app's private state to a host tar. full=true also captures shared_prefs+databases (the signed-in session) for session-replay.",
         "inputSchema": obj(json!({"pkg":{"type":"string"},"out":{"type":"string"},"full":{"type":"boolean"}}), json!(["pkg"]))},
        {"name":"restore","description":"Replay a snapshot tar into an app (no sign-in).",
         "inputSchema": obj(json!({"pkg":{"type":"string"},"tar":{"type":"string"}}), json!(["pkg","tar"]))},
        {"name":"flow","description":"Run a Scaena flow file (launch/seed/wait/capture). Returns captured PNG paths.",
         "inputSchema": obj(json!({"file":{"type":"string"},"out":{"type":"string"}}), json!(["file"]))},
        {"name":"capture_host","description":"Screenshot the macOS host screen (optional region x,y,w,h), orange-dot scrubbed.",
         "inputSchema": obj(json!({"region":{"type":"string","description":"x,y,w,h"},"out":{"type":"string"}}), json!([]))},
        {"name":"frame","description":"Wrap a screenshot PNG in a padded, rounded device frame. Returns the framed path.",
         "inputSchema": obj(json!({"src":{"type":"string"},"out":{"type":"string"},"pad":{"type":"integer"},"radius":{"type":"integer"}}), json!(["src"]))},
        {"name":"export","description":"Make a screenshot store-dimension compliant (play|appstore). Returns the path.",
         "inputSchema": obj(json!({"src":{"type":"string"},"store":{"type":"string"},"out":{"type":"string"}}), json!(["src"]))},
        {"name":"tokens","description":"Import brand color tokens from a Theme.kt or CSS file. Returns JSON.",
         "inputSchema": obj(json!({"file":{"type":"string"}}), json!(["file"]))},
        {"name":"mock","description":"Render a branded mock app screen (header + card titles) when the app can't run. Returns the PNG.",
         "inputSchema": obj(json!({"header":{"type":"string"},"titles":{"type":"array","items":{"type":"string"}},"out":{"type":"string"}}), json!(["titles"]))}
    ])
}

fn text(s: impl Into<String>) -> Value {
    json!({"content":[{"type":"text","text": s.into()}]})
}

fn call_tool(params: Option<&Value>) -> Result<Value, (i64, String)> {
    let p = params.ok_or((-32602, "missing params".into()))?;
    let name = p.get("name").and_then(Value::as_str).ok_or((-32602, "missing tool name".into()))?;
    let a = p.get("arguments").cloned().unwrap_or(json!({}));
    let adb = adb_path();
    let arg = |k: &str| a.get(k).and_then(Value::as_str).map(str::to_string);

    match name {
        "devices" => {
            let ds = all_devices();
            let list: Vec<Value> = ds.iter().map(|d| json!({
                "platform": d.platform.tag(), "serial": d.serial, "ready": d.is_ready(),
                "state": d.state, "name": d.name
            })).collect();
            Ok(text(serde_json::to_string_pretty(&list).unwrap_or_default()))
        }
        "capture" => {
            let device = pick(arg("device").as_deref()).ok_or((-32000, "no ready device".into()))?;
            let out = std::env::temp_dir().join(format!("scaena-{}.png", device.serial));
            let bytes = capture(&device, &adb, &out).map_err(|e| (-32000, e.to_string()))?;
            let (w, h) = png_dimensions(&bytes).unwrap_or((0, 0));
            Ok(json!({"content":[
                {"type":"text","text": format!("captured {w}x{h} from {} -> {}", device.serial, out.display())},
                {"type":"image","data": b64(&bytes), "mimeType":"image/png"}
            ]}))
        }
        "seed" => {
            let device = pick(None).ok_or((-32000, "no ready device".into()))?;
            let (pkg, rel, src) = (req_str(&a, "pkg")?, req_str(&a, "rel")?, req_str(&a, "src")?);
            scaena_core::flow::seed_app_file(&adb, &device.serial, &pkg, &rel, Path::new(&src))
                .map_err(|e| (-32000, e.to_string()))?;
            Ok(text(format!("seeded {rel} into {pkg}")))
        }
        "snapshot" => {
            let device = pick(None).ok_or((-32000, "no ready device".into()))?;
            let pkg = req_str(&a, "pkg")?;
            let out = arg("out").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(format!("{pkg}.snapshot.tar")));
            let full = a.get("full").and_then(Value::as_bool).unwrap_or(false);
            let n = if full {
                snapshot_dirs(&adb, &device.serial, &pkg, &out, &["files", "shared_prefs", "databases"])
            } else {
                snapshot(&adb, &device.serial, &pkg, &out)
            }
            .map_err(|e| (-32000, e.to_string()))?;
            Ok(text(format!("snapshot{} {} ({n} bytes)", if full { " --full" } else { "" }, out.display())))
        }
        "restore" => {
            let device = pick(None).ok_or((-32000, "no ready device".into()))?;
            let (pkg, tar) = (req_str(&a, "pkg")?, req_str(&a, "tar")?);
            restore(&adb, &device.serial, &pkg, Path::new(&tar)).map_err(|e| (-32000, e.to_string()))?;
            Ok(text(format!("restored {tar} into {pkg}")))
        }
        "flow" => {
            let device = pick(None).ok_or((-32000, "no ready device".into()))?;
            let file = req_str(&a, "file")?;
            let text_src = std::fs::read_to_string(&file).map_err(|e| (-32000, e.to_string()))?;
            let steps = parse_flow(&text_src).map_err(|e| (-32000, e))?;
            let base = Path::new(&file).parent().unwrap_or(Path::new("."));
            let out = arg("out").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("scaena-out"));
            let shots = run_flow(&adb, &device, &steps, base, &out).map_err(|e| (-32000, e.to_string()))?;
            let paths: Vec<String> = shots.iter().map(|p| p.display().to_string()).collect();
            Ok(text(format!("flow ok, {} capture(s): {}", paths.len(), paths.join(", "))))
        }
        "capture_host" => {
            let region = arg("region").and_then(|s| {
                let n: Vec<u32> = s.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                (n.len() == 4).then(|| (n[0], n[1], n[2], n[3]))
            });
            let out = arg("out").map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("scaena-host.png"));
            let raw = capture_host(region, &out).map_err(|e| (-32000, e.to_string()))?;
            let png = scrub_orange_dot(&raw, 60, [28, 28, 30]).map_err(|e| (-32000, e))?;
            std::fs::write(&out, &png).map_err(|e| (-32000, e.to_string()))?;
            let (w, h) = png_dimensions(&png).unwrap_or((0, 0));
            Ok(json!({"content":[
                {"type":"text","text": format!("host capture {w}x{h} (orange-dot scrubbed) -> {}", out.display())},
                {"type":"image","data": b64(&png), "mimeType":"image/png"}
            ]}))
        }
        "frame" => {
            let src = req_str(&a, "src")?;
            let pad = a.get("pad").and_then(Value::as_u64).unwrap_or(64) as u32;
            let radius = a.get("radius").and_then(Value::as_u64).unwrap_or(48) as u32;
            let out = arg("out").unwrap_or_else(|| format!("{src}.framed.png"));
            let bytes = std::fs::read(&src).map_err(|e| (-32000, e.to_string()))?;
            let png = frame_png(&bytes, pad, [14, 13, 16, 255], radius).map_err(|e| (-32000, e))?;
            std::fs::write(&out, &png).map_err(|e| (-32000, e.to_string()))?;
            let (w, h) = png_dimensions(&png).unwrap_or((0, 0));
            Ok(text(format!("framed -> {out} ({w}x{h})")))
        }
        "export" => {
            let src = req_str(&a, "src")?;
            let store = arg("store").unwrap_or_else(|| "play".into());
            let p = preset(&store).ok_or((-32602, format!("unknown store: {store}")))?;
            let out = arg("out").unwrap_or_else(|| format!("{src}.store.png"));
            let bytes = std::fs::read(&src).map_err(|e| (-32000, e.to_string()))?;
            let (png, w, h) = export_store(&bytes, &p).map_err(|e| (-32000, e))?;
            std::fs::write(&out, &png).map_err(|e| (-32000, e.to_string()))?;
            Ok(text(format!("exported {} ({w}x{h}) -> {out}", p.name)))
        }
        "tokens" => {
            let file = req_str(&a, "file")?;
            let text_src = std::fs::read_to_string(&file).map_err(|e| (-32000, e.to_string()))?;
            let toks = tokens::import(&text_src, &file);
            Ok(text(tokens::to_json(&toks)))
        }
        "mock" => {
            let header = arg("header").unwrap_or_else(|| "Today".into());
            let titles: Vec<String> = a.get("titles").and_then(Value::as_array)
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            if titles.is_empty() {
                return Err((-32602, "titles must be a non-empty array".into()));
            }
            let accents = [[235u8, 169, 72], [255, 90, 110], [111, 168, 220], [127, 200, 169], [139, 124, 246]];
            let cards: Vec<scaena_core::mock::Card> = titles.iter().enumerate()
                .map(|(i, t)| scaena_core::mock::Card { title: t.clone(), accent: accents[i % accents.len()] })
                .collect();
            let font = scaena_core::mock::load_font(None).map_err(|e| (-32000, e))?;
            let png = scaena_core::mock::render_mock(
                &header, &cards, [14, 13, 16], [25, 23, 28, 255], [245, 243, 239], [235, 169, 72], &font,
            ).map_err(|e| (-32000, e))?;
            let out = arg("out").map(PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("scaena-mock.png"));
            std::fs::write(&out, &png).map_err(|e| (-32000, e.to_string()))?;
            let (w, h) = png_dimensions(&png).unwrap_or((0, 0));
            Ok(json!({"content":[
                {"type":"text","text": format!("mock {w}x{h} ({} cards) -> {}", cards.len(), out.display())},
                {"type":"image","data": b64(&png), "mimeType":"image/png"}
            ]}))
        }
        other => Err((-32601, format!("unknown tool: {other}"))),
    }
}

fn req_str(a: &Value, k: &str) -> Result<String, (i64, String)> {
    a.get(k).and_then(Value::as_str).map(str::to_string).ok_or((-32602, format!("missing argument: {k}")))
}

fn pick(serial: Option<&str>) -> Option<Device> {
    let ds = all_devices();
    match serial {
        Some(s) => ds.into_iter().find(|d| d.serial == s),
        None => ds.into_iter().find(Device::is_ready),
    }
}

/// Standard base64 (no external crate) for returning image bytes inline.
fn b64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        s.push(T[(n >> 18 & 63) as usize] as char);
        s.push(T[(n >> 12 & 63) as usize] as char);
        s.push(if c.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        s.push(if c.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    s
}
