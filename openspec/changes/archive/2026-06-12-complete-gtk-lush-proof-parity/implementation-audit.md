## Baseline Audit

Captured before implementation on 2026-06-12.

## Python Live Runner Contract

Current live visual proof authority is `scripts/visual-geometry-smoke.py`.

- CLI: `--artifact-dir`, `--binary`, `--scenario-dir`, `--case-filter`; hidden
  internal modes are `--internal-run`, `--mutter-child`, and `--case-json`.
- Host probes require `dbus-run-session`, `gdbus`, `gsettings`,
  `gst-launch-1.0`, `mutter`, `pipewire`, `pw-dump`, `wireplumber`,
  `/usr/bin/python3`, and an executable LushText debug binary.
- Artifact reset refuses `/`, home, repo root, and the repo parent.
- Manifest loading reads `scripts/visual-geometry-scenarios/*.json`, requires
  `schema_version`, `scenario_id`, `scenario_type`, `matrix`, and
  `protected_regions`, rejects unsupported schema versions, and requires
  `invariant_id` when `pixel_anchors` are present.
- Matrix expansion currently supports `minimap-sidebar` and
  `command-palette-overlay` scenarios, including minimap size/color/wrap/
  direction/viewport/fixture axes and explicit exclusions.
- Outer execution writes per-case `case.json`, runs each case through an
  isolated `dbus-run-session`, aggregates per-case summaries, and writes root
  `summary.json`.
- Per-case execution creates isolated `data`, `config`, `cache`, and
  `XDG_RUNTIME_DIR` state, launches PipeWire and WirePlumber, applies
  GSettings, starts headless Mutter, launches LushText, waits on Automation1,
  and captures same-session before/after screenshots.
- Same-session capture waits for `file-open-complete`,
  `visual-geometry-settled`, final sidebar/editor/minimap geometry, and final
  rendered-anchor stability before accepting screenshots.
- State changes are driven through documented action paths from the
  GTK/D-Bus capture helper, not private widget mutation.
- Pixel proof uses `scripts/visual_geometry_png.py` for PNG read/write, crops,
  masks, exact protected-region comparisons, screenshot-derived anchors,
  native minimap edge/content detectors, and rendered-vs-app diagnostics.
- Animation proof supports stream capture through PipeWire/GStreamer and a
  screenshot fallback path. It records frame/sample timing, intermediate phase
  mapping, skew, anchor rows, row drift, failures, and final-settle status in
  `animation/animation-report.json`.
- Warning proof scans `lushtext.stderr`, `mutter-child.log`, `session.log`,
  `pipewire.log`, and `wireplumber.log` for unexpected GTK, GDK, GSK,
  Adwaita, Libadwaita, AT-SPI, accessibility, warning, critical, or error
  patterns.

## Artifact Inventory

Current artifact roots use `build/smoke/visual-geometry`.

- Root summary: `summary.json`.
- Root summary keys observed from current artifacts:
  `schema_version`, `status`, `case_filter`, `case_count`, `passed`,
  `failed`, `skipped`, `verified_invariant_ids`,
  `pixel_verified_invariant_ids`, `animation_verified_invariant_ids`,
  `pixel_anchor_assertion_count`, `animation_frame_sample_count`,
  `visual_proof_policy`, and `cases`.
- Current root summary sample status: `passed`, `case_count=40`,
  `passed=40`, `failed=0`, `skipped=0`,
  `verified_invariant_ids=native-minimap-highlight-anchors`,
  `pixel_verified_invariant_ids=native-minimap-highlight-anchors`,
  `animation_verified_invariant_ids=native-minimap-animation-highlight-anchors`.
- Per-case row keys observed from root summary: `case_id`, `status`,
  `failure_status`, `invariant_id`, `pixel_anchor_assertion_count`,
  `pixel_verified_invariant_ids`, `final_geometry`,
  `pixel_anchor_evidence`, `app_vs_rendered_disagreements`,
  `rendered_anchor_stability`, `animation_verified_invariant_ids`,
  `animation_frame_evidence`, `animation_frame_sample_count`,
  `artifact_dir`, and `manifest`.
- Per-case files include `case.json`, `scenario-manifest.json`,
  `summary.json`, `warning-scan.json`, `automation-waits.txt`,
  `before.png`, `after.png`, optional warmup screenshots,
  `before-geometry-snapshot.json`, `after-geometry-snapshot.json`,
  rendered-anchor stability JSON, final geometry samples, logs, runtime status
  files, fixture files, and `comparisons/comparison-report.json`.
- Comparison artifacts include protected-region before/after crops and
  pixel-anchor crops such as
  `comparisons/minimap-native-viewport-top-edge-before-anchor.png`.
- Animation artifacts live under `animation/` and include
  `animation-report.json`, `frames/`, `crops/`, and stream capture logs when
  produced.
- Skip summary shape writes root `summary.json` with `schema_version=1`,
  `status=skipped`, `skip_reason`, `case_count=0`, and `cases=[]`.

## Current `cargo gtk-proof` Behavior

Observed with `cargo run -q -p cargo-gtk-proof -- ...` before implementation.

- `--help` lists `run`, `schema`, `summarize`, `corpus`, and `policy`, plus
  default artifact and scenario roots.
- `schema list` returns an `ok=true`, `status=passed`, `command=schema`
  envelope listing `visual-scenario`, `expanded-case`, `root-summary`,
  `comparison-report`, `animation-report`, `proof-policy`, and
  `artifact-envelope`.
- `schema validate scripts/visual-geometry-scenarios/minimap-sidebar-live-threshold.json`
  returns `status=passed`, `document_kind=visual-scenario`, and
  `schema_version=1`.
- `corpus` returns `status=passed`, `compared=6`, `failed=0`, and reports the
  embedded PNG corpus.
- `policy --self-test` returns `status=passed`.
- `summarize build/smoke/visual-geometry` currently validates
  `build/smoke/visual-geometry/summary.json` and returns a schema-validation
  envelope whose command field is `schema`.
- `run` exits `3` with `status=unsupported-host`, `command=run`, and detail
  `live visual runner is not implemented in this slice`.

## Stable Wrapper And Documentation Surface

- `make visual-geometry-smoke` currently builds debug and invokes
  `./scripts/visual-geometry-smoke.py --artifact-dir "$(SMOKE_ARTIFACT_DIR)/visual-geometry" --binary "$(PWD)/target/debug/lushtext"`.
- `make check-visual-proof-policy` currently invokes
  `./scripts/check-visual-proof-policy.py --self-test` and then
  `./scripts/check-visual-proof-policy.py`.
- `make automation-client-self-test` invokes
  `./scripts/lushtext-automation.py self-test`.
- `scripts/check-end-user-smoke-workflow.py` requires the scheduled
  visual-geometry lane command `make visual-geometry-smoke SMOKE_ARTIFACT_DIR=build/smoke`
  and artifact path `build/smoke/visual-geometry`.
- `.github/workflows/end-user-smoke.yml` has a `visual-geometry` matrix lane
  with the same command and upload path.
- `docs/end-user-coverage.md`, `docs/automation.md`,
  `docs/automation-reference.md`, `docs/gtk-proof-schemas.md`, and
  `docs/next/gtk-lush.md` currently state that live visual execution remains
  Python until Rust runner parity is recorded.
- `scripts/lushtext-automation.py artifact-summary` reads both per-case
  `scenario-manifest.json` artifacts and root/generic `summary.json`
  artifacts. It preserves the client envelope and maps failed visual cases to
  statuses such as `visual-comparison-failed`, `pixel-anchor-failed`,
  `state-mismatch`, `warning-scan-failed`, `artifact-error`, and
  `artifact-skipped`.

## Phase Boundary Notes

This implementation is still Phase 4 proof parity completion.

- In scope: Rust live proof parity, proof-policy parity, wrapper migration,
  scheduled smoke compatibility, automation summary compatibility, bounded
  artifacts, docs, governance, and review evidence.
- Out of scope: publishing, second-consumer adoption, repository split,
  crates.io release, first `0.1.0` GTK Lush releases, and Phase 6 upstreaming.
- `cargo-gtk-proof` remains a workspace tool outside `crates/gtk-lush/`; it is
  not a family crate and must not introduce a GTK Lush framework abstraction.
