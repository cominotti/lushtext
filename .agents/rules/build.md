---
description: Build system and compilation rules
globs: "{Cargo.toml,Makefile,.cargo/**,.config/**,build.rs,meson.build,meson_options.txt,build-aux/**}"
---

# Build Rules

## Dev Builds

Use `make` targets for development. The Makefile auto-detects nextest for non-widget tests across the workspace; `.config/nextest.toml` excludes the `widget` binary from nextest's default filter, while full-suite widget coverage in `make test` flows through the shared headless `scripts/run-widget-tests.sh` path so local verification matches CI. Widget tests must never use the developer's live desktop session: the script has no native mode, and the Cargo-visible widget harness self-supervises into private `mutter --headless` before GTK initializes.

```
make dev-tools  # Flatpak runtime/SDK deps + GTK debug input/screenshot helpers
make run        # build + force a fresh launch with temporary GNOME desktop staging
make refresh-dock-icon # regenerate icon assets + force a fresh GNOME Shell dock icon reload
make verify-flatpak-identity # verify Flatpak export identity, permissions, and MIME registration
make test       # all tests
make test-prop  # bounded property tests for pure deterministic logic
make test-prop-deep # opt-in deeper property run with more generated cases
make fuzz-corpus-replay # stable replay of committed fuzz corpus seeds
make fuzz-smoke # bounded local cargo-fuzz smoke, requires nightly tooling
make fuzz-operation-smoke # bounded structured-operation fuzz smoke
make test-widget-headless # CI-style mutter/dbus widget run
make visual-smoke # real-session screenshot smoke with artifacts
make portal-sandbox-smoke # available Flatpak/Snap confinement diagnostics
make accessibility-smoke # AT-SPI-enabled accessibility smoke
make performance-smoke # lightweight Criterion performance smoke
./scripts/check-filesystem-boundary.sh # no disallowed raw filesystem calls/examples
make check-agent-docs # validate agent rules/skills guidance
make end-user-smoke # run all host-supported end-user smoke lanes
make mutants-smoke # small cargo-mutants smoke run
make mutants-diff  # mutation test current changes against origin/main
make mutants-full  # mutation test the configured deterministic scope
make check      # clippy + fmt
make pre-commit # repo pre-commit gate (fmt + clippy)
make install-git-hooks
```

Filesystem-sensitive changes must also pass `make check-agent-docs`; that target
verifies the `services::filesystem` guidance in rules and skills, then runs the
raw filesystem no-leftovers audit.

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
5. **`rust-version`** — keep `rust-version = "1.96.0"` in `[workspace.package]` and inherited by every package so `cargo check` surfaces MSRV violations early. `rust-toolchain.toml` pins the local toolchain to the same version.

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
vectors, Markdown fragments, replacement lists, encodings, sidecar hashes, and
tiny deterministic tempdir-backed service fixtures. Do not put GTK widget
construction, compositor behavior, D-Bus/portal state, file chooser flows,
watcher timing, or live session behavior in this target.

`make test-prop` uses the CI-safe default of 64 cases per property. Use
`make test-prop-deep PROPTEST_DEEP_CASES=1024` for a manual or scheduled pass.
Do not raise the default pull-request case count just to investigate one broad
invariant; tighten the generator or use the deep lane.

## Fuzzing

- Framework: `cargo-fuzz`, isolated under `fuzz/`
- Feature: `lushtext-core/fuzzing`
- Targets: `editor_bytes`, `markdown_preprocess`, `operation_script`
- Makefile targets: `fuzz-list`, `fuzz-corpus-replay`, `fuzz-smoke`,
  `fuzz-operation-smoke`
- Default tool: `cargo +nightly fuzz` (override with `CARGO_FUZZ=...`)
- Docs: `docs/fuzzing.md`

The fuzz project is excluded from the normal Cargo workspace. Fuzz discovery
enables `lushtext-core/fuzzing` through the isolated `fuzz/` crate, while stable
corpus replay enables the same helper feature from an ordinary Rust test target.
Default `make test`, nextest, property, widget, benchmark, and mutation lanes
must not invoke fuzz targets or corpus replay, and they must not require
fuzz-only dependencies. GitHub Actions runs stable corpus replay as its own
ordinary CI job through `make fuzz-corpus-replay`; keep coverage-guided
`cargo-fuzz` smoke in scheduled/manual lanes instead of pull-request CI.

Local fuzz smoke needs a nightly Rust toolchain, `cargo-fuzz`, and a C++
compiler for `libfuzzer-sys` (`gcc-c++` on Fedora/toolbox).

Use fuzzing for hostile byte ingestion and bounded structured operation scripts:
arbitrary bytes through editor decoding, encoding-state and file-health
classification, text-level Markdown preprocessing/parser setup, replacement
preview generation, save-formatting, session/draft JSON round trips, and corrupt
session/draft JSON decode attempts. Fuzz targets must stay deterministic and
must not start GTK, construct widgets, open file choosers, watch filesystems,
use D-Bus/portals, or require a compositor.

`make fuzz-smoke` runs each configured target with explicit run, time, and input
length bounds against temporary corpus copies so generated corpus growth does
not dirty the checkout. `make fuzz-operation-smoke` runs only the structured
operation target with the same smoke bounds; the operation-script harness also
caps generated scripts at 32 operations. Longer runs should be manual or
scheduled with explicit budgets.

`make fuzz-corpus-replay` replays committed `fuzz/corpus/**` seeds through
stable Rust tests. It must stay read-only: no corpus mutation, no fuzz artifact
or coverage writes, no `cargo-fuzz`, no `libfuzzer-sys`, no sanitizer flags, no
nightly requirement, and no C/C++ compiler requirement.

Do not add LibAFL unless a future OpenSpec change identifies a concrete need
for custom executors, feedback, scheduling, distributed orchestration, or fuzzer
state persistence.

Real fuzz crashes should be minimized with
`cargo +nightly fuzz tmin <target> <crash>` and fixed with a minimized corpus
seed, deterministic regression test, or a reviewed rationale for why no durable
seed is appropriate.

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
- Output: `mutants.out` / `mutants.out.old` (gitignored; uploaded only when
  mutation CI is explicitly re-enabled)

Mutation tests are local-only by default. The GitHub Actions workflow remains in
place as a re-enable template, but its mutation jobs are gated by the repository
variable `LUSHTEXT_ENABLE_MUTATION_CI=true`. Leave that variable unset or any
value other than `true` for normal PR, scheduled, and manual CI. Use the local
Makefile targets above while the lane is disabled.

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
parallelism: `MUTANTS_JOBS` defaults to about `nproc / 4`, then two per-job caps
keep `jobs x per-job-parallelism` near the logical CPU count instead of
oversubscribing. `MUTANTS_TEST_THREADS` (default `4`) bounds each job's nextest,
and `MUTANTS_BUILD_JOBS` (derived) bounds each job's `cargo build` via
`CARGO_BUILD_JOBS` — the build phase is what spikes load average, since six
concurrent cold builds each fan out to every core by default even while IO and
memory pressure stay near zero. `scripts/run-mutants.sh` only exports these /
passes `--jobs` when the matching env var is set. If the gated CI mutation lane
is re-enabled, it must leave all three unset so the sharded small runners keep
the serial default and fan out through `MUTANTS_SHARD` instead.

Treat survivors in this order: first decide whether the mutant represents a
real missed behavior, then add or tighten deterministic tests, then consider
small refactors that make the behavior testable. Only equivalent or explicitly
out-of-scope mutants should be excluded, and exclusions must stay narrow enough
that nearby behavior still mutates.

## End-User Smoke Lanes

`docs/end-user-coverage.md` is the coverage map for behavior that default unit,
integration, property, fuzz, widget, benchmark, and mutation lanes cannot prove
honestly. Keep those lane boundaries current when adding new smoke checks.

- `make visual-smoke` builds the debug binary, launches LushText under isolated
  headless Mutter through the existing screenshot automation, captures a
  representative editor/search/minimap screenshot, and stores environment and
  session artifacts under `build/smoke/visual` by default.
- `make portal-sandbox-smoke` records available Flatpak/Snap runtime state and
  invokes supported confined smoke checks. It must skip explicitly when neither
  runtime is installed or buildable; a skip is not proof that confinement works.
- `make accessibility-smoke` keeps the accessibility bridge enabled and uses the
  AT-SPI path. Do not rely on the widget harness for this class of coverage
  because `scripts/run-widget-tests.sh` intentionally sets `NO_AT_BRIDGE=1`.
- `make performance-smoke` runs a small Criterion smoke filter with coarse
  timing artifacts. It is distinct from full `bench-report` output and should
  stay forgiving enough to avoid routine shared-runner noise.
- `make end-user-smoke` runs the host-supported smoke lanes together. Individual
  scripts own their dependency checks, artifact paths, and skip messages.

The default pull-request path should stay bounded. Wire only cheap and stable
parts of these smoke lanes into PR CI; keep screenshot, portal/sandbox,
AT-SPI, installed-package, and deeper performance checks scheduled, manual,
release-only, or opt-in unless a later change proves they are reliable as
blocking PR gates.
`.github/workflows/end-user-smoke.yml` is the scheduled/manual artifact lane
for visual, portal/sandbox, accessibility, performance smoke, and full
benchmark report coverage.

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
- The Rust stable extension must satisfy the workspace MSRV. If a local user
  installation is stale after an MSRV bump, update it explicitly with
  `flatpak update --user org.freedesktop.Sdk.Extension.rust-stable//25.08`
  before treating a Flatpak build failure as an application regression.
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
- **Rust toolchain**: Ubuntu 24.04 packages `rustc` below the 1.96 MSRV (edition 2024), so the manifest bootstraps the pinned toolchain via rustup in a `rust-toolchain` part.
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

- `.github/workflows/ci.yml` — split `Lint`, `Non-widget Tests`, `Widget Tests`, `Bench Compile`, and `Dependency Policy` jobs. The Fedora 44 container jobs cover rustfmt, Clippy, the rustdoc lint gate, non-widget tests, widget tests, and benchmark compilation; widget tests run through `scripts/run-widget-tests.sh --headless --retries 1`, which wraps the same `mutter --headless` Wayland path GNOME GTK CI uses while filtering known-benign headless-session noise. The runner defaults to `GSK_RENDERER=cairo` so headless containers do not emit Mesa/EGL GPU-probe warnings, but callers may override the renderer for explicit renderer debugging. Two retry layers serve different failures: the custom harness in `crates/lushtext/tests/widget.rs` retries each **test** once in a fresh process and reports a recovered transient loudly as `ok (FLAKY: passed on attempt N)` plus a stderr `FLAKY:` warning, while `--retries 1` reruns the **whole suite** in a brand-new Mutter + dbus session. Both nets exist to keep CI moving and to make flakes visible, not to excuse them — a `FLAKY` line is a blocker to investigate per `preexisting-blockers.md`, not accepted noise. Shared widget wait helpers (`wait_until`/`flush_events`/`flush_after_delay`/`present_window`) live once in `tests/widget/common.rs`; `wait_until` polls and drains all ready main-loop sources (which is required to dispatch `spawn_blocking_then`'s low-priority idle completion), and async/realization waits use generous (≥5–10s) budgets so they do not flake under load. The `Dependency Policy` job runs `cargo deny check advisories bans sources`.
- `.github/workflows/ci.yml` also has a separate `Property Tests` job that runs `make test-prop` with the `property-tests` feature enabled. Keep that lane separate from the default non-widget and mutation jobs.
- `.github/workflows/end-user-smoke.yml` — scheduled/manual artifact workflow for host-sensitive visual, portal/sandbox, accessibility, and performance-smoke lanes plus a full benchmark report. Keep it outside required PR checks unless a future slice proves one lane is cheap and stable enough to promote.
- `.github/workflows/flatpak.yml` — Flatpak build via `flatpak-github-actions` in `ghcr.io/flathub-infra/flatpak-github-actions:gnome-50` container (Docker Hub `bilelmoussaoui/` stopped at gnome-47; GNOME 48+ images are on ghcr.io) with cache keys tied to actual Flatpak build inputs rather than commit SHA alone.
- `.github/workflows/release-dry-run.yml` — path-filtered release automation check for release scripts, Flatpak manifests, AppStream metadata, desktop metadata, and cargo vendoring; runs release helper tests, Flathub manifest tests, Cominotti repository metadata tests, a no-mutation release preview, and current metadata validation.
- `.github/workflows/release.yml` — `v*` tag release validation and manual dry-run workflow. It validates release metadata, builds the Flatpak from the release source, prepares/deploys Cominotti Flatpak repository artifacts when signing and deploy configuration are available, creates or updates the GitHub Release context, and opens an optional Flathub manifest PR when `FLATHUB_TOKEN` and `FLATHUB_REPOSITORY` are configured.
- `.github/workflows/release-benchmark.yml` — full benchmark run + markdown report uploaded as release asset on `v*` tags, same `fedora:44` container
- `.github/workflows/snap.yml` — always-on `validate` job runs `snapcraft expand-extensions` (structural/extension validation only; a full build cannot succeed until the GNOME 50 platform snap exists). The `build-publish` job (`snapcore/action-build` + `snapcore/action-publish`, release `edge`) is gated behind the `SNAP_PLATFORM_AVAILABLE` repository variable so the missing platform never reds the pipeline; it uses the `SNAPCRAFT_STORE_CREDENTIALS` secret.

**When bumping gtk-rs version:** update the Fedora version in ci.yml and release-benchmark.yml, and the GNOME tag in flatpak.yml and the Flatpak manifest, to match the new minimum GTK requirement.
