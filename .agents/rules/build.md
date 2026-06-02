---
description: Build system and compilation rules
globs: "{Cargo.toml,Makefile,.cargo/**,.config/**,build.rs,meson.build,meson_options.txt,build-aux/**}"
---

# Build Rules

## Dev Builds

Use `make` targets for development. The Makefile auto-detects nextest for non-widget tests across the workspace, while full-suite widget coverage in `make test` flows through the shared headless `scripts/run-widget-tests.sh` path so local verification matches CI. `make test-widget` still uses the same runner in auto/native mode for interactive debugging.

```
make dev-tools  # Flatpak runtime/SDK deps + GTK debug input/screenshot helpers
make run        # build + force a fresh launch with temporary GNOME desktop staging
make refresh-dock-icon # regenerate icon assets + force a fresh GNOME Shell dock icon reload
make verify-flatpak-identity # verify Flatpak export identity, permissions, and MIME registration
make test       # all tests
make test-prop  # bounded property tests for pure deterministic logic
make test-prop-deep # opt-in deeper property run with more generated cases
make test-widget-headless # CI-style mutter/dbus widget run
make mutants-smoke # small cargo-mutants smoke run
make mutants-diff  # mutation test current changes against origin/main
make mutants-full  # mutation test the configured deterministic scope
make check      # clippy + fmt
make pre-commit # repo pre-commit gate (fmt + clippy)
make install-git-hooks
```

Direct `cargo` works too — Rust 1.90+ uses `rust-lld` by default on x86_64-linux for fast linking.

The repo-managed Git hooks live under `.githooks/`. Install them with `make install-git-hooks`, which sets `core.hooksPath` for this checkout. The pre-commit hook runs `make pre-commit`, which must stay aligned with the formatting and Clippy gates enforced in CI.

Use `make dev-tools` on a fresh local checkout before deep GTK debugging. It must remain idempotent and depend on `flatpak-deps` so Flatpak runtime/SDK setup and live-debug helper setup stay available through one command. The helper script installs headless Mutter/PipeWire/WirePlumber/GStreamer screenshot tooling, portal screenshot tools, system Python AT-SPI bindings, ydotool, isolated Xvfb fallback tooling, and D-Bus/GSettings utilities when missing, then starts a user `ydotoold` socket under `$XDG_RUNTIME_DIR` only when `/dev/uinput` is present and writable. Do not pollute the host/global environment: no shell-wide exports, no pip installs, no dotfile edits, and no automatic rpm-ostree host layering. Host layering requires `LUSHTEXT_DEV_TOOLS_ALLOW_RPM_OSTREE=1`.

On GNOME Shell, `make run` asks any already-running `dev.cominotti.lushtext` owner to quit before staging a desktop file plus `hicolor` icons and launching the freshly built debug binary. If the existing owner refuses to close, the launcher must fail instead of activating stale code. The staged desktop entry uses a content-addressed absolute icon file path. This avoids Shell holding onto a stale themed-icon cache entry when the app icon bytes change between dev runs while keeping the icon file alive for as long as a restored user-local desktop entry might reference it. The launcher must repair any stale absolute `Icon=` path before backing up or restoring an existing `dev.cominotti.lushtext.desktop` override. Because the staged desktop file also carries `MimeType` associations, the launcher must refresh the applications desktop database after staging or restoring it so GNOME Settings and `gio mime` see current handler metadata.

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

## Property Testing

- Framework: `proptest`, wired only for the `lushtext-core` property-test target
- Feature: `lushtext-core/property-tests`
- Target: `cargo nextest run -p lushtext-core --features property-tests --test properties --profile property`
- Makefile targets: `test-prop`, `test-prop-deep`
- Regression file location: `crates/lushtext-core/proptest-regressions/properties.txt`

The property target is guarded by `required-features = ["property-tests"]` and
must stay outside default non-widget nextest and default mutation runs. Use it
for pure deterministic invariants over bounded generated strings, paths,
vectors, Markdown fragments, replacement lists, encodings, and sidecar hashes.
Do not put GTK widget construction, compositor behavior, D-Bus/portal state,
file chooser flows, watcher timing, or live session behavior in this target.

`make test-prop` uses the CI-safe default of 64 cases per property. Use
`make test-prop-deep PROPTEST_DEEP_CASES=1024` for a manual or scheduled pass.
Do not raise the default pull-request case count just to investigate one broad
invariant; tighten the generator or use the deep lane.

## GResources

- **Dev builds**: `build.rs` in `lushtext-core` compiles resources via `glib-build-tools`. Embedded in the binary via `include_bytes!` in `lib.rs`.
- **Installed/Flatpak builds**: `resources/meson.build` compiles resources via `gnome.compile_resources()` and installs to `$(pkgdatadir)/`. `lib.rs` loads the `.gresource` file from `LUSHTEXT_PKGDATADIR` at runtime, falling back to `include_bytes!`.

## GSettings Schemas

- Schema XML: `data/dev.cominotti.lushtext.gschema.xml`
- `build.rs` in `lushtext-core` runs `glib-compile-schemas data/` to produce `data/gschemas.compiled` (gitignored).
- `lib.rs::init_schema_dir()` sets `GSETTINGS_SCHEMA_DIR` to point to `data/` for dev builds. Installed builds use the system schema directory.
- Requires `glib-compile-schemas` on the build machine (from `glib2-devel` / `libglib2.0-dev`).
- Widget tests use `GSETTINGS_BACKEND=memory` for isolation (set in `ensure_gtk_init()`).

## Mutation Testing

- Framework: `cargo-mutants` 27.x, configured in `.cargo/mutants.toml`
- Test runner: `cargo nextest`, matching the non-widget CI lane
- Wrapper: `scripts/run-mutants.sh`
- Makefile targets: `mutants-smoke`, `mutants-diff`, `mutants-full`, `mutants-list`
- Output: `mutants.out` / `mutants.out.old` (gitignored and uploaded from CI)

The default mutation scope is intentionally deterministic: model code, service
code, and a few pure helper-heavy UI modules. Do not add GTK widget construction,
live signal wiring, file dialogs, or display-server-dependent code directly to
the cargo-mutants scope; keep that behavior in `scripts/run-widget-tests.sh`,
where Mutter, D-Bus, renderer settings, retries, and warning filtering are
controlled.

Default mutation runs must also omit `lushtext-core/property-tests`. Generated
property cases belong in `make test-prop`; otherwise mutation runtime becomes
mutants multiplied by generated cases. If a future change needs a tiny property
under mutation, add a separate documented opt-in mode rather than changing the
default wrapper.

Local in-place mutation runs are guarded. `MUTANTS_IN_PLACE=1` refuses dirty
worktrees outside CI because cargo-mutants rewrites source files while testing.
Use a clean checkout, a disposable worktree, or the default copy-based local
mode when experimenting.

cargo-mutants is serial by default, so the local Makefile targets auto-tune
parallelism: `MUTANTS_JOBS` defaults to about `nproc / 4` and
`MUTANTS_TEST_THREADS` caps each mutant job's nextest so `jobs x test-threads`
stays near the logical CPU count instead of oversubscribing. `scripts/run-mutants.sh`
only passes `--jobs` when `MUTANTS_JOBS` is set, and CI leaves it unset, so the
sharded small runners keep the serial default and fan out through `MUTANTS_SHARD`
instead.

Treat survivors in this order: first decide whether the mutant represents a
real missed behavior, then add or tighten deterministic tests, then consider
small refactors that make the behavior testable. Only equivalent or explicitly
out-of-scope mutants should be excluded, and exclusions must stay narrow enough
that nearby behavior still mutates.

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

## Flatpak Release Automation

- Release command surface: `make release VERSION=vX.Y.Z RELEASE_NOTES_FILE=...` for explicit releases and `make release-bump TYPE=major|minor|patch` for computed releases. Use `DRY_RUN=1` before real releases. `PRERELEASE=alpha|beta|rc` starts or continues prerelease streams, and `PROMOTE=1` is required to promote a prerelease stream to stable.
- Real release commands must run from a clean `main` branch. The release helper stages only intended release metadata and packaging files, commits with `chore(release): vX.Y.Z`, creates a signed tag, and pushes `main` plus the tag only after validation passes.
- `RELEASE_NOTES_FILE` is mandatory for real releases because the notes are inserted into `data/dev.cominotti.lushtext.metainfo.xml.in` as the AppStream release description.
- Version surfaces that move together: `meson.build`, `crates/lushtext/Cargo.toml`, `crates/lushtext-core/Cargo.toml`, `Cargo.lock`, AppStream releases, and `build-aux/cargo-sources.json`.
- The primary Flatpak publication channel is the Cominotti-owned remote at `https://flatpak.cominotti.dev/`. Use `make cominotti-flatpak-repo VERSION=vX.Y.Z COMINOTTI_FLATPAK_PUBLIC_KEY=... COMINOTTI_FLATPAK_GPG_KEY=...` to generate a signed repository under `build-aux/cominotti-flatpak/flatpak/repo/`, plus `cominotti.flatpakrepo` and `lushtext.flatpakref`. Always run `make verify-cominotti-flatpak-repo` and `make verify-cominotti-pages-limits`; use `make cominotti-flatpak-smoke` when a real repo summary should be present. Cloudflare Pages is the default hosted backend, `COMINOTTI_FLATPAK_DEPLOY_COMMAND` is an override, and Cloudflare R2 is the first fallback when Pages asset or file-count limits are exceeded.
- Public Cominotti install instructions must keep GPG verification enabled. Do not publish `flatpak remote-add --no-gpg-verify` for the Cominotti remote except in clearly private local-testing notes.
- The local Flatpak manifest remains checkout-oriented with `type: "dir"`. Flathub updates are generated with `make flathub-manifest VERSION=vX.Y.Z`, producing `build-aux/flathub/dev.cominotti.lushtext.json` plus `cargo-sources.json` with a public Git tag/commit source and `CARGO_NET_OFFLINE=true`.
- Always run `make verify-flathub-manifest` after generating a Flathub-facing manifest. It rejects local `type: "dir"` sources and checks that app ID, runtime, SDK, Rust extension, command, finish args, cleanup rules, Meson profile, and Cargo sources match the reviewed local manifest.
- Flathub app verification for `dev.cominotti.lushtext` is domain-based on `cominotti.dev`; linked GitHub accounts do not verify this custom-domain ID. Use `make verify-flathub-domain FLATHUB_VERIFICATION_TOKEN=<token>` after publishing the Flathub token to `https://cominotti.dev/.well-known/org.flathub.VerifiedApps.txt`.
- Flathub publication is optional and stays reviewable by default. Missing Flathub credentials must be reported separately from Cominotti publication, and must not make the primary Cominotti release look incomplete. Do not enable `flathub.json` automerge unless a later explicit policy change accepts that successful Flathub builds do not prove runtime behavior.

## Snap

- Manifest: `snap/snapcraft.yaml`. Strict confinement + `home` / `removable-media` plugs; the `gnome` extension supplies Wayland/X11/GPU/portals. Identity stays `dev.cominotti.lushtext` via `common-id`; the snap NAME (`lushtext`) lives in Snap's flat namespace.
- **Reuses the Meson/Cargo build**: the `meson` plugin drives the same `meson.build` → `cargo.sh` path as the Flatpak. A `layout:` bind-mounts the baked `LUSHTEXT_PKGDATADIR` (`/usr/share/lushtext`) to `$SNAP/usr/share/lushtext`, so `register_resources()` / `init_schema_dir()` work under confinement with no Rust changes.
- **No `cargo-sources.json` for Snap**: snap builds are online by default, so the Flatpak vendoring artifact is not required here; crates are fetched during the build.
- **Rust toolchain**: Ubuntu 24.04 packages `rustc` below the 1.95 MSRV (edition 2024), so the manifest bootstraps the pinned toolchain via rustup in a `rust-toolchain` part.
- **App schema compile**: meson's `gnome.post_install()` skips `glib-compile-schemas` under DESTDIR staging and the extension does not compile the app's own schema, so the part compiles `dev.cominotti.lushtext.gschema.xml` explicitly in `override-build`.
- **GATED on the GNOME 50 platform snap**: LushText needs GTK 4.22; the extension currently provides only `gnome-46-2404` (GTK 4.14, `core24`). The matching `core26` / GNOME 50 platform snap (`gnome-50-2604` or equivalent) is not published yet, so `make snap` is expected to fail until `base:` is switched to `core26` (the `core26` base itself is published). Do not vendor the GNOME stack from source or lower the GTK floor to work around this.
- `make snap` (LXD build), `make snap-smoke` (confined smoke test — skips cleanly until the platform snap exists), `make verify-snap-identity` (confinement/plugs/common-id). Smoke test fails on AppArmor/seccomp denials.
- **Release posture**: Unlisted visibility + `edge`-only channel, so the snap is absent from search and `snap install lushtext` fails by default (`snap install lushtext --edge` is required). Promotion to stable/public is a store action, not a rebuild.

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

- `.github/workflows/ci.yml` — split `Lint`, `Non-widget Tests`, `Widget Tests`, `Bench Compile`, and `Dependency Policy` jobs. The Fedora 44 container jobs cover rustfmt, Clippy, the rustdoc lint gate, non-widget tests, widget tests, and benchmark compilation; widget tests run through `scripts/run-widget-tests.sh --headless --retries 1`, which wraps the same `mutter --headless` Wayland path GNOME GTK CI uses while filtering known-benign headless-session noise. The runner defaults to `GSK_RENDERER=cairo` so headless containers do not emit Mesa/EGL GPU-probe warnings, but callers may override the renderer for explicit renderer debugging. The `Dependency Policy` job runs `cargo deny check advisories bans sources`.
- `.github/workflows/ci.yml` also has a separate `Property Tests` job that runs `make test-prop` with the `property-tests` feature enabled. Keep that lane separate from the default non-widget and mutation jobs.
- `.github/workflows/flatpak.yml` — Flatpak build via `flatpak-github-actions` in `ghcr.io/flathub-infra/flatpak-github-actions:gnome-50` container (Docker Hub `bilelmoussaoui/` stopped at gnome-47; GNOME 48+ images are on ghcr.io) with cache keys tied to actual Flatpak build inputs rather than commit SHA alone.
- `.github/workflows/release-dry-run.yml` — path-filtered release automation check for release scripts, Flatpak manifests, AppStream metadata, desktop metadata, and cargo vendoring; runs release helper tests, Flathub manifest tests, Cominotti repository metadata tests, a no-mutation release preview, and current metadata validation.
- `.github/workflows/release.yml` — `v*` tag release validation and manual dry-run workflow. It validates release metadata, builds the Flatpak from the release source, prepares/deploys Cominotti Flatpak repository artifacts when signing and deploy configuration are available, creates or updates the GitHub Release context, and opens an optional Flathub manifest PR when `FLATHUB_TOKEN` and `FLATHUB_REPOSITORY` are configured.
- `.github/workflows/release-benchmark.yml` — full benchmark run + markdown report uploaded as release asset on `v*` tags, same `fedora:44` container
- `.github/workflows/snap.yml` — always-on `validate` job runs `snapcraft expand-extensions` (structural/extension validation only; a full build cannot succeed until the GNOME 50 platform snap exists). The `build-publish` job (`snapcore/action-build` + `snapcore/action-publish`, release `edge`) is gated behind the `SNAP_PLATFORM_AVAILABLE` repository variable so the missing platform never reds the pipeline; it uses the `SNAPCRAFT_STORE_CREDENTIALS` secret.

**When bumping gtk-rs version:** update the Fedora version in ci.yml and release-benchmark.yml, and the GNOME tag in flatpak.yml and the Flatpak manifest, to match the new minimum GTK requirement.
