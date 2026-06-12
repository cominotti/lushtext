# GTK Proof Schemas

`cargo-gtk-proof` owns the versioned Rust-side proof schema descriptors for
LushText's extracted GTK proof toolchain. The current supported
`schema_version` is `1`.

The machine-readable descriptors live in
`crates/cargo-gtk-proof/schemas/*.schema.json`. They are intentionally compact:
they name the schema ID, supported version, required fields, and optional fields
enforced by the Rust validator. During proof parity work, Rust accepts a few
legacy Python artifact shapes that were historically unversioned, but every new
Rust-owned output should carry `schema_version: 1`.

## Schema IDs

| Schema ID | Typical path | Purpose |
| --- | --- | --- |
| `visual-scenario` | `scripts/visual-geometry-scenarios/*.json` | Source manifest for a visual invariant matrix. |
| `expanded-case` | per-case manifest artifact | Concrete case produced from one scenario matrix entry. |
| `case-manifest` | per-case `scenario-manifest.json` | Runtime manifest for one expanded visual case, including source manifest, case settings, same-session metadata, screenshots, geometry snapshots, and warning paths. |
| `root-summary` | `build/smoke/visual-geometry/summary.json` | Root visual proof summary consumed by policy and artifact-summary tooling. |
| `case-summary` | per-case `summary.json` | Per-case status, invariant, geometry, comparison, warning, and animation summary data. |
| `comparison-report` | per-case comparison artifact | Protected-region, rendered-anchor, mask, and app-vs-rendered diagnostics. |
| `animation-report` | per-case animation artifact | Timestamp-correlated frame/sample evidence for animation-sensitive invariants. |
| `warning-scan` | per-case `warning-scan.json` | Bounded unexpected-warning scan result for toolkit, renderer, capture, app, and proof-tool logs. |
| `parity-report` | parity run artifact | Python-oracle versus Rust result comparison used before wrapper migration. |
| `environment-report` | run-level or case-level environment artifact | Host capability, isolated runtime, and unsupported-host diagnostics. |
| `proof-policy` | embedded in summaries or policy metadata | Changed-file fingerprint and required invariant metadata. |
| `artifact-envelope` | stdout from `cargo gtk-proof` commands | Stable command result envelope. |

## Required Fields

`visual-scenario` requires `schema_version`, `scenario_id`, `scenario_type`,
`matrix`, and `protected_regions`. The Rust validator also requires non-empty
matrix sizes and color schemes, non-empty protected regions, and an
`invariant_id` whenever pixel anchors are declared.

`expanded-case` requires `schema_version`, `case_id`, `manifest`, `size`,
`color_scheme`, and `artifact_dir`. Rust-produced cases may include the
per-case `gsettings` plan that mirrors the Python runner's setup values.

`case-manifest` requires `schema_version`, `scenario_id`, and `scenario_type`.
It may include `source_manifest`, the expanded `case`, same-session metadata,
the per-case `gsettings` plan, capture paths, warning paths, protected
regions, anchors, and animation settings.

`root-summary` requires `schema_version`, `status`, `case_count`, `passed`,
`failed`, `skipped`, and `cases`. The default Rust live runner also records
authoritative `cargo-gtk-proof` engine metadata, scenario-source metadata, the
artifact root, missing capabilities, parity status, current-diff proof-policy
metadata, and aggregated rendered/animation invariant IDs.

`case-summary` requires `status` and either `case_id` or `scenario_id`. Legacy
Python per-case summaries did not include `schema_version`, so the Rust
validator accepts that specific unversioned shape for compatibility. New Rust
per-case summaries should include `schema_version`.

`comparison-report` requires `status`. Legacy Python reports may use `regions`
and `pixel_anchors` without `schema_version`; Rust reports should include
`schema_version` and may use `protected_regions`,
`allowed_changing_regions`, `pixel_anchor_evidence`,
`rendered_anchor_stability`, and `app_vs_rendered_disagreements`. App geometry
may explain or bound rendered-anchor diagnostics, but screenshot-derived
anchor rows remain the pass/fail oracle for rendered effects.

`animation-report` requires `schema_version` and `status`. Stream-mode reports
also need `max_sample_skew_ms` so policy can distinguish real frame/sample
evidence from final-settle-only artifacts.

`warning-scan` requires `status`. Legacy Python reports may omit
`schema_version`; Rust reports should include it and may carry bounded
`matches`, warning counts, unexpected counts, and log paths.

`parity-report` requires `schema_version` and `status`. When it records a
successful comparison, include compared and failed counts, mismatch rows, Rust
engine metadata, Python oracle metadata, and corpus identity.

`environment-report` requires `schema_version` and `status`, with optional host
capabilities, missing capabilities, and isolated runtime metadata.

`proof-policy` requires `schema_version` and may carry
`changed_files_digest`, `visual_sensitive_changes`,
`required_invariant_ids`, and `required_animation_invariant_ids`.

`artifact-envelope` requires `ok`, `status`, `command`, `detail`, `version`,
and `data`. The `version` object carries the proof-spine schema version and the
tool version. For `artifact-summary` envelopes, `data` remains the
automation-client-compatible bounded summary object with case rows, invariant
IDs, warning/comparison details, optional engine/parity metadata, and artifact
paths.

## Validation Commands

Use the Rust tool for schema checks:

```sh
cargo gtk-proof schema list
cargo gtk-proof schema validate scripts/visual-geometry-scenarios/minimap-sidebar-live-threshold.json
cargo gtk-proof summarize build/smoke/visual-geometry
cargo gtk-proof corpus --parity
cargo gtk-proof run
cargo gtk-proof run --oracle python --case-filter minimap
cargo gtk-proof policy --require-rust-engine
```

Schema failures use stable statuses:

- `unsupported-schema-version` for a future or otherwise unsupported
  `schema_version`.
- `malformed-field` for missing or invalid required fields.
- `artifact-error` for unreadable paths or invalid JSON.

The JSON reader and writer enforce an 8 MiB per-file cap for command-owned
schema artifacts. Larger logs, screenshots, crops, or frame streams belong in
bounded artifact files referenced by path, not inside summaries or stdout
envelopes.

`cargo gtk-proof run` is the default same-session live visual proof runner. It
probes host capture dependencies, materializes scenario cases, launches each
case under one private headless Mutter session, aggregates protected crop,
pixel-anchor, animation-stream, warning-scan, and workflow evidence, and writes
`summary.json` with `engine.authoritative=true`.

`cargo gtk-proof run --oracle python` is a compatibility and diagnostic path:
Rust supervises the legacy Python runner with bounded logs and returns a
proof-spine envelope, but the emitted engine metadata is
`python-visual-oracle` with `authoritative=false`. That output is useful for
parity investigation and artifact-summary compatibility checks; it does not
count as default Rust live proof.

Policy callers can add `--require-rust-engine` to reject Python-only or stale
summaries. That mode requires authoritative `cargo-gtk-proof` engine metadata,
a supported `schema_version`, and scenario-source metadata in addition to the
normal current-diff fingerprint and rendered/animation invariant checks.

## Artifact Layout

The default visual proof artifact root remains
`build/smoke/visual-geometry`. Generated artifacts under `build/smoke/` are
ignored by git because screenshots, crops, warning logs, and runtime metadata
may contain host-specific or privacy-sensitive evidence. The Rust writer
refuses unsafe artifact roots such as `/`, the home directory, the workspace
root, non-directory paths, symlinks, and, on Linux, existing directories not
owned by the current process owner.

Checked-in compatibility fixtures live under
`crates/cargo-gtk-proof/fixtures/proof-corpus`. Keep them small, synthetic, and
free of user document content. Curated fixtures should model statuses, exit
classes, invariant IDs, warning-scan outcomes, artifact path shapes, engine
metadata, bounded details, and bounded PNG evidence. `cargo gtk-proof corpus
--parity` compares the Python-oracle and Rust fixture fields and fails on any
mismatch. The Rust command also replays an embedded deterministic PNG corpus
for exact crop comparison, allowed-changing/masked regions, minimap detector
rows, bounded crop artifacts, and rendered-anchor drift diagnostics; generated
smoke artifacts belong under ignored build paths.

## Privacy Boundary

Schemas and result envelopes may expose paths, counts, invariant IDs, tool
versions, schema versions, warning classifications, and relative artifact
paths. They must not expose document text, note bodies, draft bodies, complete
search result text, raw image bytes, private persistence identifiers, or
unbounded logs.
