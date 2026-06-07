#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

MODE="${1:-}"
MUTANTS_TIMEOUT="${MUTANTS_TIMEOUT:-300}"
MUTANTS_SMOKE_FILE="${MUTANTS_SMOKE_FILE:-crates/lushtext-core/src/services/file_limits.rs}"
MUTANTS_DIFF_FILE="${MUTANTS_DIFF_FILE:-git.diff}"
MUTANTS_BASE="${MUTANTS_BASE:-origin/main}"
MUTANTS_SHARD="${MUTANTS_SHARD:-}"
MUTANTS_BASELINE_SKIP="${MUTANTS_BASELINE_SKIP:-0}"
MUTANTS_IN_PLACE="${MUTANTS_IN_PLACE:-0}"
MUTANTS_JOBS="${MUTANTS_JOBS:-}"
MUTANTS_TEST_THREADS="${MUTANTS_TEST_THREADS:-}"
MUTANTS_BUILD_JOBS="${MUTANTS_BUILD_JOBS:-}"

# Cap per-job parallelism so concurrent jobs do not oversubscribe the host.
# cargo-mutants runs MUTANTS_JOBS build/test pipelines at once; each one launches
# its own cargo build AND its own nextest, both of which otherwise grab every
# core. Capping each phase so jobs x per-job-parallelism stays near the logical
# CPU count is what makes --jobs a speedup instead of thrash:
#   - NEXTEST_TEST_THREADS bounds the test phase.
#   - CARGO_BUILD_JOBS bounds the build phase (the one that spikes load average,
#     since six concurrent cold builds each fan out to every core by default).
# CI leaves all three unset, so sharded runners stay serial and uncapped.
if [[ -n "${MUTANTS_TEST_THREADS}" ]]; then
    export NEXTEST_TEST_THREADS="${MUTANTS_TEST_THREADS}"
fi
if [[ -n "${MUTANTS_BUILD_JOBS}" ]]; then
    export CARGO_BUILD_JOBS="${MUTANTS_BUILD_JOBS}"
fi

usage() {
    cat <<'EOF'
Usage: scripts/run-mutants.sh <mode> [diff-file]

Modes:
  smoke    Run a small bounded mutation pass against MUTANTS_SMOKE_FILE.
  diff     Run changed-code mutation against a git diff file or MUTANTS_BASE.
  full     Run the configured full mutation scope, optionally with MUTANTS_SHARD.
  ci-pr    CI pull-request mode: diff + baseline skip + in-place allowed.
  ci-full  CI full-shard mode: full + baseline skip + in-place allowed.
  list     List mutants in the configured scope without running tests.

Environment:
  MUTANTS_TIMEOUT       Explicit cargo-mutants timeout in seconds (default: 300).
  MUTANTS_SMOKE_FILE    File used by smoke mode.
  MUTANTS_DIFF_FILE     Diff file used by diff/ci-pr when no argument is passed.
  MUTANTS_BASE          Base ref used to create MUTANTS_DIFF_FILE (default: origin/main).
  MUTANTS_SHARD         Shard identity, for example 0/4.
  MUTANTS_BASELINE_SKIP Set to 1 to pass --baseline=skip.
  MUTANTS_IN_PLACE      Set to 1 to pass --in-place; guarded outside CI.
  MUTANTS_JOBS          Build/test this many mutants in parallel (default: serial).
  MUTANTS_TEST_THREADS  Cap nextest threads per mutant job (pairs with MUTANTS_JOBS).
  MUTANTS_BUILD_JOBS    Cap cargo build jobs per mutant job (pairs with MUTANTS_JOBS).
EOF
}

fail() {
    echo "run-mutants.sh: $*" >&2
    exit 1
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

ensure_tooling() {
    require_command cargo
    require_command cargo-mutants
    require_command cargo-nextest
}

ensure_clean_worktree_for_in_place() {
    if [[ "${CI:-}" == "true" ]]; then
        return
    fi

    require_command git
    if ! git diff --quiet --ignore-submodules --; then
        fail "MUTANTS_IN_PLACE=1 requires a clean unstaged worktree outside CI"
    fi
    if ! git diff --cached --quiet --ignore-submodules --; then
        fail "MUTANTS_IN_PLACE=1 requires a clean index outside CI"
    fi
    if [[ -n "$(git ls-files --others --exclude-standard)" ]]; then
        fail "MUTANTS_IN_PLACE=1 requires no untracked files outside CI"
    fi
}

mutants_args() {
    local args=(
        --workspace
        --test-workspace=true
        --test-tool
        nextest
        --no-shuffle
        --timeout
        "${MUTANTS_TIMEOUT}"
    )
    # Do not pass `--features property-tests` here. Property tests run in their
    # own lane so generated cases are not multiplied by every mutant.

    if [[ "${MUTANTS_BASELINE_SKIP}" == "1" ]]; then
        args+=(--baseline=skip)
    fi

    if [[ "${MUTANTS_IN_PLACE}" == "1" ]]; then
        ensure_clean_worktree_for_in_place
        args+=(--in-place)
    fi

    if [[ -n "${MUTANTS_SHARD}" ]]; then
        args+=(--shard "${MUTANTS_SHARD}")
    fi

    # Local runs set this to fan out across cores; CI leaves it empty so the
    # sharded small runners keep cargo-mutants' serial default.
    if [[ -n "${MUTANTS_JOBS}" ]]; then
        args+=(--jobs "${MUTANTS_JOBS}")
    fi

    printf '%s\n' "${args[@]}"
}

run_cargo_mutants() {
    local args=("$@")
    local log_file
    log_file="$(mktemp)"

    set +e
    cargo mutants "${args[@]}" 2>&1 | tee "${log_file}"
    local status="${PIPESTATUS[0]}"
    set -e

    if (( status != 0 )) && grep -Eiq 'no relevant mutants|no mutants were generated|found 0 mutants|(^|[^0-9])0 mutants (to test|tested)' "${log_file}"; then
        echo "No relevant mutants were generated; treating this mutation lane as passing."
        rm -f "${log_file}"
        return 0
    fi

    rm -f "${log_file}"
    return "${status}"
}

ensure_diff_file() {
    local diff_file="$1"

    if [[ -f "${diff_file}" ]]; then
        return
    fi

    require_command git
    echo "Creating mutation diff against ${MUTANTS_BASE}..."
    git diff "${MUTANTS_BASE}..." >"${diff_file}"
}

run_smoke() {
    [[ -f "${MUTANTS_SMOKE_FILE}" ]] || fail "smoke file does not exist: ${MUTANTS_SMOKE_FILE}"

    mapfile -t args < <(mutants_args)
    run_cargo_mutants "${args[@]}" --no-config --file "${MUTANTS_SMOKE_FILE}"
}

run_diff() {
    local diff_file="${1:-${MUTANTS_DIFF_FILE}}"
    ensure_diff_file "${diff_file}"

    if ! grep -q '^@@ ' "${diff_file}"; then
        echo "No diff hunks found; skipping changed-code mutation run."
        return 0
    fi

    mapfile -t args < <(mutants_args)
    run_cargo_mutants "${args[@]}" --in-diff "${diff_file}"
}

run_full() {
    mapfile -t args < <(mutants_args)
    run_cargo_mutants "${args[@]}"
}

run_list() {
    mapfile -t args < <(mutants_args)
    cargo mutants "${args[@]}" --list
}

ensure_tooling

case "${MODE}" in
    smoke)
        run_smoke
        ;;
    diff)
        run_diff "${2:-}"
        ;;
    full)
        run_full
        ;;
    ci-pr)
        MUTANTS_BASELINE_SKIP=1
        MUTANTS_IN_PLACE=1
        run_diff "${2:-}"
        ;;
    ci-full)
        MUTANTS_BASELINE_SKIP=1
        MUTANTS_IN_PLACE=1
        run_full
        ;;
    list)
        run_list
        ;;
    -h|--help|help|"")
        usage
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
