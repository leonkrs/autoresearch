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
