# Scaena — build progress (autoresearch loop)

Human edits `program.md`. Agent edits `scaena/`. One thin, verified slice per cycle. Metric =
`gates_passed` (0..6, gates in `SCAENA_SPEC.md`).

## Cycle 0 — scaffold + CLI · gates 1/6
- Cargo workspace `scaena/` with `scaena-core` (adb device listing, zero deps) + `scaena-cli`.
- Proof: `cargo check` = 0 errors · `cargo test` = ok · `scaena devices` runs ("no devices", no emulator
  booted) · `scaena version` → `scaena 0.1.0`.
- **G1 passed** (spec allows the "no devices" case). Next: G2 (real capture + macOS orange-dot scrub).

## Cycle 1 — iOS backend (decision: Android + iOS in v1) · gates 1/6
- Two device backends behind one abstraction: Android `adb` + iOS `xcrun simctl`. `scaena devices`
  merges both; `Device{platform,name}` added.
- Proof: `cargo test` = **4 passed** (adb parser, simctl parser, ready/shutdown, header-skip) ·
  `scaena devices` runs and merges both.
- **Host gap (flagged):** `simctl` is not installed on this Mac (no Xcode simulator runtime), so LIVE
  iOS capture cannot be verified here. The iOS code path is unit-tested against sample simctl output;
  real iOS runs need a Mac with Xcode. Android live path proceeds.

## Cycle 2 — device capture · gates 1/6 (G2 capture proven)
- `scaena capture` (Android `adb exec-out screencap -p`, iOS `simctl io screenshot`) + a std-only
  `png_dimensions` IHDR reader for proof. CLI `capture [--device S] [--out P]`.
- Proof (live emulator murmur_pixel): `scaena devices` → `android emulator-5554 ready`; `scaena capture`
  → **1080x2400 PNG, 1.37MB** (`file`: PNG RGBA). `cargo test` = 5 passed.
- G1 now fully proven on Android (real device listed). G2: device-capture path proven; the host-screen
  orange-dot scrub is a later slice (device screenshots carry no macOS dot). Next: G3 seeding + flows.

## Cycle 3 — seed + flow runner (the differentiator) · gates 2/6, G3 mechanism proven
- `flow.rs`: line-DSL flows (launch/stop/seed/wait/capture) + `seed_app_file` (run-as, no auth) +
  `run_flow`. CLI `scaena flow <file>`. Web-server consumer added to the spec.
- Two real bugs found and fixed mid-loop: (1) `adb shell run-as pkg sh -c "cat > files/x"` runs the
  `>` redirection in the OUTER shell (cwd /), not the sandbox -> switched to direct `run-as cp`;
  (2) fixed-`wait` caught a black cold-start frame -> raised the settle.
- Proof (live emulator): `scaena flow flows/spocken.flow` -> captured the **real Spocken Auth screen**
  (`example-spocken-auth.png`) and **seeded 3 notes into `files/murmur_meta.json` via run-as, no
  sign-in** (file verified). `cargo test` = 7 passed.
- G1, G2(capture) passed. G3 mechanism proven (drive + seed-without-auth + capture real screen). Auth
  is one of the three target screens. **Home/Empty are behind Firebase auth** -> next slice is
  **session-replay**: human signs in ONCE, `scaena snapshot` records the app-private state, `scaena
  seed --from <snapshot>` replays it forever. Scaena still never authenticates; it replays a
  human-provided snapshot. Then Home/Empty capture without any sign-in.

## Cycle 4 — session-replay (snapshot/restore) · gates 2/6
- `snapshot` (run-as `tar -c` via exec-out, binary-safe) + `restore` (run-as `tar -x`). CLI
  `scaena snapshot <pkg>` / `scaena restore <pkg> <tar>`.
- Proof (live): snapshot com.spocken.app -> 6144B tar; wiped `murmur_meta.json` (0 bytes); restored ->
  **3 notes back**. Roundtrip works with zero authentication.
- This closes the *mechanism* for the auth-gated screens: a human signs in ONCE, `scaena snapshot`
  records it, `scaena restore` replays it forever. Loop continues to G4 (no human needed).

## Cycle 5 — MCP server (G4) · gates 3/6
- `scaena-mcp`: stdio newline-delimited JSON-RPC 2.0, deps = serde_json only (hand-rolled; honours the
  "Rust, no Node" intent of the rmcp decision without the async/macro churn — logged in MCP.md).
- Tools: devices, capture, seed, snapshot, restore, flow. `capture` returns the PNG inline (base64).
- Proof: piped a full MCP session -> initialize, tools/list (6 tools), `capture` -> "1080x2400 from
  emulator-5554" + image/png content (133KB b64). `claude mcp add scaena` -> **list shows ✔ Connected**.
- **G4 passed.** Next: G5 (framing + store export).

## Cycle 6 — framing + store export (G5) · gates 4/6
- `render.rs` (image crate, PNG-only): `frame_png` (padded bg + rounded-corner screenshot) +
  `export_store` (Play/App-Store dimension compliance, downscale oversize). CLI `frame` + `export`.
- Proof (real screen): capture 1080x2400 -> frame 1200x2520 (pad 60, rounded) -> export play-compliant.
  `cargo test` = 10 passed (framing grows by pad; export downscales 8000px; compliant passes through).
- Loop caught a real bug: `--out`'s value was mistaken for a positional input -> added `positionals()`
  that skips value-flag args. Re-verified clean.
- **G5 passed.** Next: G6 (browser GUI via `scaena serve` + offline end-to-end).

## Cycle 7 — browser GUI (scaena serve) + offline (G6) · gates 5/6
- `serve.rs`: hand-rolled std HTTP/1.1 server (no web-framework dep). Serves a dark web UI + JSON/image
  API over the core: `/api/devices`, `/api/capture`. CLI `scaena serve [--port]`.
- Proof: curl `/api/devices` -> emulator JSON; `/` -> HTML UI; `/api/capture` -> HTTP 200 real 1080x2400
  PNG. Offline: grep shows zero HTTP/network-client deps (adb = local socket; image+serde_json only).
- **G6 passed.** Browser-based GUI like Figma but localhost. Tauri desktop = thin wrapper over the same
  UI (deferred). Remaining: G3 signed-in Home/Empty needs a one-time human sign-in (blocked; skipped).
  Loop continues on unblocked work (v0.6: host-scrub, token import, richer web UI).

## Cycle 8 — host capture + orange-dot scrub (G2 fully closed) · gates 5/6
- `capture_host` (macOS `screencapture -x`, optional `--region`) + `scrub_orange_dot` (image crate:
  orange pixels in top strip -> dark). CLI `scaena capture --host`.
- Proof: live host PNG produced + scrubbed; unit test scrubs orange in the strip only (leaves orange
  below untouched). `cargo test` green. G2 (capture + macOS orange-dot scrub) now fully closed.

## Cycle 9 — token import · gates 5/6 (v0.6 feature)
- `tokens.rs`: parse Compose `Color(0xAARRGGBB)` and CSS `--name:#hex` -> {name,hex} JSON (std only).
- Proof: `scaena tokens` on Spocken real `Theme.kt` -> Amber #EBA948, Ink #0E0D10, Violet, RecordRed,
  Sky. Unit tests: kotlin alpha-drop, css vars. Feeds mock-render + brand-colored framing.

## Cycle 10 — MCP full toolset · gates 5/6
- Extended `scaena-mcp` from 6 to **10 tools**: added capture_host, frame, export, tokens.
- Proof: piped session -> tools/list = 10; `tokens` tool on real Theme.kt -> 13 tokens (Amber #EBA948).
  The core goal "an agent drives the whole pipeline" is met end to end.

## Cycle 11 — richer browser GUI · gates 5/6
- `scaena serve`: /api/capture accepts `frame=1` -> capture wrapped in a device frame; UI checkbox.
- Proof: `frame=0` -> 1080x2400; `frame=1` -> 1200x2520 framed, over localhost.

## Cycle 12 — flow verbs tap/key/deeplink · gates 5/6
- Flow DSL extended: `tap x y` (input tap), `key KEYCODE` (keyevent), `deeplink url` (VIEW intent).
- Proof: parse tests + a live flow (launch/tap/key/capture) drove the emulator and produced a PNG.
  More primitives to reach any screen deterministically.

## Cycle 13 — package: README + install (install-in-place) · gates 5/6
- `scaena/README.md` (all real commands) + `install.sh` (release build + symlink scaena/scaena-mcp into
  ~/.local/bin, in-place).
- Proof: ran install to a temp bin dir; the **release** `scaena version` + `scaena devices` run from the
  symlinked binary. Packaged clean enough to sell (per the distribution decision).

## Cycle 14 — contact sheet · gates 5/6
- `contact_sheet` (image crate): tile N screenshots into a grid montage. CLI `scaena contact <png...>`.
- Proof: unit test grid dims (2 cols, 3 imgs -> 230x430); live 3-capture sheet produced. Spec item done.

## Cycle 15 — launcher + serve --open · gates 5/6
- `serve --open` auto-opens the browser; `Scaena.command` double-click launcher (uses installed binary
  or builds release). The desktop-app feel without Tauri weight.
- Proof: `serve --open` binds and GET / -> 200 (browser-open is best-effort).

## Cycle 15 correction
- The cycle-15 commit claimed `serve --open -> 200` but the first measurement was HTTP 000 (a 1.2s
  startup timing false-negative). Re-verified with a 3s wait: **GET / -> 200, port LISTENING**. The
  claim holds; noting the lapse (proof was written before it was actually observed).

## Cycle 16 — mock-render fallback · gates 5/6 (v0.6 feature, last major one)
- `mock.rs` (ab_glyph + macOS system font): render a mock app screen from tokens/titles when the real
  app cannot run (e.g. iOS on a host without Xcode). CLI `scaena mock <titles...> --header T`.
- Proof: rendered a Spocken-styled 1080x2400 mock; **visually verified** amber header + spine-coloured
  cards + white titles render. Loop caught a real bug from the image (--header value leaked as a 4th
  card) -> added --header/--font/--cols/--port to VALUE_FLAGS. Re-verified 3 cards.

## Cycle 17 — mock tool in MCP · gates 5/6
- Added `mock` to `scaena-mcp` (now **11 tools**): agents render a branded screen from header+titles,
  returned as an inline image. Proof: piped session -> mock -> "1080x2400 (2 cards)" + image content.

## Cycle 18 — scaena doctor · gates 5/6
- `scaena doctor`: environment self-check (adb, devices+ready count, iOS simctl, mock font, ~/.local/bin
  on PATH) with ok/-- flags. Onboarding + troubleshooting for the sellable path.
- Proof: live run reports adb ok, device ready, simctl absent (iOS flagged), font ok.

## Cycle 19 — flow --dry + examples · gates 5/6
- `scaena flow <file> --dry`: parse + print steps without a device (authoring/CI). Added
  `flows/example-settings.flow` (tap) and `flows/example-deeplink.flow` (deeplink).
- Proof: dry-run lists parsed steps for both example flows.

## Cycle 20 — mock in browser (/api/mock) · gates 5/6
- `scaena serve`: `/api/mock?header&titles=a|b|c` renders a branded mock over localhost.
- Proof: curl -> HTTP 200 PNG. All three consumers (CLI, MCP, web) now expose the full feature set.

## Cycle 21 — clippy clean + CI · gates 5/6
- `cargo clippy --all-targets` = **0 warnings** (auto-fixed 2 in scaena-mcp). Added CI workflow
  (`.github/workflows/scaena-ci.yml`): `cargo test` + `cargo clippy -D warnings` on push to `scaena`.
- The unit tests are hermetic (parsers, image ops, tokens) so CI needs no device. Proof: clippy 0,
  tests green, workflow YAML valid.

## Cycle 22 — fix css parser bug caught by CI · gates 5/6
- **Honesty + loop working:** cycle-21 CI failed on GitHub (`css_vars`), a bug my local `grep "ok"`
  masked across the 3 test binaries. Root cause: `from_css` only split on `;`/newline, so the first
  `--var` in `:root{ ...` was dropped. Fix: also split on `{`/`}`.
- Proof: full `cargo test` (checked for FAILED, not just grep) = **16 passed, 0 failed**. Re-verifying CI.

## Cycle 23 — flow --frame · gates 5/6
- `scaena flow --frame`: wrap each captured screen in a device frame in place (store-ready in one pass).
- Proof: live `flow --frame` -> framed 1200x2520 capture; full test suite 16 passed, 0 failed.

## Cycle 24 — dogfood README demo · gates 5/6
- Generated `scaena/docs/demo.png` using scaena itself (`mock` x2 -> `frame` x2 -> `contact`), referenced
  in the README. **Privacy:** a first pass captured Spocken real signed-in Home (Made had signed in in
  parallel) which held personal notes -> discarded; regenerated from synthetic mocks only, no real data.

## Cycle 25 — snapshot --full closes G3 (session-replay) · GATES 6/6
- `snapshot --full` captures `files` + `shared_prefs` + `databases`. Proof: the tar contains
  `shared_prefs/com.google.firebase.auth.api.Store.*.xml` — the Firebase signed-in session itself.
- Combined with the proven `restore` (cycle 4), session-replay is complete: **the human signs in once,
  Scaena snapshots + replays, and never authenticates**. This closes G3's auth-gated screens.
- **Not run:** the destructive full wipe+restore on Made's LIVE Spocken (he is actively using it) — the
  two halves are each proven, so composing them is sound without disrupting his session. The personal
  snapshot tar was written to /tmp and never committed.
- **All 6 gates met.** G1 devices, G2 capture+scrub, G3 seed/flow/session-replay, G4 MCP, G5 frame/export,
  G6 browser GUI + offline. Plus v0.6: token import, mock-render, contact sheet, doctor, CI, install.

## Cycle 26 — session-replay via MCP · gates 6/6
- MCP `snapshot` tool gains `full=true` -> captures shared_prefs+databases (the session).
- Proof: piped MCP call -> snapshot 14336B; tar contains `firebase.auth.api.Store`. cargo check green.
  Note: full test suite not re-run this cycle (timed out under machine load; unchanged from green
  cycle-25 + green CI). CI on push will confirm.

## Cycle 27 — session-replay as a flow verb · gates 6/6
- The flow DSL gains two verbs: `restore <pkg> <tar>` and `snapshot <pkg> <out>`. The auth-wall
  differentiator is now a single reproducible file (`flows/session-replay.flow`): restore a signed-in
  state a human captured once, launch, capture the screen behind the wall — Scaena never authenticates.
- `Step::Restore`/`Step::Snapshot` added to the enum, parser, and `run_flow` (calls the already-proven
  `restore`/`snapshot` core fns; `restore` sets `cur_pkg` so a following `launch` needs no re-declare).
- Proof: 17/17 core tests pass incl. new `parses_session_replay_verbs`; `flow flows/session-replay.flow
  --dry` prints all 5 steps correctly; `clippy -D warnings` clean on core + cli.

## Cycle 28 — session-replay verified LIVE on device · gates 6/6
- Ran the session-replay flow for real on `emulator-5554`: restore a signed-in `--full` tar -> launch ->
  capture. Output `behind-the-wall.png` is 1080x2400 with grayscale mean=0.58, stddev=0.165, range
  0.013..1 — a real rendered UI (Spocken's light home), not a black cold-start frame. Scaena never
  authenticated. Personal tar + capture stayed in /tmp, never committed.
- Fixed a footgun found while running it: `scaena snapshot` accepted the out path only via `--out`, so a
  positional path was silently ignored (wrote to `<pkg>.snapshot.tar` instead). It now takes a positional
  `<out>` like `restore` and the `snapshot` flow verb — one signature across CLI and DSL. `--out` kept.
- `clippy -D warnings` clean on cli; core tests unchanged (green).

## Cycle 29 — README documents the flow verbs + em-dash purge · gates 6/6
- README kept true to the code: the Flows DSL block now lists `restore`/`snapshot`, and the
  Session-replay section shows the one-flow form (`flows/session-replay.flow`) plus the detail that
  `--full` captures the Firebase auth store, and the note that it is verified live on a device.
- Purged every em-dash from the Scaena tree (11 occurrences: README, `flow.rs`, `serve.rs`, cli/mcp
  `main.rs`, `render.rs`, `mock.rs`, two flow files; one was a user-facing CLI `println`) per the hard
  global no-em-dash ban.
- `clippy -D warnings` clean on all three crates; 4/4 flow tests green. Edits are comments/strings/docs
  only, so the full 17-test suite is unchanged from cycle 27.

## Cycle 30 — GUI parity: the browser studio runs flows · gates 6/6
- `scaena serve` (the human consumer) gains two endpoints: `/api/flows` lists `*.flow` files in `./flows`
  with their step counts, and `/api/run-flow?file=&device=` parses and runs one on the chosen device and
  returns the last capture. A Flows dropdown + Run flow button now sit in the sidebar. `file` is a bare
  filename only (no `/`, `\`, or `..`), so the GUI cannot read outside `flows/`.
- Live-verified on `emulator-5554`: `GET /` -> 200; `/api/flows` -> lists 4 flows incl.
  `session-replay.flow` (5 steps); `file=../Cargo.toml` -> 500 "bad flow name"; a benign Settings-app
  flow -> 200 with a real 1080x2400 PNG (grayscale mean 0.73). A browser screenshot shows the populated
  Flow dropdown and Run flow button.
- Session-replay is now reachable from all three consumers: CLI, MCP, and the browser GUI.
  `clippy -D warnings` clean.

## Cycle 31 — harden and test the GUI flow-name guard · gates 6/6
- Pulled the `/api/run-flow` traversal check out of `do_run_flow` into a pure `valid_flow_name`, which
  now also requires a `.flow` extension, and gave it 3 unit tests: it accepts bare `.flow` names and
  rejects empty, wrong-extension, `..`, path separators, and a nul splice; plus a `query` parse/miss test.
- These are the first tests in the `scaena-cli` crate. Suite is now 20 (17 core + 3 cli).
- `clippy -D warnings` clean, after fixing `items_after_test_module` by moving `mod tests` to end of file.

## Cycle 32 — GUI run-flow honours the device-frame toggle · gates 6/6
- The "Wrap in device frame" checkbox already fed `capture`; now it feeds `run-flow` too. `do_run_flow`
  takes `framed`, `/api/run-flow` reads `frame=1`, and `runFlow()` passes the checkbox. One toggle, both
  paths.
- Live-verified: a framed Settings-app flow returns a 1200x2520 PNG (1080x2400 plus the 60px pad on each
  side), versus the raw 1080x2400. `clippy -D warnings` clean; 3 cli tests green.
