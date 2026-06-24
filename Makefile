# SPDX-License-Identifier: GPL-3.0-or-later
#
# Makefile for LushText development builds.
#
# Targets:
#   make build       - Release build
#   make build-debug - Debug build
#   make run         - Debug build + force a fresh dev run with GNOME desktop staging
#   make run-format-upgrade-newer-manual-test - Launch with isolated future-version app data
#   make run-format-upgrade-older-manual-test - Launch with isolated upgradeable old-version app data
#   make run-command-palette-notes-manual-test - Launch with isolated Notes palette fixtures
#   make refresh-dock-icon - Regenerate app icon assets and restart dev run so GNOME Shell reloads the app icon
#   make clear-lushtext-xdg - Remove LushText-owned XDG data/config/cache/state and reset app settings
#   make test        - Run all tests (unit + integration + widget)
#   make test-unit   - Unit tests only (fast)
#   make test-int    - Integration tests only
#   make test-prop   - Bounded property tests for pure deterministic logic
#   make test-prop-deep - Opt-in deeper property run with more generated cases
#   make fuzz-list   - List configured cargo-fuzz targets
#   make fuzz-corpus-replay - Replay committed fuzz corpus seeds on stable Rust
#   make fuzz-smoke  - Run bounded fuzz smoke against temporary corpus copies
#   make fuzz-operation-smoke - Run bounded structured operation fuzz smoke
#   make test-widget - Widget tests under the private headless runner
#   make test-widget-headless - Widget tests under mutter --headless
#   make test-workspace-row-states - Focused idempotent workspace file-row state widget tests
#   make automation-smoke - Real-process D-Bus automation smoke under headless Mutter
#   make builder-diagnostics-smoke - GtkBuilder diagnostics under debug-enabled GTK
#   make command-palette-notes-smoke - Focused Notes command-palette smoke with all note kinds
#   make visual-smoke - Real-session screenshot smoke under headless Mutter
#   make visual-geometry-smoke - Rust same-session visual invariant proof
#   make visual-geometry-oracle-smoke - Python oracle visual invariant diagnostics
#   make crash-recovery-smoke - Real-process crash/restart recovery smoke with artifacts
#   make portal-sandbox-smoke - Confined runtime smoke for available Flatpak/Snap paths
#   make accessibility-smoke - AT-SPI-enabled accessibility smoke
#   make performance-smoke - Lightweight Criterion performance smoke
#   make end-user-smoke - Run all host-supported end-user smoke lanes
#   Smoke and full benchmark report lanes are artifact-rich scheduled/manual/release checks, not default PR gates.
#   make mutants-smoke - Small cargo-mutants smoke run
#   make mutants-diff  - Mutation test current changes against origin/main
#   make mutants-full  - Mutation test the configured deterministic scope
#   make check       - fmt + all-feature clippy + fast policy audits
#   make blueprint-generate - Regenerate generated GtkBuilder .ui files from Blueprint .blp sources
#   make check-blueprint - Validate Blueprint drift and generated UI template contract
#   make lint-blueprint - Advisory grouped lint triage for Blueprint templates
#   make check-flatpak-permissions - Ensure Flatpak keeps full filesystem access
#   make check-end-user-smoke-workflow - Ensure scheduled smoke lanes match docs
#   make check-workflow-timeouts - Enforce the 30-minute GitHub Actions job budget
#   make check-accessibility-policy - Enforce accessibility helper, matrix, and current-tree guardrails
#   make check-visual-proof-policy - Require visual geometry proof for local visual-sensitive changes
#   make check-gtk-lush-policy - Verify GTK Lush family scaffolding and constitution rails
#   make check-gtk-lush-adoption - Run GTK Lush adoption lab, stock fixture, and matrix checks
#   make gtk-lush-adoption-lab - Build/test the maintained GTK Lush adoption lab
#   make gtk-lush-stock-fixtures - Check stock one-crate GTK Lush adoption fixtures
#   make gtk-lush-adoption-matrix - Validate GTK Lush adoption matrix and evidence locations
#   make gtk-lush-doctests - Run doctests for GTK Lush family crates
#   make gtk-lush-examples - Compile standalone GTK Lush adoption examples
#   make gtk-lush-msrv - Check GTK Lush family crates with the declared MSRV
#   make gtk-lush-api-advisory - Run advisory semver/public-API checks for GTK Lush crates
#   make automation-client-self-test - Validate the reusable D-Bus automation CLI helper
#   make check-agent-docs - validate agent rules/skills guidance
#   make lint-advisory - grouped advisory lint discovery for Rust policy reviews
#   make pre-commit  - repo pre-commit gate (fmt + all-feature clippy + policy audits)
#   make flatpak-deps - Install Flatpak runtime/SDK deps into the user installation
#   make flatpak-install - Build and install Flatpak into the user installation
#   make verify-flatpak-identity - Verify Flatpak desktop identity and MIME registration
#   make cominotti-flatpak-repo - Build signed Cominotti Flatpak repository artifacts
#   make verify-cominotti-pages-limits - Verify generated artifacts fit Cloudflare Pages
#   make verify-flathub-domain - Verify cominotti.dev is ready for Flathub app verification
#   make release     - Prepare, validate, commit, tag, and push an explicit release version
#   make release-bump - Compute the next release version, then release it
#   make snap-store-readiness - Check Snap Store/platform gates without mutating them
#   make dev-tools   - Prepare local dev tooling (Flatpak deps + GTK debug helpers)
#   make install-git-hooks - configure this repo to use .githooks/
#   make clean       - Clean build artifacts
#   make help        - Show available targets

.PHONY: build build-debug run run-format-upgrade-manual-test run-format-upgrade-newer-manual-test run-format-upgrade-older-manual-test run-command-palette-notes-manual-test refresh-dock-icon clear-lushtext-xdg test test-unit test-int test-prop test-prop-deep fuzz-list fuzz-corpus-replay fuzz-smoke fuzz-operation-smoke test-widget test-widget-headless test-workspace-row-states automation-smoke builder-diagnostics-smoke command-palette-notes-smoke visual-smoke visual-geometry-smoke visual-geometry-oracle-smoke crash-recovery-smoke portal-sandbox-smoke accessibility-smoke performance-smoke end-user-smoke mutants-smoke mutants-diff mutants-full mutants-list \
       check-fmt check-clippy check-filesystem-boundary check-blueprint check-ui-template-contract lint-blueprint check-flatpak-permissions check-end-user-smoke-workflow check-workflow-timeouts check-accessibility-policy check-visual-proof-policy check-gtk-lush-policy check-gtk-lush-adoption gtk-lush-adoption-lab gtk-lush-stock-fixtures gtk-lush-adoption-matrix gtk-lush-doctests gtk-lush-examples gtk-lush-msrv gtk-lush-api-advisory gtk-lush-semver-advisory gtk-lush-public-api-advisory automation-client-self-test check-policy lint-advisory check check-agent-docs check-automation-docs pre-commit dev-tools install-git-hooks clean help \
       blueprint-generate \
       meson-build flatpak-deps flatpak flatpak-install cargo-sources verify-flatpak-identity test-flatpak-identity-verifier test-dev-desktop-staging \
       flathub-manifest verify-flathub-manifest verify-flathub-domain \
       cominotti-flatpak-repo verify-cominotti-flatpak-repo verify-cominotti-pages-limits cominotti-flatpak-smoke test-cominotti-flatpak-repo \
       test-release-helper test-flathub-manifest release release-bump \
       snap snap-smoke verify-snap-identity snap-store-readiness \
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
CARGO_TEST_WIDGET          = ./scripts/run-widget-tests.sh --headless
CARGO_TEST_WIDGET_HEADLESS = ./scripts/run-widget-tests.sh --headless --retries 1
CARGO_TEST_WORKSPACE_ROW_STATES = ./scripts/run-widget-tests.sh --headless --retries 1 -- workspace_row_state
CARGO_TEST_PROP           = cargo nextest run -p lushtext-core --features property-tests --test properties --profile property
CARGO_TEST_FUZZ_CORPUS_REPLAY = cargo test -p lushtext-core --features fuzzing --test fuzz_corpus_replay
PROPTEST_DEEP_CASES ?= 512
FORMAT_UPGRADE_TEST_HOME ?=
FORMAT_UPGRADE_TEST_VERSION ?= 999
COMMAND_PALETTE_NOTES_MANUAL_HOME ?=
COMMAND_PALETTE_NOTES_QUERY ?= palette

GTK_LUSH_PACKAGES := -p gtk-lush-signals -p gtk-lush-settle -p gtk-lush-tasks -p gtk-lush-viewport -p gtk-lush-widgets -p gtk-lush-proof-harness -p gtk-lush-proof-spine
GTK_LUSH_CRATES := crates/gtk-lush/signals crates/gtk-lush/settle crates/gtk-lush/tasks crates/gtk-lush/viewport crates/gtk-lush/widgets crates/gtk-lush/proof-harness crates/gtk-lush/proof-spine
GTK_LUSH_ADOPTION_LAB_PACKAGE := -p gtk-lush-adoption-lab
GTK_LUSH_STOCK_FIXTURES := fixtures/gtk-lush-adoption/stock-settle
GTK_LUSH_MSRV ?= 1.96.0
GTK_LUSH_PUBLIC_API_TOOLCHAIN ?= nightly-2026-06-01
GTK_LUSH_PUBLIC_API_OUT_DIR ?= target/gtk-lush-public-api

CARGO_FUZZ ?= cargo +nightly fuzz
FUZZ_TARGETS ?= editor_bytes markdown_preprocess operation_script
FUZZ_OPERATION_TARGET ?= operation_script
FUZZ_SMOKE_RUNS ?= 64
FUZZ_SMOKE_SECONDS ?= 5
FUZZ_SMOKE_MAX_LEN ?= 4096

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

# Preserve the original manual-test command name; it now means the newer-data drill.
run-format-upgrade-manual-test: run-format-upgrade-newer-manual-test

# Prepare isolated app data that looks like it came from a newer LushText and
# launch the normal dev app against that XDG_DATA_HOME.
run-format-upgrade-newer-manual-test: build-debug
	@FORMAT_UPGRADE_TEST_HOME="$(FORMAT_UPGRADE_TEST_HOME)" FORMAT_UPGRADE_TEST_VERSION="$(FORMAT_UPGRADE_TEST_VERSION)" bash ./scripts/run-format-upgrade-manual-test.sh newer

# Prepare isolated app data that can be upgraded through a test-only legacy
# converter and launch a debug build with that manual fixture enabled.
run-format-upgrade-older-manual-test:
	@echo "Building LushText with manual format-upgrade fixtures..."
	cargo build -p lushtext --features manual-format-upgrade-fixtures
	@FORMAT_UPGRADE_TEST_HOME="$(FORMAT_UPGRADE_TEST_HOME)" FORMAT_UPGRADE_TEST_VERSION=0 bash ./scripts/run-format-upgrade-manual-test.sh older

# Prepare isolated bookmark, folder-note, document-note, and open-tab note data,
# then launch the normal dev app with the command palette open in Notes mode.
run-command-palette-notes-manual-test: build-debug
	@COMMAND_PALETTE_NOTES_MANUAL_HOME="$(COMMAND_PALETTE_NOTES_MANUAL_HOME)" COMMAND_PALETTE_NOTES_QUERY="$(COMMAND_PALETTE_NOTES_QUERY)" bash ./scripts/run-command-palette-notes-manual-test.sh

# Force a fresh dev relaunch so GNOME Shell reloads the dock icon
refresh-dock-icon:
	@echo "Regenerating LushText app icon assets..."
	rsvg-convert -w 32 -h 32 data/icons/dev.cominotti.lushtext.svg -o data/icons/hicolor/32x32/apps/dev.cominotti.lushtext.png
	rsvg-convert -w 64 -h 64 data/icons/dev.cominotti.lushtext.svg -o data/icons/hicolor/64x64/apps/dev.cominotti.lushtext.png
	rsvg-convert -w 128 -h 128 data/icons/dev.cominotti.lushtext.svg -o data/icons/hicolor/128x128/apps/dev.cominotti.lushtext.png
	@$(MAKE) build-debug
	@echo "Refreshing the LushText GNOME Shell dock icon..."
	LUSHTEXT_DEV_RUN_FORCE_RESTART=1 LUSHTEXT_DEV_RUN_TERMINATE_STALE=1 ./scripts/run-dev-app.sh

# Remove only LushText-owned user XDG/config state. Use DRY_RUN=1 to preview.
clear-lushtext-xdg:
	@DRY_RUN="$(DRY_RUN)" INCLUDE_FLATPAK="$(INCLUDE_FLATPAK)" RESET_GSETTINGS="$(RESET_GSETTINGS)" ALLOW_RUNNING="$(ALLOW_RUNNING)" ./scripts/clear-lushtext-xdg.sh

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

# List configured cargo-fuzz targets without running them.
fuzz-list:
	@echo "Listing cargo-fuzz targets..."
	$(CARGO_FUZZ) list

# Replay committed corpus seeds through stable Rust tests. This intentionally
# avoids cargo-fuzz, libFuzzer, sanitizer flags, nightly, and C++ toolchain setup.
fuzz-corpus-replay:
	@echo "Replaying committed fuzz corpus seeds on stable Rust..."
	$(CARGO_TEST_FUZZ_CORPUS_REPLAY)

# Bounded fuzz smoke for configured targets. Each run uses a temporary copy of
# the seed corpus so libFuzzer can grow inputs without dirtying the checkout.
fuzz-smoke:
	@echo "Running fuzz smoke ($(FUZZ_SMOKE_RUNS) runs, max_len=$(FUZZ_SMOKE_MAX_LEN), max_total_time=$(FUZZ_SMOKE_SECONDS)s per target)..."
	@set -eu; \
	tmp=$$(mktemp -d); \
	trap 'rm -rf "$$tmp"' EXIT; \
	for target in $(FUZZ_TARGETS); do \
		corpus="$$tmp/$$target"; \
		mkdir -p "$$corpus"; \
		if [ -d "fuzz/corpus/$$target" ]; then \
			cp -R "fuzz/corpus/$$target/." "$$corpus/"; \
		fi; \
		echo "Running fuzz smoke target $$target..."; \
		$(CARGO_FUZZ) run "$$target" "$$corpus" -- -runs=$(FUZZ_SMOKE_RUNS) -max_len=$(FUZZ_SMOKE_MAX_LEN) -max_total_time=$(FUZZ_SMOKE_SECONDS); \
	done

# Focused smoke for the structured operation target when byte-ingestion targets
# are not part of the question being investigated.
fuzz-operation-smoke:
	@echo "Running structured operation fuzz smoke ($(FUZZ_SMOKE_RUNS) runs, max_len=$(FUZZ_SMOKE_MAX_LEN), max_total_time=$(FUZZ_SMOKE_SECONDS)s)..."
	@set -eu; \
	tmp=$$(mktemp -d); \
	trap 'rm -rf "$$tmp"' EXIT; \
	corpus="$$tmp/$(FUZZ_OPERATION_TARGET)"; \
	mkdir -p "$$corpus"; \
	if [ -d "fuzz/corpus/$(FUZZ_OPERATION_TARGET)" ]; then \
		cp -R "fuzz/corpus/$(FUZZ_OPERATION_TARGET)/." "$$corpus/"; \
	fi; \
	$(CARGO_FUZZ) run "$(FUZZ_OPERATION_TARGET)" "$$corpus" -- -runs=$(FUZZ_SMOKE_RUNS) -max_len=$(FUZZ_SMOKE_MAX_LEN) -max_total_time=$(FUZZ_SMOKE_SECONDS)

# Widget tests under the private headless runner.
test-widget:
	@echo "Running widget tests..."
	$(CARGO_TEST_WIDGET)

# Widget tests with the same headless setup used in CI
test-widget-headless:
	@echo "Running widget tests under mutter --headless..."
	$(CARGO_TEST_WIDGET_HEADLESS)

# Focused workspace-sidebar file-row state tests. The filter is a shared test
# name substring, so this stays narrow while still exercising section and window
# coverage through the normal isolated widget harness.
test-workspace-row-states:
	@echo "Running focused workspace file-row state widget tests..."
	$(CARGO_TEST_WORKSPACE_ROW_STATES)

# Real-process D-Bus smoke under isolated headless Mutter. This proves the
# app-owned automation object, snapshots, waits, and a parameterized action.
automation-smoke: build-debug
	@echo "Running D-Bus automation smoke lane..."
	./scripts/run-automation-smoke.sh --artifact-dir "$(SMOKE_ARTIFACT_DIR)/automation"

# Runtime GtkBuilder diagnostics under a debug-enabled GTK runtime. The script
# owns provider selection: host if debug channels are available, otherwise the
# configured reusable container image when a container runner exists.
builder-diagnostics-smoke: build-debug
	@echo "Running GtkBuilder diagnostics smoke lane..."
	./scripts/run-builder-diagnostics.sh --artifact-dir "$(SMOKE_ARTIFACT_DIR)/builder-diagnostics"

# Focused visual smoke for the command palette Notes category. The fixture covers
# Bookmarks, Folder Notes, Document Notes, and Open Tabs in one isolated session.
command-palette-notes-smoke: build-debug
	@echo "Running command palette Notes smoke lane..."
	COMMAND_PALETTE_NOTES_QUERY="$(COMMAND_PALETTE_NOTES_QUERY)" ./scripts/run-command-palette-notes-smoke.sh --artifact-dir "$(SMOKE_ARTIFACT_DIR)/command-palette-notes"

# Real-session screenshot smoke under isolated headless Mutter. This is an
# artifact-producing lane for rendered-pixel and compositor behavior; it skips
# cleanly when host desktop-capture dependencies are unavailable.
visual-smoke: build-debug
	@echo "Running visual smoke lane..."
	./scripts/run-visual-smoke.sh --artifact-dir "$(SMOKE_ARTIFACT_DIR)/visual"

visual-geometry-smoke: build-debug
	@echo "Running same-session visual geometry invariant lane..."
	cargo run -q -p cargo-gtk-proof -- run --artifact-dir "$(SMOKE_ARTIFACT_DIR)/visual-geometry" --scenario-dir scripts/visual-geometry-scenarios --binary "$(PWD)/target/debug/lushtext"

visual-geometry-oracle-smoke: build-debug
	@echo "Running Python visual geometry oracle diagnostics..."
	cargo run -q -p cargo-gtk-proof -- run --oracle python --artifact-dir "$(SMOKE_ARTIFACT_DIR)/visual-geometry-python-oracle" --scenario-dir scripts/visual-geometry-scenarios --binary "$(PWD)/target/debug/lushtext"

# Real-process crash/restart smoke under isolated headless Mutter. This lane
# creates draft/session recovery state through the app, SIGKILLs the process,
# relaunches with the same app data, and preserves recovery artifacts.
crash-recovery-smoke: build-debug
	@echo "Running crash recovery smoke lane..."
	./scripts/run-crash-recovery-smoke.sh --artifact-dir "$(SMOKE_ARTIFACT_DIR)/crash-recovery"

# Confined runtime smoke for available Flatpak/Snap paths. This records runtime
# identity and skips clearly when neither confined runtime is installed.
portal-sandbox-smoke:
	@echo "Running portal/sandbox smoke lane..."
	./scripts/run-portal-sandbox-smoke.sh --artifact-dir "$(SMOKE_ARTIFACT_DIR)/portal-sandbox"

# AT-SPI-enabled smoke lane. Unlike widget tests, this keeps the accessibility
# bridge enabled so accessible-name and focus automation can be verified.
accessibility-smoke: build-debug
	@echo "Running accessibility smoke lane..."
	./scripts/run-accessibility-smoke.sh --artifact-dir "$(SMOKE_ARTIFACT_DIR)/accessibility"

BENCH_REPORT_OUT_DIR ?= docs/benchmarks
SMOKE_ARTIFACT_DIR ?= build/smoke

# Lightweight performance smoke distinct from full Criterion reports.
performance-smoke:
	@echo "Running performance smoke lane..."
	./scripts/run-performance-smoke.sh --artifact-dir "$(SMOKE_ARTIFACT_DIR)/performance"

# Run all host-supported end-user smoke lanes. Individual scripts own their
# dependency checks, artifact paths, and skip messages.
end-user-smoke: automation-smoke builder-diagnostics-smoke visual-geometry-smoke visual-smoke crash-recovery-smoke portal-sandbox-smoke accessibility-smoke performance-smoke

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

# Run benchmarks (quick, default Criterion sample size)
bench:
	@echo "Running benchmarks..."
	cargo bench -p lushtext-core

# Run benchmarks and generate markdown report (short sampling)
bench-report:
	@echo "Running benchmarks and generating report..."
	./scripts/bench-report.sh --mode short --scope release --out-dir $(BENCH_REPORT_OUT_DIR)

# Run benchmarks with full sampling and generate report
bench-report-full:
	@echo "Running full benchmarks and generating report..."
	./scripts/bench-report.sh --mode full --scope diagnostic --out-dir $(BENCH_REPORT_OUT_DIR)

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
	cargo clippy --workspace --all-targets --all-features -- -D warnings

# Fast path-aware policy audit for lint-adjacent architecture drift.
check-filesystem-boundary:
	@echo "Checking filesystem boundary policy..."
	./scripts/check-filesystem-boundary.sh

# Regenerate generated GtkBuilder resources from Blueprint sources.
blueprint-generate:
	@echo "Generating GtkBuilder templates from Blueprint sources..."
	./scripts/blueprint-templates.sh generate

# Drift and contract check for Blueprint-authored UI templates.
check-blueprint:
	@echo "Checking Blueprint template drift and generated UI contract..."
	./scripts/blueprint-templates.sh check

# Structural contract audit without running the compiler drift check.
check-ui-template-contract:
	@echo "Checking generated UI template contract..."
	./scripts/blueprint-templates.sh audit

# Advisory lint triage for Blueprint templates. This groups current diagnostics
# and fails only when a rule is unclassified or escalates to an error.
lint-blueprint:
	@echo "Running advisory Blueprint lint triage..."
	./scripts/blueprint-templates.sh lint

# Guard the intentional Flatpak permission posture. Portal/sandbox automation
# may record diagnostics, but this change must not narrow filesystem access.
check-flatpak-permissions:
	@echo "Checking Flatpak filesystem permission policy..."
	./scripts/check-flatpak-permissions.py --self-test --manifest "$(FLATPAK_MANIFEST)"

# Guard the scheduled/manual smoke workflow matrix. This keeps the artifact-rich
# lanes documented in `docs/end-user-coverage.md` from drifting away from CI.
check-end-user-smoke-workflow:
	@echo "Checking end-user smoke workflow matrix..."
	./scripts/check-end-user-smoke-workflow.py

# Enforce the repository-wide CI job budget. Missing job timeouts are violations
# because GitHub's implicit default is much higher than LushText's hard limit.
check-workflow-timeouts:
	@echo "Checking GitHub Actions job timeouts..."
	./scripts/check-workflow-timeouts.py --self-test

# Guard new UI-sensitive lines against bypassing accessibility helper and proof conventions.
check-accessibility-policy:
	@echo "Checking accessibility policy..."
	./scripts/check-accessibility-policy.py --self-test --strict-current-tree

# Guard local UI-sensitive edits against missing same-session visual proof.
check-visual-proof-policy:
	@echo "Checking visual geometry proof policy..."
	cargo run -q -p cargo-gtk-proof -- policy --self-test
	cargo run -q -p cargo-gtk-proof -- policy

# Validate the reusable agent/developer client without needing a live D-Bus app.
automation-client-self-test:
	@echo "Checking automation CLI helper..."
	./scripts/lushtext-automation.py self-test

check-gtk-lush-policy:
	@echo "Checking GTK Lush family policy..."
	./scripts/check-gtk-lush-policy.py

gtk-lush-adoption-matrix:
	@echo "Checking GTK Lush adoption matrix and evidence..."
	./scripts/check-gtk-lush-adoption.py

gtk-lush-adoption-lab:
	@echo "Testing GTK Lush adoption lab..."
	cargo test $(GTK_LUSH_ADOPTION_LAB_PACKAGE) --all-targets

gtk-lush-stock-fixtures:
	@echo "Checking stock GTK Lush adoption fixtures..."
	@set -eu; \
	for fixture in $(GTK_LUSH_STOCK_FIXTURES); do \
		echo "Checking $$fixture..."; \
		CARGO_TARGET_DIR="$(PWD)/target" cargo check --manifest-path "$$fixture/Cargo.toml" --locked; \
	done

check-gtk-lush-adoption: gtk-lush-adoption-matrix gtk-lush-adoption-lab gtk-lush-stock-fixtures

gtk-lush-doctests:
	@echo "Running GTK Lush doctests..."
	cargo test $(GTK_LUSH_PACKAGES) --doc

gtk-lush-examples:
	@echo "Compiling GTK Lush standalone examples..."
	cargo check $(GTK_LUSH_PACKAGES) --examples

gtk-lush-msrv:
	@echo "Checking GTK Lush family MSRV ($(GTK_LUSH_MSRV))..."
	cargo +$(GTK_LUSH_MSRV) check $(GTK_LUSH_PACKAGES) --all-targets

gtk-lush-semver-advisory:
	@echo "Running advisory GTK Lush semver checks..."
	@command -v cargo-semver-checks >/dev/null || { \
		echo "cargo-semver-checks is required for GTK Lush advisory checks."; \
		exit 1; \
	}; \
	status=0; \
	for crate in $(GTK_LUSH_CRATES); do \
		echo "Checking semver for $$crate..."; \
		(cd "$$crate" && cargo semver-checks) || status=1; \
	done; \
	if [ "$$status" -ne 0 ]; then \
		echo "Advisory only: cargo-semver-checks reported issues or missing baselines."; \
	fi

gtk-lush-public-api-advisory:
	@echo "Generating advisory GTK Lush public API snapshots..."
	@command -v cargo-public-api >/dev/null || { \
		echo "cargo-public-api is required for GTK Lush public API snapshots."; \
		exit 1; \
	}; \
	mkdir -p "$(GTK_LUSH_PUBLIC_API_OUT_DIR)"; \
	for crate in $(GTK_LUSH_CRATES); do \
		package=$$(basename "$$crate"); \
		output="$(GTK_LUSH_PUBLIC_API_OUT_DIR)/$$package.txt"; \
		echo "Generating public API for $$crate..."; \
		cargo +$(GTK_LUSH_PUBLIC_API_TOOLCHAIN) public-api --manifest-path "$$crate/Cargo.toml" >"$$output"; \
		test -s "$$output" || { \
			echo "GTK Lush public API snapshot is missing or empty: $$output"; \
			exit 1; \
		}; \
	done

gtk-lush-api-advisory: gtk-lush-semver-advisory gtk-lush-public-api-advisory

# Aggregate policy target for fast audits that sit beside rustfmt and Clippy.
check-policy: check-filesystem-boundary check-blueprint check-automation-docs check-flatpak-permissions check-end-user-smoke-workflow check-workflow-timeouts check-accessibility-policy check-visual-proof-policy check-gtk-lush-policy gtk-lush-adoption-matrix automation-client-self-test

# Advisory lint discovery; fails if a finding category has no checked-in policy.
lint-advisory:
	@echo "Running advisory lint discovery..."
	./scripts/lint-advisory.py

# Repo pre-commit gate
pre-commit: check-fmt check-clippy check-policy

# Lint + format + fast policy check
check: pre-commit

# Validate agent-facing rules and skills after guidance changes.
check-agent-docs:
	@echo "Checking agent documentation..."
	./scripts/check-agent-docs.sh

# Validate user/developer automation docs against the exported catalog, D-Bus
# interface, snapshot schema, and readiness blockers.
check-automation-docs:
	@echo "Checking automation documentation..."
	./scripts/check-automation-docs.py --self-test

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

# Check external Snap Store and platform readiness without registering,
# publishing, or mutating store state.
snap-store-readiness:
	@echo "Checking Snap Store and platform readiness..."
	./scripts/check-snap-store-readiness.sh

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
	@echo "  run-format-upgrade-newer-manual-test Launch with isolated future-version app data"
	@echo "  run-format-upgrade-older-manual-test Launch with isolated upgradeable old-version app data"
	@echo "  run-command-palette-notes-manual-test Launch with isolated Notes palette fixtures"
	@echo "  refresh-dock-icon Regenerate app icon assets + force a fresh dock icon reload in GNOME Shell"
	@echo "  clear-lushtext-xdg Remove LushText-owned XDG data/config/cache/state and reset app settings"
	@echo ""
	@echo "Test targets:"
	@echo "  test         All tests (unit + integration + widget)"
	@echo "  test-unit    Unit tests only (fast)"
	@echo "  test-int     Integration tests only"
	@echo "  test-prop    Bounded property tests for pure deterministic logic"
	@echo "  test-prop-deep Deeper property run with PROPTEST_DEEP_CASES"
	@echo "  test-widget  Widget tests under the private headless runner"
	@echo "  test-widget-headless Widget tests with the CI headless setup"
	@echo "  test-workspace-row-states Focused workspace file-row state widget tests"
	@echo "  automation-smoke Real-process D-Bus automation smoke under headless Mutter"
	@echo "  builder-diagnostics-smoke GtkBuilder diagnostics under debug-enabled GTK"
	@echo "  command-palette-notes-smoke Focused Notes palette smoke with all note kinds"
	@echo "  check-end-user-smoke-workflow Verify scheduled/manual smoke matrix lanes"
	@echo "  check-workflow-timeouts Enforce the 30-minute GitHub Actions job budget"
	@echo "  check-accessibility-policy Enforce accessibility helper, matrix, and current-tree guardrails"
	@echo "  check-visual-proof-policy Require visual geometry proof for local visual-sensitive changes"
	@echo "  check-gtk-lush-policy Verify GTK Lush family scaffolding and dependency direction"
	@echo "  check-gtk-lush-adoption Run adoption lab, stock fixture, and matrix checks"
	@echo "  gtk-lush-adoption-lab Build/test the maintained GTK Lush adoption lab"
	@echo "  gtk-lush-stock-fixtures Check stock one-crate GTK Lush adoption fixtures"
	@echo "  gtk-lush-adoption-matrix Validate GTK Lush adoption matrix and evidence"
	@echo "  gtk-lush-doctests Run GTK Lush family doctests"
	@echo "  gtk-lush-examples Compile GTK Lush standalone examples"
	@echo "  gtk-lush-msrv Check GTK Lush family crates with GTK_LUSH_MSRV"
	@echo "  gtk-lush-api-advisory Run advisory semver/public-API checks"
	@echo "  automation-client-self-test Validate the reusable D-Bus automation CLI helper"
	@echo "  visual-smoke Real-session screenshot smoke under headless Mutter"
	@echo "  visual-geometry-smoke Rust same-session visual invariant proof"
	@echo "  visual-geometry-oracle-smoke Python oracle visual invariant diagnostics"
	@echo "  portal-sandbox-smoke Confined runtime smoke for available Flatpak/Snap paths"
	@echo "  accessibility-smoke AT-SPI-enabled accessibility smoke"
	@echo "  performance-smoke Lightweight Criterion performance smoke"
	@echo "  end-user-smoke Run all host-supported end-user smoke lanes"
	@echo "  (Smoke lanes preserve artifacts and are scheduled/manual/release checks, not default PR gates)"
	@echo ""
	@echo "Fuzz targets (explicit lanes):"
	@echo "  fuzz-corpus-replay Replay committed fuzz corpus seeds on stable Rust"
	@echo "  fuzz-list    List configured cargo-fuzz targets"
	@echo "  fuzz-smoke   Bounded cargo-fuzz smoke against temporary corpus copies"
	@echo "  fuzz-operation-smoke Bounded structured operation fuzz smoke"
	@echo ""
	@echo "Mutation targets:"
	@echo "  mutants-smoke Small cargo-mutants smoke run"
	@echo "  mutants-diff Changed-code mutation against origin/main"
	@echo "  mutants-full Configured deterministic mutation scope"
	@echo "  mutants-list List configured mutants without running tests"
	@echo "  pre-commit   Repo pre-commit gate (fmt + all-feature clippy + policy audits)"
	@echo "  check-policy Fast policy audits, including filesystem and Blueprint checks"
	@echo "  check-gtk-lush-policy Verify GTK Lush family scaffolding and dependency direction"
	@echo "  check-gtk-lush-adoption Run adoption lab, stock fixture, and matrix checks"
	@echo "  gtk-lush-adoption-lab Build/test the maintained GTK Lush adoption lab"
	@echo "  gtk-lush-stock-fixtures Check stock one-crate GTK Lush adoption fixtures"
	@echo "  gtk-lush-adoption-matrix Validate GTK Lush adoption matrix and evidence"
	@echo "  blueprint-generate Regenerate GtkBuilder .ui files from Blueprint sources"
	@echo "  check-blueprint Validate Blueprint drift and UI template contract"
	@echo "  check-flatpak-permissions Verify Flatpak keeps intentional full filesystem access"
	@echo "  lint-blueprint Advisory grouped Blueprint lint triage"
	@echo "  lint-advisory Grouped advisory Rust lint discovery"
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
	@echo "  snap-store-readiness Check Snap Store/platform gates without mutating them"
	@echo ""
	@echo "Other targets:"
	@echo "  check-fmt    rustfmt --check"
	@echo "  check-clippy clippy -D warnings"
	@echo "  gtk-lush-doctests Run GTK Lush family doctests"
	@echo "  gtk-lush-examples Compile GTK Lush standalone examples"
	@echo "  gtk-lush-msrv Check GTK Lush family crates with GTK_LUSH_MSRV"
	@echo "  gtk-lush-api-advisory Run advisory semver/public-API checks"
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
