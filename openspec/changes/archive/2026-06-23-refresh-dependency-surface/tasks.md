## 1. Preflight

- [x] 1.1 Re-run `cargo update --dry-run --verbose` for the main workspace and confirm the refresh remains within current manifest ranges.
- [x] 1.2 Re-run dry-run updates for `fuzz/Cargo.toml` and `fixtures/gtk-lush-adoption/stock-settle/Cargo.toml` and note the expected side-lock changes.
- [x] 1.3 Confirm `sha2 0.11`, `notify-debouncer-full 0.8.0-rc.*`, extra `pulldown-cmark` parser flags, and GTK/Libadwaita UI feature adoption remain out of scope for this change.

## 2. Refresh Dependency Artifacts

- [x] 2.1 Run `cargo update` for the main workspace lockfile.
- [x] 2.2 Run `cargo update --manifest-path fuzz/Cargo.toml` for the fuzz side lockfile.
- [x] 2.3 Run `cargo update --manifest-path fixtures/gtk-lush-adoption/stock-settle/Cargo.toml` for the GTK Lush stock fixture lockfile.
- [x] 2.4 Run `make cargo-sources` to regenerate `build-aux/cargo-sources.json` from the refreshed main workspace lockfile.
- [x] 2.5 Review `git diff -- Cargo.toml Cargo.lock fuzz/Cargo.toml fuzz/Cargo.lock fixtures/gtk-lush-adoption/stock-settle/Cargo.toml fixtures/gtk-lush-adoption/stock-settle/Cargo.lock build-aux/cargo-sources.json` and verify no manifest range, Flatpak permission, app-code, schema, or UI behavior changes slipped in.

## 3. Focused Validation

- [x] 3.1 Run `cargo tree --workspace --duplicates` and record any remaining duplicate dependency families as expected or investigate them.
- [x] 3.2 Run focused Markdown preview and content-search tests affected by `pulldown-cmark`, `regex`, `ignore`, and `memchr` refreshes.
- [x] 3.3 Run `make fuzz-corpus-replay` to validate the stable fuzz corpus against the refreshed fuzz-side graph.
- [x] 3.4 Run `make gtk-lush-stock-fixtures` to validate the refreshed stock fixture lockfile.
- [x] 3.5 Run `make check` for the repository fast gate.

## 4. Packaging Validation

- [x] 4.1 Run a Flatpak vendoring/build validation such as `make flatpak` or an equivalent `flatpak-builder` invocation.
- [x] 4.2 If local Flatpak validation is unavailable, record the exact missing host/runtime dependency and run the strongest available generated-source validation instead.

## 5. Closeout

- [x] 5.1 Run `openspec validate refresh-dependency-surface --strict`.
- [x] 5.2 Run `openspec validate --changes --strict`.
- [x] 5.3 Run `git diff --check`.
- [x] 5.4 Summarize the refreshed packages, validation results, and deferred feature candidates in the implementation closeout.
