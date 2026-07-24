# Scaena — build progress (autoresearch loop)

Human edits `program.md`. Agent edits `scaena/`. One thin, verified slice per cycle. Metric =
`gates_passed` (0..6, gates in `SCAENA_SPEC.md`).

## Cycle 0 — scaffold + CLI · gates 1/6
- Cargo workspace `scaena/` with `scaena-core` (adb device listing, zero deps) + `scaena-cli`.
- Proof: `cargo check` = 0 errors · `cargo test` = ok · `scaena devices` runs ("no devices", no emulator
  booted) · `scaena version` → `scaena 0.1.0`.
- **G1 passed** (spec allows the "no devices" case). Next: G2 (real capture + macOS orange-dot scrub).
