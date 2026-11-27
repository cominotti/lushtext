#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

MODE="${1:-}"
MUTANTS_TIMEOUT="${MUTANTS_TIMEOUT:-300}"
MUTANTS_SMOKE_FILE="${MUTANTS_SMOKE_FILE:-crates/lushtext-model/src/document/document_scale.rs}"
# Whether the caller named the diff file. The default one is ours and is
# regenerated on every run (ensure_diff_file).
MUTANTS_DIFF_FILE_NAMED="${MUTANTS_DIFF_FILE:+1}"
MUTANTS_DIFF_FILE="${MUTANTS_DIFF_FILE:-git.diff}"
MUTANTS_BASE="${MUTANTS_BASE:-origin/main}"
MUTANTS_SHARD="${MUTANTS_SHARD:-}"
MUTANTS_BASELINE_SKIP="${MUTANTS_BASELINE_SKIP:-0}"
MUTANTS_IN_PLACE="${MUTANTS_IN_PLACE:-0}"
MUTANTS_JOBS="${MUTANTS_JOBS:-}"
MUTANTS_TEST_THREADS="${MUTANTS_TEST_THREADS:-}"
MUTANTS_BUILD_JOBS="${MUTANTS_BUILD_JOBS:-}"
MUTANTS_RE="${MUTANTS_RE:-}"
MUTANTS_EXCLUDE="${MUTANTS_EXCLUDE:-}"
MUTANTS_EXCLUDE_RE="${MUTANTS_EXCLUDE_RE:-}"
MUTANTS_MIRROR="${MUTANTS_MIRROR:-0}"
MUTANTS_LOCAL_JOBS="${MUTANTS_LOCAL_JOBS:-1}"
MUTANTS_POOL_SIZE="${MUTANTS_POOL_SIZE:-3}"
# About twice a warm slot with incremental kept: 10.4-11.1 GiB after a
# focused run, 4.9 GiB of it incremental cache (2026-11-15).
MUTANTS_SLOT_MAX_GIB="${MUTANTS_SLOT_MAX_GIB:-24}"
# Design D3's measured decision for the mirror slots: "" leaves
# CARGO_INCREMENTAL as the caller set it (incremental, Cargo's default), "0"
# builds without it. Without it a mutant's build took 2.2x as long (25.5 s
# against 11.4 s median per mutant, 2026-11-15), past D3's 25% bar, though
# it wrote 40% less; the slots' own build dirs keep that cache out of every
# other build either way.
MUTANTS_SLOT_INCREMENTAL="${MUTANTS_SLOT_INCREMENTAL-}"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# The phrase scan beside the live stream, in keep-only mode: its exact kept
# count says whether cargo-mutants printed the no-mutants phrase, and nothing
# is written to a named file (bls_start, bls_finish).
# shellcheck source=scripts/bounded-log-scan.sh
source "${here}/bounded-log-scan.sh"
# cargo-mutants' ways of saying that it generated nothing to test.
NO_MUTANTS_REGEX='no relevant mutants|no mutants were generated|found 0 mutants|(^|[^0-9])0 mutants (to test|tested)'

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
  MUTANTS_RE            Optional regex for focused mutant proof, matched against --list names.
  MUTANTS_EXCLUDE       Optional file glob excluded after focused matching.
  MUTANTS_EXCLUDE_RE    Optional regex for extra focused-run exclusions.
  MUTANTS_MIRROR        Set to 1 (the Makefile's mutants-* targets do) to run smoke, diff
                        and full in place in mirror slots of the build cache root.
  MUTANTS_LOCAL_JOBS    Mirror mode: how many slots to run side by side (default: 1).
  MUTANTS_POOL_SIZE     Mirror mode: slots in the host-wide pool (default: 3).
  MUTANTS_SLOT_MAX_GIB  Mirror mode: empty a slot whose build + target exceed this (default: 24).
  LUSHTEXT_TEST_ADDRESS_SPACE  Mirror mode: per-test address-space limit (K/M/G/T or none).

Mirror mode (design D2 of openspec/changes/archive/2026-11-15-reduce-build-disk-writes): the
checkout's tracked and untracked, non-ignored files, uncommitted edits
included, are synced into slots under `scripts/build-cache.sh root`/mutants,
and each slot runs `cargo mutants --in-place` with its own fixed target and
build directories, so warm slots rebuild only what changed and no build
directory is left behind. N slots run `--shard i/N` side by side, never
`--jobs` (cargo-mutants refuses it with --in-place), and their reports are
merged into the checkout's mutants.out. A slot is taken with a non-blocking
lock: when none is free the run exits 75 and names the holders. The checkout
itself is never mutated.
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

    # Focused survivor proof uses cargo-mutants' name regex so the repo config
    # and its calibrated excludes remain in force while narrowing the run.
    if [[ -n "${MUTANTS_RE}" ]]; then
        args+=(--re "${MUTANTS_RE}")
    fi

    if [[ -n "${MUTANTS_EXCLUDE}" ]]; then
        args+=(--exclude "${MUTANTS_EXCLUDE}")
    fi

    if [[ -n "${MUTANTS_EXCLUDE_RE}" ]]; then
        args+=(--exclude-re "${MUTANTS_EXCLUDE_RE}")
    fi

    # Local runs set this to fan out across cores; CI leaves it empty so the
    # sharded small runners keep cargo-mutants' serial default.
    if [[ -n "${MUTANTS_JOBS}" ]]; then
        args+=(--jobs "${MUTANTS_JOBS}")
    fi

    printf '%s\n' "${args[@]}"
}

# Whether the last phrase scan finished and saw the no-mutants phrase.
no_mutants_seen() {
    (( BLS_COMPLETE == 1 && BLS_KEPT_COUNT > 0 ))
}

# Run `cargo mutants "${@:2}"` with the phrase scan beside its live stream,
# each live line prefixed with $1 when it is not empty. Returns cargo's exit
# status; no_mutants_seen then reads the scan.
cargo_mutants_scanned() {
    local prefix=$1 status
    shift
    local live=(cat)
    if [[ -n "${prefix}" ]]; then
        # The prefix is the replacement of a sed `s`: escape its specials.
        prefix=${prefix//\\/\\\\}
        prefix=${prefix//&/\\&}
        prefix=${prefix//\//\\/}
        live=(sed -u "s/^/${prefix}/")
    fi
    if ! bls_start '' '' "${NO_MUTANTS_REGEX}" --keep-ignore-case; then
        echo "Error: ${BLS__START_ERROR}; not running cargo mutants." >&2
        return 1
    fi
    set +e
    bls_isolated cargo mutants "$@" 2>&1 | tee -p "/dev/fd/${BLS_IN}" | "${live[@]}"
    status="${PIPESTATUS[0]}"
    set -e
    bls_finish
    bls_release
    return "${status}"
}

run_cargo_mutants() {
    local status=0
    cargo_mutants_scanned '' "$@" || status=$?
    if (( status != 0 )) && no_mutants_seen; then
        echo "No relevant mutants were generated; treating this mutation lane as passing."
        return 0
    fi
    return "${status}"
}

# A diff file the caller named (an argument or MUTANTS_DIFF_FILE) is used as
# it is. The default `git.diff` is written again on every run: an earlier
# run's copy is gitignored and survives in the checkout, and reusing it
# mutation-tests that run's change set (it failed one run on 2026-11-24 only
# because its lines no longer matched the source).
ensure_diff_file() {
    local diff_file="$1" named="$2"

    if [[ -n "${named}" ]]; then
        [[ -f "${diff_file}" ]] && return
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

    # An EXPLICIT diff path must exist. `ensure_diff_file` generates a
    # `git diff ${MUTANTS_BASE}...` three-dot diff when the named file is absent,
    # which is right for the default path but silently substitutes an unrelated
    # scope when a caller passes one. That failure mode is not theoretical: it
    # reported a plausible-looking "54 mutants: 1 missed" against a previous
    # slot's committed diff and was caught only because a survivor named a file
    # outside the intended set. Mirror run_smoke and fail instead.
    if [[ $# -ge 1 && -n "${1}" ]]; then
        [[ -f "${diff_file}" ]] || fail "diff file does not exist: ${diff_file}"
        [[ -s "${diff_file}" ]] || fail "diff file is empty: ${diff_file}"
    fi

    ensure_diff_file "${diff_file}" "${1:-${MUTANTS_DIFF_FILE_NAMED}}"

    # An empty hunk set means the run proved nothing. Exiting 0 here let a
    # mis-scoped or stale diff read as a pass.
    if ! grep -q '^@@ ' "${diff_file}"; then
        fail "no diff hunks found in ${diff_file}; nothing was mutation tested"
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

# --- mirror mode ---------------------------------------------------------------

slot_fds=()
slot_dirs=()
slot_pids=()

mirror_fail() {
    echo "run-mutants.sh: $*" >&2
    exit "${2:-1}"
}

# Release nothing explicitly: the locks are on descriptors only this process
# holds (every child closes them first), so they go when it exits.
close_slot_fds() {
    local fd
    for fd in "${slot_fds[@]}"; do
        eval "exec ${fd}>&-"
    done
}

cachedir_tag() {
    [[ -e "$1/CACHEDIR.TAG" ]] && return
    printf '%s\n' 'Signature: 8a477f597d28d172789f06886806bc55' \
        '# A LushText mutation slot (scripts/run-mutants.sh); it can be regenerated.' >"$1/CACHEDIR.TAG"
}

acquire_slots() {
    local want=$1 pool=$2 checkout=$3 k slot fd owner holders=() stale
    for (( k = 1; k <= MUTANTS_POOL_SIZE && ${#slot_fds[@]} < want; k++ )); do
        slot="${pool}/slot-${k}"
        mkdir -p "${slot}"
        cachedir_tag "${slot}"
        exec {fd}>>"${slot}/lock"
        if flock -n "${fd}"; then
            stale=""
            if [[ -s "${slot}/owner" ]]; then
                stale="$(tr '\n' ' ' <"${slot}/owner")"
                echo "run-mutants.sh: slot-${k}: replacing a stale owner record (${stale% }); its lock was free"
            fi
            printf 'pid=%s checkout=%s started=%s\n' "$$" "${checkout}" "$(date -Is)" >"${slot}/owner"
            slot_fds+=("${fd}")
            slot_dirs+=("${slot}")
        else
            eval "exec ${fd}>&-"
            owner="$(tr '\n' ' ' <"${slot}/owner" 2>/dev/null || true)"
            holders+=("slot-${k}: held by ${owner:-an unrecorded process}")
        fi
    done
    if (( ${#slot_fds[@]} == 0 )); then
        echo "run-mutants.sh: no free mutation slot in ${pool} (pool of ${MUTANTS_POOL_SIZE}); not waiting:" >&2
        printf '  %s\n' "${holders[@]}" >&2
        echo "run-mutants.sh: retry later, or with MUTANTS_LOCAL_JOBS=1 once one slot is free" >&2
        exit 75
    fi
    if (( ${#slot_fds[@]} < want )); then
        echo "run-mutants.sh: running on ${#slot_fds[@]} of ${want} requested slots (others busy):"
        printf '  %s\n' "${holders[@]}"
    fi
}

slot_size_kib() {
    du -sk "$1/build" "$1/target" 2>/dev/null | awk '{ s += $1 } END { print s + 0 }'
}

prepare_slot() {
    local slot=$1 checkout=$2 kib limit
    kib="$(slot_size_kib "${slot}")"
    limit=$(( MUTANTS_SLOT_MAX_GIB * 1024 * 1024 ))
    if (( kib > limit )); then
        echo "run-mutants.sh: $(basename "${slot}"): build + target hold $(( kib / 1024 )) MiB, over MUTANTS_SLOT_MAX_GIB=${MUTANTS_SLOT_MAX_GIB}; emptying them (this run builds it cold)"
        rm -rf "${slot}/build" "${slot}/target"
    fi
    (close_slot_fds; exec python3 "${here}/mutants-mirror.py" sync "${checkout}" "${slot}/src") | sed "s/^/[$(basename "${slot}")] /"
    return "${PIPESTATUS[0]}"
}

slot_runner_var() {
    local triple
    triple="$(rustc -vV | sed -n 's/^host: //p')"
    [[ -n "${triple}" ]] || mirror_fail "could not read the host triple from rustc -vV"
    triple="${triple^^}"
    printf 'CARGO_TARGET_%s_RUNNER\n' "${triple//-/_}"
}

# One slot: its environment, then cargo mutants in place from its src/.
run_slot() {
    local slot=$1 out=$2 shard=$3 runner_var=$4 wrapper=$5
    shift 5
    local name status started="${SECONDS}"
    name="$(basename "${slot}")"
    close_slot_fds
    cd "${slot}/src"
    export CARGO_TARGET_DIR="${slot}/target" CARGO_BUILD_BUILD_DIR="${slot}/build"
    export RUSTC_WORKSPACE_WRAPPER="${wrapper}"
    export "${runner_var}=${here}/test-address-space-runner.sh"
    if [[ -n "${MUTANTS_SLOT_INCREMENTAL}" ]]; then
        export CARGO_INCREMENTAL="${MUTANTS_SLOT_INCREMENTAL}"
    fi
    unset CARGO_MUTANTS_JOBS
    local args=("$@" --output "${out}")
    if [[ -n "${shard}" ]]; then
        args+=(--shard "${shard}")
    fi
    status=0
    cargo_mutants_scanned "[${name}] " "${args[@]}" || status=$?
    if (( status != 0 )) && no_mutants_seen; then
        echo "[${name}] No relevant mutants in this slot's share; counting it as passing."
        status=0
    fi
    if (( status == 4 )); then
        echo "[${name}] the unmutated baseline failed, so no mutant was tested; it ran under the per-test address-space limit LUSHTEXT_TEST_ADDRESS_SPACE=${LUSHTEXT_TEST_ADDRESS_SPACE:-none} (scripts/test-address-space-runner.sh): check its log for an allocation failure before raising the limit" >&2
    fi
    echo "[${name}] slot wall time $(( SECONDS - started )) s (shard ${shard:-all})"
    return "${status}"
}

# Remove this run's owner records while it still holds the slots' locks.
release_owner_records() {
    local slot
    for slot in "${slot_dirs[@]}"; do
        rm -f "${slot}/owner"
    done
}

forward_signal() {
    local pid
    for pid in "${slot_pids[@]}"; do
        kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
    done
    wait 2>/dev/null || true
    release_owner_records
    echo "run-mutants.sh: interrupted; the slots heal on their next sync (the checkout was never mutated)" >&2
    exit 143
}

run_mirror() {
    local checkout root pool wrapper runner_var want n k shard out status merged
    local statuses=() outs=()
    checkout="$(git rev-parse --show-toplevel 2>/dev/null)" || mirror_fail "mirror mode needs a git checkout"
    checkout="$(realpath "${checkout}")"
    want="${MUTANTS_LOCAL_JOBS}"
    [[ "${want}" =~ ^[1-9][0-9]*$ ]] || mirror_fail "MUTANTS_LOCAL_JOBS must be a positive integer, not '${want}'"
    (( want <= MUTANTS_POOL_SIZE )) || want="${MUTANTS_POOL_SIZE}"
    if [[ -n "${MUTANTS_SHARD}" ]] && (( want > 1 )); then
        mirror_fail "MUTANTS_SHARD=${MUTANTS_SHARD} with ${want} slots would compose two shardings ambiguously; set MUTANTS_LOCAL_JOBS=1 to run one shard"
    fi
    require_command flock
    root="$("${here}/build-cache.sh" root)"
    pool="${root}/mutants"
    mkdir -p "${pool}"
    "${here}/build-cache.sh" checkout "${checkout}" >/dev/null
    runner_var="$(slot_runner_var)"
    acquire_slots "${want}" "${pool}" "${checkout}"
    n="${#slot_dirs[@]}"
    # The workspace wrapper at one stable path: Cargo hashes its path into
    # every member's units, so a per-checkout path would rebuild every member
    # whenever runs from different checkouts alternate in a slot.
    wrapper="${pool}/rustc-passthrough.sh"
    if ! cmp -s "${here}/rustc-passthrough.sh" "${wrapper}"; then
        cp "${here}/rustc-passthrough.sh" "${wrapper}.tmp.$$"
        chmod 755 "${wrapper}.tmp.$$"
        mv -f "${wrapper}.tmp.$$" "${wrapper}"
    fi
    for slot in "${slot_dirs[@]}"; do
        prepare_slot "${slot}" "${checkout}" || mirror_fail "syncing ${slot} failed"
    done
    rm -rf "${checkout}/mutants.out.slots"
    echo "run-mutants.sh: ${n} slot(s) in ${pool}, in place, $([[ -n "${MUTANTS_SLOT_INCREMENTAL}" ]] && echo "CARGO_INCREMENTAL=${MUTANTS_SLOT_INCREMENTAL}" || echo "incremental as configured"), per-test address space ${LUSHTEXT_TEST_ADDRESS_SPACE:-unlimited}"
    trap forward_signal INT TERM HUP
    for (( k = 0; k < n; k++ )); do
        shard="${MUTANTS_SHARD}"
        if (( n > 1 )); then
            shard="${k}/${n}"
            out="${checkout}/mutants.out.slots/$(basename "${slot_dirs[k]}")"
        else
            out="${checkout}"
        fi
        mkdir -p "${out}"
        outs+=("${out}")
        set -m
        run_slot "${slot_dirs[k]}" "${out}" "${shard}" "${runner_var}" "${wrapper}" "$@" &
        set +m
        slot_pids+=("$!")
    done
    for (( k = 0; k < n; k++ )); do
        status=0
        wait "${slot_pids[k]}" || status=$?
        statuses+=("${status}")
        echo "run-mutants.sh: $(basename "${slot_dirs[k]}") finished with status ${status}"
    done
    trap - INT TERM HUP
    release_owner_records
    if (( n == 1 )); then
        (close_slot_fds; exec python3 "${here}/mutants-mirror.py" scan "${checkout}/mutants.out") || true
        return "${statuses[0]}"
    fi
    local specs=()
    for (( k = 0; k < n; k++ )); do
        specs+=("${statuses[k]}:${outs[k]}")
    done
    merged=0
    (close_slot_fds; exec python3 "${here}/mutants-mirror.py" merge "${checkout}" "${specs[@]}") || merged=$?
    rm -rf "${checkout}/mutants.out.slots"
    return "${merged}"
}

# The cargo-mutants arguments of a mirror-mode run: never --jobs (refused with
# --in-place) and never the in-checkout --in-place guard (the slot is mutated,
# not the checkout).
mirror_args() {
    MUTANTS_JOBS="" MUTANTS_IN_PLACE=0 mutants_args
    printf '%s\n' --in-place
}

mirror_smoke() {
    [[ -f "${MUTANTS_SMOKE_FILE}" ]] || fail "smoke file does not exist: ${MUTANTS_SMOKE_FILE}"
    mapfile -t args < <(mirror_args)
    run_mirror "${args[@]}" --no-config --file "${MUTANTS_SMOKE_FILE}"
}

mirror_diff() {
    local diff_file="${1:-${MUTANTS_DIFF_FILE}}"
    if [[ $# -ge 1 && -n "${1}" ]]; then
        [[ -f "${diff_file}" ]] || fail "diff file does not exist: ${diff_file}"
        [[ -s "${diff_file}" ]] || fail "diff file is empty: ${diff_file}"
    fi
    ensure_diff_file "${diff_file}" "${1:-${MUTANTS_DIFF_FILE_NAMED}}"
    if ! grep -q '^@@ ' "${diff_file}"; then
        fail "no diff hunks found in ${diff_file}; nothing was mutation tested"
    fi
    mapfile -t args < <(mirror_args)
    run_mirror "${args[@]}" --in-diff "$(realpath "${diff_file}")"
}

mirror_full() {
    mapfile -t args < <(mirror_args)
    run_mirror "${args[@]}"
}

# --- self-test -------------------------------------------------------------------

self_test_fail() {
    echo "run-mutants self-test: $*" >&2
    exit 1
}

self_test() {
    local dir me status out holder holders=()
    me="${here}/run-mutants.sh"
    dir="$(mktemp -d "${TMPDIR:-/tmp}/run-mutants-self-test.XXXXXX")"
    # shellcheck disable=SC2064 # expand now: the trap must remove this dir
    trap "jobs -p | xargs -r kill 2>/dev/null || true; rm -rf '${dir}'" EXIT
    unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_PREFIX GIT_OBJECT_DIRECTORY GIT_COMMON_DIR
    unset MUTANTS_SHARD MUTANTS_RE MUTANTS_EXCLUDE MUTANTS_EXCLUDE_RE MUTANTS_IN_PLACE MUTANTS_BASELINE_SKIP CI
    export LUSHTEXT_BUILD_CACHE="${dir}/cache"
    mkdir -p "${dir}/bin" "${dir}/repo/src"
    # A stub cargo: records its arguments and environment, and writes the
    # report cargo-mutants would write under --output.
    cat >"${dir}/bin/cargo" <<'STUB'
#!/usr/bin/env bash
out=""; prev=""
for a in "$@"; do [[ "$prev" == --output ]] && out="$a"; prev="$a"; done
# Two slots run this stub at once: build the record first and append it in
# one write, so their lines cannot interleave ("ARGSARGS ...").
record="ARGS$(printf ' %q' "$@")"
record+=$'\n'"$(printf 'ENV jobs=%s inc=%s wrapper=%s target=%s pwd=%s' "${CARGO_MUTANTS_JOBS-unset}" "${CARGO_INCREMENTAL-unset}" "${RUSTC_WORKSPACE_WRAPPER-unset}" "${CARGO_TARGET_DIR-unset}" "$PWD")"
printf '%s\n' "$record" >>"${STUB_LOG}"
if [[ -n "$out" ]]; then
    mkdir -p "$out/mutants.out/log"
    printf '{"outcomes": [], "total_mutants": 0, "missed": 0, "caught": 0, "timeout": 0, "unviable": 0, "success": 0, "cargo_mutants_version": "27.0.0"}\n' >"$out/mutants.out/outcomes.json"
    printf '[]\n' >"$out/mutants.out/mutants.json"
fi
exit "${STUB_STATUS:-0}"
STUB
    printf '#!/bin/sh\nexit 0\n' >"${dir}/bin/cargo-mutants"
    cp "${dir}/bin/cargo-mutants" "${dir}/bin/cargo-nextest"
    chmod +x "${dir}/bin/"*
    export PATH="${dir}/bin:${PATH}" STUB_LOG="${dir}/stub.log"
    cd "${dir}/repo"
    git init -q .
    printf 'fn f() {}\n' >src/lib.rs
    git add src/lib.rs
    git -c user.name=t -c user.email=t@example.invalid -c commit.gpgsign=false commit -q -m fixture
    mkdir -p crates/lushtext-model/src/document
    printf 'fn g() {}\n' >crates/lushtext-model/src/document/document_scale.rs

    # Two slots: --shard i/N each, --in-place, never --jobs, CARGO_MUTANTS_JOBS unset.
    : >"${STUB_LOG}"
    MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=2 MUTANTS_JOBS=2 CARGO_MUTANTS_JOBS=2 "${me}" smoke >"${dir}/out" 2>&1 \
        || self_test_fail "a two-slot smoke run failed: $(cat "${dir}/out")"
    [[ "$(grep -c '^ARGS' "${STUB_LOG}")" == 2 ]] || self_test_fail "expected two cargo mutants runs: $(cat "${STUB_LOG}")"
    if ! { grep -q -- '--shard 0/2' "${STUB_LOG}" && grep -q -- '--shard 1/2' "${STUB_LOG}"; }; then
        self_test_fail "missing --shard i/2: $(cat "${STUB_LOG}")"
    fi
    ! grep -q -- '--jobs' "${STUB_LOG}" || self_test_fail "a mirror-mode run passed --jobs: $(cat "${STUB_LOG}")"
    [[ "$(grep -c -- ' --in-place ' "${STUB_LOG}")" == 2 ]] || self_test_fail "a slot did not run --in-place"
    ! grep -q 'jobs=2' "${STUB_LOG}" || self_test_fail "CARGO_MUTANTS_JOBS reached cargo-mutants"
    grep -q "wrapper=${LUSHTEXT_BUILD_CACHE}/mutants/rustc-passthrough.sh" "${STUB_LOG}" || self_test_fail "the workspace wrapper is not the slot-stable copy"
    grep -q "pwd=${LUSHTEXT_BUILD_CACHE}/mutants/slot-1/src" "${STUB_LOG}" || self_test_fail "a slot did not run from its src/"
    [[ -f "${dir}/repo/mutants.out/outcomes.json" && ! -e "${dir}/repo/mutants.out.slots" ]] || self_test_fail "the slots' reports were not merged"
    [[ -f "${LUSHTEXT_BUILD_CACHE}/mutants/slot-1/src/crates/lushtext-model/src/document/document_scale.rs" ]] \
        || self_test_fail "an untracked file was not synced into the slot"

    # One slot writes the checkout's mutants.out itself (--output <checkout>).
    : >"${STUB_LOG}"
    MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=1 "${me}" smoke >"${dir}/out" 2>&1 || self_test_fail "a one-slot run failed: $(cat "${dir}/out")"
    grep -q -- "--output ${dir}/repo\( \|$\)" "${STUB_LOG}" || self_test_fail "one slot did not use --output <checkout>: $(cat "${STUB_LOG}")"
    ! grep -q -- '--shard' "${STUB_LOG}" || self_test_fail "one slot passed --shard"

    # A user shard with several slots is refused before any slot is taken.
    : >"${STUB_LOG}"
    rm -f "${LUSHTEXT_BUILD_CACHE}"/mutants/slot-*/owner
    status=0; MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=2 MUTANTS_SHARD=0/4 "${me}" full >"${dir}/out" 2>&1 || status=$?
    if ! { (( status == 1 )) && grep -q 'ambiguously' "${dir}/out"; }; then
        self_test_fail "MUTANTS_SHARD with 2 slots was not refused ($status): $(cat "${dir}/out")"
    fi
    if [[ -s "${STUB_LOG}" ]] || ls "${LUSHTEXT_BUILD_CACHE}"/mutants/slot-*/owner >/dev/null 2>&1; then
        self_test_fail "the refused run took a slot or ran cargo"
    fi

    # Every slot held: exit 75 without waiting, naming each holder.
    for k in 1 2 3; do
        mkdir -p "${LUSHTEXT_BUILD_CACHE}/mutants/slot-${k}"
        printf 'pid=%s checkout=/elsewhere-%s started=now\n' "999${k}" "${k}" >"${LUSHTEXT_BUILD_CACHE}/mutants/slot-${k}/owner"
        # The holder IS the sleeping process, so killing it frees the lock.
        (exec 9>>"${LUSHTEXT_BUILD_CACHE}/mutants/slot-${k}/lock"; flock -x 9; exec sleep 30) &
        holders+=("$!")
    done
    sleep 0.3
    status=0; timeout 10 env MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=1 "${me}" smoke >"${dir}/out" 2>&1 || status=$?
    (( status == 75 )) || self_test_fail "all slots held gave status ${status}, not 75: $(cat "${dir}/out")"
    for k in 1 2 3; do
        grep -q "slot-${k}: held by pid=999${k} checkout=/elsewhere-${k}" "${dir}/out" || self_test_fail "holder of slot-${k} not named: $(cat "${dir}/out")"
    done
    kill "${holders[@]}" 2>/dev/null || true
    wait "${holders[@]}" 2>/dev/null || true

    # A stale owner record with a free lock: the slot is taken normally.
    printf 'pid=1 checkout=/gone started=then\n' >"${LUSHTEXT_BUILD_CACHE}/mutants/slot-1/owner"
    MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=1 "${me}" smoke >"${dir}/out" 2>&1 || self_test_fail "a stale owner blocked the run: $(cat "${dir}/out")"
    grep -q 'slot-1: replacing a stale owner record (pid=1 checkout=/gone' "${dir}/out" || self_test_fail "the stale owner was not reported: $(cat "${dir}/out")"
    # A run removes its own records when it ends, so a record found later
    # means a run that was killed, not one that finished.
    [[ ! -e "${LUSHTEXT_BUILD_CACHE}/mutants/slot-1/owner" ]] || self_test_fail "the run left its owner record: $(cat "${LUSHTEXT_BUILD_CACHE}/mutants/slot-1/owner")"
    MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=1 "${me}" smoke >"${dir}/out" 2>&1 || self_test_fail "the second run failed: $(cat "${dir}/out")"
    ! grep -q 'stale owner record' "${dir}/out" || self_test_fail "a finished run's record read as stale: $(cat "${dir}/out")"

    # The lock is not inherited: a daemon a slot run leaves behind does not keep it.
    cat >"${dir}/bin/cargo" <<'STUB'
#!/usr/bin/env bash
setsid sleep 30 </dev/null >/dev/null 2>&1 &
echo "$!" >"${STUB_LOG}.daemon"
exit 0
STUB
    MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=1 "${me}" smoke >"${dir}/out" 2>&1 || self_test_fail "the daemon run failed: $(cat "${dir}/out")"
    holder="$(cat "${STUB_LOG}.daemon")"
    flock -n "${LUSHTEXT_BUILD_CACHE}/mutants/slot-1/lock" true || { kill "${holder}"; self_test_fail "a daemon left by the run kept slot-1's lock"; }
    kill "${holder}" 2>/dev/null || true

    # An oversized slot is emptied before the sync.
    mkdir -p "${LUSHTEXT_BUILD_CACHE}/mutants/slot-1/build"
    head -c 4096 /dev/zero >"${LUSHTEXT_BUILD_CACHE}/mutants/slot-1/build/unit"
    MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=1 MUTANTS_SLOT_MAX_GIB=0 "${me}" smoke >"${dir}/out" 2>&1 || self_test_fail "the oversized-slot run failed"
    if ! grep -q 'over MUTANTS_SLOT_MAX_GIB=0; emptying them' "${dir}/out" || [[ -e "${LUSHTEXT_BUILD_CACHE}/mutants/slot-1/build/unit" ]]; then
        self_test_fail "an oversized slot was not emptied: $(cat "${dir}/out")"
    fi

    # The CI modes and list keep their arguments (and mirror nothing).
    cat >"${dir}/bin/cargo" <<'STUB'
#!/usr/bin/env bash
{ printf 'ARGS'; printf ' %s' "$@"; printf '\n'; } >>"${STUB_LOG}"
STUB
    printf -- '--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-fn f() {}\n+fn f() { }\n' >"${dir}/pr.diff"
    : >"${STUB_LOG}"
    CI=true MUTANTS_MIRROR=1 "${me}" ci-pr "${dir}/pr.diff" >/dev/null 2>&1 || self_test_fail "ci-pr failed"
    CI=true MUTANTS_MIRROR=1 MUTANTS_SHARD=1/4 "${me}" ci-full >/dev/null 2>&1 || self_test_fail "ci-full failed"
    MUTANTS_MIRROR=1 "${me}" list >/dev/null 2>&1 || self_test_fail "list failed"
    out="$(cat "${STUB_LOG}")"
    [[ "${out}" == "ARGS mutants --workspace --test-workspace=true --test-tool nextest --no-shuffle --timeout 300 --baseline=skip --in-place --in-diff ${dir}/pr.diff
ARGS mutants --workspace --test-workspace=true --test-tool nextest --no-shuffle --timeout 300 --baseline=skip --in-place --shard 1/4
ARGS mutants --workspace --test-workspace=true --test-tool nextest --no-shuffle --timeout 300 --list" ]] \
        || self_test_fail "the CI/list arguments changed: ${out}"

    # The no-mutants phrase, in the mirror slot and in the CI path: seen, the
    # lane passes; absent, the exit status stays; either way nothing is left
    # in a private TMPDIR (the scan names no file).
    cat >"${dir}/bin/cargo" <<'STUB'
#!/usr/bin/env bash
[[ -z "${STUB_PHRASE:-}" ]] || printf 'INFO noise\n%s\nINFO done\n' "${STUB_PHRASE}"
exit "${STUB_STATUS:-0}"
STUB
    mkdir -p "${dir}/scan-base"
    if ! STUB_STATUS=4 STUB_PHRASE='Found 0 mutants to test' TMPDIR="${dir}/scan-base" MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=1 "${me}" smoke >"${dir}/out" 2>&1 \
        || ! grep -q 'No relevant mutants in this slot' "${dir}/out"; then
        self_test_fail "a slot's no-mutants phrase was not detected: $(cat "${dir}/out")"
    fi
    grep -qx '\[slot-1\] INFO noise' "${dir}/out" || self_test_fail "a slot's live line lost its [slot-1] prefix: $(cat "${dir}/out")"
    status=0; STUB_STATUS=4 TMPDIR="${dir}/scan-base" MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=1 "${me}" smoke >"${dir}/out" 2>&1 || status=$?
    (( status == 4 )) || self_test_fail "a slot without the phrase gave status ${status}, not 4: $(cat "${dir}/out")"
    if ! STUB_STATUS=2 STUB_PHRASE='No relevant mutants were found' TMPDIR="${dir}/scan-base" CI=true MUTANTS_MIRROR=1 MUTANTS_SHARD=1/4 "${me}" ci-full >"${dir}/out" 2>&1 \
        || ! grep -q 'treating this mutation lane as passing' "${dir}/out"; then
        self_test_fail "the CI path's no-mutants phrase was not detected: $(cat "${dir}/out")"
    fi
    grep -qx 'INFO noise' "${dir}/out" || self_test_fail "the CI path's live stream lost or prefixed a line: $(cat "${dir}/out")"
    status=0; STUB_STATUS=2 STUB_PHRASE='10 mutants tested' TMPDIR="${dir}/scan-base" CI=true MUTANTS_MIRROR=1 MUTANTS_SHARD=1/4 "${me}" ci-full >"${dir}/out" 2>&1 || status=$?
    (( status == 2 )) || self_test_fail "the CI path without the phrase gave status ${status}, not 2: $(cat "${dir}/out")"
    [[ -z "$(find "${dir}/scan-base" -mindepth 1 -print -quit)" ]] || self_test_fail "the phrase scan left $(find "${dir}/scan-base" -mindepth 1 | head -n 3)"

    # The default diff file is written again on every run, never reused.
    printf 'fn f() { let _ = 1; }\n' >src/lib.rs
    git add src/lib.rs
    git -c user.name=t -c user.email=t@example.invalid -c commit.gpgsign=false commit -q -m change
    printf 'diff --git a/stale.rs b/stale.rs\n@@ -1 +1 @@\n-stale\n+stale\n' >git.diff
    : >"${STUB_LOG}"
    MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=1 MUTANTS_BASE=HEAD~1 "${me}" diff >"${dir}/out" 2>&1 \
        || self_test_fail "a default-diff run failed: $(cat "${dir}/out")"
    git diff HEAD~1... >"${dir}/expected.diff"
    cmp -s git.diff "${dir}/expected.diff" || self_test_fail "a stale default git.diff was reused: $(cat git.diff)"
    # A diff file the caller names is used as it is.
    cp "${dir}/expected.diff" "${dir}/named.diff"
    printf '\n' >>"${dir}/named.diff"
    MUTANTS_MIRROR=1 MUTANTS_LOCAL_JOBS=1 MUTANTS_BASE=HEAD~1 MUTANTS_DIFF_FILE="${dir}/named.diff" "${me}" diff >"${dir}/out" 2>&1 \
        || self_test_fail "a named-diff run failed: $(cat "${dir}/out")"
    ! cmp -s "${dir}/named.diff" "${dir}/expected.diff" || self_test_fail "a named diff file was rewritten"

    echo "run-mutants self-test: ok"
}

if [[ "${MODE}" == "--self-test" ]]; then
    self_test
    exit 0
fi

ensure_tooling

case "${MODE}" in
    smoke)
        if [[ "${MUTANTS_MIRROR}" == "1" ]]; then mirror_smoke; else run_smoke; fi
        ;;
    diff)
        if [[ "${MUTANTS_MIRROR}" == "1" ]]; then mirror_diff "${2:-}"; else run_diff "${2:-}"; fi
        ;;
    full)
        if [[ "${MUTANTS_MIRROR}" == "1" ]]; then mirror_full; else run_full; fi
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
