---
description: Build system and compilation rules
globs: "{Cargo.toml,Makefile,.cargo/**,.config/**,build.rs,meson.build,meson_options.txt,build-aux/**}"
---

# Build Rules

## Dev Builds

Use `make` targets for development. The Makefile auto-detects nextest for non-widget tests across the workspace, while full-suite widget coverage in `make test` flows through the shared headless `scripts/run-widget-tests.sh` path so local verification matches CI. `make test-widget` still uses the same runner in auto/native mode for interactive debugging.

```
make run        # build + launch the app with temporary GNOME desktop staging for dock icon matching
make refresh-dock-icon # regenerate icon assets + force a fresh GNOME Shell dock icon reload
make verify-flatpak-identity # verify Flatpak export identity, permissions, and MIME registration
make test       # all tests
make test-widget-headless # CI-style mutter/dbus widget run
make check      # clippy + fmt
make pre-commit # repo pre-commit gate (fmt + clippy)
make install-git-hooks
```

Direct `cargo` works too — Rust 1.90+ uses `rust-lld` by default on x86_64-linux for fast linking.

The repo-managed Git hooks live under `.githooks/`. Install them with `make install-git-hooks`, which sets `core.hooksPath` for this checkout. The pre-commit hook runs `make pre-commit`, which must stay aligned with the formatting and Clippy gates enforced in CI.

On GNOME Shell, `make run` stages a desktop file plus `hicolor` icons and writes the desktop entry with a content-addressed absolute icon file path. This avoids Shell holding onto a stale themed-icon cache entry when the app icon bytes change between dev runs while keeping the icon file alive for as long as a restored user-local desktop entry might reference it. The launcher must repair any stale absolute `Icon=` path before backing up or restoring an existing `dev.cominotti.lushtext.desktop` override. Because the staged desktop file also carries `MimeType` associations, the launcher must refresh the applications desktop database after staging or restoring it so GNOME Settings and `gio mime` see current handler metadata.

Normal dev staging is temporary and may use the production desktop ID only while the debug process is running. Persistent dev staging (`LUSHTEXT_DEV_RUN_KEEP_STAGED=1`) must use a non-production ID such as `dev.cominotti.lushtext.Devel`; leaving a same-ID non-Flatpak `~/.local/share/applications/dev.cominotti.lushtext.desktop` shadows the Flatpak export and makes GNOME Settings treat LushText as unsandboxed. Use `make verify-flatpak-identity` after Flatpak or desktop-entry work to confirm the exported desktop entry has `X-Flatpak=dev.cominotti.lushtext`, no same-ID dev shadow exists, effective Flatpak permissions are reported, and required MIME handlers remain registered.

For GNOME Settings File Types work, treat the desktop entry's explicit `MimeType=` line as the allowlist source of truth. `gio mime <type>` can still list LushText for source-like MIME types that inherit from `text/plain`, even when LushText does not explicitly advertise those types; that inherited output must not be used as proof that the File Types allowlist is wrong.

Changing `data/icons/dev.cominotti.lushtext.svg` alone is not enough to refresh every icon surface. The fixed-size PNG fallbacks must be regenerated, and an already-running dev session still needs a fresh Shell app object. Use `make refresh-dock-icon` after icon asset changes: it regenerates the PNG fallbacks from the canonical SVG and then replaces the running dev instance so the dock rebuilds from the fresh file-backed icon.

## Compilation Speed

These patterns are replicated from invowk-rust and must be maintained:

1. **Profiles** in workspace `Cargo.toml` — do not change without benchmarking.
2. **rust-lld** — default linker on x86_64-linux since Rust 1.90 (~10x faster than BFD, zero config). No manual linker override needed.
3. **cargo-hakari** — run `cargo hakari generate` after any dependency change.
4. **.config/nextest.toml** — configure nextest parallelism for non-widget tests here.
5. **`rust-version`** — keep `rust-version = "1.95.0"` in `[workspace.package]` and inherited by every package so `cargo check` surfaces MSRV violations early. `rust-toolchain.toml` pins the local toolchain to the same version.

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
- Runtime: `org.gnome.Platform` 50, SDK extension: `org.freedesktop.Sdk.Extension.rust-stable`
- Use `make flatpak` for a local build without installing it; it first ensures the user Flathub remote and manifest runtime/SDK dependencies are available
- Use `make flatpak-install` to build and install the latest checkout into the user Flatpak installation; the target is idempotent and installs missing runtime/SDK dependencies from Flathub
- Use `make verify-flatpak-identity` after install/export changes to catch stale same-ID dev launchers and verify `X-Flatpak`, permissions, and MIME registration
- `build-aux/cargo-sources.json` vendors all Cargo dependencies for offline builds
- Regenerate after dependency changes: `make cargo-sources` (requires `flatpak-cargo-generator`)
- Dependency update chain: `cargo update` → `cargo hakari generate` → `make cargo-sources`

## Benchmarks

- Framework: Criterion.rs (`criterion = "0.8"` with `html_reports` feature)
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

All CI jobs use container images because `ubuntu-latest` ships GTK 4.14, but this repo targets the GNOME 50 platform family (GTK 4.22, Libadwaita 1.9).

- `.github/workflows/ci.yml` — split `Lint`, `Non-widget Tests`, `Widget Tests`, `Bench Compile`, and `Dependency Policy` jobs. The Fedora 44 container jobs cover rustfmt, Clippy, the rustdoc lint gate, non-widget tests, widget tests, and benchmark compilation; widget tests run through `scripts/run-widget-tests.sh --headless --retries 1`, which wraps the same `mutter --headless` Wayland path GNOME GTK CI uses while filtering known-benign headless-session noise. The `Dependency Policy` job runs `cargo deny check advisories bans sources`.
- `.github/workflows/flatpak.yml` — Flatpak build via `flatpak-github-actions` in `ghcr.io/flathub-infra/flatpak-github-actions:gnome-50` container (Docker Hub `bilelmoussaoui/` stopped at gnome-47; GNOME 48+ images are on ghcr.io) with cache keys tied to actual Flatpak build inputs rather than commit SHA alone.
- `.github/workflows/release-benchmark.yml` — full benchmark run + markdown report uploaded as release asset on `v*` tags, same `fedora:44` container

**When bumping gtk-rs version:** update the Fedora version in ci.yml and release-benchmark.yml, and the GNOME tag in flatpak.yml and the Flatpak manifest, to match the new minimum GTK requirement.
