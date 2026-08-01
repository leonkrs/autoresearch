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
            let framed = query(target, "frame").as_deref() == Some("1");
            match do_run_flow(&file, device.as_deref(), framed) {
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

/// A flow name from the GUI is safe iff it is a non-empty bare filename ending in `.flow`, with no path
/// separators and no `..`. This keeps `/api/run-flow` from reading anything outside `flows/`. Pure so the
/// guard has a regression test.
fn valid_flow_name(file: &str) -> bool {
    !file.is_empty()
        && file.ends_with(".flow")
        && !file.contains('/')
        && !file.contains('\\')
        && !file.contains("..")
        && !file.contains('\0')
}

/// Run a named flow from `flows_dir` on the chosen device and return its last capture as PNG bytes.
/// `file` must pass `valid_flow_name` so the GUI cannot read outside `flows/`.
fn do_run_flow(file: &str, serial: Option<&str>, framed: bool) -> Result<Vec<u8>, String> {
    if !valid_flow_name(file) {
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
    if shots.is_empty() {
        return Err("flow ran but produced no capture step".into());
    }
    // A flow's value is often its before/after: with more than one capture, return a contact sheet of
    // them all. A single capture returns as-is (framed on request). frame_png takes one image, so it
    // does not apply to the multi-capture sheet.
    if shots.len() > 1 {
        let images: Vec<Vec<u8>> = shots
            .iter()
            .map(|p| std::fs::read(p).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        let cols = (shots.len() as u32).min(3);
        return scaena_core::render::contact_sheet(&images, cols, 360, 16, [14, 13, 16, 255]);
    }
    let bytes = std::fs::read(&shots[0]).map_err(|e| e.to_string())?;
    if framed {
        frame_png(&bytes, 60, [14, 13, 16, 255], 44)
    } else {
        Ok(bytes)
    }
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
 :root{--pit:#0b0b0f;--velvet:#131019;--sink:#0f0c15;--gild:#c9a227;--gildlit:#e6c454;--limelight:#f6e7c1;--chalk:#ece7de;--dim:#8a8496;--line:#2a2431}
 *{box-sizing:border-box;margin:0}
 html,body{height:100%}
 body{background:var(--pit);color:var(--chalk);display:flex;font-family:ui-monospace,"SF Mono",Menlo,Consolas,monospace;font-size:13px}
 .booth{width:300px;flex:none;display:flex;flex-direction:column;padding:26px 22px;background:linear-gradient(180deg,var(--velvet),var(--pit));border-right:1px solid var(--line)}
 .mark{font-family:"Bodoni 72","Didot",Georgia,serif;font-size:31px;font-weight:600;letter-spacing:.22em;color:var(--chalk);padding-left:.22em}
 .tag{font-size:10px;letter-spacing:.16em;text-transform:uppercase;color:var(--dim);margin-top:7px}
 .rule{height:1px;background:linear-gradient(90deg,var(--gild),transparent);opacity:.55;margin:22px 0 16px}
 .cue{font-size:10px;letter-spacing:.2em;text-transform:uppercase;color:var(--gild);margin-bottom:9px}
 select{width:100%;font:inherit;font-size:13px;padding:10px 12px;border-radius:8px;border:1px solid var(--line);background:var(--sink);color:var(--chalk)}
 .check{display:flex;align-items:center;gap:9px;margin-top:11px;font-size:12px;color:var(--chalk);cursor:pointer}
 .check input{accent-color:var(--gild);width:15px;height:15px;margin:0}
 .btn{width:100%;margin-top:13px;font:inherit;font-size:13px;font-weight:600;letter-spacing:.02em;padding:11px 12px;border:0;border-radius:8px;background:var(--gild);color:#20180a;cursor:pointer;transition:filter .15s}
 .btn:hover{filter:brightness(1.08)}
 .btn.ghost{background:transparent;color:var(--gild);border:1px solid var(--gild)}
 .btn.ghost:hover{filter:none;background:rgba(201,162,39,.12)}
 .btn:focus-visible,select:focus-visible,.check input:focus-visible{outline:2px solid var(--gildlit);outline-offset:2px}
 .hint{font-size:11px;line-height:1.55;color:var(--dim);margin-top:9px}
 .status{font-size:11px;line-height:1.5;color:var(--limelight);margin-top:auto;padding-top:18px;min-height:16px}
 .house{flex:1;position:relative;display:flex;align-items:center;justify-content:center;padding:44px;overflow:auto}
 .house::before{content:"";position:absolute;inset:0;background:radial-gradient(58% 52% at 50% 40%,rgba(246,231,193,.10),transparent 72%);pointer-events:none}
 .perform{position:relative;z-index:1;display:flex;flex-direction:column;align-items:center}
 .perform img{max-height:76vh;max-width:100%;border-radius:10px;display:block;box-shadow:0 26px 74px rgba(0,0,0,.62);animation:rise .5s ease}
 .foot{width:62%;max-width:340px;height:2px;margin-top:15px;background:linear-gradient(90deg,transparent,var(--gild),transparent);opacity:.75}
 .empty{max-width:300px;text-align:center;color:var(--dim);font-size:12px;line-height:1.65}
 .empty b{display:block;font-family:"Bodoni 72","Didot",Georgia,serif;font-weight:500;font-size:21px;letter-spacing:.03em;color:var(--chalk);margin-bottom:9px}
 @keyframes rise{from{opacity:0;transform:translateY(10px)}to{opacity:1;transform:none}}
 @media(prefers-reduced-motion:reduce){.perform img{animation:none}}
 @media(max-width:720px){body{flex-direction:column}.booth{width:100%;border-right:0;border-bottom:1px solid var(--line)}.house{padding:26px}}
</style></head><body>
<div class="booth">
 <div class="mark">SCAENA</div><div class="tag">local app-screen studio</div>
 <div class="rule"></div>
 <div class="cue" id="cue-dev">Device</div>
 <select id="dev" aria-labelledby="cue-dev"></select>
 <label class="check"><input type="checkbox" id="frame"> Wrap in device frame</label>
 <button class="btn" onclick="cap()">Capture screen</button>
 <div class="rule"></div>
 <div class="cue" id="cue-flow">Flow</div>
 <select id="flow" aria-labelledby="cue-flow"></select>
 <button class="btn ghost" onclick="runFlow()">Run flow</button>
 <div class="hint">Flows drive the app and can restore a signed-in state, no login.</div>
 <div class="status" id="status" aria-live="polite"></div>
</div>
<div class="house"><div class="perform" id="stage"><div class="empty"><b>The stage is empty.</b>Pick a device and capture its screen.</div></div></div>
<script>
// Render a captured screen on the stage. Built with DOM nodes (not innerHTML) so the blob URL is never
// parsed as markup; the previous blob is revoked first so object URLs do not pile up over a session.
function showShot(u){
 const stage=document.getElementById('stage');
 const prev=stage.querySelector('img');
 if(prev&&prev.src.startsWith('blob:'))URL.revokeObjectURL(prev.src);
 stage.replaceChildren();
 const img=document.createElement('img');img.src=u;img.alt='captured screen';
 const foot=document.createElement('div');foot.className='foot';
 stage.append(img,foot);
}
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
 const fr=document.getElementById('frame').checked?'1':'0';
 const url='/api/run-flow?file='+encodeURIComponent(file)+'&device='+encodeURIComponent(dev)+'&frame='+fr+'&t='+Date.now();
 const r=await fetch(url,{method:'POST'});
 if(!r.ok){document.getElementById('status').textContent='error: '+await r.text();return;}
 const blob=await r.blob();const u=URL.createObjectURL(blob);
 showShot(u);
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
 showShot(u);
 document.getElementById('status').textContent='captured '+new Date().toLocaleTimeString();
}
load();
</script></body></html>"#;


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_flow_name_accepts_bare_flow_files() {
        assert!(valid_flow_name("session-replay.flow"));
        assert!(valid_flow_name("spocken.flow"));
        assert!(valid_flow_name("_tmp-verify.flow"));
    }

    #[test]
    fn valid_flow_name_rejects_traversal_and_junk() {
        assert!(!valid_flow_name(""));
        assert!(!valid_flow_name("Cargo.toml"));
        assert!(!valid_flow_name("../Cargo.toml"));
        assert!(!valid_flow_name("../../etc/passwd.flow"));
        assert!(!valid_flow_name("sub/dir.flow"));
        assert!(!valid_flow_name("a\\b.flow"));
        assert!(!valid_flow_name("evil.flow\0.png"));
    }

    #[test]
    fn query_parses_and_misses() {
        let t = "/api/run-flow?file=session-replay.flow&device=emulator-5554";
        assert_eq!(query(t, "file").as_deref(), Some("session-replay.flow"));
        assert_eq!(query(t, "device").as_deref(), Some("emulator-5554"));
        assert_eq!(query(t, "missing"), None);
        assert_eq!(query("/api/flows", "file"), None);
    }
}
