---
description: Build system and compilation rules
globs: "{Cargo.toml,Makefile,.cargo/**,.config/**,build.rs,meson.build,meson_options.txt,build-aux/**}"
---

# Build Rules

## Dev Builds

Use `make` targets for development. The Makefile auto-detects nextest for non-widget tests across the workspace, while full-suite widget coverage in `make test` flows through the shared headless `scripts/run-widget-tests.sh` path so local verification matches CI. `make test-widget` still uses the same runner in auto/native mode for interactive debugging.

```
make run        # build + launch the app
make test       # all tests
make test-widget-headless # CI-style mutter/dbus widget run
make check      # clippy + fmt
make pre-commit # repo pre-commit gate (fmt + clippy)
make install-git-hooks
```

Direct `cargo` works too — Rust 1.90+ uses `rust-lld` by default on x86_64-linux for fast linking.

The repo-managed Git hooks live under `.githooks/`. Install them with `make install-git-hooks`, which sets `core.hooksPath` for this checkout. The pre-commit hook runs `make pre-commit`, which must stay aligned with the formatting and Clippy gates enforced in CI.

## Compilation Speed

These patterns are replicated from invowk-rust and must be maintained:

1. **Profiles** in workspace `Cargo.toml` — do not change without benchmarking.
2. **rust-lld** — default linker on x86_64-linux since Rust 1.90 (~10x faster than BFD, zero config). No manual linker override needed.
3. **cargo-hakari** — run `cargo hakari generate` after any dependency change.
4. **.config/nextest.toml** — configure nextest parallelism for non-widget tests here.
5. **`rust-version`** — consider adding `rust-version = "1.94.1"` to `[workspace.package]` in root `Cargo.toml` so `cargo check` surfaces MSRV violations early. Currently enforced only via `rust-toolchain.toml`.

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
- Runtime: `org.gnome.Platform` 49, SDK extension: `org.freedesktop.Sdk.Extension.rust-stable`
- `build-aux/cargo-sources.json` vendors all Cargo dependencies for offline builds
- Regenerate after dependency changes: `make cargo-sources` (requires `flatpak-cargo-generator`)
- Dependency update chain: `cargo update` → `cargo hakari generate` → `make cargo-sources`

## Benchmarks

- Framework: Criterion.rs (`criterion = "0.5"` with `html_reports` feature)
- Benchmark file: `crates/lushtext-core/benches/benchmarks.rs` (single file, all groups)
- All benchmarked code is GTK-free — no display server needed for `cargo bench`
- `[profile.bench]` in workspace `Cargo.toml`: `opt-level = 3`, `lto = "thin"`, `codegen-units = 1` (no strip — criterion needs symbols)
- `FileIndex::from(Vec<IndexedFile>)` enables synthetic index construction for benchmarks
- Report script: `scripts/bench-report.sh` — clears stale Criterion `new/` results before each run, fails closed if `cargo bench` fails, then parses fresh JSON into markdown. Requires `jq`.
- Report output: `docs/benchmarks/<timestamp>.md`
- Makefile targets: `bench`, `bench-report`, `bench-report-full`, `bench-baseline`, `bench-compare`
- Baseline workflow: `make bench-baseline` saves as "main", `make bench-compare` diffs against it

## Runtime Warnings

**CRITICAL: GTK/pixman warnings are bugs, not noise.** When running the app via `make run`, the stderr output must be free of these warnings:

- `*** BUG *** In pixman_region32_init_rect: Invalid rectangle passed` — a widget is being allocated with zero or negative dimensions. Typically caused by toggling `shrink-start-child` on GtkPaned, or by animating a raw pane child to 0 instead of animating a clipping wrapper (for example `GtkRevealer`) and hiding that wrapper at the collapsed endpoint.
- `Gtk-CRITICAL` or `Gtk-WARNING` messages — usually indicate incorrect widget lifecycle, invalid property access, or constraint violations.
- `GLib-GObject-WARNING` — usually indicate signal or property misuse.

**Development is not finished if any of these warnings appear during normal usage.** Before considering a UI change complete, run the app and exercise the affected feature (toggle sidebar, resize window, open/close tabs, etc.) while watching stderr. Fix the root cause — do not suppress or ignore the warnings.

## CI

All CI jobs use container images because `ubuntu-latest` ships GTK 4.14, but `gtk4-rs 0.11` requires GTK >= 4.20 (GNOME 49).

- `.github/workflows/ci.yml` — split `Lint`, `Non-widget Tests`, `Widget Tests`, and `Bench Compile` jobs in `fedora:43` containers (GNOME 49, GTK 4.20). Widget tests run through `scripts/run-widget-tests.sh --headless --retries 1`, which wraps the same `mutter --headless` Wayland path GNOME GTK CI uses.
- `.github/workflows/flatpak.yml` — Flatpak build via `flatpak-github-actions` in `ghcr.io/flathub-infra/flatpak-github-actions:gnome-49` container (Docker Hub `bilelmoussaoui/` stopped at gnome-47; GNOME 48+ images are on ghcr.io) with cache keys tied to actual Flatpak build inputs rather than commit SHA alone.
- `.github/workflows/release-benchmark.yml` — full benchmark run + markdown report uploaded as release asset on `v*` tags, same `fedora:43` container

**When bumping gtk-rs version:** update the Fedora version in ci.yml and release-benchmark.yml, and the GNOME tag in flatpak.yml and the Flatpak manifest, to match the new minimum GTK requirement.
