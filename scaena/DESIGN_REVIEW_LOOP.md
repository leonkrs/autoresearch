# Scaena design + review loop

A Ralph loop: each iteration redoes the browser-GUI design with the mandated design setup
(`frontend-design`) and runs a code review with the latest review tooling
(`pr-review-toolkit:code-reviewer` plus the em-dash, anti-AI-writing, clippy, and test gates), then
applies the fixes. The only visual surface is `scaena serve` (the embedded UI in
`crates/scaena-cli/src/serve.rs`); the CLI and MCP have no design surface. Runs until manual completion.

## Iteration 1 — direction: backstage / limelight

The prior GUI was the generic near-black + one-warm-accent look (`#0e0d10` bg, `#eba948` amber). With the
accent axis free, `frontend-design` calls that a default to leave behind. Scaena is the Latin word for the
stage of a Roman theatre, and the tool stages app screens and pulls back the curtain (the auth wall) via
replay. So the design is now a stage.

- **Palette**: velvet-tinted darks (`--pit #0b0b0f`, `--velvet #131019`), brass gilt (`--gild #c9a227`)
  for hairlines and the wordmark rather than big fills, a warm limelight (`--limelight #f6e7c1`), playbill
  paper text (`--chalk #ece7de`), cool lavender-grey labels (`--dim #8a8496`).
- **Type**: a Bodoni/Didot playbill serif for the SCAENA wordmark and the empty-state title (marquee
  letterspacing); a mono face for every control, so the booth reads like a technical cue sheet.
- **Layout**: a left control booth (the wings) and a stage house on the right.
- **Signature**: the limelight. A soft radial warm glow centres the stage, and a captured screen sits under
  it on a thin brass footlight. One memorable element; everything else stays quiet.
- **Copy**: empty state "The stage is empty. Pick a device and capture its screen." Buttons unchanged
  (active voice, already clear).
- **Motion**: a single fade-and-rise as the performer takes the stage; reduced-motion respected.

Verified live on `emulator-5554`: `GET / -> 200`; browser screenshots show the wordmark, booth, gilt
controls, and limelight stage; a real capture renders center-stage with the footlight and a `captured
HH:MM:SS` status. `showShot` builds the image with `createElement` + `replaceChildren`, never innerHTML,
so the blob URL is not parsed as markup. Gates: em-dash none, anti-AI-writing none, `clippy -D warnings`
clean, 3/3 cli tests.

**Review (pr-review-toolkit:code-reviewer)** confirmed the blob-image path is safe (createElement +
replaceChildren, never innerHTML) and all constraints held (no em-dash, offline, no new crates). It raised
one real regression and three minors, all fixed this iteration:

- **A11y (fixed)**: the redesign swapped the selects' `<label>` for `<div class="cue">`, which cannot name
  a control. Wired each `.cue` an `id` and pointed the select at it with `aria-labelledby`. Verified in the
  browser: `#dev` resolves to "Device", `#flow` to "Flow".
- **Blob-URL leak (fixed)**: `showShot` is now the single render path, so it revokes the previous `blob:`
  URL before swapping, instead of leaking one per capture.
- **Status not announced (fixed)**: `#status` gained `aria-live="polite"`.
- **Comment tone (fixed)**: trimmed the decorative `showShot` comment to match the file's sober style.

Design refinement this iteration: the two peer gilt buttons were visually heavy, so only the primary
"Capture screen" keeps the solid gilt fill; "Run flow" became a gilt outline (ghost). The solid accent is
concentrated and the limelight stays the one bold element. Verified on desktop (1280x720) and mobile
(375x812, booth stacks above the stage).

