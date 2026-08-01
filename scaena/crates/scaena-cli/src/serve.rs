//! `scaena serve`: a local web UI, browser-based like Figma but localhost-only (it needs local device
//! access). Hand-rolled HTTP/1.1 over std TcpListener (no web-framework dep). Serves a small UI plus a
//! JSON/image API over scaena-core. Zero network of its own; adb talks to a local emulator socket.

use scaena_core::flow::{parse_flow, run_flow};
use scaena_core::render::frame_png;
use scaena_core::{adb_path, all_devices, capture, Device};
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

pub fn run(port: u16) -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!("scaena serve -> http://127.0.0.1:{port}  (Ctrl-C to stop)");
    for stream in listener.incoming().flatten() {
        std::thread::spawn(move || {
            let _ = handle(stream);
        });
    }
    Ok(())
}

fn handle(mut stream: TcpStream) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut req_line = String::new();
    if reader.read_line(&mut req_line)? == 0 {
        return Ok(());
    }
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    // Drain headers.
    loop {
        let mut h = String::new();
        let n = reader.read_line(&mut h)?;
        if n == 0 || h == "\r\n" || h == "\n" {
            break;
        }
    }
    let path = target.split('?').next().unwrap_or("/");
    match (method, path) {
        ("GET", "/") => respond(&mut stream, 200, "text/html; charset=utf-8", INDEX.as_bytes()),
        ("GET", "/api/devices") => respond(&mut stream, 200, "application/json", devices_json().as_bytes()),
        ("GET", "/api/capture") | ("POST", "/api/capture") => {
            let device = query(target, "device");
            let framed = query(target, "frame").as_deref() == Some("1");
            match do_capture(device.as_deref(), framed) {
                Ok(png) => respond(&mut stream, 200, "image/png", &png),
                Err(e) => respond(&mut stream, 500, "text/plain", e.as_bytes()),
            }
        }
        ("GET", "/api/mock") | ("POST", "/api/mock") => {
            let header = query(target, "header").unwrap_or_else(|| "Today".into());
            let titles = query(target, "titles").unwrap_or_default();
            match do_mock(&header, &titles) {
                Ok(png) => respond(&mut stream, 200, "image/png", &png),
                Err(e) => respond(&mut stream, 500, "text/plain", e.as_bytes()),
            }
        }
        ("GET", "/api/flows") => respond(&mut stream, 200, "application/json", flows_json().as_bytes()),
        ("GET", "/api/run-flow") | ("POST", "/api/run-flow") => {
            let file = query(target, "file").unwrap_or_default();
            let device = query(target, "device");
            match do_run_flow(&file, device.as_deref()) {
                Ok(png) => respond(&mut stream, 200, "image/png", &png),
                Err(e) => respond(&mut stream, 500, "text/plain", e.as_bytes()),
            }
        }
        _ => respond(&mut stream, 404, "text/plain", b"not found"),
    }
}

fn respond(stream: &mut TcpStream, code: u16, ctype: &str, body: &[u8]) -> io::Result<()> {
    let status = match code {
        200 => "200 OK",
        404 => "404 Not Found",
        _ => "500 Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn query(target: &str, key: &str) -> Option<String> {
    let q = target.split('?').nth(1)?;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

fn devices_json() -> String {
    let items: Vec<String> = all_devices()
        .iter()
        .map(|d| {
            format!(
                r#"{{"platform":"{}","serial":"{}","ready":{},"state":"{}","name":{}}}"#,
                d.platform.tag(),
                d.serial,
                d.is_ready(),
                d.state,
                d.name.as_ref().map(|n| format!("\"{n}\"")).unwrap_or_else(|| "null".into())
            )
        })
        .collect();
    format!("[{}]", items.join(","))
}

fn do_mock(header: &str, titles_pipe: &str) -> Result<Vec<u8>, String> {
    let titles: Vec<&str> = titles_pipe.split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
    if titles.is_empty() {
        return Err("no titles (pass titles=a|b|c)".into());
    }
    let accents = [[235u8, 169, 72], [255, 90, 110], [111, 168, 220], [127, 200, 169], [139, 124, 246]];
    let cards: Vec<scaena_core::mock::Card> = titles.iter().enumerate()
        .map(|(i, t)| scaena_core::mock::Card { title: (*t).to_string(), accent: accents[i % accents.len()] })
        .collect();
    let font = scaena_core::mock::load_font(None)?;
    scaena_core::mock::render_mock(header, &cards, [14, 13, 16], [25, 23, 28, 255], [245, 243, 239], [235, 169, 72], &font)
}

/// Where the GUI looks for flow files: `./flows` relative to where `scaena serve` was launched.
fn flows_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_default().join("flows")
}

/// List `*.flow` files in `flows_dir` with their parsed step count. Sorted by name for a stable UI.
fn flows_json() -> String {
    let mut items: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(flows_dir()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("flow") {
                continue;
            }
            let Some(name) = p.file_name().and_then(|x| x.to_str()) else { continue };
            let steps = std::fs::read_to_string(&p)
                .ok()
                .and_then(|t| parse_flow(&t).ok())
                .map(|s| s.len())
                .unwrap_or(0);
            items.push(format!(r#"{{"name":"{name}","steps":{steps}}}"#));
        }
    }
    items.sort();
    format!("[{}]", items.join(","))
}

/// Run a named flow from `flows_dir` on the chosen device and return its last capture as PNG bytes.
/// `file` must be a bare filename (no path separators, no `..`) so the GUI cannot read outside `flows/`.
fn do_run_flow(file: &str, serial: Option<&str>) -> Result<Vec<u8>, String> {
    if file.is_empty() || file.contains('/') || file.contains('\\') || file.contains("..") {
        return Err("bad flow name".into());
    }
    let dir = flows_dir();
    let path = dir.join(file);
    if !path.exists() {
        return Err(format!("flow not found: {file}"));
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let steps = parse_flow(&text)?;
    let devices = all_devices();
    let device = match serial {
        Some(s) => devices.into_iter().find(|d| d.serial == s),
        None => devices.into_iter().find(Device::is_ready),
    }
    .ok_or_else(|| "no ready device".to_string())?;
    let out_dir = std::env::temp_dir().join("scaena-serve-flow");
    let shots = run_flow(&adb_path(), &device, &steps, &dir, &out_dir).map_err(|e| e.to_string())?;
    let last = shots.last().ok_or_else(|| "flow ran but produced no capture step".to_string())?;
    std::fs::read(last).map_err(|e| e.to_string())
}

fn do_capture(serial: Option<&str>, framed: bool) -> Result<Vec<u8>, String> {
    let devices = all_devices();
    let device: Option<Device> = match serial {
        Some(s) => devices.into_iter().find(|d| d.serial == s),
        None => devices.into_iter().find(Device::is_ready),
    };
    let device = device.ok_or_else(|| "no ready device".to_string())?;
    let out = std::env::temp_dir().join(format!("scaena-serve-{}.png", device.serial));
    let bytes = capture(&device, &adb_path(), &out).map_err(|e| e.to_string())?;
    if framed {
        frame_png(&bytes, 60, [14, 13, 16, 255], 44)
    } else {
        Ok(bytes)
    }
}

const INDEX: &str = r#"<!doctype html><html><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>Scaena</title>
<style>
 *{box-sizing:border-box;margin:0;font-family:-apple-system,BlinkMacSystemFont,Inter,sans-serif}
 body{background:#0e0d10;color:#f5f3ef;display:flex;height:100vh}
 aside{width:280px;padding:20px;border-right:1px solid #2e2a32;background:#131117}
 h1{font-size:18px;color:#eba948;margin-bottom:2px}
 .sub{font-size:12px;color:#9a938c;margin-bottom:20px}
 label{font-size:11px;text-transform:uppercase;letter-spacing:.6px;color:#9a938c}
 select,button{width:100%;margin-top:8px;padding:10px;border-radius:10px;border:1px solid #2e2a32;background:#19171c;color:#f5f3ef;font-size:14px}
 button{background:#eba948;color:#211703;font-weight:600;border:0;cursor:pointer;margin-top:14px}
 button:hover{filter:brightness(1.05)}
 main{flex:1;display:flex;align-items:center;justify-content:center;padding:30px;overflow:auto}
 .frame{border-radius:36px;background:#000;padding:10px;box-shadow:0 30px 80px rgba(0,0,0,.6)}
 img{max-height:78vh;border-radius:28px;display:block}
 .empty{color:#9a938c;font-size:14px}
</style></head><body>
<aside>
 <h1>Scaena</h1><div class="sub">local app-screen studio</div>
 <label>Device</label>
 <select id="dev"></select>
 <label style="display:flex;align-items:center;gap:8px;margin-top:12px;text-transform:none;letter-spacing:0;font-size:13px;color:#f5f3ef"><input type="checkbox" id="frame" style="width:auto;margin:0"> Wrap in device frame</label>
 <button onclick="cap()">Capture screen</button>
 <label style="margin-top:22px;display:block">Flow</label>
 <select id="flow"></select>
 <button onclick="runFlow()">Run flow</button>
 <div class="sub" style="margin-top:6px">Flows drive the app and can restore a signed-in state, no login.</div>
 <div class="sub" id="status" style="margin-top:16px"></div>
</aside>
<main><div id="stage"><div class="empty">Pick a device and capture.</div></div></main>
<script>
async function load(){
 const r=await fetch('/api/devices');const ds=await r.json();
 const s=document.getElementById('dev');s.innerHTML='';
 if(!ds.length){s.innerHTML='<option>no devices</option>';}
 else ds.forEach(d=>{const o=document.createElement('option');o.value=d.serial;o.textContent=d.platform+' · '+d.serial+(d.ready?'':' ('+d.state+')');s.appendChild(o);});
 const fr=await fetch('/api/flows');const fs=await fr.json();
 const f=document.getElementById('flow');f.innerHTML='';
 if(!fs.length){f.innerHTML='<option value="">no flows</option>';}
 else fs.forEach(x=>{const o=document.createElement('option');o.value=x.name;o.textContent=x.name+' ('+x.steps+' steps)';f.appendChild(o);});
}
async function runFlow(){
 const dev=document.getElementById('dev').value;
 const file=document.getElementById('flow').value;
 if(!file){document.getElementById('status').textContent='no flow selected';return;}
 document.getElementById('status').textContent='running '+file+'…';
 const url='/api/run-flow?file='+encodeURIComponent(file)+'&device='+encodeURIComponent(dev)+'&t='+Date.now();
 const r=await fetch(url,{method:'POST'});
 if(!r.ok){document.getElementById('status').textContent='error: '+await r.text();return;}
 const blob=await r.blob();const u=URL.createObjectURL(blob);
 document.getElementById('stage').innerHTML='<div class="frame"><img src="'+u+'"></div>';
 document.getElementById('status').textContent='flow '+file+' done '+new Date().toLocaleTimeString();
}
async function cap(){
 const dev=document.getElementById('dev').value;
 document.getElementById('status').textContent='capturing…';
 const fr=document.getElementById('frame').checked?'1':'0';
 const url='/api/capture?device='+encodeURIComponent(dev)+'&frame='+fr+'&t='+Date.now();
 const r=await fetch(url);
 if(!r.ok){document.getElementById('status').textContent='error: '+await r.text();return;}
 const blob=await r.blob();const u=URL.createObjectURL(blob);
 document.getElementById('stage').innerHTML='<div class="frame"><img src="'+u+'"></div>';
 document.getElementById('status').textContent='captured '+new Date().toLocaleTimeString();
}
load();
</script></body></html>"#;
