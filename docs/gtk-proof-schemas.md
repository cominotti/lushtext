# GTK Proof Schemas

`cargo-gtk-proof` owns the versioned Rust-side proof schema descriptors for
LushText's extracted GTK proof toolchain. The current supported
`schema_version` is `1`.

The machine-readable descriptors live in
`crates/cargo-gtk-proof/schemas/*.schema.json`. They are intentionally small in
this phase: they name the schema ID, supported version, required fields, and
optional fields enforced by the Rust validator. The Python visual runner remains
the live same-session execution path until Rust corpus, live-runner, animation,
and wrapper parity are recorded.

## Schema IDs

| Schema ID | Typical path | Purpose |
| --- | --- | --- |
| `visual-scenario` | `scripts/visual-geometry-scenarios/*.json` | Source manifest for a visual invariant matrix. |
| `expanded-case` | per-case manifest artifact | Concrete case produced from one scenario matrix entry. |
| `root-summary` | `build/smoke/visual-geometry/summary.json` | Root visual proof summary consumed by policy and artifact-summary tooling. |
| `comparison-report` | per-case comparison artifact | Protected-region, rendered-anchor, mask, and app-vs-rendered diagnostics. |
| `animation-report` | per-case animation artifact | Timestamp-correlated frame/sample evidence for animation-sensitive invariants. |
| `proof-policy` | embedded in summaries or policy metadata | Changed-file fingerprint and required invariant metadata. |
| `artifact-envelope` | stdout from `cargo gtk-proof` commands | Stable command result envelope. |

## Required Fields

`visual-scenario` requires `schema_version`, `scenario_id`, `matrix`, and
`protected_regions`. The Rust validator also requires non-empty matrix sizes
and color schemes, non-empty protected regions, and an `invariant_id` whenever
pixel anchors are declared.

`expanded-case` requires `schema_version`, `case_id`, `manifest`, `size`,
`color_scheme`, and `artifact_dir`.

`root-summary` requires `schema_version`, `status`, `case_count`, `passed`,
`failed`, `skipped`, and `cases`.

`comparison-report` and `animation-report` require `schema_version` and
`status`; their evidence arrays remain optional so skipped and unsupported-host
artifacts can validate without claiming proof.

`proof-policy` requires `schema_version` and may carry
`changed_files_digest`, `visual_sensitive_changes`,
`required_invariant_ids`, and `required_animation_invariant_ids`.

`artifact-envelope` requires `ok`, `status`, `command`, `detail`, `version`,
and `data`. The `version` object carries the proof-spine schema version and the
tool version.

## Validation Commands

Use the Rust tool for schema checks:

```sh
cargo gtk-proof schema list
cargo gtk-proof schema validate scripts/visual-geometry-scenarios/minimap-sidebar-live-threshold.json
cargo gtk-proof summarize build/smoke/visual-geometry
```

Schema failures use stable statuses:

- `unsupported-schema-version` for a future or otherwise unsupported
  `schema_version`.
- `malformed-field` for missing or invalid required fields.
- `artifact-error` for unreadable paths or invalid JSON.

## Artifact Layout

The default visual proof artifact root remains
`build/smoke/visual-geometry`. Generated artifacts under `build/smoke/` are
ignored by git because screenshots, crops, warning logs, and runtime metadata
may contain host-specific or privacy-sensitive evidence.

Checked-in compatibility fixtures live under
`crates/cargo-gtk-proof/fixtures/proof-corpus`. Keep them small, synthetic, and
free of user document content. Curated fixtures should model statuses and
bounded PNG evidence; generated smoke artifacts belong under ignored build
paths.

## Privacy Boundary

Schemas and result envelopes may expose paths, counts, invariant IDs, tool
versions, schema versions, warning classifications, and relative artifact
paths. They must not expose document text, note bodies, draft bodies, complete
search result text, raw image bytes, private persistence identifiers, or
unbounded logs.
