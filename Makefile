# SPDX-License-Identifier: GPL-3.0-or-later
#
# Makefile for LushText development builds.
#
# Targets:
#   make build       - Release build
#   make build-debug - Debug build
#   make run         - Debug build + run with temporary GNOME desktop staging
#   make refresh-dock-icon - Regenerate app icon assets and restart dev run so GNOME Shell reloads the app icon
#   make test        - Run all tests (unit + integration + widget)
#   make test-unit   - Unit tests only (fast)
#   make test-int    - Integration tests only
#   make test-widget - Widget tests with shared native/headless runner
#   make test-widget-headless - Widget tests under mutter --headless
#   make check       - clippy + fmt check
#   make pre-commit  - repo pre-commit gate (fmt + clippy)
#   make flatpak-install - Build and install Flatpak into the user installation
#   make install-git-hooks - configure this repo to use .githooks/
#   make clean       - Clean build artifacts
#   make help        - Show available targets

.PHONY: build build-debug run refresh-dock-icon test test-unit test-int test-widget test-widget-headless \
       check-fmt check-clippy check pre-commit install-git-hooks clean help \
       meson-build flatpak flatpak-install cargo-sources \
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

# Build the project (release, optimized)
build:
	@echo "Building LushText (release)..."
	cargo build --release

# Build the project (debug)
build-debug:
	@echo "Building LushText (debug)..."
	cargo build

# Debug build and run
run: build-debug
	@echo "Running LushText..."
	./scripts/run-dev-app.sh

# Force a fresh dev relaunch so GNOME Shell reloads the dock icon
refresh-dock-icon:
	@echo "Regenerating LushText app icon assets..."
	rsvg-convert -w 32 -h 32 data/icons/dev.cominotti.lushtext.svg -o data/icons/hicolor/32x32/apps/dev.cominotti.lushtext.png
	rsvg-convert -w 64 -h 64 data/icons/dev.cominotti.lushtext.svg -o data/icons/hicolor/64x64/apps/dev.cominotti.lushtext.png
	rsvg-convert -w 128 -h 128 data/icons/dev.cominotti.lushtext.svg -o data/icons/hicolor/128x128/apps/dev.cominotti.lushtext.png
	@$(MAKE) build-debug
	@echo "Refreshing the LushText GNOME Shell dock icon..."
	LUSHTEXT_DEV_RUN_FORCE_RESTART=1 ./scripts/run-dev-app.sh

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

# Widget tests (auto-detect display; fall back to mutter --headless when available)
test-widget:
	@echo "Running widget tests..."
	$(CARGO_TEST_WIDGET)

# Widget tests with the same headless setup used in CI
test-widget-headless:
	@echo "Running widget tests under mutter --headless..."
	$(CARGO_TEST_WIDGET_HEADLESS)

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

# Clean all build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean

# Meson build (installed layout)
meson-build:
	@echo "Building with Meson..."
	meson setup _build -Dprofile=release
	meson compile -C _build

# Flatpak build (requires flatpak-builder + org.gnome.Sdk)
flatpak:
	@echo "Building Flatpak..."
	flatpak-builder --disable-rofiles-fuse --force-clean build-flatpak build-aux/dev.cominotti.lushtext.Flatpak.json

# Flatpak build and install into the user installation
flatpak-install:
	@echo "Building and installing Flatpak..."
	flatpak-builder --disable-rofiles-fuse --force-clean --user --install build-flatpak build-aux/dev.cominotti.lushtext.Flatpak.json

# Regenerate cargo-sources.json (requires flatpak-cargo-generator)
cargo-sources: Cargo.lock
	@echo "Generating cargo-sources.json..."
	flatpak-cargo-generator Cargo.lock -o build-aux/cargo-sources.json

# Show available targets
help:
	@echo "LushText Build System"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Build targets:"
	@echo "  build        Release build (optimized)"
	@echo "  build-debug  Debug build"
	@echo "  run          Debug build and run with temporary GNOME desktop staging"
	@echo "  refresh-dock-icon Regenerate app icon assets + force a fresh dock icon reload in GNOME Shell"
	@echo ""
	@echo "Test targets:"
	@echo "  test         All tests (unit + integration + widget)"
	@echo "  test-unit    Unit tests only (fast)"
	@echo "  test-int     Integration tests only"
	@echo "  test-widget  Widget tests (auto-detect display; falls back to headless)"
	@echo "  test-widget-headless Widget tests with the CI headless setup"
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
	@echo "  flatpak         Build Flatpak (needs flatpak-builder)"
	@echo "  flatpak-install Build and install Flatpak into the user installation"
	@echo "  cargo-sources   Regenerate cargo-sources.json"
	@echo ""
	@echo "Other targets:"
	@echo "  check-fmt    rustfmt --check"
	@echo "  check-clippy clippy -D warnings"
	@echo "  check        Clippy + format check"
	@echo "  clean        Remove build artifacts"
	@echo "  help         Show this help message"
	@echo ""
	@echo "Optional build accelerators (auto-detected):"
	@echo "  cargo-nextest    : parallel test execution"
	@echo "  cargo-hakari     : unified dependency features"
