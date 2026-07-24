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
