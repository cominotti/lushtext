# hexagonal-crate-boundaries Specification

## Purpose

Enforce LushText's three-crate hexagonal workspace layout (`lushtext-model`, `lushtext-services`, `lushtext-ui`), bounded-context module organization, dependency allowlists, and production file-size ceilings mechanically.

## Requirements

### Requirement: Workspace application crates follow strict hexagonal dependency direction
The Cargo workspace SHALL organize LushText application logic into `lushtext-model`, `lushtext-services`, and `lushtext-ui`, consumed by `lushtext`. Dependency direction SHALL be strictly `lushtext -> lushtext-ui -> lushtext-services -> lushtext-model` with no upward or shortcut engine leaks.

#### Scenario: Model crate stays free of GTK and I/O crates
- **WHEN** `cargo metadata` is inspected for `lushtext-model`
- **THEN** its dependencies include no GTK/GLib/Adwaita/SourceView/Pango/GDK crate, no filesystem/watcher/search-engine crate (`rustix`, `notify*`, `ignore`, `dirs`, `regex`, `pulldown-cmark`), and no `anyhow`

#### Scenario: Services crate stays free of GTK and UI crates
- **WHEN** `cargo metadata` is inspected for `lushtext-services`
- **THEN** its dependencies include no GTK/GLib/Adwaita/SourceView/Pango/GDK crate, no `gtk-lush-*` widget/UI crate, and no `lushtext-ui`

#### Scenario: UI crate avoids direct infrastructure engine dependencies
- **WHEN** `cargo metadata` is inspected for `lushtext-ui`
- **THEN** it has no direct dependency on `regex`, `pulldown-cmark`, `rustix`, `notify-debouncer-full`, `ignore`, `simdutf8`, `encoding_rs`, `nucleo-matcher`, or `editorconfig-parser`
- **AND** narrow UI serialization/channel plumbing (`serde_json` for D-Bus automation, `crossbeam-channel` for UI polling/disposal) stays on an explicit allowlist

### Requirement: Domain and service crates group modules by bounded context
`crates/lushtext-model/src/` SHALL group modules by domain context (`values/`, `document/`, `workspace/`, `persistence/`, `budgets/`, `automation/`), and `crates/lushtext-services/src/` SHALL separate driven adapters from application services (`filesystem/`, `persistence/`, `watch/` vs. domain services, `coordination/`, `instrumentation/`).

#### Scenario: Pure workflow policy remains co-located in lushtext-ui
- **WHEN** pure policy belongs to a single UI workflow (`ui/**/policy.rs`)
- **THEN** it remains inside `crates/lushtext-ui/src/ui/` under `.agents/rules/workflow-convention.md` rather than moving to `lushtext-model`

#### Scenario: GTK-free tests and benchmarks build without GTK
- **WHEN** unit, property, persistence-format tests or benchmarks do not touch GTK widgets
- **THEN** they live in `lushtext-model` or `lushtext-services` and build with `cargo test -p lushtext-model` / `cargo test -p lushtext-services` without linking GTK4

### Requirement: Production file growth above 1,000 lines is ratcheted mechanically
`scripts/check-hexagonal-boundaries.py` (with `--self-test`, run in `make check-policy`) SHALL verify dependency rules via `cargo metadata` and SHALL enforce a ceiling of at most 1,000 production lines (excluding `#[cfg(test)]` and test files) on new files, ratcheting the legacy over-limit set at or below its baseline (30 files).

#### Scenario: New oversized file fails policy
- **WHEN** a new production `.rs` file outside the recorded legacy list exceeds 1,000 production lines
- **THEN** `make check-policy` fails naming the file and its production line count

#### Scenario: Self-test proves dependency and size gates
- **WHEN** `python3 scripts/check-hexagonal-boundaries.py --self-test` runs
- **THEN** it proves failure on a forbidden crate edge in each layer and on an oversized new production file
