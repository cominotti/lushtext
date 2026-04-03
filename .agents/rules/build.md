---
description: Build system and compilation rules
globs: "{Cargo.toml,Makefile,.cargo/**,.config/**,build.rs,meson.build,meson_options.txt,build-aux/**}"
---

# Build Rules

## Dev Builds

Use `make` targets for development. The Makefile auto-detects mold and nextest.

```
make run        # build + launch the app
make test       # all tests
make check      # clippy + fmt
```

Direct `cargo` works too, but won't have mold linker unless you export RUSTFLAGS manually.

## Compilation Speed

These patterns are replicated from invowk-rust and must be maintained:

1. **Profiles** in workspace `Cargo.toml` — do not change without benchmarking.
2. **Mold linker** set via Makefile `RUSTFLAGS`, NOT in `.cargo/config.toml` (so builds work without mold).
3. **cargo-hakari** — run `cargo hakari generate` after any dependency change.
4. **.config/nextest.toml** — configure test parallelism here.

## Adding Dependencies

1. Add to `[workspace.dependencies]` in root `Cargo.toml`.
2. Reference with `{ workspace = true }` in crate `Cargo.toml`.
3. Run `cargo hakari generate` to update workspace-hack.
4. Verify gtk-rs version alignment if adding any gtk/glib/gio/pango crate.
5. Run `make cargo-sources` to regenerate `build-aux/cargo-sources.json` for Flatpak.

## GResources

- **Dev builds**: `build.rs` in `lushtext-core` compiles resources via `glib-build-tools`. Embedded in the binary via `include_bytes!` in `lib.rs`.
- **Installed/Flatpak builds**: `resources/meson.build` compiles resources via `gnome.compile_resources()` and installs to `$(pkgdatadir)/`. `lib.rs` loads the `.gresource` file from `LUSHTEXT_PKGDATADIR` at runtime, falling back to `include_bytes!`.

## GSettings Schemas

- Schema XML: `data/dev.cominotti.lushtext.gschema.xml`
- `build.rs` in `lushtext-core` runs `glib-compile-schemas data/` to produce `data/gschemas.compiled` (gitignored).
- `lib.rs::init_schema_dir()` sets `GSETTINGS_SCHEMA_DIR` to point to `data/` for dev builds. Installed builds use the system schema directory.
- Requires `glib-compile-schemas` on the build machine (from `glib2-devel` / `libglib2.0-dev`).
- Widget tests use `GSETTINGS_BACKEND=memory` for isolation (set in `ensure_gtk_init()`).

## Meson Build (Installed / Flatpak)

Meson wraps Cargo for installed and Flatpak builds:
- Root `meson.build` → `subdir('resources')`, `subdir('data')`, `subdir('po')` → `cargo.sh` → `cargo build`
- `build-aux/cargo.sh` bridges Meson→Cargo, exports `LUSHTEXT_PKGDATADIR` for GResource/GSettings dual-path
- `data/meson.build` installs desktop file, metainfo, icons, GSettings schema
- `gnome.post_install()` compiles schemas, updates icon cache and desktop database
- `build.rs` skips `glib-compile-schemas` when `LUSHTEXT_PKGDATADIR` is set (source tree may be read-only in Flatpak)

## Flatpak

- Manifest: `build-aux/dev.cominotti.lushtext.Flatpak.json` (local builds, `type: "dir"`)
- Runtime: `org.gnome.Platform` 48, SDK extension: `org.freedesktop.Sdk.Extension.rust-stable`
- `build-aux/cargo-sources.json` vendors all Cargo dependencies for offline builds
- Regenerate after dependency changes: `make cargo-sources` (requires `flatpak-cargo-generator`)
- Dependency update chain: `cargo update` → `cargo hakari generate` → `make cargo-sources`

## Benchmarks

- Framework: Criterion.rs (`criterion = "0.5"` with `html_reports` feature)
- Benchmark file: `crates/lushtext-core/benches/benchmarks.rs` (single file, all groups)
- All benchmarked code is GTK-free — no display server needed for `cargo bench`
- `[profile.bench]` in workspace `Cargo.toml`: `opt-level = 3`, `lto = "thin"`, `codegen-units = 1` (no strip — criterion needs symbols)
- `FileIndex::from(Vec<IndexedFile>)` enables synthetic index construction for benchmarks
- Report script: `scripts/bench-report.sh` — parses Criterion JSON from `target/criterion/`, generates markdown. Requires `jq`.
- Report output: `docs/benchmarks/<timestamp>.md`
- Makefile targets: `bench`, `bench-report`, `bench-report-full`, `bench-baseline`, `bench-compare`
- Baseline workflow: `make bench-baseline` saves as "main", `make bench-compare` diffs against it

## CI

- `.github/workflows/ci.yml` — Cargo check/clippy/test + benchmark compile-check on `ubuntu-latest`
- `.github/workflows/flatpak.yml` — Flatpak build via `flatpak-github-actions` in GNOME 48 container
- `.github/workflows/release-benchmark.yml` — full benchmark run + markdown report uploaded as release asset on `v*` tags
