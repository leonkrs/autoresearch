# Scaena — autonomous build loop

Adaptation of Karpathy's `autoresearch` loop (see `README.md`) to **build a software tool** instead
of training a model. Same philosophy: the **human edits this `program.md`**; the **agent iterates on
the work** in tight, verifiable cycles. The metric is not `val_bpb` but **acceptance-gate progress**.
Verification is the bottleneck, so every cycle ends with **observed proof, never a claim**.

## The work

Build **Scaena** — a local, offline, no-login app-screen studio with **one Rust core** and **three
consumers** (CLI, MCP server for agents, Tauri GUI for humans). Its differentiator: **seed app state
deterministically to capture screens behind an auth wall, without ever authenticating**. Full target
and acceptance gates: `SCAENA_SPEC.md`. Source lives under `scaena/`.

## Setup (once per run)

1. Branch: `scaena` (you are on it). **Never commit to `master`** — that is Karpathy's nanochat.
2. Read `SCAENA_SPEC.md` (target + gates G1..G6) and this file.
3. Verify toolchain: `cargo --version`, `adb version`. Missing → tell the human and stop.
4. Ensure `results.tsv` exists with header `cycle\tts\tslice\tgate\tgates_passed\tkept\tproof`.
5. Scaffold the cargo workspace under `scaena/`. Confirm `cargo check` = 0 errors. That is **cycle 0**.

## The loop (repeat until all gates pass or a human decision is needed)

One cycle = one **thin vertical slice**:

1. **Pick** the smallest next slice that advances the lowest un-passed gate (G1 → G6, in order).
2. **Implement** it — a small, reviewable diff, only under `scaena/`.
3. **Run it and capture real proof**: `cargo check` / `cargo run` output, a real screenshot, an actual
   MCP tool result, a device capture on disk. **No proof = the slice is not done.**
4. **Measure**: which gate moved; compute `gates_passed` (0..6).
5. **Keep or revert**: verified and better → keep. Regressed or unprovable → `git checkout` the slice
   and retry smaller.
6. **Log**: append one row to `results.tsv` and one line to `PROGRESS.md`.
7. **Commit** the cycle: `scaena: cycle N — <slice> — gates X/6`. Repeat.

## Metric

`gates_passed` ∈ [0, 6] from `SCAENA_SPEC.md`. Monotonic target = 6. Every-cycle guardrails:
`cargo check` green and the binary runs.

## What you CAN do

- Edit anything under `scaena/`. Add crates the spec justifies (prefer std + thin, audited deps).

## What you CANNOT do (hard)

- Claim "done" without observed, cited proof (verify-before-report).
- **Authenticate / create accounts / type passwords.** Scaena's whole point is **seeding state**, not
  signing in. If a screen is behind auth, seed the state; never cross the wall yourself.
- Big-bang diffs. **One slice per cycle**, reviewable.
- Touch `master`, `train.py`, or `prepare.py` (Karpathy's nanochat — left untouched).
- Break the human's global rules: **English UI + docs**, **no em-dash anywhere**, **AI-optional core**
  (Scaena's engine runs with zero AI, zero keys, zero network), **install-in-place** (one canonical
  location, never duplicate), project reports **only via `/cairn`**.

## Stop and ask the human when

- A slice needs a **product decision** (the open questions in `SCAENA_SPEC.md`: iOS in v1?, MCP server
  in Rust `rmcp` vs a TS wrapper?, personal vs packaged-for-sale). Surface it; **do not guess**.
- A gate is blocked by something **only a human can do** (e.g. signing into a third-party account for a
  real end-to-end test). Note it in `PROGRESS.md` and move to the next unblocked gate.

## Definition of done

All 6 gates pass with cited proof in `results.tsv`, `cargo check` is green, and the built artifact
(CLI + MCP for early milestones, `Scaena.app` for the GUI milestone) actually runs.
