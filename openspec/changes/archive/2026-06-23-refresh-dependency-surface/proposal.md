## Why

LushText has a set of compatible Rust dependency updates available that reduce dependency drift, refresh parser/search/watch/test tooling patches, and keep Flatpak vendoring aligned without changing product behavior. The recent dependency review also surfaced several tempting platform and parser feature paths; this change keeps the safe refresh separate from speculative UI or parser work so the implementation stays low risk.

## What Changes

- Refresh compatible Cargo dependency locks for the main workspace, fuzz crate, and GTK Lush stock fixture.
- Regenerate Flatpak Cargo vendoring metadata so sandboxed/offline builds match the refreshed lockfile.
- Validate that the dependency refresh does not alter user-facing editor, workspace, search, Markdown preview, or persistence behavior.
- Record feature-adoption decisions for the dependency review: use stable maintenance patches now, defer `notify-debouncer-full` pre-release adoption, defer `sha2` major-line work unless `cargo-gtk-proof` needs it, and leave GTK/Libadwaita/Markdown feature changes to separate proposals.

## Capabilities

### New Capabilities

- `dependency-surface-maintenance`: Defines how LushText safely refreshes dependency locks, Flatpak vendoring metadata, side locks, and dependency-adoption decisions without accidentally broadening product scope.

### Modified Capabilities

- None.

## Impact

- Affected manifests and lock data: `Cargo.lock`, `fuzz/Cargo.lock`, `fixtures/gtk-lush-adoption/stock-settle/Cargo.lock`, and `build-aux/cargo-sources.json`.
- Affected validation surfaces: Cargo workspace checks, fuzz corpus replay or fuzz metadata checks, GTK Lush stock fixture checks, Flatpak vendoring/build validation, and repository policy gates.
- No intentional changes to runtime APIs, UI contracts, app-data formats, GSettings schemas, automation contracts, or user-facing Markdown/search/workspace behavior.
