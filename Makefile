# SPDX-License-Identifier: GPL-3.0-or-later
#
# Makefile for LushText development builds.
#
# Targets:
#   make build       - Release build
#   make build-debug - Debug build
#   make run         - Debug build + force a fresh dev run with GNOME desktop staging
#   make refresh-dock-icon - Regenerate app icon assets and restart dev run so GNOME Shell reloads the app icon
#   make test        - Run all tests (unit + integration + widget)
#   make test-unit   - Unit tests only (fast)
#   make test-int    - Integration tests only
#   make test-prop   - Bounded property tests for pure deterministic logic
#   make test-prop-deep - Opt-in deeper property run with more generated cases
#   make test-widget - Widget tests with shared native/headless runner
#   make test-widget-headless - Widget tests under mutter --headless
#   make mutants-smoke - Small cargo-mutants smoke run
#   make mutants-diff  - Mutation test current changes against origin/main
#   make mutants-full  - Mutation test the configured deterministic scope
#   make check       - clippy + fmt check
#   make pre-commit  - repo pre-commit gate (fmt + clippy)
#   make flatpak-deps - Install Flatpak runtime/SDK deps into the user installation
#   make flatpak-install - Build and install Flatpak into the user installation
#   make verify-flatpak-identity - Verify Flatpak desktop identity and MIME registration
#   make cominotti-flatpak-repo - Build signed Cominotti Flatpak repository artifacts
#   make verify-cominotti-pages-limits - Verify generated artifacts fit Cloudflare Pages
#   make verify-flathub-domain - Verify cominotti.dev is ready for Flathub app verification
#   make release     - Prepare, validate, commit, tag, and push an explicit release version
#   make release-bump - Compute the next release version, then release it
#   make dev-tools   - Prepare local dev tooling (Flatpak deps + GTK debug helpers)
#   make install-git-hooks - configure this repo to use .githooks/
#   make clean       - Clean build artifacts
#   make help        - Show available targets

.PHONY: build build-debug run refresh-dock-icon test test-unit test-int test-prop test-prop-deep test-widget test-widget-headless mutants-smoke mutants-diff mutants-full mutants-list \
       check-fmt check-clippy check pre-commit dev-tools install-git-hooks clean help \
       meson-build flatpak-deps flatpak flatpak-install cargo-sources verify-flatpak-identity test-flatpak-identity-verifier test-dev-desktop-staging \
       flathub-manifest verify-flathub-manifest verify-flathub-domain \
       cominotti-flatpak-repo verify-cominotti-flatpak-repo verify-cominotti-pages-limits cominotti-flatpak-smoke test-cominotti-flatpak-repo \
       test-release-helper test-flathub-manifest release release-bump \
       snap snap-smoke verify-snap-identity \
       bench bench-report bench-report-full bench-baseline bench-compare

.DEFAULT_GOAL := help

# Test runner: prefer cargo-nextest for non-widget tests. Widget tests always
# go through the shared runner so native and headless execution stay aligned.
HAS_NEXTEST := $(shell command -v cargo-nextest 2>/dev/null && echo 1)
ifdef HAS_NEXTEST
CARGO_TEST_NON_WIDGET = cargo nextest run --workspace
CARGO_TEST_UNIT       = cargo nextest run --workspace --lib
CARGO_TEST_INT        = cargo nextest run --workspace --test integration
else
CARGO_TEST_NON_WIDGET = cargo test --workspace --lib --bins --test integration
CARGO_TEST_UNIT       = cargo test --workspace --lib
CARGO_TEST_INT        = cargo test --workspace --test integration
endif
CARGO_TEST_WIDGET          = ./scripts/run-widget-tests.sh
CARGO_TEST_WIDGET_HEADLESS = ./scripts/run-widget-tests.sh --headless --retries 1
CARGO_TEST_PROP           = cargo nextest run -p lushtext-core --features property-tests --test properties --profile property
PROPTEST_DEEP_CASES ?= 512

# Local cargo-mutants parallelism. cargo-mutants defaults to serial (one mutant
# at a time), which leaves a multi-core box mostly idle on the slowest workload.
# Locally we fan out: MUTANTS_LOCAL_JOBS defaults to about cores / 4, and each
# job's nextest is capped to MUTANTS_LOCAL_TEST_THREADS so jobs x threads stays
# near the logical CPU count instead of oversubscribing it. CI lanes call
# scripts/run-mutants.sh directly and leave MUTANTS_JOBS unset, so the sharded
# small runners keep the serial default.
MUTANTS_LOCAL_JOBS ?= $(shell nproc 2>/dev/null | awk '{j = int($$1 / 4); if (j < 1) j = 1; print j}')
MUTANTS_LOCAL_TEST_THREADS ?= 4
# Build-phase cap: derived so jobs x build-jobs stays near the CPU count. Without
# it, each of the MUTANTS_LOCAL_JOBS concurrent cargo builds fans out to every
# core, spiking load average far above ncpu during the cold-build phase.
MUTANTS_LOCAL_BUILD_JOBS ?= $(shell nproc 2>/dev/null | awk '{n = $$1; j = int(n / 4); if (j < 1) j = 1; b = int(n / j); if (b < 1) b = 1; print b}')
MUTANTS_LOCAL_PARALLELISM = MUTANTS_JOBS=$(MUTANTS_LOCAL_JOBS) MUTANTS_TEST_THREADS=$(MUTANTS_LOCAL_TEST_THREADS) MUTANTS_BUILD_JOBS=$(MUTANTS_LOCAL_BUILD_JOBS)

FLATPAK_REMOTE ?= flathub
FLATPAK_REMOTE_URL ?= https://dl.flathub.org/repo/flathub.flatpakrepo
FLATPAK_BUILD_DIR := build-flatpak
FLATPAK_MANIFEST := build-aux/dev.cominotti.lushtext.Flatpak.json
FLATPAK_BUILDER_FLAGS := --disable-rofiles-fuse --force-clean --user
FLATPAK_BUILDER_DEPS_FLAGS := --assumeyes --install-deps-from=$(FLATPAK_REMOTE)
FLATHUB_MANIFEST_OUT_DIR ?= build-aux/flathub
FLATHUB_MANIFEST_COMMIT ?= $(shell git rev-parse HEAD 2>/dev/null || echo unknown)
COMINOTTI_FLATPAK_OUT_DIR ?= build-aux/cominotti-flatpak
COMINOTTI_FLATPAK_COMMIT ?= $(shell git rev-parse HEAD 2>/dev/null || echo unknown)

# Build the project (release, optimized)
build:
	@echo "Building LushText (release)..."
	cargo build --release

# Build the project (debug)
build-debug:
	@echo "Building LushText (debug)..."
	cargo build

# Debug build and force a fresh dev run
run: build-debug
	@echo "Running LushText..."
	LUSHTEXT_DEV_RUN_FORCE_RESTART=1 ./scripts/run-dev-app.sh

# Force a fresh dev relaunch so GNOME Shell reloads the dock icon
refresh-dock-icon:
	@echo "Regenerating LushText app icon assets..."
	rsvg-convert -w 32 -h 32 data/icons/dev.cominotti.lushtext.svg -o data/icons/hicolor/32x32/apps/dev.cominotti.lushtext.png
	rsvg-convert -w 64 -h 64 data/icons/dev.cominotti.lushtext.svg -o data/icons/hicolor/64x64/apps/dev.cominotti.lushtext.png
	rsvg-convert -w 128 -h 128 data/icons/dev.cominotti.lushtext.svg -o data/icons/hicolor/128x128/apps/dev.cominotti.lushtext.png
	@$(MAKE) build-debug
	@echo "Refreshing the LushText GNOME Shell dock icon..."
	LUSHTEXT_DEV_RUN_FORCE_RESTART=1 LUSHTEXT_DEV_RUN_TERMINATE_STALE=1 ./scripts/run-dev-app.sh

# Run all tests
test:
	@echo "Running all tests..."
	$(CARGO_TEST_NON_WIDGET)
	$(CARGO_TEST_WIDGET_HEADLESS)

# Unit tests only (fast, no I/O)
test-unit:
	@echo "Running unit tests..."
	$(CARGO_TEST_UNIT)

# Integration tests only
test-int:
	@echo "Running integration tests..."
	$(CARGO_TEST_INT)

# Property tests for pure deterministic logic. The feature-gated target stays
# outside default nextest and mutation runs so generated cases do not multiply
# ordinary feedback time.
test-prop:
	@echo "Running bounded property tests..."
	$(CARGO_TEST_PROP)

# Deeper opt-in property pass for local investigation or scheduled checks.
test-prop-deep:
	@echo "Running deep property tests with $(PROPTEST_DEEP_CASES) generated cases per property..."
	LUSHTEXT_PROPTEST_CASES=$(PROPTEST_DEEP_CASES) $(CARGO_TEST_PROP)

# Widget tests (auto-detect display; fall back to mutter --headless when available)
test-widget:
	@echo "Running widget tests..."
	$(CARGO_TEST_WIDGET)

# Widget tests with the same headless setup used in CI
test-widget-headless:
	@echo "Running widget tests under mutter --headless..."
	$(CARGO_TEST_WIDGET_HEADLESS)

# Small mutation pass for checking cargo-mutants tooling and timeout behavior.
mutants-smoke:
	@echo "Running cargo-mutants smoke scope (jobs=$(MUTANTS_LOCAL_JOBS), build-jobs=$(MUTANTS_LOCAL_BUILD_JOBS), test-threads=$(MUTANTS_LOCAL_TEST_THREADS))..."
	$(MUTANTS_LOCAL_PARALLELISM) ./scripts/run-mutants.sh smoke

# Mutation-test the current diff against origin/main.
mutants-diff:
	@echo "Running cargo-mutants against changed code (jobs=$(MUTANTS_LOCAL_JOBS), build-jobs=$(MUTANTS_LOCAL_BUILD_JOBS), test-threads=$(MUTANTS_LOCAL_TEST_THREADS))..."
	$(MUTANTS_LOCAL_PARALLELISM) ./scripts/run-mutants.sh diff

# Mutation-test the configured deterministic scope.
mutants-full:
	@echo "Running configured cargo-mutants scope (jobs=$(MUTANTS_LOCAL_JOBS), build-jobs=$(MUTANTS_LOCAL_BUILD_JOBS), test-threads=$(MUTANTS_LOCAL_TEST_THREADS))..."
	$(MUTANTS_LOCAL_PARALLELISM) ./scripts/run-mutants.sh full

# List configured mutants without running tests.
mutants-list:
	@echo "Listing configured cargo-mutants scope..."
	./scripts/run-mutants.sh list

BENCH_REPORT_OUT_DIR ?= docs/benchmarks

# Run benchmarks (quick, default Criterion sample size)
bench:
	@echo "Running benchmarks..."
	cargo bench -p lushtext-core

# Run benchmarks and generate markdown report (short sampling)
bench-report:
	@echo "Running benchmarks and generating report..."
	./scripts/bench-report.sh --mode short --out-dir $(BENCH_REPORT_OUT_DIR)

# Run benchmarks with full sampling and generate report
bench-report-full:
	@echo "Running full benchmarks and generating report..."
	./scripts/bench-report.sh --mode full --out-dir $(BENCH_REPORT_OUT_DIR)

# Save current benchmarks as baseline for comparison
bench-baseline:
	@echo "Saving benchmark baseline..."
	cargo bench -p lushtext-core --bench benchmarks -- --save-baseline main

# Compare current performance against saved baseline
bench-compare:
	@echo "Comparing against baseline..."
	cargo bench -p lushtext-core --bench benchmarks -- --baseline main

# Formatting check
check-fmt:
	@echo "Checking formatting..."
	cargo fmt --all -- --check

# Clippy gate matching CI
check-clippy:
	@echo "Running clippy..."
	cargo clippy --workspace --all-targets -- -D warnings

# Repo pre-commit gate
pre-commit: check-fmt check-clippy

# Lint + format check
check: pre-commit

# Install repo-managed Git hooks
install-git-hooks:
	@echo "Configuring core.hooksPath to use .githooks..."
	git config core.hooksPath .githooks
	chmod +x .githooks/pre-commit
	@echo "Git hooks installed."

# Prepare local development tooling, including Flatpak runtime dependencies and
# helper tools used by live GTK debugging sessions.
dev-tools: flatpak-deps
	@echo "Installing local development helper tools..."
	./scripts/setup-dev-tools.sh

# Clean all build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean

# Meson build (installed layout)
meson-build:
	@echo "Building with Meson..."
	meson setup _build -Dprofile=release
	meson compile -C _build

# Install the manifest's runtime, SDK, and SDK extensions into the user Flatpak installation.
flatpak-deps:
	@echo "Ensuring Flathub remote is available for user Flatpak builds..."
	flatpak remote-add --if-not-exists --user $(FLATPAK_REMOTE) $(FLATPAK_REMOTE_URL)
	@echo "Installing Flatpak runtime dependencies from $(FLATPAK_REMOTE)..."
	flatpak-builder $(FLATPAK_BUILDER_FLAGS) $(FLATPAK_BUILDER_DEPS_FLAGS) --install-deps-only $(FLATPAK_BUILD_DIR) $(FLATPAK_MANIFEST)

# Flatpak build (sets up missing runtime/SDK deps from Flathub)
flatpak: flatpak-deps
	@echo "Building Flatpak..."
	flatpak-builder $(FLATPAK_BUILDER_FLAGS) $(FLATPAK_BUILD_DIR) $(FLATPAK_MANIFEST)

# Flatpak build and install into the user installation
flatpak-install: flatpak-deps
	@echo "Building and installing Flatpak..."
	flatpak-builder $(FLATPAK_BUILDER_FLAGS) --assumeyes --install $(FLATPAK_BUILD_DIR) $(FLATPAK_MANIFEST)

# Verify the installed Flatpak export is the active production desktop identity
verify-flatpak-identity:
	@echo "Verifying Flatpak desktop identity..."
	./scripts/verify-flatpak-identity.sh

# Unit-style shell checks for the Flatpak identity verifier
test-flatpak-identity-verifier:
	@echo "Testing Flatpak identity verifier..."
	./scripts/test-flatpak-identity-verifier.sh

# Unit-style shell checks for development desktop-entry staging
test-dev-desktop-staging:
	@echo "Testing development desktop-entry staging..."
	./scripts/test-dev-desktop-staging.sh

# Regenerate cargo-sources.json (requires flatpak-cargo-generator)
cargo-sources: Cargo.lock
	@echo "Generating cargo-sources.json..."
	flatpak-cargo-generator Cargo.lock -o build-aux/cargo-sources.json

# Generate a Flathub-facing manifest from an immutable Git release source.
# Usage: make flathub-manifest VERSION=v0.2.0 [FLATHUB_MANIFEST_COMMIT=<sha>]
flathub-manifest:
ifndef VERSION
	$(error VERSION is required. Usage: make flathub-manifest VERSION=v0.2.0)
endif
	@echo "Generating Flathub manifest for $(VERSION)..."
	./scripts/generate-flathub-manifest.sh "$(VERSION)" "$(FLATHUB_MANIFEST_COMMIT)" "$(FLATHUB_MANIFEST_OUT_DIR)"

# Verify the generated Flathub-facing manifest preserves the local packaging contract.
verify-flathub-manifest:
	@echo "Verifying Flathub manifest..."
	./scripts/verify-flathub-manifest.sh "$(FLATHUB_MANIFEST_OUT_DIR)/dev.cominotti.lushtext.json"

# Verify cominotti.dev is ready for Flathub app-id verification.
# Pass FLATHUB_VERIFICATION_TOKEN=<token> after Flathub gives you a token.
verify-flathub-domain:
	@echo "Verifying Flathub domain ownership endpoint..."
	./scripts/verify-flathub-domain.sh "$(FLATHUB_VERIFICATION_TOKEN)"

# Build signed Cominotti-owned Flatpak repository artifacts.
# Usage: make cominotti-flatpak-repo VERSION=v0.2.0 COMINOTTI_FLATPAK_PUBLIC_KEY=public.gpg COMINOTTI_FLATPAK_GPG_KEY=<key-id>
cominotti-flatpak-repo:
ifndef VERSION
	$(error VERSION is required. Usage: make cominotti-flatpak-repo VERSION=v0.2.0)
endif
	@echo "Generating Cominotti Flatpak repository artifacts for $(VERSION)..."
	./scripts/generate-cominotti-flatpak-repo.sh "$(VERSION)" "$(COMINOTTI_FLATPAK_COMMIT)" "$(COMINOTTI_FLATPAK_OUT_DIR)"

# Verify generated Cominotti Flatpak repository metadata and, when present, app refs.
verify-cominotti-flatpak-repo:
	@echo "Verifying Cominotti Flatpak repository artifacts..."
	./scripts/verify-cominotti-flatpak-repo.sh "$(COMINOTTI_FLATPAK_OUT_DIR)"

# Verify generated Cominotti Flatpak artifacts fit Cloudflare Pages static asset limits.
verify-cominotti-pages-limits:
	@echo "Verifying Cominotti Flatpak Cloudflare Pages limits..."
	./scripts/verify-cominotti-pages-limits.sh "$(COMINOTTI_FLATPAK_OUT_DIR)/flatpak"

# Strict local smoke check for generated repository output.
cominotti-flatpak-smoke:
	@echo "Smoke-testing Cominotti Flatpak repository artifacts..."
	COMINOTTI_FLATPAK_VERIFY_INSTALL=1 ./scripts/verify-cominotti-flatpak-repo.sh "$(COMINOTTI_FLATPAK_OUT_DIR)"

# Unit-style shell checks for Cominotti Flatpak repository metadata generation.
test-cominotti-flatpak-repo:
	@echo "Testing Cominotti Flatpak repository tooling..."
	./scripts/test-cominotti-flatpak-repo.sh

# Unit-style shell checks for release helper behavior.
test-release-helper:
	@echo "Testing release helper..."
	./scripts/test-release.sh

# Unit-style shell checks for Flathub manifest generation.
test-flathub-manifest:
	@echo "Testing Flathub manifest generation..."
	./scripts/test-flathub-manifest.sh

# Release: prepare metadata, validate, commit, create a signed tag, and push.
# Usage: make release VERSION=v0.2.0 RELEASE_NOTES_FILE=release-notes.md [YES=1] [DRY_RUN=1]
release:
ifeq ($(filter command line,$(origin VERSION)),)
	$(error VERSION is required. Usage: make release VERSION=v0.2.0 RELEASE_NOTES_FILE=release-notes.md)
endif
	@./scripts/release.sh tag "$(VERSION)" "$(YES)" "$(DRY_RUN)"

# Release bump: compute next version and run the release flow.
# Usage: make release-bump TYPE=minor [PRERELEASE=alpha] [PROMOTE=1] [YES=1] [DRY_RUN=1]
release-bump:
ifndef TYPE
	$(error TYPE is required (major, minor, or patch). Usage: make release-bump TYPE=minor)
endif
	@./scripts/release.sh bump "$(TYPE)" "$(PRERELEASE)" "$(PROMOTE)" "$(YES)" "$(DRY_RUN)"

# Build the Snap (LXD backend). GATED: needs the GNOME 50 platform snap to
# satisfy the GTK 4.22 floor; expected to fail against core24 (GTK 4.14) today.
snap:
	@echo "Building Snap (LXD backend)..."
	snapcraft pack --use-lxd

# Local confined smoke test of the built Snap. Skips cleanly when snapcraft or
# the GNOME 50 platform snap is unavailable.
snap-smoke:
	@echo "Running Snap confined smoke test..."
	./scripts/run-snap-smoke.sh

# Verify the Snap's confinement, plug connections, and common-id linkage.
# Pass a built artifact (make verify-snap-identity ARGS=./lushtext_*.snap) or run
# with no args against the installed snap.
verify-snap-identity:
	@echo "Verifying Snap identity and permissions..."
	./scripts/verify-snap-identity.sh $(ARGS)

# Show available targets
help:
	@echo "LushText Build System"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Build targets:"
	@echo "  build        Release build (optimized)"
	@echo "  build-debug  Debug build"
	@echo "  run          Debug build and force a fresh dev run with GNOME desktop staging"
	@echo "  refresh-dock-icon Regenerate app icon assets + force a fresh dock icon reload in GNOME Shell"
	@echo ""
	@echo "Test targets:"
	@echo "  test         All tests (unit + integration + widget)"
	@echo "  test-unit    Unit tests only (fast)"
	@echo "  test-int     Integration tests only"
	@echo "  test-prop    Bounded property tests for pure deterministic logic"
	@echo "  test-prop-deep Deeper property run with PROPTEST_DEEP_CASES"
	@echo "  test-widget  Widget tests (auto-detect display; falls back to headless)"
	@echo "  test-widget-headless Widget tests with the CI headless setup"
	@echo "  mutants-smoke Small cargo-mutants smoke run"
	@echo "  mutants-diff Changed-code mutation against origin/main"
	@echo "  mutants-full Configured deterministic mutation scope"
	@echo "  mutants-list List configured mutants without running tests"
	@echo "  pre-commit   Repo pre-commit gate (fmt + clippy)"
	@echo "  install-git-hooks Configure this repo to use .githooks/"
	@echo ""
	@echo "Benchmark targets:"
	@echo "  bench            Run Criterion benchmarks"
	@echo "  bench-report     Run + generate markdown report (short)"
	@echo "  bench-report-full Run + generate markdown report (full)"
	@echo "  bench-baseline   Save current results as baseline"
	@echo "  bench-compare    Compare against saved baseline"
	@echo ""
	@echo "Packaging targets:"
	@echo "  meson-build     Meson release build (installed layout)"
	@echo "  flatpak-deps    Install Flatpak runtime/SDK deps into the user installation"
	@echo "  flatpak         Build Flatpak (sets up missing runtime/SDK deps)"
	@echo "  flatpak-install Build and install Flatpak into the user installation"
	@echo "  verify-flatpak-identity Verify Flatpak desktop identity and MIME registration"
	@echo "  test-flatpak-identity-verifier Test the Flatpak identity verifier"
	@echo "  test-dev-desktop-staging Test dev-run desktop staging behavior"
	@echo "  cargo-sources   Regenerate cargo-sources.json"
	@echo "  flathub-manifest Generate a Flathub-facing manifest for VERSION=vX.Y.Z"
	@echo "  verify-flathub-manifest Verify generated Flathub manifest invariants"
	@echo "  verify-flathub-domain Verify cominotti.dev Flathub verification endpoint"
	@echo "  cominotti-flatpak-repo Generate signed Cominotti Flatpak repo artifacts"
	@echo "  verify-cominotti-flatpak-repo Verify Cominotti Flatpak repo artifacts"
	@echo "  verify-cominotti-pages-limits Verify Cloudflare Pages size/count limits"
	@echo "  cominotti-flatpak-smoke Require installable app refs in generated Cominotti repo"
	@echo "  test-cominotti-flatpak-repo Test Cominotti Flatpak repo tooling"
	@echo "  test-release-helper Test release versioning and metadata helper"
	@echo "  test-flathub-manifest Test Flathub manifest generation"
	@echo "  release         Prepare, validate, commit, signed-tag, and push VERSION=vX.Y.Z"
	@echo "  release-bump    Compute next version from TYPE=major|minor|patch, then release"
	@echo "  snap            Build the Snap (LXD); gated on the GNOME 50 platform snap"
	@echo "  snap-smoke      Confined smoke test of the built Snap (skips if unavailable)"
	@echo "  verify-snap-identity Verify Snap confinement, plugs, and common-id"
	@echo ""
	@echo "Other targets:"
	@echo "  check-fmt    rustfmt --check"
	@echo "  check-clippy clippy -D warnings"
	@echo "  check        Clippy + format check"
	@echo "  dev-tools    Flatpak deps + GTK debug input/screenshot helpers"
	@echo "  clean        Remove build artifacts"
	@echo "  help         Show this help message"
	@echo ""
	@echo "Optional build accelerators (auto-detected):"
	@echo "  cargo-nextest    : parallel test execution"
	@echo "  cargo-hakari     : unified dependency features"
	@echo ""
	@echo "Release variables:"
	@echo "  VERSION                 Explicit release version, e.g. v0.2.0"
	@echo "  TYPE                    Bump type for release-bump: major, minor, or patch"
	@echo "  PRERELEASE              Pre-release label: alpha, beta, or rc"
	@echo "  PROMOTE                 Set to 1 to promote a prerelease stream to stable"
	@echo "  RELEASE_NOTES_FILE      Required for real releases; inserted into AppStream"
	@echo "  YES                     Set to 1 to skip release confirmation"
	@echo "  DRY_RUN                 Set to 1 to preview without mutating the repo"
	@echo "  FLATHUB_VERIFICATION_TOKEN Token from Flathub Developer Portal"
	@echo "  COMINOTTI_FLATPAK_PUBLIC_KEY Public GPG key file for Cominotti Flatpak metadata"
	@echo "  COMINOTTI_FLATPAK_GPG_KEY GPG signing key ID for Cominotti Flatpak publication"
	@echo "  COMINOTTI_FLATPAK_PAGES_MAX_FILE_BYTES Cloudflare Pages per-asset byte limit"
	@echo "  COMINOTTI_FLATPAK_PAGES_MAX_FILES Cloudflare Pages file-count limit"
