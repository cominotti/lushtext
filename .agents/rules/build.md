---
description: Build system, CI, packaging and lane rules
paths:
  - "Cargo.toml"
  - "Cargo.lock"
  - "*/Cargo.toml"
  - "crates/**/Cargo.toml"
  - "crates/gtk-lush/**"
  - "crates/gtk-lush-adoption-lab/**"
  - "crates/cargo-gtk-proof/**"
  - "**/benches/**"
  - "**/kani_proofs.rs"
  - "Makefile"
  - ".cargo/**"
  - ".config/**"
  - ".github/workflows/**"
  - "**/build.rs"
  - "meson.build"
  - "**/meson.build"
  - "meson_options.txt"
  - "build-aux/**"
  - "scripts/**"
  - "snap/**"
  - "fuzz/**"
  - "sonar-project.properties"
---

# Build Rules

Rules that every Rust edit needs (test isolation, temporary files, proof
obligations, gate order, Kani and axiom authoring, runtime warnings) are in
[`testing.md`](./testing.md). How agents run heavy work and measurements on
the shared workstation, and which verification runs a change makes, is in
[`verification-runs.md`](./verification-runs.md).

## Dev Builds

Use `make` targets for development; `make help` prints the catalogue. The
Makefile auto-detects nextest for non-widget tests across the workspace;
`.config/nextest.toml` excludes the `widget` binary (and `binary(=axiom_probes)`)
from nextest's default filter, while widget coverage in `make test` flows
through the shared headless `scripts/run-widget-tests.sh` path so local
verification matches CI.

Before GTK starts, the widget harness applies the **five** settings returned by
`gtk_lush_proof_harness::recommended_pre_gtk_environment()` (including
`NO_AT_BRIDGE=1` and `GTK_IM_MODULE=gtk-im-context-simple`): the private
compositor advertises desktop services it cannot provide, so the client half is
not started against it. The input-method entry is a crash guard, not a claim
that the underlying GTK race is fixed — see
`crates/gtk-lush/proof-harness/README.md` and
`docs/next/persistent-format-hardening.md` S7B-6.

**Validation gates.** These must stay aligned between the Makefile, the
pre-commit hook, CI, and contributor docs:

- rustfmt and the blocking Clippy command `cargo clippy --workspace
  --all-targets --all-features -- -D warnings`, identical everywhere unless a
  narrower command is documented as a non-blocking smoke shortcut;
- the fast policy audits, `make check-policy` (filesystem boundary, Blueprint
  drift and template contract, workflow boundaries, accessibility and visual
  proof policy, terminology, automation docs, the CI job timeouts
  (`check-workflow-timeouts`), the CI Flatpak build mechanism
  (`check-flatpak-ci-builds`), and more);
- the rustdoc lint gate (run by hand; [`testing.md`](./testing.md));
- `cargo deny check advisories bans sources licenses`;
- `openspec validate --all --strict` for OpenSpec changes and specs.

`make check` runs rustfmt (`make check-fmt`, which also covers the widget test
modules), all-feature Clippy and `make check-policy`; `make pre-commit` is the
same gate. The repo-managed Git hooks live under `.githooks/`; install them
with `make install-git-hooks`, which sets `core.hooksPath` for this checkout.
The pre-commit hook runs `make pre-commit`, except for a **docs-only commit**:
when `scripts/precommit-scope.py docs-only` finds that the commit is not a merge
and every staged path (both sides of a rename, and deletions) is under `docs/`,
`openspec/`, `.claude/` or `.agents/`, or is a `*.md` at the repository root, the
hook runs `make pre-commit-docs` (rustfmt and `make check-policy`) and skips
Clippy, which no such path can affect. Anything else, including an empty commit,
a merge, `*.md` under `crates/`, and any git error, runs the full gate, so the
skip fails closed. `make check-precommit-scope` (in `make check-policy`) keeps
the allowlist honest: it fails when a compile-time input of a Rust source
resolves to an allowlisted path (an `include_str!`/`include_bytes!`/`include!`
argument or a `#[path]` attribute, resolved against its file, or a path a
`build.rs` names; an include it cannot resolve fails when it names one). Runtime
reads of documentation stay allowed. `make pre-commit` and `make check` always
include Clippy. A docs-only commit still runs `check-policy`, which builds two
crates cold in a fresh worktree, so commit through `scripts/heavy.sh` either way.

`make check-agent-docs` verifies the `services::filesystem` guidance in rules
and skills, runs the raw filesystem no-leftovers audit, self-tests rename-safe
Cargo/release/test topology discovery and the agent-skill validator, and
validates every rule's scope and every maintained skill's metadata, links and
structure. `.agents/skills/skill-policy.toml` is the single source of truth for
maintained skill exclusions, implicit invocation, performance umbrella/leaf
roles, filesystem-contract consumers, and required release workflow roles.
Cargo packages expose move-safe review roots through
`[package.metadata.lushtext-agent]`; keep those roots beside the package they
describe and update them in the same change as an internal layout move.
Required release workflows carry an `agent-release-role` marker, and
`scripts/agent-topology.py validate` rejects missing role roots, missing or
duplicate required workflow owners, and required workflows that no longer
trigger on `v*` tags. Do not duplicate these registries in shell or Python
allowlists.

`make check-workflow-boundaries` (in `make check-policy`) enforces the
mechanical half of the workflow readability convention; the rules it checks
are normative in [`workflow-convention.md`](./workflow-convention.md), and the
script's own docstring lists them. `docs/workflow-readability-matrix.md` is the
completion source of truth, and the ordered procedure is the
`lushtext-workflow` skill.

`make check-terminology` (in `make check-policy`) runs the
`workspace_terminology` integration test, which scans `AGENTS.md`, `README.md`,
`data/`, `docs/`, `openspec/specs/`, `resources/ui/`, and the crate sources for
the retired folder-root and note vocabulary, allowing it only inside the named
sidecar compatibility constants and fixtures. Keep it in the local gate: it is
the only gate that reads the AppStream release notes `scripts/release.sh`
inserts ([why](../../docs/memorials/release-terminology-guard.md)). Never quote
the retired vocabulary in `docs/`.

`make builder-diagnostics-smoke` runs runtime GtkBuilder diagnostics
(`GTK_DEBUG=builder,builder-objects`, standalone templates where possible,
manifest-backed widget probes under headless Mutter; artifacts under
`build/smoke/builder-diagnostics`). Local runs choose `auto`, `host`, or
`container` through `LUSHTEXT_BUILDER_DIAGNOSTICS_PROVIDER`; scheduled CI uses
the debug GTK image from `.github/workflows/builder-diagnostics-runtime.yml`. A
local unsupported-host skip is evidence about the host only, not template
coverage.

Run `make lint-advisory` after Rust/Clippy toolchain updates or lint-policy
reviews. It fails if a new category appears without a classification in
`scripts/lint-advisory-policy.toml`. Rules with `max_count = 0`, including
`clippy::debug_assert_with_mut_call`, document release-safety invariants and
must stay zero across every target: fix the call site instead of adding an
allow/expect or widening its path scope.

`make dev-tools` sets up a fresh checkout for deep GTK debugging or template
editing (Flatpak runtime/SDK through `flatpak-deps`, headless screenshot and
input tooling, AT-SPI bindings, `blueprint-compiler`). It must remain idempotent
and must not pollute the host: no shell-wide exports, no pip installs, no
dotfile edits, and no automatic rpm-ostree host layering (that requires
`LUSHTEXT_DEV_TOOLS_ALLOW_RPM_OSTREE=1`).

**Live desktop runs** ([how the launcher works](../../docs/runbooks/dev-desktop-staging.md)):

- `make run` must fail rather than activate stale code when an existing
  `dev.cominotti.lushtext` owner refuses to quit, must quarantine a stale
  same-ID non-Flatpak `~/.local/share/applications/dev.cominotti.lushtext.desktop`
  shadow, and must refresh the applications desktop database after staging or
  restoring its desktop file.
- All dev staging, temporary or persistent, uses a non-production ID such as
  `dev.cominotti.lushtext.Devel`; a same-ID non-Flatpak desktop file shadows
  the Flatpak export and makes GNOME Settings treat LushText as unsandboxed.
- After Flatpak or desktop-entry work, run `make flatpak-install` immediately
  before `make verify-flatpak-identity`, so the verifier cannot pass against an
  older installed build. Every checkout installs through the same shared
  repository (Flatpak section below), so the install origin survives deleting
  the worktree. If the task does not authorize modifying the user's
  Flatpak installation, omit installed-state verification and report only
  build-time evidence.
- `make clear-lushtext-xdg` is destructive: it refuses to delete when stdin is
  not a TTY unless `ASSUME_YES=1` is given. Never invoke
  `scripts/clear-lushtext-xdg.sh` speculatively to discover its usage; use
  `DRY_RUN=1` to preview.
- For GNOME Settings File Types work, the desktop entry's explicit `MimeType=`
  line is the allowlist source of truth; inherited `gio mime` output is never
  proof that the allowlist is wrong.
- After icon asset changes, run `make refresh-dock-icon`, which regenerates the
  fixed-size PNG fallbacks from the canonical SVG and replaces the running dev
  instance.

### Build Cache Root

Lanes that own their build state keep it outside the snapshotted checkout, in
the repository's **build cache root**, which only `scripts/build-cache.sh`
computes: `${LUSHTEXT_BUILD_CACHE:-${XDG_CACHE_HOME:-$HOME/.cache}/lushtext-build}`.
Each checkout gets `checkouts/<basename>-<sha256(realpath)[:16]>` there (with
a `CACHEDIR.TAG` and a record of its real path; `checkout-path` resolves it
without creating it, for the Makefile's `BUILD_CACHE_CHECKOUT` and
`proof-strength.py`'s import), and the mutation slots live in `mutants/`.
`make build-cache-prune` removes the directories of checkouts that no longer
exist and nothing else. `make check-build-cache-helpers` (in
`make check-policy`) self-tests the helpers and audits the Makefile, `scripts/`
and `.githooks/`: nothing exports `CARGO_INCREMENTAL=1`, the widget, test,
Clippy and dev-build targets (and `run-widget-tests.sh`, `widget-shards.py`)
never set `CARGO_INCREMENTAL` (they share the checkout's build dir, where a
toggle recompiles every member), and no other script names a build path under
the home cache. Ordinary builds keep CST's per-checkout
`build.build-dir` and `target/` as they are; their final artifacts are reflinks
of build-dir extents.

## Compilation Speed

These patterns are replicated from invowk-rust and must be maintained:

1. **Profiles** in workspace `Cargo.toml` — do not change without benchmarking.
2. **rust-lld** — default linker on x86_64-linux since Rust 1.90. No manual linker override needed.
3. **cargo-hakari** — run `cargo hakari generate` after any dependency change.
4. **.config/nextest.toml** — configure nextest parallelism for non-widget tests here. CI pins one cargo-nextest version, `CARGO_NEXTEST_VERSION` (currently **0.9.151**), in **every** workflow that installs it (`ci.yml` and `mutation-testing.yml`; they must agree). The floor is **0.9.143**: the first release that resolves dylib and artifact paths under Cargo's build-directory layout v2 and `build.build-dir`, which a cargo newer than the pin writes. The local install is upgraded separately, with `cargo install --locked cargo-nextest --version <pin>`; cargo 1.98 still writes layout v1, so an older local nextest keeps working until the pin moves.
5. **`rust-version`** — keep `rust-version = "1.98.0"` in `[workspace.package]`, inherited by every package, so `cargo check` surfaces MSRV violations early. `rust-toolchain.toml` (and every CI toolchain step and the Snap bootstrap) pins **1.98.0**, the compiler the shipped Flatpak's SDK extension ships (`org.freedesktop.Sdk.Extension.rust-stable//26.08`, the branch `org.gnome.Sdk//51` declares), because the binary we test and measure must be built by the same compiler as the binary we ship. Before changing the pin, check what that extension ships: `flatpak remote-info --user flathub-user runtime/org.freedesktop.Sdk.Extension.rust-stable/x86_64/26.08`. GTK Lush crates also have the `make gtk-lush-msrv` lane so their independently adoptable surface cannot drift ahead of the workspace floor.

   **Version policy.** Some releases are never pinned, whatever the extension ships:
   - **Never Rust 1.99.0.** It miscompiles FFI `bool` returns on x86_64 (rust#163911, fixed in 1.99.1). LushText has no `extern "C" fn … -> bool` today (gtk-rs passes `gboolean`, an `i32`), but the miscompile is silent, so no build of the shipped binary may use that compiler. **1.99.1** is the floor of any 1.99 pin. If Flathub moves `rust-stable//26.08` to 1.99.0, the pin does **not** follow: LushText stays on the current pin and waits for an extension that ships 1.99.1 or later.
   - **The shipped 1.98.0 lacks the 1.98.1 fix** for a vtable miscompile reported against a boxed `async` service (rust#161441). LushText's exposure is low, because it has no `async` services; a change that introduces one while the pin is 1.98.0 must re-check that issue first.

   **Exceptions to "every CI toolchain step pins 1.98.0".** The nightly lanes (Kani's own nightly, fuzz smoke, the GTK Lush public-API snapshot) and the weekly **advisory next-stable lane**, `.github/workflows/next-stable.yml`. That lane runs `cargo +stable clippy --workspace --all-targets --all-features --locked -- -D warnings` on the latest stable toolchain, weekly and on manual dispatch. It prints the version it tested, and it names `+stable` explicitly, because otherwise `rust-toolchain.toml` would pin it back to 1.98.0. It never runs on pull requests or pushes and is not a required check. Its failures are advisory: whoever reads one fixes the churn with APIs available at the 1.98 MSRV (as `rust-1-100-adoption` did for Clippy 1.100's `map_unwrap_or` sites) or records why not.

## SonarQube Cloud Scope

`sonar-project.properties` is a maintained CI contract. Any change that adds,
removes, renames, or materially changes a source-bearing directory, workflow
family, script family, build/packaging surface, generated-output directory,
local cache/artifact root, or intentional Sonar suppression must review and
update `sonar.sources`, `sonar.exclusions`, and any
`sonar.issue.ignore.multicriteria` entry in the same change.

Keep `sonar.sources` explicit and code-focused: Rust crates, scripts,
workflows, packaging/build inputs, resources, and root build/configuration
files. Do not let broad repo-root scanning pull in docs, agent guidance,
archived OpenSpec changes, local worktrees, proof/smoke artifacts, generated
vendoring data, or build output.

When the first scan after a scope or suppression change reports new issues,
triage them before marking the Sonar work complete: fix actionable findings, or
add a documented rule-and-resource-scoped suppression with a project-specific
rationale. Blanket ignores without a concrete first-scan finding are not
allowed.

`make sonar-local` (`scripts/sonar-local.sh`) is also the CI verifier after the
scan upload. On a pull request it checks that pull request's analysis:
`.github/workflows/sonarqube.yml` passes `SONAR_PULL_REQUEST` and
`SONAR_EXPECTED_REVISION` as the PR **head** SHA, because SonarQube Cloud
records a pull-request analysis against its head commit, never the merge ref
`github.sha` names. Branch runs keep `github.sha`. Locally,
`SONAR_PULL_REQUEST=<n> make sonar-local` reads one PR. When
`api/project_pull_requests/list` omits an analyzed PR, the verifier falls back
to the latest successful REPORT task's scanner context, which is why the scan
step passes `sonar.scm.revision` explicitly.

## Adding Dependencies

1. Add to `[workspace.dependencies]` in root `Cargo.toml`.
2. Reference with `{ workspace = true }` in crate `Cargo.toml`.
3. Run `cargo hakari generate` to update workspace-hack.
4. Verify gtk-rs version alignment if adding any gtk/glib/gio/pango crate (`rust.md`, "Version Alignment").
5. Run `make cargo-sources` to regenerate `build-aux/cargo-sources.json` for Flatpak.

Adding a new internal workspace crate still requires `cargo hakari generate`
and `Cargo.lock` review. Run `make cargo-sources` whenever the lockfile or
external dependency set changes in a way that affects Flatpak vendoring; if
the generated `build-aux/cargo-sources.json` stays byte-identical, note that
in the verification summary.

### Dependency cooldown

Refresh the lockfile with **`make update-deps-cooldown`**, never a bare
`cargo update`. It runs five steps and stops at the first failure:

1. `cargo +1.100.0 update` with `CARGO_REGISTRY_GLOBAL_MIN_PUBLISH_AGE="7 days"`
   (no release younger than a week is chosen) and
   `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback` (the workspace's
   `resolver = "2"` is not MSRV-aware, so the fallback to versions whose
   `rust-version` the 1.98.0 MSRV satisfies is asked for explicitly);
2. `cargo hakari generate`;
3. `make cargo-sources`;
4. `cargo +1.98.0 check --workspace --all-targets --all-features --locked`,
   which proves the refreshed graph builds with the pinned compiler;
5. the whole dependency policy, `cargo deny check advisories bans sources licenses`.

It fails with a clear message when the 1.100.0 toolchain is missing.

- **Environment variables only.** The cooldown lives in the target's
  environment, never in `.cargo/config.toml`: cargo 1.98 warns on the unknown
  `registry.global-min-publish-age` key on every command. `cargo install`
  ignores the cooldown, so a tool installed that way is not covered by it.
- **Exceptions.** Cargo has no per-package override, and
  `CARGO_RESOLVER_INCOMPATIBLE_PUBLISH_AGE=allow` drops the cooldown for every
  crate, so it is never used. A release needed before its week is up is taken
  **after** the cooldown run, without the cooldown, with
  `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +1.100.0 update -p <crate> --precise <version>`,
  followed by steps 2–5. Record the exception here with its reason, versions,
  command and expiry (publish time plus seven days). Until it expires,
  `make update-deps-cooldown` fails at step 1 with Cargo's "too new" message,
  which is accepted and expected.
- **Duplicate majors.** `multiple-versions = "deny"` stays. A duplicate that no
  LushText manifest can unify (an upstream crate moved to a new major while
  others still use the old one) gets a version-scoped `skip` entry in
  `deny.toml` (`{ crate = "<name>@<major>", reason = … }`) whose reason names
  the lagging crates and the condition for removing it. A `--precise` hold on
  the newer side is not used for this: the next cooldown run undoes it and it
  keeps upstream fixes out.
- **New licence, advisory or source.** A refresh that fails one of these
  stops; it is never waved through. A new licence is added to
  `[licenses] allow` only if it is compatible with distributing a
  GPL-3.0-or-later application, with a `deny.toml` comment naming the crate
  that brought it; otherwise that crate is held with `--precise` and the hold
  recorded. An advisory is fixed by a newer release or a `--precise` hold; an
  `[advisories] ignore` entry needs a reason and a removal condition. Every
  source stays crates.io. (`zlib-rs`, licence `Zlib`, is in `Cargo.lock` but
  outside cargo-deny's graph for the configured targets; `cargo-sources.json`
  still vendors it, which is accepted: `Zlib` is GPL-compatible, and a
  vendored but unbuilt crate is not distributed in the binary.)
- **Side lockfiles.** `fuzz/Cargo.lock` depends on `lushtext-core` by path and
  inherits the workspace's manifest floors, so it is refreshed with step 1's
  command in `fuzz/` in the same change (before any floor moves past what the
  cooldown allows), checked with
  `cargo +1.100.0 metadata --locked --format-version 1 --manifest-path fuzz/Cargo.toml`.
  `fixtures/gtk-lush-adoption/stock-settle/Cargo.lock` is refreshed the same
  way. `formal/evaluation/stateright/Cargo.lock` is disposable, local-only
  evaluation tooling and is left alone.

**Current exceptions:**

- **sourceview5 / sourceview5-sys 0.11.3** (`rust-1-100-adoption`, 2026-11-14).
  Published 2026-11-14 08:57 UTC; the cooldown held them at 0.11.2. Reason:
  0.11.3 contains LushText's upstream fix (sourceview5-rs#19, !42), so
  `SearchContext::replace_all` returns `Result<u32, glib::Error>` and no longer
  panics when nothing matches, which retires the one `unsafe` FFI block in
  `ui/search_bar/execution.rs`; the workspace floor is therefore `"0.11.3"`.
  Applied after the cooldown run, in the root workspace and in `fuzz/`:
  `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo +1.100.0 update -p sourceview5 -p sourceview5-sys --precise 0.11.3`.
  **Expires 2026-11-21 08:57 UTC.** Until then `make update-deps-cooldown`
  fails at step 1 with `failed to select a version for the requirement
  sourceview5 = "^0.11.3"` / `version 0.11.3 is too new (published … ago,
  minimum age 7 days)`, and leaves `Cargo.lock` untouched. After the expiry
  the target runs normally; delete this entry then.

The same refresh found that sourceview5 0.11.2 made `View` implement
`gtk::Accessible` only under a `gtk_v4_*` feature (0.11.0 did unconditionally),
so the workspace dependency enables `gtk_v4_18`; see the comment in the root
`Cargo.toml`.

## GTK Lush Family Foundation

GTK Lush family crates live under `crates/gtk-lush/<member>` and must stay
independently adoptable leaf crates. They are current in-tree LushText platform
APIs; use the family lanes while working on them: `make check-gtk-lush-policy`,
`make check-gtk-lush-adoption`, `make gtk-lush-adoption-lab`,
`make gtk-lush-stock-fixtures`, `make gtk-lush-adoption-matrix`,
`make gtk-lush-doctests`, `make gtk-lush-examples`, `make gtk-lush-msrv`, and
`make gtk-lush-api-advisory`.

`make check-gtk-lush-policy` (in `make check-policy`) fails on missing family
scaffolding, missing MIT/Apache license metadata, missing SPDX headers,
LushText crate dependencies, or runtime dependencies between GTK Lush family
crates. `make gtk-lush-adoption-matrix` (also in `make check-policy`) fails if
the adoption lab, stock fixtures, matrix, evidence documents, ignored artifact
roots, or one-crate fixture dependency contracts drift. `gtk-lush-doctests` and
`gtk-lush-examples` cover what `nextest` does not exercise.

`gtk-lush-api-advisory` runs `cargo-semver-checks` and `cargo-public-api`
snapshots in advisory mode while GTK Lush remains an internal platform. A
missing published baseline may produce advisory output, but missing tools,
command rot, or missing/empty public API snapshots are infrastructure failures,
and the lane must leave inspectable public API snapshots. Once a crate
publishes, its semver/public-API job must become blocking for that crate.

CI installs `cargo-public-api` through `scripts/install-cargo-public-api.sh`,
not `cargo install --locked cargo-public-api`: 0.52.0 (the newest release)
locks `openssl-sys` 0.9.88, which refuses the Fedora 45 image's OpenSSL 4. The
script downloads the crate, checks it against `CARGO_PUBLIC_API_SHA256`, raises
exactly one lockfile entry, `openssl-sys`, to
`CARGO_PUBLIC_API_OPENSSL_SYS_VERSION`, refuses any other lock change, and
installs with `--locked` from the crate. The job also installs
`libcurl-devel`, so `curl-sys` links the image's libcurl: its bundled curl
8.10.1 does not compile against OpenSSL 4, and either half alone fails.
**Retirement:** drop the override (and `libcurl-devel`, if nothing else needs
it) once a release locks `openssl-sys >= 0.9.114` and a `curl-sys` that builds
there. The script enforces it: it fails when the crate's own lock already pins
`openssl-sys` at or above the override, so the first such version bump is red
until the override is removed. `make check-cargo-public-api-install` (in
`make check-policy`, and a step of the advisory job itself, since CI runs no
`check-policy`) self-tests the script offline, with a fake crate and a fake
`cargo`. To reproduce the CI install, run the script in a `fedora:45`
container with the three variables from `ci.yml`.
Governance and publication gates are in `crates/gtk-lush/GOVERNANCE.md`.

## Property Testing

- Framework: `proptest`, wired only for the `lushtext-core` property-test target
- Feature: `lushtext-core/property-tests` (implies `test-utils`, because the suite seeds bodies through the test-only fixture modules)
- Target: `cargo nextest run -p lushtext-core --features property-tests --test properties --profile property`
- Makefile targets: `test-prop` (64 cases per property, the CI-safe default), `test-prop-deep` (`PROPTEST_DEEP_CASES=1024` for a manual or scheduled pass)
- Regression file: `crates/lushtext-core/proptest-regressions/properties.txt`
- Docs: `docs/property-testing.md`

The property target is guarded by `required-features = ["property-tests"]` and
must stay outside default non-widget nextest and default mutation runs. Do not
raise the default case count to investigate one broad invariant; tighten the
generator or use the deep lane. Use it for pure deterministic invariants over
bounded generated data and tiny deterministic tempdir-backed service fixtures,
never for GTK widget construction, compositor behavior, D-Bus/portal state,
file chooser flows, watcher timing, or live session behavior.

**Fixture gating.** Both fixture modules, `services::filesystem::fixture` and
`services::draft_service::fixture`, compile only under
`#[cfg(any(test, feature = "test-utils"))]`. `test-utils` is never a default
feature, and no shipping build (the `lushtext` `[dependencies]`,
`build-aux/cargo.sh`, the Flatpak manifest, Snap) enables it, directly or
through a feature that implies it (the check resolves those from the
manifests). The raw backend operations only the filesystem fixture uses live in
one gated `sys::fixture_ops` module. `make check-filesystem-boundary` enforces
all of that, and CI's lint job runs `cargo check -p lushtext --bins --locked`
with default features, the build that fails if production names a fixture.
Unit tests get the fixtures from `cfg(test)`, the `lushtext` tests from their
dev-dependency feature, and every benchmark command passes
`--features test-utils`.

Failure-path fixtures use per-invocation fault seams, never ambient Unix
permissions ([`testing.md`](./testing.md)); see the Replace/Undo
after-metadata hook registry for a seam keyed by operation target that returns
cleanup ownership from registration.

## Formal Verification (Kani)

Which shards a change runs, and when, is decided by
[`verification-runs.md`](./verification-runs.md); harness placement and style
are in [`testing.md`](./testing.md). This section covers the lane.

- **Tool.** Kani is the programme's **only** formal tool
  (`docs/next/formal-verification.md` §2). Do not add a second specification
  language without a recorded maintainer decision. The disposable Quint vs
  TLA+ and Verus evaluation models in `formal/evaluation/` run only through
  the local-only `make formal-evaluation` (`FORMAL_EVAL_TARGET=verus-*` for
  Verus, run directly, never inside one `heavy.sh`: the script wraps each
  check in its own `heavy.sh` call), which is never a prerequisite of `check`, `check-policy`,
  `test`, `kani`, or `end-user-smoke`. `formal/evaluation/verus/proofs` and
  `formal/evaluation/verus/drift` are standalone packages in the root
  `Cargo.toml` `exclude`, built with Verus's pinned rustc and with the
  workspace toolchain respectively.
- **Pin.** `KANI_VERSION` in the Makefile, currently `0.68.0` (CBMC 6.11.0), is
  the only copy: `.github/workflows/kani.yml`'s `shards` job derives it from
  the Makefile. Kani pins its own nightly, independent of
  `rust-toolchain.toml`; upgrades are deliberate. Install with
  `cargo install --locked kani-verifier --version 0.68.0 && cargo kani setup`.
  `cfg(kani)` is a declared expected cfg, so ordinary builds, Clippy and tests
  compile none of the harnesses.
- **Lane.** `make kani` runs every harness, shard by shard, through
  `scripts/kani-shards.py` with its own `KANI_TARGET_DIR` (`target/kani`), and
  fails when the installed Kani is not the pinned version or any harness
  fails; `make kani KANI_SHARD=<shard>` runs one shard
  (`scripts/kani-shards.py list`). Kani cannot run two jobs on one target
  directory. It runs with `CARGO_INCREMENTAL=0`: Kani's units sit in the
  build dir's own target-triple tree, which no other build shares, and an
  incremental cache there (22 MB per `gtk-lush-widgets` compile, 355 MB per
  `lushtext-core` one) bought a 12% faster rebuild (design D3 of
  `reduce-build-disk-writes`).
- **Memory.** The run executes inside a memory-capped systemd user scope
  (`scripts/with-memory-cap.sh`, `KANI_MEMORY_MAX`, default `16G`, no swap,
  each process held to 7/8 of the cap in address space), admitted into the
  host-wide `user-lushtext-heavy.slice`; the wrapper's header documents
  admission, the desktop reserve, nested in-place runs and the scope settings.
  The `user-` prefix nests that slice under the user manager's `user.slice`
  (`user.slice/user-lushtext.slice/user-lushtext-heavy.slice`), so heavy work
  inherits the toolboxes' CPU policy from the host's CPU tooling (CST: its
  cpuset, `CPUWeight=30`, and the idle class on CPUs 12-23 during a game); the
  wrapper sets no CPU property. Every scope checks its own placement before its
  command runs (under both slices, affinity within the inner `user.slice`'s
  effective cpuset) and exits 78 otherwise. Admission also counts the legacy
  `lushtext-heavy.slice`, where copies of the wrapper from before the rename
  still start scopes, and never touches it. One risk is recorded, not changed:
  systemd-oomd's per-slice pressure kill (80% for 20 s) on the inner
  `user.slice` now sees our scopes' pressure, and its candidates include the
  toolbox containers; a thrashing heavy scope is normally the one killed, and
  each scope's no-swap cap makes the kernel kill inside it first, so an oomd
  kill of a toolbox is traced to here. Any command
  run under the wrapper sizes its own parallelism to fit the cap (`cargo -j 4`,
  at most 4 test threads), one capped heavy run per agent at a time. A run that
  hit the cap is discarded, and hitting it is a finding to fix by the fit
  order, never a reason to raise it; do not retry a harness that hits the
  limit, shrink its bounds first. Locally the wrapper refuses to run uncapped
  when no user manager exists (`KANI_MEMORY_MAX=none` opts out explicitly); CI
  containers run uncapped under the shard budget.
- **Shards.** Every harness belongs to exactly one shard, and
  `make check-kani-shards` (in `make check-policy`, no Kani needed) fails when
  a harness anywhere under `crates/` matches no shard or several. The
  workflow's `shards` job builds its matrix from
  `scripts/kani-shards.py github-outputs`; CI runs one shard per matrix job in
  `.github/workflows/kani.yml` (Fedora 45 container, `timeout-minutes: 30`).
- **Shard gates and budgets.** Each `SHARDS` record carries a `gate`
  (`pull-request` or `scheduled`) and its measured CI runner budget
  (`ci_minutes`, `ci_peak_gib`, and `measured_in`, the run ids it came from;
  developer-machine timing does not count). Pull requests and pushes to `main`
  run only the `pull-request` shards; the weekly schedule and
  `workflow_dispatch` run every shard. `make check-kani-shards` fails when a
  shard is unmeasured, over 25 minutes, over 12 GiB, or gated `pull-request`
  while over 15 minutes. A record **pending dispatch** may hold a local figure
  only as `PENDING DISPATCH (owed by <change>)`, and the check fails on a
  pending record (`PENDING DISPATCH` or `provisional`) whose owner is not an
  active directory under `openspec/changes/`, so the commit that archives a
  change still owing a figure fails its hook. Record a budget from a
  dispatched run, the larger of two; when a shard breaks a margin, fit it by
  the order in [`docs/runbooks/kani-shards.md`](../../docs/runbooks/kani-shards.md)
  (split, then solver, then a recorded bound reduction). Never raise
  `timeout-minutes` instead. The Kani install is cached on `KANI_VERSION`.
- **Counterexamples** are triaged like a failing test: decide whether the code,
  the stated domain, or the envelope is wrong, and record the outcome in the
  programme record.
- **Proof strength.** `ORACLES` in `scripts/kani-shards.py` maps every
  Kani-checked module (optionally scoped to named functions) to its harnesses,
  cheapest first, with each harness's measured local seconds and a `ci` or
  `local` tier; `NOT_ORACLE_HARNESSES` and `NOT_ORACLE_MODULES` name the
  deliberate exceptions with reasons. `make check-kani-shards` fails when an
  oracle harness is missing, a harness is in no oracle list and not excepted,
  or a harness file imports from a module that is neither, so a new harness is
  added to both tables. `make proof-strength` (`scripts/proof-strength.py`,
  `docs/mutation-testing.md`; `PROOF_STRENGTH_MEMORY_MAX`, default `16G`) runs
  the module's mutants against those harnesses in a disposable worktree,
  `<checkout-cache>/proof-strength/wt` in the build cache root (results stay in
  `target/proof-strength/results`). The Kani arm pins its build dir inside the
  worktree with `CARGO_INCREMENTAL=0`; the tests arm runs `cargo mutants
  --in-place` in the same clean worktree, without `--jobs`, with its own pinned
  build and target dirs, workspace crates out of sccache and the per-test
  address-space limit. A `target/proof-strength/wt` left by an earlier version
  is safe to delete (`./scripts/proof-strength.py clean` only removes the
  current default). It is a measurement lane, not a gate. A survivor is triaged in the programme
  record: a real defect is fixed failing-first; a missing property gets an
  assertion or a harness in the owning shard, re-measured against the shard's
  budget.

## GTK Axioms and Toolkit Updates

The ledger's isolated probes live in the GTK Lush family crate
`gtk-lush-axioms` (`crates/gtk-lush/axioms/`), one sample per probe under its
`examples/`. The probe runner is the `axiom_probes` test of
`gtk-lush-adoption-lab` (family crates may not depend on
`gtk-lush-proof-harness`), so `.config/nextest.toml` excludes
`binary(=axiom_probes)` and `make gtk-lush-adoption-lab` runs `--bins`.

```
make gtk-axioms                          # every probe + every sample --check, private headless Mutter (CI job `GTK Axioms`)
make gtk-axiom-sample AXIOM=A9           # interactive sample on the current desktop, for a person to watch
make gtk-axiom-sample AXIOM=A9 CHECK=1   # that sample's headless verdict (exit 0 holds, 1 violated, 2 fixture invalid)
make gtk-axioms-runtimes                 # LOCAL ONLY: sample checks inside each installed GNOME SDK (50 floor, 51 shipped, nightly); GTK_AXIOMS_ONLY=A28,A79 for a subset
make check-gtk-axioms                    # (in check-policy) ledger, catalogue, probes, samples agree
```

**Slow probes.** The runner and a sample's `--check` skip a probe marked `slow`
unless `GTK_LUSH_AXIOMS_INCLUDE_SLOW=1`, printing `skipped (slow): <id>`. Local
`make gtk-axioms` and `make gtk-axiom-sample` default it to `1`; the
`GTK Axioms` PR jobs run with `0`; the nightly
`.github/workflows/responsiveness-smoke.yml` runs with `1`. A92 is marked slow.
The text probes build GtkTextView, Pango and GtkSourceView fixtures, so the
`GTK Axioms` jobs install `gtksourceview5-devel`.

A sample's `--check` refuses with exit 77 unless `GTK_LUSH_AXIOMS_HEADLESS=1`,
which only the headless wrapper sets. `--help`/`-h` print the usage and exit 0,
and any argument a sample does not know prints the usage and exits 2, both
before GTK starts (`gtk_lush_axioms::classify_sample_args`, `support::guard`).

**Warning scan.** `scripts/run-gtk-axioms.sh` fails the lane on **any**
`WARNING`, `CRITICAL`, `Broken pipe`, `cannot remove`, or `MESA: error:` line
in the probe or sample output, whatever process printed it, except the private
session's own `dbus-daemon` / `libmutter-Message` / EGL lines and the three
mutter warnings its `BENIGN_NOISE_REGEX` names. The runtimes lane runs every
SDK sandbox with `--no-session-bus --no-a11y-bus --no-documents-portal`
(`SANDBOX_FLAGS`). `./scripts/run-gtk-axioms.sh --self-test` (in
`make check-gtk-axioms`) asserts the benign lines pass and each defect class
fails; a new class gets a case there, never a wider benign list
([why](../../docs/memorials/axiom-lane-warning-scan.md)).

**Toolkit updates.** On any GTK, Libadwaita, GtkSourceView, Pango or GLib
version change — the CI Fedora container image, the shipped GNOME runtime in
the Flatpak manifest (51), the GNOME 50 source floor, the Snap `core26`
platform — run `make gtk-axioms` **first**, before any other work. A failing
probe is an axiom change: never adjust a probe to match the new behaviour
first, and when the shipped runtime and the floor disagree, version-qualify the
entry rather than rewrite it (`make check-gtk-axioms` enforces the ledger
form). The steps are in
[`docs/runbooks/gtk-toolkit-update.md`](../../docs/runbooks/gtk-toolkit-update.md).
Interleaved `GTK_AXIOMS_ONLY` re-test rounds run through `scripts/measure.sh`.
A nightly verdict is a preview of the next adoption, never a ledger
restatement on its own.

## Fuzzing

- Framework: `cargo-fuzz`, isolated under `fuzz/`
- Feature: `lushtext-core/fuzzing`
- Targets: `editor_bytes`, `markdown_preprocess`, `operation_script`
- Makefile targets: `fuzz-list`, `fuzz-corpus-replay`, `fuzz-smoke`, `fuzz-operation-smoke`
- Default tool: `cargo +nightly fuzz` (override with `CARGO_FUZZ=...`)
- Docs: `docs/fuzzing.md`

The fuzz project is excluded from the normal Cargo workspace. Default
`make test`, nextest, property, widget, benchmark, and mutation lanes must not
invoke fuzz targets or corpus replay, and must not require fuzz-only
dependencies. CI runs stable corpus replay as its own job through
`make fuzz-corpus-replay`; coverage-guided `cargo-fuzz` smoke stays in
scheduled/manual lanes. Local fuzz smoke needs a nightly toolchain,
`cargo-fuzz`, and a C++ compiler for `libfuzzer-sys`.

Fuzz targets must stay deterministic and must not start GTK, construct widgets,
open file choosers, watch filesystems, use D-Bus/portals, or require a
compositor. `make fuzz-smoke` and `make fuzz-operation-smoke` run with explicit
run, time, and input-length bounds against temporary corpus copies (the
operation-script harness caps scripts at 32 operations); longer runs are manual
or scheduled with explicit budgets. `make fuzz-corpus-replay` must stay
read-only: no corpus mutation, no fuzz artifact or coverage writes, no
`cargo-fuzz`, no `libfuzzer-sys`, no sanitizer flags, no nightly, and no C/C++
compiler.

Do not add LibAFL unless a future OpenSpec change identifies a concrete need.
Minimize real crashes with `cargo +nightly fuzz tmin <target> <crash>` and fix
them with a minimized corpus seed, a deterministic regression test, or a
reviewed rationale for why no durable seed is appropriate.

## GResources

- **Dev builds**: `build.rs` in `lushtext-core` compiles resources via `glib-build-tools`, embedded through `include_bytes!` in `lib.rs`.
- **Installed/Flatpak builds**: `resources/meson.build` compiles resources via `gnome.compile_resources()` and installs them to `$(pkgdatadir)/`. `lib.rs` loads the `.gresource` file from `LUSHTEXT_PKGDATADIR` at runtime, falling back to `include_bytes!`.
- **UI templates**: edit `resources/ui/*.blp`, then run `make blueprint-generate` and `make check-blueprint`. The generated `resources/ui/*.ui` files stay committed because every runtime, Meson, Flatpak, and Snap resource build consumes GtkBuilder XML; `blueprint-compiler` is a contributor and CI tool only. Unknown compile warnings are blocking. `make lint-blueprint` is curated advisory triage: promoted diagnostics stay clean and accepted findings are bounded by rule/file ceilings.

## GSettings Schemas

- Schema XML: `data/dev.cominotti.lushtext.gschema.xml`
- `build.rs` in `lushtext-core` runs `glib-compile-schemas data/` to produce `data/gschemas.compiled` (gitignored), and skips it when `LUSHTEXT_PKGDATADIR` is set (the source tree may be read-only in Flatpak).
- `lib.rs::init_schema_dir()` sets `GSETTINGS_SCHEMA_DIR` to `data/` for dev builds. Installed builds use the system schema directory.
- Requires `glib-compile-schemas` on the build machine (from `glib2-devel` / `libglib2.0-dev`).

## Mutation Testing

- Framework: `cargo-mutants` 27.x, configured in `.cargo/mutants.toml`
- Test runner: `cargo nextest`, matching the non-widget CI lane
- Wrapper: `scripts/run-mutants.sh`
- Makefile targets: `mutants-smoke`, `mutants-diff`, `mutants-full`, `mutants-list`
- Output: `mutants.out` / `mutants.out.old` (gitignored)
- Docs: `docs/mutation-testing.md`

Mutation tests are local-only by default. The GitHub Actions workflow stays as
a re-enable template whose jobs are gated by the repository variable
`LUSHTEXT_ENABLE_MUTATION_CI=true`; leave it unset for normal CI.

The default scope is deterministic and decided by **convention, not
directory**: `examine_globs` covers `model/**`, `services/**`, and
`crates/lushtext-core/src/ui/**/policy.rs`. A `policy.rs` module must contain no
`gtk4`, `glib`, `gio`, `libadwaita`, or `sourceview5` import; that purity is
what makes it eligible. Do not add GTK widget construction, live signal wiring,
file dialogs, or display-server-dependent code to the scope. The remaining
hand-listed UI entries and the `exclude_re` entries naming GTK adapter methods
retire when their workflow's pure logic lands in a `policy.rs`, never by being
widened. When pure policy relocates, mutation-coverage parity is required
evidence: record the generated and killed counts before and after the move via
`make mutants-diff`.

Default mutation runs omit `lushtext-core/property-tests`. For survivor
triage `scripts/run-mutants.sh` accepts `MUTANTS_RE`, `MUTANTS_EXCLUDE` and
`MUTANTS_EXCLUDE_RE`. Keep `MUTANTS_LOCAL_JOBS=1` and small per-job thread caps
when the user has asked to keep Rust build pressure low.

**Local runs go in place in mirror slots** (`MUTANTS_MIRROR=1`, which the
`mutants-smoke|diff|full` targets set). The run syncs the checkout's tracked and
untracked, non-ignored files, uncommitted edits included and `.git` never, into
`MUTANTS_LOCAL_JOBS` (default 3) slots of a host-wide pool
(`<build-cache root>/mutants/slot-<k>`, `MUTANTS_POOL_SIZE`, default 3), and
each slot runs `cargo mutants --in-place` from its `src/` with its own fixed
`target/` and `build/`. Only bytes that differ are rewritten, so a warm slot
rebuilds only the members that changed; symlinks and the executable bit are
mirrored, and a symlink resolving outside the slot is refused. The checkout is
never mutated, and an interrupted mutant heals at the slot's next sync. Copy
mode used to cold-build one tree per job (5.2 GB each) into a new
`~/.cache/cargo-build/<hash>` that nothing deleted.

- **Slots are taken without waiting.** A slot is held by a non-inheritable
  `flock` on its `lock`, with an `owner` record; a stale record whose lock is
  free is replaced and reported. If no slot is free the run **exits 75** and
  names every holder; retry later, or with `MUTANTS_LOCAL_JOBS=1` once one
  slot is free. Waiting would hold `heavy.sh`'s shared lock and starve a
  pending measurement.
- **Parallelism is sharding.** cargo-mutants refuses `--in-place` together
  with any `--jobs` or `CARGO_MUTANTS_JOBS`, so N slots run `--shard i/N` side
  by side and mirror mode never passes a jobs count. A user `MUTANTS_SHARD`
  with more than one slot is refused. Static shards can finish unevenly.
- **One report.** The slots' reports are merged into the checkout's
  `mutants.out` (the previous one rotated to `mutants.out.old`): every slot's
  logs and diffs under `log/slot-<k>/` and `diff/slot-<k>/` (each shard
  restarts the `_001` suffixes and writes its own baseline), counters summed,
  and the exit status as cargo-mutants computes it (a failure status first,
  then 3 for timeouts, then 2 for missed mutants). One slot writes
  `mutants.out` directly.
- **Build state.** Each slot keeps incremental compilation in its own build
  dir (design D3: without it a mutant's build took 2.2x as long, past the 25%
  bar) and passes workspace crates straight to rustc (`RUSTC_WORKSPACE_WRAPPER`, a copy of
  `scripts/rustc-passthrough.sh` at one stable path, because Cargo hashes the
  wrapper's path into every member), so mutated crates never enter the shared
  sccache; dependencies still use it. A slot whose `build/` plus `target/`
  exceeds `MUTANTS_SLOT_MAX_GIB` (default 24, about twice a warm slot) is
  emptied before use.
- **A runaway test is stopped alone.** Every test process runs under
  `RLIMIT_AS` through a Cargo target runner
  (`scripts/test-address-space-runner.sh`, `LUSHTEXT_TEST_ADDRESS_SPACE`,
  default `4G`, derived from the unmutated baselines in
  `openspec/changes/archive/2026-11-15-reduce-build-disk-writes/evidence/address-space.md`); rustc,
  the linker, build scripts, nextest and cargo-mutants are not limited. A
  caught mutant whose log shows an allocation failure is listed in
  `mutants.out/address-space-caught.txt` and counted in the summary: it is
  still caught (the unmutated code passes under the same limit), but triage
  confirms each one the way it confirms any non-assertion kill. Such a run is
  **not** discarded. The whole-run `MUTANTS_MEMORY_MAX` scope (default `36G`,
  with `MUTANTS_LOCAL_BUILD_JOBS=4` and `MUTANTS_LOCAL_TEST_THREADS=4` per slot)
  stays, for builds; a run whose scope was OOM-killed is still discarded,
  because an OOM-killed test reads as caught.
- **Disk.** The pool is bounded by its size, not by the number of worktrees:
  at most 3 x `MUTANTS_SLOT_MAX_GIB` (24 GiB). A warm slot holds about
  10-11 GiB per dependency set, and one past the bound is emptied and built
  cold at its next run. `make build-cache-prune` never touches the slots.

`MUTANTS_IN_PLACE=1` without mirror mode (calling `scripts/run-mutants.sh`
directly) mutates the checkout itself and keeps refusing a dirty worktree
outside CI. The CI modes (`ci-pr`, `ci-full`) and `list` are unchanged: they
run in copy or in-place mode exactly as before and never use slots. If the CI
mutation lane is re-enabled, it leaves the local knobs unset and fans out
through `MUTANTS_SHARD`.

Treat survivors in order: decide whether the mutant is a real missed behavior,
then add or tighten deterministic tests, then consider small refactors that
make the behavior testable. Exclude only equivalent or explicitly out-of-scope
mutants, narrowly enough that nearby behavior still mutates.

## End-User Smoke Lanes

`docs/end-user-coverage.md` is the coverage map for behavior that unit,
integration, property, fuzz, widget, benchmark, and mutation lanes cannot prove
honestly; keep its lane boundaries current when adding smoke checks. Each
script owns its dependency checks, artifact paths (under `build/smoke/<lane>`)
and skip messages; `make end-user-smoke` runs the host-supported lanes together.

- `make automation-smoke`: the app under isolated headless Mutter and a private
  D-Bus session, exercised through the automation object and
  `scripts/lushtext-automation.py`, with a runtime-log warning scan.
- `make automation-client-self-test`: the client's parser, status envelope,
  action-parameter rendering and artifact summaries, without the app; a policy
  gate, not a replacement for real-process smoke.
- `make visual-geometry-smoke`: same-session before/after screenshots and
  bounded geometry snapshots, protected regions compared exactly, with a
  visual-sensitive diff fingerprint.
- `make visual-smoke` and `make accessibility-smoke` (AT-SPI bridge enabled;
  the widget harness sets `NO_AT_BRIDGE=1` and cannot prove this class). Both
  run their failed-load captures on a second binary built with the smoke-only
  `smoke-read-faults` feature (`make smoke-read-faults-build`,
  `target/smoke-read-faults`, `LUSHTEXT_DENY_READ=<fixture>`), because a
  mode-000 fixture is still readable by root. The same binary carries
  `smoke-list-budgets` (`LUSHTEXT_SEARCH_RETAINED_BYTES`,
  `LUSHTEXT_SIDEBAR_DIRECTORY_RETAINED_BYTES`). The accessibility lane's
  warning allowlist lives once in `scripts/accessibility_warning_allowlist.py`;
  edit it there instead of embedding a second predicate copy.
- `make command-palette-notes-smoke` and
  `make run-command-palette-notes-manual-test`: a focused Notes palette drill
  and its live manual launch with isolated XDG directories; not part of
  `make end-user-smoke`.
- `make crash-recovery-smoke`: draft/session recovery through a real
  `SIGKILL` and relaunch, plus a second binary built with the smoke-only
  `crash-kill-points` feature (`target/crash-kill-points`,
  `LUSHTEXT_KILL_AT`) aborted inside named protocol windows.
- `make portal-sandbox-smoke`: Flatpak/Snap runtime state and confined checks.
  It skips explicitly when neither runtime is available; a skip is not proof
  that confinement works, and its diagnostics must not imply a portals-only
  migration while the Flatpak keeps host filesystem access.
- `make performance-smoke`: a small Criterion filter with coarse timing
  artifacts (`PERFORMANCE_SMOKE_GROUP=search-index|io-recovery` runs one half);
  forgiving enough to avoid shared-runner noise, distinct from `bench-report`.
- `make responsiveness-smoke` (`RESPONSIVENESS_TIER=quick|full`): long-line
  responsiveness of a **separate** release binary built with the smoke-only
  `responsiveness-probes` feature into `target/responsiveness/`
  (`make responsiveness-build`), so `target/debug/lushtext` never carries it.
  It generates synthetic fixtures only (`scripts/generate-long-line-fixtures.py`,
  seeded, written into the run's temp scope), runs with an affinity of at most
  16 CPUs taken from the inherited mask (never widened), never writes a
  fixture, and fails on any fixture digest change. Under `scripts/measure.sh`
  it runs on the reference CPU set measure.sh pinned and fails before any case
  when its inherited affinity differs from it or it exceeds the cap (never
  trimmed); it records the set and its source, and labels unpinned runs,
  overrides and mixed merges as not comparable with reference runs. Budget overruns are results,
  not failures. The nightly `quick` tier is trend evidence only; verdicts come
  from a `full` run on the reference machine under the reference
  precondition, through `scripts/measure.sh`, recorded in
  `docs/next/long-line-documents.md`. It is not part of `make end-user-smoke`.
  How to drive it and read its failure classes:
  [`docs/runbooks/responsiveness-lane.md`](../../docs/runbooks/responsiveness-lane.md).
  `make check-responsiveness-lane` (in `make check-policy`) self-tests the
  generator and driver.

The default pull-request path stays bounded: wire only cheap and stable parts
of these lanes into PR CI, and keep screenshot, portal/sandbox, AT-SPI,
installed-package, and deeper performance checks scheduled, manual,
release-only, or opt-in until a change proves one reliable as a blocking gate.
`.github/workflows/end-user-smoke.yml` is the scheduled/manual artifact lane.

## Meson Build (Installed / Flatpak)

Meson wraps Cargo for installed and Flatpak builds:
- Root `meson.build` → `subdir('resources')`, `subdir('data')`, `subdir('po')` → `cargo.sh` → `cargo build`
- `build-aux/cargo.sh` bridges Meson→Cargo, exports `LUSHTEXT_PKGDATADIR` for the GResource/GSettings dual path
- `data/meson.build` installs desktop file, metainfo, icons, GSettings schema
- `gnome.post_install()` compiles schemas, updates icon cache and desktop database
- `make meson-test` builds with Meson and runs the registered desktop/AppStream metadata tests

## Flatpak

- Manifest: `build-aux/dev.cominotti.lushtext.Flatpak.json` (local builds, `type: "dir"`)
- Runtime: `org.gnome.Platform` 51 (GTK 4.24, GtkSourceView 5.22, Libadwaita 1.10), SDK extension `org.freedesktop.Sdk.Extension.rust-stable` (the SDK's `26.08` branch, Rust 1.98.0). The gtk-rs feature set stays at the GNOME 50 source floor, so compiling on 4.24 proves no newer API is used.
- The Rust stable extension must satisfy the workspace MSRV. If a local user installation is stale after an MSRV bump, update it with `flatpak update --user org.freedesktop.Sdk.Extension.rust-stable//26.08` before treating a Flatpak build failure as an application regression.
- `make flatpak` builds without installing; it first ensures the user Flathub remote and manifest runtime/SDK dependencies are available
- `flatpak-builder`'s state dir (it holds the whole release Cargo build) and the app dir live in the checkout's build-cache directory, side by side on one mount because flatpak-builder hardlinks between them: `FLATPAK_STATE_DIR` (`<checkout-cache>/flatpak-builder`, passed as `--state-dir`) and `FLATPAK_BUILD_DIR` (`<checkout-cache>/flatpak-app`). The release validation build uses `<checkout-cache>/flatpak-release-builder` and `flatpak-release-app`; the Cominotti repository generator keeps its state dir beside its app dir under its output dir (`$out_dir/flatpak-builder`), because in CI that dir is on another mount than `$HOME`. A `.flatpak-builder/` or `build-flatpak/` left in the checkout by an earlier version is no longer used and is safe to delete.
- `make flatpak-install` builds and installs the checkout into the user Flatpak installation; idempotent. It exports with `--repo` (never `--install`) into **one shared install repository**, `${XDG_DATA_HOME:-$HOME/.local/share}/lushtext-flatpak-install/repo`, computed only by `scripts/flatpak-install-origin.sh path` (`LUSHTEXT_FLATPAK_INSTALL_REPO` is a test hook, never a per-checkout setting). The installed ref's origin remote `lushtext-origin` takes that repository's `file://` URL, so it must outlive every checkout, worktree and cache: a per-checkout location (the old `.flatpak-builder/cache`, or anything a build-cache prune reclaims) dangles once deleted, `~/.cache` is disposable by XDG definition, and `make clear-lushtext-xdg` deletes `$XDG_CACHE_HOME/lushtext` and `$XDG_DATA_HOME/lushtext`. `scripts/flatpak-install-origin.sh install` then, under `flock` on `<repo>.lock`, repoints and re-enables `lushtext-origin` (flatpak reuses a disabled origin with a matching URL and skips the update with exit 0), installs, prunes the repository to its heads, and fails unless the deployed commit is the one the enabled origin serves; a failed build touches no remote
- `make verify-flatpak-identity` starts with the install-origin guard (`flatpak-install-origin.sh check`): it fails when a `lushtext*-origin` remote, enabled or disabled, points at a path with no OSTree `config` and `summary` (GNOME Software's "server has no summary file"), and prints the one-line fix, `make flatpak-install` from any checkout (or `flatpak uninstall --user dev.cominotti.lushtext//master`). `make test-flatpak-install-origin` (in `make check-policy`) proves the guard and the flatpak behaviour it relies on against real `flatpak` in a private `FLATPAK_USER_DIR`, and prints `SKIP:` without `flatpak`, `ostree` or `flock`
- `make check-flatpak-permissions` after automation, portal/sandbox, or packaging changes proves the manifest still declares the intentional `--filesystem=host` permission
- `make verify-flatpak-identity` after install/export changes catches stale same-ID dev launchers and verifies `X-Flatpak`, permissions, and MIME registration
- `build-aux/cargo-sources.json` vendors all Cargo dependencies for offline builds; regenerate after dependency changes with `make cargo-sources` (requires `flatpak-cargo-generator`). Update chain: `make update-deps-cooldown`, which runs the cooldown `cargo update` → `cargo hakari generate` → `make cargo-sources` → a `cargo +1.98.0` check → `cargo deny` (see "Dependency cooldown" under Adding Dependencies)

## Flatpak Release Automation

The release procedure is the `publish-release` skill. Rules:

- Commands: `make release VERSION=vX.Y.Z RELEASE_NOTES_FILE=...` or `make release-bump TYPE=major|minor|patch`; use `DRY_RUN=1` first. `PRERELEASE=alpha|beta|rc` starts or continues a prerelease stream, and `PROMOTE=1` is required to promote one to stable.
- Real releases run from a clean `main`. The helper stages only intended release files, commits `chore(release): vX.Y.Z`, creates a signed tag, and pushes `main` plus the tag only after validation passes.
- `RELEASE_NOTES_FILE` is mandatory for real releases: the notes become the AppStream release description in `data/dev.cominotti.lushtext.metainfo.xml.in`.
- Version surfaces that move together: `meson.build`, `crates/lushtext/Cargo.toml`, `crates/lushtext-core/Cargo.toml`, `Cargo.lock`, AppStream releases, and `build-aux/cargo-sources.json`.
- The Flatpak publication channel is the Cominotti-owned remote at `https://flatpak.cominotti.dev/` (`make cominotti-flatpak-repo`, then always `make verify-cominotti-flatpak-repo` and `make verify-cominotti-pages-limits`). Cloudflare Pages is the default backend, `COMINOTTI_FLATPAK_DEPLOY_COMMAND` an override, and Cloudflare R2 the first fallback when Pages limits are exceeded.
- Public Cominotti install instructions keep GPG verification enabled; never publish `flatpak remote-add --no-gpg-verify` for the Cominotti remote except in clearly private local-testing notes.
- **Flathub publication is not planned, and this is a standing decision rather than a deferral.** A skipped Flathub PR is expected output: do not report it as an outstanding action, do not treat absent `FLATHUB_TOKEN` / `FLATHUB_REPOSITORY` as configuration to complete, and do not propose work to enable it. The dormant `make flathub-manifest`, `make verify-flathub-manifest` and `make verify-flathub-domain` tooling stays covered by `scripts/test-flathub-manifest.sh`. The Flathub *remote* remains the source of the GNOME runtime and SDK.

## Snap

- Manifest: `snap/snapcraft.yaml`. Strict confinement + `home` / `removable-media` plugs; the `gnome` extension supplies Wayland/X11/GPU/portals. Identity stays `dev.cominotti.lushtext` via `common-id`; the snap NAME (`lushtext`) lives in Snap's flat namespace.
- **Reuses the Meson/Cargo build**: the `meson` plugin drives the same `meson.build` → `cargo.sh` path as the Flatpak. A `layout:` bind-mounts the baked `LUSHTEXT_PKGDATADIR` (`/usr/share/lushtext`) to `$SNAP/usr/share/lushtext`, so `register_resources()` / `init_schema_dir()` work under confinement with no Rust changes.
- No `cargo-sources.json` for Snap: snap builds are online.
- **Rust toolchain**: Ubuntu 24.04 packages `rustc` below the MSRV, so the manifest bootstraps the pinned toolchain via rustup in a `rust-toolchain` part.
- **App schema compile**: meson's `gnome.post_install()` skips `glib-compile-schemas` under DESTDIR staging, so the part compiles `dev.cominotti.lushtext.gschema.xml` explicitly in `override-build`.
- **GATED on the GNOME 50 platform snap**: LushText needs GTK 4.22; the extension currently provides only `gnome-46-2404` (GTK 4.14, `core24`). The `core26` / GNOME 50 platform snap is not published yet, so `make snap` is expected to fail until `base:` is switched to `core26`. Do not vendor the GNOME stack from source or lower the GTK floor to work around this.
- Building locally: `make snap` (LXD build), `make snap-smoke` (confined smoke; skips cleanly until the platform snap exists; fails on AppArmor/seccomp denials), `make verify-snap-identity` (confinement/plugs/common-id), and `make snap-store-readiness` (non-mutating store/platform gate report; exits nonzero while a gate is pending).
- **Release posture**: Unlisted visibility + `edge`-only channel (`snap install lushtext --edge`). Promotion to stable/public is a store action, not a rebuild.

## Benchmarks

- Framework: Criterion.rs (`criterion = "0.8"` with `html_reports`)
- Benchmark file: `crates/lushtext-core/benches/benchmarks.rs` (single file, all groups); all benchmarked code is GTK-free
- Every command passes `--features test-utils`: `cargo bench -p lushtext-core --features test-utils` (the Makefile `bench*` targets, `scripts/bench-report.sh`, `scripts/run-performance-smoke.sh`, and CI's bench compile already do)
- `[profile.bench]`: `opt-level = 3`, `lto = "thin"`, `codegen-units = 1` (no strip — criterion needs symbols)
- `FileIndex::from(Vec<OwnedIndexedFile>)` and the streaming `FileIndex::from_paths` (both `test-utils`) build synthetic indexes; a 2^20-file fixture streams through `from_paths`, never a `Vec` of owned paths
- The 2^20-file groups (`file_index_scale`, `file_index_scale_rebuild`, `palette_pipeline_hardening_1048576`) run only with `LUSHTEXT_BENCH_SCALE=1`, which `make bench` sets: their setup runs before Criterion's filter, so ungated they would build the index and write a million files in every filtered run
- `scripts/bench-report.sh` clears stale Criterion `new/` results, fails closed if `cargo bench` fails, and writes `docs/benchmarks/<timestamp>.md` (requires `jq`; `--scope release` is the bounded release-safe report, `--scope diagnostic` the deeper one, splittable with repeated `--filter`)
- Makefile targets: `bench`, `bench-report` (release-safe), `bench-report-full` (diagnostic), `bench-baseline` (saves "main"), `bench-compare` (lenient: benchmark IDs added after the baseline are measured without aborting)

## CI

All CI jobs use container images, because `ubuntu-latest` ships GTK 4.14. Test,
measurement and lint jobs run `fedora:45` (the shipped GNOME 51 toolkit); only
`GTK Axioms (GNOME 50 floor)` (`gtk-axioms-floor`) runs `fedora:44`, so the
floor's toolkit behaviour stays pinned by the axioms. The Snap change must run
the widget suite on its platform first.

- `.github/workflows/ci.yml` — `Lint` (rustfmt, all-targets/all-features
  Clippy, filesystem-boundary audit, workflow-timeout policy, Blueprint drift
  and contract, the rustdoc gate, and the default-feature
  `cargo check -p lushtext --bins --locked`), `Non-widget Tests`, `GTK Axioms`
  and `GTK Axioms (GNOME 50 floor)` (two jobs rather than a matrix, so the
  shipped-runtime check keeps its name), `Widget Shard Table`,
  `Widget Tests (<shard>)`, `Property Tests` (keep it separate from the
  non-widget and mutation jobs), `Bench Compile`, and `Dependency Policy`
  (`cargo deny check advisories bans sources licenses`). Required checks on
  `main` include `GTK Axioms`, `GTK Axioms (GNOME 50 floor)` and the Kani
  `widgets-geometry`, `core-geometry-policies` and `core-minimap-fit` shards.
- **Widget shards.** `scripts/widget-shards.py` is the single source of truth
  for membership; its docstring documents the parallel local run, logs,
  summary and retries. A test is owned by the shard naming it explicitly, else
  by the shard listing its module; there is no catch-all, so a **new widget
  module file must be added to a shard** or `make check-widget-shards` (in
  `make check-policy`) fails, as it does for a module or name in two shards, a
  stale entry, or a redundant explicit name. Each shard job cross-checks the
  static discovery against the binary's `--list` and the harness's
  `running N tests`, so the shards provably sum to the full suite. Each shard
  records `ci_minutes` and `measured_in` (the largest widget-step wall time
  across at least two CI runs; developer timing never counts), and the check
  fails when a shard is unmeasured or over 20 minutes. When a shard nears the
  margin, move a module or one slow test by name; never raise
  `timeout-minutes` (`scripts/check-workflow-timeouts.py` enforces the hard
  ceiling) and never shrink a threshold-sized test.
- **CI image lane.** A widget failure seen only on CI is run in the CI widget
  job's own image under the runner's CPU budget before it is treated as
  CI-only: `scripts/ci-image-widget-test.sh` reads the `widget-tests` job's
  image and `dnf` line from `ci.yml`, refuses to run when the image's toolkit
  differs from the host's, and runs the binary already built here (build it
  first, `scripts/heavy.sh -- cargo test -p lushtext --test widget --no-run`)
  in a fresh container per run, N times (`--runs`), limited by `--cpus N` (a
  quota; never `taskset`). Each run is CI's own step, with `cargo` shimmed to
  that binary: `scripts/run-widget-tests.sh --headless --retries 1 -- <test
  args>`, or `scripts/widget-shards.py run NAME` for `--shard NAME`, one shard
  per call (`--runs 1`). So the temp scope, the warning scan and both retries
  are CI's; the kernel, cgroup driver and virtualisation are not. Run each
  call as `scripts/heavy.sh --mem none -- scripts/ci-image-widget-test.sh ...`
  and keep single-test batches under about 15 minutes, so a call holds the
  heavy-work lock once. A run is green only with exit 0, no `FLAKY:` or
  `FLAKY-SHARD:` line and, for a shard, exactly the shard's tests selected. A
  failure reproduced there is fixed failing-first against it and proved by 20
  consecutive green lane runs; one seen only inside a full shard runs the
  whole shard there at least five times first. A test run alone in the lane
  sees two-digit process ids in its temporary paths (seven on a busy host), so
  a test whose verdict follows path length shows up there first. Its
  `--self-test` runs in `make check-agent-helpers`; it exits 77 without
  Docker.
- **Retries make flakes loud, not acceptable.** The harness retries a **test**
  once in a fresh process and reports `ok (FLAKY: passed on attempt N)` plus a
  stderr `FLAKY:` line; `--retries 1` reruns a failed **shard** in a new
  session and prints `FLAKY-SHARD:`. Both are blockers per
  `preexisting-blockers.md`. Under `with-memory-cap.sh` a run that OOM-killed
  anything fails with `OOM-KILL:`, never green. Start `make test` and
  `make test-widget` unwrapped (they admit one scope per shard plus one for
  the shared build); `make test-widget-shard WIDGET_SHARD=<shard>` reproduces
  one CI shard. The runner defaults to `GSK_RENDERER=cairo`.
- `.github/workflows/kani.yml` — the Kani lane in measurement mode, one shard
  per matrix job; pull requests and pushes to `main` run only the
  `pull-request` shards (`widgets-geometry`, `core-geometry-policies`,
  `core-minimap-fit`, `core-list-budget`, `core-protected-fold`); schedule and
  dispatch run every shard.
- `.github/workflows/end-user-smoke.yml` — scheduled/manual smoke lanes plus a
  split benchmark-report matrix. Its containers run as root with no system
  D-Bus and no `/dev/fuse`: failed-load fixtures use the `smoke-read-faults`
  binary, missing-bus and FUSE reports are classified once in
  `scripts/smoke_warning_classifiers.py` (self-tested by
  `make check-end-user-smoke-workflow`), and the screenshot pipeline needs
  `pipewire-gstreamer` and `gstreamer1-plugins-good`. Keep it outside required
  PR checks unless a lane is proven cheap and stable.
- `.github/workflows/flatpak.yml` — Flatpak build in the
  `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51` container
  (`--privileged`), with cache keys tied to the Flatpak build inputs; also
  `workflow_dispatch`. It builds through the composite action
  `.github/actions/flatpak-build`, the **only** place that names
  `flatpak/flatpak-github-actions/flatpak-builder@`. The image keeps its GNOME
  SDK and Platform in the system installation `/var/lib/flatpak` on overlayfs;
  when Flathub has newer refs, flatpak's update renames deployment directories,
  an overlay without `redirect_dir` refuses that for lower-layer directories
  (`Invalid cross-device link`, then a flatpak-builder segfault, exit 139).
  Which runners fail varies (PR #98, run `43642169956`: two attempts failed,
  a rerun on another runner image passed); the passing runner's cause
  (`redirect_dir` on, or another snapshotter) is unverified, and the fix does
  not depend on it. So the action
  first runs `scripts/ci-flatpak-installation.py diagnose` (one log group:
  kernel, `redirect_dir`, overlay mounts, `findmnt`/`df` of the installation and
  `$RUNNER_TEMP`, and a `rename(2)` probe of the first runtime's deployed commit
  directory), then `relocate`: `cp -a` of the installation to
  `$RUNNER_TEMP/flatpak-system` and a `mount(2)` bind over `/var/lib/flatpak`
  (the image has no `mount` binary), refused inside the checkout, failing unless
  the re-probe returns `ok`, and logging its wall time and `df -h` (budget 90 s,
  judged from CI logs, never enforced). Shell steps read `$RUNNER_TEMP` and
  `$GITHUB_WORKSPACE` from the environment, never the `${{ }}` contexts.
  `make check-flatpak-ci-builds` (in `make check-policy` and CI's `Lint` job)
  fails when a workflow names `flatpak-builder@`, when a job in a
  `flatpak-github-actions` container lacks `uses: ./.github/actions/flatpak-build`
  after its checkout, or when the one exemption (`release.yml`
  `cominotti-flatpak`, which installs with `--user` into the host-bound
  `$HOME`) no longer passes `--user`.
- `.github/workflows/release-dry-run.yml` — path-filtered release automation
  check (release helper, Flathub manifest and Cominotti repository tests, a
  no-mutation preview, metadata validation).
- `.github/workflows/release.yml` — `v*` tag release validation and manual
  dry run; its Flathub PR step stays skipped. A tag push builds the tag. A
  dispatch builds the dispatched commit (`checkout-ref` is `$GITHUB_SHA`) in
  every job; a dispatch that is not a dry run is refused unless it came from
  `refs/heads/main`. A dry run (the default) validates an **untagged** version
  (`make release DRY_RUN=1` refuses an existing tag, so use one such as
  `v0.8.4-dryrun.1`), generates the Flathub artifact from `HEAD`, and runs the
  `Release Flatpak Build` job through `.github/actions/flatpak-build`, so a dry
  run dispatched from a branch proves a release-workflow change before merge;
  the Cominotti repository, deploy and publish jobs stay skipped.
- `.github/workflows/release-benchmark.yml` — release-safe benchmark report on
  `v*` tags or recovery dispatches, `fedora:45`.
- `.github/workflows/snap.yml` — `validate` runs `snapcraft expand-extensions`;
  `build-publish` (release `edge`, `SNAPCRAFT_STORE_CREDENTIALS`) is gated on the
  `SNAP_PLATFORM_AVAILABLE` repository variable.

Rust validation helpers installed in CI are version-pinned in workflow `env`
blocks: `CARGO_DENY_VERSION=0.19.8`, `CARGO_NEXTEST_VERSION=0.9.151`,
`CARGO_FUZZ_VERSION=0.13.1`, `CARGO_MUTANTS_VERSION=27.0.0`,
`CARGO_SEMVER_CHECKS_VERSION=0.48.0`, `CARGO_PUBLIC_API_VERSION=0.52.0`
(with `CARGO_PUBLIC_API_SHA256=80f55cea…7511`, the crate's checksum, and
`CARGO_PUBLIC_API_OPENSSL_SYS_VERSION=0.9.118`, its one lockfile override; see
the GTK Lush Family Foundation section), `GTK_LUSH_MSRV=1.98.0`, and `GTK_LUSH_PUBLIC_API_TOOLCHAIN=nightly-2026-06-01`;
`KANI_VERSION` comes from the Makefile. Update the pin and rerun the affected
local validation command when intentionally refreshing a tool.

**When bumping gtk-rs:** update the Fedora version in `ci.yml` and
`release-benchmark.yml`, and the GNOME tag of every
`flatpak-github-actions` image use (`flatpak.yml`, and both container jobs of
`release.yml`: `flatpak` and `cominotti-flatpak`) and of the Flatpak manifest,
to match the new minimum GTK requirement.
