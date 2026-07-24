# Scaena — build spec (target of the autoresearch loop)

A local, offline, no-login **app-screen studio**: one Rust core, three consumers (CLI, MCP server for
agents, Tauri GUI for humans). It captures, seeds, frames and exports app screens reproducibly. Its
differentiator is **deterministic state seeding** so it can capture screens behind an auth wall
**without ever authenticating** (write app-private files via `run-as`, deep links, scripted input).

Full French brief: `~/Desktop/New Softwares/To be Developed/scaena.md`. This file is the English,
gate-driven build spec the loop works against.

## Architecture

```
                 scaena-core (Rust)  — devices, capture, seed, flows, framing, export
                 /        |        \
     scaena (CLI)   scaena-mcp (stdio)   Scaena.app (Tauri GUI)
```

Everything content-bearing runs with **zero AI, zero keys, zero network**. Any future AI feature is a
badged bonus with graceful degradation.

## Acceptance gates (the metric = how many pass, 0..6)

- **G1 — Core + CLI compile and run.** `cargo check` = 0 errors; `scaena devices` lists connected
  **Android devices (adb) AND iOS simulators (simctl)** behind one abstraction, or "no devices". Proof:
  command output.
- **G2 — Real capture + macOS orange-dot scrub.** `scaena capture` saves a real PNG of a booted
  Android emulator (`adb exec-out screencap`) and an iOS simulator (`xcrun simctl io … screenshot`);
  host-side captures get the orange screen-recording dot scrubbed. Proof: PNG on disk + dimensions.
- **G3 — State seeding + declarative flows (the differentiator).** `scaena flow run <flow.yaml>` drives
  an app to target screens via `seed_state` — Android (`run-as` app-private files / deep link / input)
  and iOS (app-container filesystem write / `simctl openurl`) — and captures each, **with no human
  authenticating**. Proof: the Spocken Auth + Home + Empty screens captured behind its mandatory
  account gate.
- **G4 — MCP server.** `scaena-mcp` speaks MCP over stdio, registers in Claude Code, and an agent call
  to `capture_screen` returns a real PNG from a running emulator. Proof: MCP tool-call result.
- **G5 — Framing + store export.** `scaena frame` wraps screens in device frames (light/dark, bg,
  caption); `scaena export --store play` emits a valid Play Store dimension pack. Proof: framed PNGs +
  a pack folder.
- **G6 — GUI + offline end-to-end.** `Scaena.app` (Tauri) launches (process visible) and drives the
  whole pipeline; the full G1..G5 flow still works with the network **off**. Proof: `open Scaena.app`
  process up + a network-off run.

## Milestones (map to gates)

- v0.1 → G1, G2 (core + CLI, devices, capture, scrub)
- v0.2 → G3 (seeding + flows — the differentiator; unblocks what stopped us on Spocken)
- v0.3 → G4 (MCP server, Claude Code registration)
- v0.4 → G5 (framing + store export + contact sheet)
- v0.5 → G6 (Tauri GUI + offline verification)
- v0.6 → mock-render fallback (port Mockbench's engine) + token import from `Theme.kt` / CSS

## Hard constraints

- English UI and docs. No em-dash anywhere.
- Core is 100% offline / no-AI / no-key. AI (if ever) is a badged, degrading bonus.
- Never authenticate, create accounts, or type passwords. Seed state instead.
- Install-in-place: one canonical location, never duplicate. Canonical app path TBD (see open Q3).
- Prefer std + thin, audited deps. Justify every dependency.

## Decisions (locked 2026-07-24)

1. **Name: Scaena** (final).
2. **Platform: Android + iOS in v1.** Two backends behind one device abstraction:
   - Android via `adb` (`run-as` for app-private files, `am`/`monkey` input, deep links).
   - iOS Simulator via `xcrun simctl` (screenshot, `openurl` deep links) plus **direct app-container
     filesystem access** under `~/Library/Developer/CoreSimulator/Devices/<UDID>/data/...` for seeding
     (no `run-as` equivalent needed; the sim's data lives on the host disk).
   Physical iOS devices are out of scope (simulator only), matching the simulator-only capability here.
3. **Install path:** app at `~/Desktop/mes applications/Scaena/`, source under
   `~/Desktop/New Softwares/autoresearch` (branch `scaena`). Install-in-place, never duplicated.
4. **MCP server: Rust `rmcp`, tried first.** Fall back to a TS wrapper only after an observed rmcp
   failure.
5. **Distribution: personal-first, kept sellable.** No store/licensing/onboarding scope gates v1;
   code stays clean and documented enough to package later.
