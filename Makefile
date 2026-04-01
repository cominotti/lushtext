# SPDX-License-Identifier: GPL-3.0-or-later
#
# Makefile for LushText development builds.
#
# Targets:
#   make build       - Release build
#   make build-debug - Debug build
#   make run         - Debug build + run
#   make test        - Run all tests (unit + integration)
#   make test-unit   - Unit tests only (fast)
#   make test-int    - Integration tests only
#   make check       - clippy + fmt check
#   make clean       - Clean build artifacts
#   make help        - Show available targets

.PHONY: build build-debug run test test-unit test-int check clean help

.DEFAULT_GOAL := help

# Fast linker: use mold on Linux if available.
# Set via RUSTFLAGS rather than .cargo/config.toml so builds
# don't hard-fail when mold is not installed.
HAS_MOLD := $(shell command -v mold 2>/dev/null && echo 1)
ifdef HAS_MOLD
export RUSTFLAGS += -C link-arg=-fuse-ld=mold
endif

# Test runner: prefer cargo-nextest for per-test process isolation and parallelism.
# Falls back to cargo test when nextest is not installed.
HAS_NEXTEST := $(shell command -v cargo-nextest 2>/dev/null && echo 1)
ifdef HAS_NEXTEST
CARGO_TEST     = cargo nextest run
CARGO_TEST_INT = cargo nextest run --test integration
else
CARGO_TEST     = cargo test
CARGO_TEST_INT = cargo test --test integration
endif

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
	cargo run

# Run all tests
test:
	@echo "Running all tests..."
	$(CARGO_TEST)

# Unit tests only (fast, no I/O)
test-unit:
	@echo "Running unit tests..."
	$(CARGO_TEST) --lib

# Integration tests only
test-int:
	@echo "Running integration tests..."
	$(CARGO_TEST_INT)

# Lint + format check
check:
	@echo "Running clippy..."
	cargo clippy --all-targets -- -D warnings
	@echo "Checking formatting..."
	cargo fmt --all -- --check

# Clean all build artifacts
clean:
	@echo "Cleaning build artifacts..."
	cargo clean

# Show available targets
help:
	@echo "LushText Build System"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Build targets:"
	@echo "  build        Release build (optimized)"
	@echo "  build-debug  Debug build"
	@echo "  run          Debug build and run"
	@echo ""
	@echo "Test targets:"
	@echo "  test         All tests (unit + integration)"
	@echo "  test-unit    Unit tests only (fast)"
	@echo "  test-int     Integration tests only"
	@echo ""
	@echo "Other targets:"
	@echo "  check        Clippy + format check"
	@echo "  clean        Remove build artifacts"
	@echo "  help         Show this help message"
	@echo ""
	@echo "Optional build accelerators (auto-detected):"
	@echo "  mold linker      : faster linking on Linux"
	@echo "  cargo-nextest    : parallel test execution"
	@echo "  cargo-hakari     : unified dependency features"
