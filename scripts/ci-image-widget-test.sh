#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Run widget tests in the CI widget job's own image, under a CPU limit, with
# the widget binary and the session helper (`gtk-lush-session`) already built
# on this host:
#
#   scripts/ci-image-widget-test.sh [--cpus N | --cpuset LIST] [--runs N]
#       [--memory SIZE] [--out DIR] [--binary PATH] [--timeout S]
#       [--image IMAGE] [--extra-packages "PKG ..."] [--env KEY=VALUE]...
#       (--shard NAME | -- <test-binary args>)
#   scripts/ci-image-widget-test.sh --self-test
#
# Why: a widget failure that passes in the toolbox can depend on the runner's
# image and CPU budget (the `ubuntu-latest` runner has 4 vCPUs). The F1 guard
# of `diagnose-ci-only-reflow-guard-failure` failed on every CI run, passed on
# every toolbox run, and failed deterministically here. Use this
# lane before spending CI rounds on a failure seen only on CI
# (`.agents/rules/build.md`, "CI image lane").
#
# What it does:
#   - reads the `widget-tests` job's container image and its `dnf` install
#     command from .github/workflows/ci.yml, so neither can drift, and caches
#     an image with those packages installed (`lushtext-ci-widget:<digest>`);
#   - refuses to run when the image's gtk4, libadwaita, gtksourceview5, pango
#     or glib2 version differs from this host's, because the host-built
#     binary is only valid against the same toolkit;
#   - bind-mounts the worktree and the cargo target and build directories
#     read-only at their own paths (SELinux labels disabled for this container only, so
#     nothing on the host is relabelled) and DIR read-write;
#   - runs CI's own widget step N times, each in a fresh container:
#     `scripts/widget-shards.py run <shard>` for `--shard` (after
#     `run-widget-tests.sh --self-test`, as CI does), else
#     `scripts/run-widget-tests.sh --headless --retries 1 -- <args>`, with
#     `cargo` shimmed to the prebuilt binary (the image has no Rust
#     toolchain), so the temp scope, the private session, the warning scan
#     and the retries are the runner's own;
#   - keeps each run's log in DIR and prints one verdict per run.
#
# `--shard NAME` runs one shard, one run per call (`--runs 1`): a shard run takes about 11 minutes, and
# one call holds the shared heavy-work lock once (`verification-runs.md`,
# "One lock per command"). Run it as
#   scripts/heavy.sh --mem none -- scripts/ci-image-widget-test.sh ...
# The container's own `--memory` (default 4g) caps it.
#
# `--cpus N` (a CFS quota, the default way to limit) does not pin. `--cpuset`
# is for a diagnostic that needs a cpuset; choose the list inside the CPUs
# this toolbox inherits. Never wrap the lane in `taskset`.
#
# Fidelity: the step is CI's; what differs is the machine (kernel, cgroup
# driver, virtualisation, load). Running the bare binary instead (its own
# headless self-supervision, no temp scope) failed two window tests that pass
# on CI and in the toolbox (`diagnose-ci-only-reflow-guard-failure` 4.1), so
# the lane never does that.
#
# `--image` replaces the base image for a diagnostic (the `dnf` line is still
# ci.yml's); `--extra-packages` adds packages (for example
# `adwaita-mono-fonts`); `--env` passes a variable to the binary.
#
# A run is green only when the step exits 0 with no `FLAKY:` or
# `FLAKY-SHARD:` line (both exit 0 on a retry-rescued pass) and, for a shard,
# the harness selected exactly the shard's tests. Exits 0 when every run was green, 1 when any run was not
# (or a check refused the run), 2 on a usage error, and 77 (the widget
# runner's unsupported-host code) when Docker is not available.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CI_WORKFLOW="$REPO_ROOT/.github/workflows/ci.yml"
WIDGET_JOB="widget-tests"
TOOLKIT_PACKAGES=(gtk4 libadwaita gtksourceview5 pango glib2)
UNSUPPORTED_HOST_EXIT_CODE=77

usage() {
    sed -n '6,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' >&2
    exit 2
}

die_usage() {
    echo "ci-image-widget-test: $*" >&2
    exit 2
}

# The `widget-tests` job's container image, from a ci.yml (`$1`).
ci_widget_image() {
    awk -v job="  $WIDGET_JOB:" '
        $0 == job { in_job = 1; next }
        in_job && /^  [A-Za-z0-9_-]+:/ { exit }
        in_job && $1 == "image:" { print $2; exit }
    ' "$1"
}

# The `widget-tests` job's `dnf ... install -y` command with its package
# lines joined, from a ci.yml (`$1`).
ci_widget_dnf_command() {
    awk -v job="  $WIDGET_JOB:" '
        $0 == job { in_job = 1; next }
        in_job && /^  [A-Za-z0-9_-]+:/ { exit }
        in_job && !in_dnf && $1 == "dnf" { in_dnf = 1 }
        in_dnf {
            line = $0
            sub(/^[ \t]+/, "", line)
            continues = sub(/[ \t]*\\$/, "", line)
            command = command (command == "" ? "" : " ") line
            if (!continues) { print command; exit }
        }
    ' "$1"
}

# The names `scripts/widget-shards.py list` assigns to shard `$1`, one per
# line, from its listing on stdin.
shard_names_from_listing() {
    awk -v shard="$1" '
        /^[^ ]/ { in_shard = ($1 == shard); next }
        in_shard && /^  / { sub(/^  /, ""); print }
    '
}

# `NAME-VERSION` of each toolkit package, from an `rpm -q` run by "$@".
toolkit_versions() {
    "$@" rpm -q --qf '%{NAME}-%{VERSION}\n' "${TOOLKIT_PACKAGES[@]}"
}

# One run's verdict from its exit status (`$1`), its `FLAKY:` line count
# (`$2`), the harness's `running N tests` count (`$3`) and, for a shard, the
# shard's size (`$4`). A retry-rescued pass is not green: the harness exits 0
# on it, and it is exactly the CI-only signature this lane exists to catch.
run_verdict() {
    local status=$1 flaky=$2 selected=$3 expected=${4:-}
    if [[ "$status" -ne 0 ]]; then
        echo "RED"
    elif [[ "$flaky" -gt 0 ]]; then
        echo "RED (FLAKY)"
    elif [[ -n "$expected" && "$selected" != "$expected" ]]; then
        echo "RED (selected ${selected:-none} of $expected)"
    else
        echo "green"
    fi
}

# Write the `cargo` shim into directory `$1` (see the call site).
write_cargo_shim() {
    mkdir -p "$1"
    cat >"$1/cargo" <<'EOF_SHIM'
#!/bin/sh
# ci-image-widget-test shim: `cargo test ... -- ARGS` runs the prebuilt widget
# binary with ARGS; any other cargo use is a lane defect.
case " $* " in
    *" test "*" --test widget "*) ;;
    *) echo "ci-image-widget-test: unexpected cargo call: $*" >&2; exit 2 ;;
esac
while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do shift; done
[ "$#" -gt 0 ] && shift
exec "$LANE_WIDGET_BINARY" "$@"
EOF_SHIM
    chmod +x "$1/cargo"
}

self_test() {
    local dir status out
    dir="$(mktemp -d "${TMPDIR:-/tmp}/ci-image-widget-test-self-test.XXXXXX")"
    # shellcheck disable=SC2064 # expand now: the trap must remove this dir
    trap "rm -rf '$dir'" EXIT

    fail() { echo "ci-image-widget-test self-test: $*" >&2; exit 1; }

    # Job-scoped extraction: a decoy job before and after `widget-tests`
    # carries another image and another package set.
    cat >"$dir/ci.yml" <<'EOF'
jobs:
  gtk-axioms-floor:
    container:
      image: registry.example/fedora:44
    steps:
      - run: |
          dnf install -y \
            decoy-before
  widget-tests:
    name: Widget Tests
    container:
      image: registry.example/fedora:45
    steps:
      - name: Install system dependencies
        run: |
          dnf --no-docs --setopt=install_weak_deps=False install -y \
            gtk4-devel \
            mutter \
            python3

      - name: Run widget tests
        run: echo dnf not-this-one
  bench-compile:
    container:
      image: registry.example/fedora:43
    steps:
      - run: dnf install -y decoy-after
EOF
    out="$(ci_widget_image "$dir/ci.yml")"
    [[ "$out" == "registry.example/fedora:45" ]] || fail "image extraction gave '$out'"
    out="$(ci_widget_dnf_command "$dir/ci.yml")"
    [[ "$out" == "dnf --no-docs --setopt=install_weak_deps=False install -y gtk4-devel mutter python3" ]] \
        || fail "dnf extraction gave '$out'"

    # The real workflow: the widget job's image and a dnf line that installs
    # the headless session (the compositor, the bus daemon, and the
    # dbus-monitor the session helper watches its buses with), which no
    # non-GTK job has. A `*packages` array is an inventory, not a spawn
    # (`make check-session-hygiene`).
    out="$(ci_widget_image "$CI_WORKFLOW")"
    [[ "$out" == registry.fedoraproject.org/fedora:* ]] || fail "ci.yml widget image is '$out'"
    out="$(ci_widget_dnf_command "$CI_WORKFLOW")"
    local widget_packages=(gtk4-devel libadwaita-devel gtksourceview5-devel mutter dbus-daemon dbus-tools python3)
    for package in "${widget_packages[@]}"; do
        [[ " $out " == *" $package "* ]] || fail "ci.yml widget dnf line lacks $package: '$out'"
    done
    [[ "$out" == dnf\ *install\ -y\ * ]] || fail "ci.yml widget dnf line is not an install: '$out'"

    # Shard names from a listing, and from the real table.
    out="$(printf '%s\n' 'window (1 min): 2 tests' '  window::a' '  window::b' \
        'editor-page (1 min): 1 tests' '  editor_page::c' | shard_names_from_listing window | paste -sd,)"
    [[ "$out" == "window::a,window::b" ]] || fail "shard parsing gave '$out'"
    local listing count
    listing="$(python3 "$REPO_ROOT/scripts/widget-shards.py" list)"
    for shard in window editor-page surfaces; do
        count="$(shard_names_from_listing "$shard" <<<"$listing" | wc -l)"
        local want
        want="$(awk -v shard="$shard" '$1 == shard { print $(NF - 1) }' <<<"$listing")"
        [[ "$count" -gt 0 && "$count" == "$want" ]] || fail "shard $shard: $count names, listing says $want"
    done

    # Verdicts: only a clean, complete pass is green.
    [[ "$(run_verdict 0 0 1 '')" == green ]] || fail "a clean pass is not green"
    [[ "$(run_verdict 1 0 1 '')" == RED ]] || fail "a failure is not red"
    [[ "$(run_verdict 0 1 1 '')" == "RED (FLAKY)" ]] || fail "a retry-rescued pass is not red"
    [[ "$(run_verdict 0 0 426 426)" == green ]] || fail "a complete shard is not green"
    [[ "$(run_verdict 0 0 425 426)" == "RED (selected 425 of 426)" ]] || fail "a short shard is not red"
    [[ "$(run_verdict 0 0 '' 426)" == "RED (selected none of 426)" ]] || fail "a shard with no count is not red"

    # The cargo shim runs the binary with the arguments after `--`, for both
    # of CI's cargo calls, and refuses anything else.
    write_cargo_shim "$dir/shim"
    printf '#!/bin/sh\necho "args:$*"\n' >"$dir/fake-widget"
    chmod +x "$dir/fake-widget"
    out="$(LANE_WIDGET_BINARY="$dir/fake-widget" "$dir/shim/cargo" test -p lushtext --test widget -- --exact a::b)"
    [[ "$out" == "args:--exact a::b" ]] || fail "shim ran '$out'"
    out="$(LANE_WIDGET_BINARY="$dir/fake-widget" "$dir/shim/cargo" test -p lushtext --test widget -- --list --format terse)"
    [[ "$out" == "args:--list --format terse" ]] || fail "shim listed '$out'"
    status=0
    LANE_WIDGET_BINARY="$dir/fake-widget" "$dir/shim/cargo" build -p lushtext >/dev/null 2>&1 || status=$?
    [[ "$status" -eq 2 ]] || fail "shim accepted a cargo build (exit $status)"

    # Argument errors, all refused before Docker is touched.
    expect_usage() {
        status=0
        "$0" "$@" >/dev/null 2>&1 || status=$?
        [[ "$status" -eq 2 ]] || fail "'$*' exited $status, expected 2"
    }
    expect_usage
    expect_usage --runs 2 --shard window
    expect_usage --shard no-such-shard
    expect_usage --cpus 4 --cpuset 0-3 -- --exact x
    expect_usage --cpus 0 -- --exact x
    expect_usage --cpus four -- --exact x
    expect_usage --runs 0 -- --exact x
    expect_usage --shard window -- --exact x
    expect_usage --env NOEQUALS -- --exact x
    expect_usage --bogus
    expect_usage --runs
    echo "ci-image-widget-test self-test passed"
}

CPUS=""
CPUSET=""
RUNS=1
MEMORY="4g"
OUT_DIR=""
BINARY=""
TIMEOUT_S=2400
IMAGE_OVERRIDE=""
EXTRA_PACKAGES=""
SHARD=""
ENV_VARS=()
TEST_ARGS=()
HAVE_TEST_ARGS=false

need_value() { [[ $# -ge 2 && -n "$2" ]] || die_usage "$1 requires a value"; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --self-test) self_test; exit 0 ;;
        --cpus) need_value "$@"; CPUS="$2"; shift 2 ;;
        --cpuset) need_value "$@"; CPUSET="$2"; shift 2 ;;
        --runs) need_value "$@"; RUNS="$2"; shift 2 ;;
        --memory) need_value "$@"; MEMORY="$2"; shift 2 ;;
        --out) need_value "$@"; OUT_DIR="$2"; shift 2 ;;
        --binary) need_value "$@"; BINARY="$2"; shift 2 ;;
        --timeout) need_value "$@"; TIMEOUT_S="$2"; shift 2 ;;
        --image) need_value "$@"; IMAGE_OVERRIDE="$2"; shift 2 ;;
        --extra-packages) need_value "$@"; EXTRA_PACKAGES="$2"; shift 2 ;;
        --env) need_value "$@"; [[ "$2" == [A-Za-z_]*=* ]] || die_usage "--env takes KEY=VALUE, got '$2'"
            ENV_VARS+=("$2"); shift 2 ;;
        --shard) need_value "$@"; SHARD="$2"; shift 2 ;;
        --) shift; TEST_ARGS=("$@"); HAVE_TEST_ARGS=true; break ;;
        -h|--help) usage ;;
        *) die_usage "unknown argument '$1'" ;;
    esac
done

[[ -n "$CPUS" && -n "$CPUSET" ]] && die_usage "--cpus and --cpuset are exclusive"
[[ -z "$CPUS" || "$CPUS" =~ ^[0-9]+(\.[0-9]+)?$ ]] || die_usage "--cpus takes a number, got '$CPUS'"
[[ -z "$CPUS" || "$CPUS" != 0 && "$CPUS" != 0.0 ]] || die_usage "--cpus must be positive"
[[ -z "$CPUSET" || "$CPUSET" =~ ^[0-9,-]+$ ]] || die_usage "--cpuset takes a CPU list, got '$CPUSET'"
[[ "$RUNS" =~ ^[1-9][0-9]*$ ]] || die_usage "--runs takes a positive integer, got '$RUNS'"
[[ "$TIMEOUT_S" =~ ^[1-9][0-9]*$ ]] || die_usage "--timeout takes seconds, got '$TIMEOUT_S'"
if [[ -n "$SHARD" ]]; then
    $HAVE_TEST_ARGS && die_usage "--shard and -- <test-binary args> are exclusive"
    [[ "$RUNS" -eq 1 ]] || die_usage "--shard takes --runs 1: one heavy.sh call per shard run"
    mapfile -t SHARD_NAMES < <(python3 "$REPO_ROOT/scripts/widget-shards.py" list | shard_names_from_listing "$SHARD")
    [[ ${#SHARD_NAMES[@]} -gt 0 ]] || die_usage "no such widget shard '$SHARD'"
    TEST_ARGS=(--exact "${SHARD_NAMES[@]}")
elif ! $HAVE_TEST_ARGS || [[ ${#TEST_ARGS[@]} -eq 0 ]]; then
    die_usage "give --shard NAME or -- <test-binary args>"
fi

if ! command -v docker >/dev/null 2>&1 || ! docker info >/dev/null 2>&1; then
    echo "ci-image-widget-test: Docker is not available on this host; skipping (exit $UNSUPPORTED_HOST_EXIT_CODE)." >&2
    exit "$UNSUPPORTED_HOST_EXIT_CODE"
fi

BASE_IMAGE="${IMAGE_OVERRIDE:-$(ci_widget_image "$CI_WORKFLOW")}"
DNF_COMMAND="$(ci_widget_dnf_command "$CI_WORKFLOW")"
[[ -n "$BASE_IMAGE" && -n "$DNF_COMMAND" ]] || { echo "ci-image-widget-test: cannot read the $WIDGET_JOB job from $CI_WORKFLOW" >&2; exit 1; }
SETUP="$DNF_COMMAND"
[[ -n "$EXTRA_PACKAGES" ]] && SETUP="$SETUP && dnf --no-docs --setopt=install_weak_deps=False install -y $EXTRA_PACKAGES"
DIGEST="$(printf '%s\n%s\n' "$BASE_IMAGE" "$SETUP" | sha256sum | cut -c1-12)"
LANE_IMAGE="lushtext-ci-widget:$DIGEST"

CONTAINER=""
setup_log=""
# Removes the run's container and the image-setup log on every exit path.
cleanup() {
    [[ -z "$setup_log" ]] || rm -f "$setup_log"
    [[ -n "$CONTAINER" ]] && docker rm -f "$CONTAINER" >/dev/null 2>&1
    true
}
trap cleanup EXIT
trap 'cleanup; exit 130' INT TERM

if ! docker image inspect "$LANE_IMAGE" >/dev/null 2>&1; then
    echo "ci-image-widget-test: preparing $LANE_IMAGE from $BASE_IMAGE"
    builder="lushtext-ci-widget-build-$$"
    docker rm -f "$builder" >/dev/null 2>&1 || true
    setup_log="$(mktemp "${TMPDIR:-/tmp}/ci-image-widget-test-setup.XXXXXX")"
    if ! docker run --name "$builder" "$BASE_IMAGE" bash -c "$SETUP" >"$setup_log" 2>&1; then
        docker rm -f "$builder" >/dev/null 2>&1 || true
        tail -n 20 "$setup_log" >&2
        echo "ci-image-widget-test: installing the widget job's packages failed" >&2
        exit 1
    fi
    rm -f "$setup_log"
    setup_log=""
    docker commit "$builder" "$LANE_IMAGE" >/dev/null
    docker rm -f "$builder" >/dev/null
fi

HOST_VERSIONS="$(toolkit_versions)"
IMAGE_VERSIONS="$(toolkit_versions docker run --rm "$LANE_IMAGE")"
if [[ "$HOST_VERSIONS" != "$IMAGE_VERSIONS" ]]; then
    echo "ci-image-widget-test: the image's toolkit differs from this host's; the host-built binary is not valid there." >&2
    diff <(echo "$HOST_VERSIONS") <(echo "$IMAGE_VERSIONS") | sed 's/^/  /' >&2 || true
    exit 1
fi

# Test executables live in Cargo's build directory (`build-dir`, which may be
# outside the target directory); mount both.
read -r TARGET_DIR BUILD_DIR < <(cd "$REPO_ROOT" && cargo metadata --no-deps --format-version 1 \
    | python3 -c 'import json, sys; d = json.load(sys.stdin); print(d["target_directory"], d.get("build_directory") or d["target_directory"])')
if [[ -z "$BINARY" ]]; then
    BINARY="$(find "$BUILD_DIR/debug/deps" -maxdepth 1 -type f -name 'widget-*' -perm -u+x \
        ! -name '*.*' -printf '%T@ %p\n' 2>/dev/null | sort -rn | head -n1 | cut -d' ' -f2-)"
    [[ -n "$BINARY" ]] || { echo "ci-image-widget-test: no widget binary under $BUILD_DIR/debug/deps; build it first (scripts/heavy.sh -- cargo test -p lushtext --test widget --no-run)" >&2; exit 1; }
fi
[[ -x "$BINARY" ]] || { echo "ci-image-widget-test: $BINARY is not an executable" >&2; exit 1; }
BINARY="$(realpath "$BINARY")"
[[ "$BINARY" == "$BUILD_DIR"/* || "$BINARY" == "$TARGET_DIR"/* ]] \
    || { echo "ci-image-widget-test: $BINARY is outside the build and target directories ($BUILD_DIR, $TARGET_DIR)" >&2; exit 1; }
# The widget launcher starts its private session through
# scripts/headless-session, which builds the session helper with cargo unless
# GTK_LUSH_SESSION_BIN names one; the image has no cargo, so the lane passes
# the host-built helper (CI's step builds it the same way). An --env setting
# of GTK_LUSH_SESSION_BIN wins.
SESSION_BIN=""
if ! printf '%s\n' "${ENV_VARS[@]}" | grep -q '^GTK_LUSH_SESSION_BIN='; then
    SESSION_BIN="$TARGET_DIR/debug/gtk-lush-session"
    [[ -x "$SESSION_BIN" ]] || { echo "ci-image-widget-test: no session helper at $SESSION_BIN; build it first (scripts/heavy.sh -- cargo build -p gtk-lush-proof-harness --bin gtk-lush-session)" >&2; exit 1; }
fi

if [[ -z "$OUT_DIR" ]]; then
    OUT_DIR="$TARGET_DIR/ci-image-widget-test/$(date +%Y%m%dT%H%M%S)-$$"
fi
mkdir -p "$OUT_DIR"
OUT_DIR="$(realpath "$OUT_DIR")"

LIMIT_ARGS=()
LIMIT_LABEL="unlimited"
if [[ -n "$CPUS" ]]; then
    LIMIT_ARGS=(--cpus "$CPUS")
    LIMIT_LABEL="--cpus=$CPUS"
elif [[ -n "$CPUSET" ]]; then
    LIMIT_ARGS=(--cpuset-cpus "$CPUSET")
    LIMIT_LABEL="--cpuset=$CPUSET"
fi
# CI's step runs `cargo test -p lushtext --test widget -- <args>` through
# run-widget-tests.sh (and `-- --list` through widget-shards.py); the image
# has no Rust toolchain, so this shim runs the prebuilt binary with the
# arguments after `--` instead, and both scripts run unchanged.
SHIM_DIR="$OUT_DIR/.lane-bin"
write_cargo_shim "$SHIM_DIR"
CONTAINER_ENV=(-e "LANE_WIDGET_BINARY=$BINARY" -e "PATH=$SHIM_DIR:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin")
[[ -n "$SESSION_BIN" ]] && CONTAINER_ENV+=(-e "GTK_LUSH_SESSION_BIN=$SESSION_BIN")
for setting in "${ENV_VARS[@]}"; do CONTAINER_ENV+=(-e "$setting"); done
if [[ -n "$SHARD" ]]; then
    # CI's widget step, verbatim but for the measurement file.
    # shellcheck disable=SC2016 # expanded by the container's shell
    LANE_STEP=(bash -c 'scripts/run-widget-tests.sh --self-test >/dev/null && exec python3 scripts/widget-shards.py run "$0"' "$SHARD")
else
    LANE_STEP=(scripts/run-widget-tests.sh --headless --retries 1 -- "${TEST_ARGS[@]}")
fi

echo "ci-image-widget-test: image $LANE_IMAGE ($BASE_IMAGE${EXTRA_PACKAGES:+ + $EXTRA_PACKAGES}), $LIMIT_LABEL, memory $MEMORY, $RUNS run(s)"
echo "ci-image-widget-test: binary $BINARY ($(date -r "$BINARY" -Is))"
echo "ci-image-widget-test: ${#TEST_ARGS[@]} binary argument(s)${SHARD:+, shard $SHARD (${#SHARD_NAMES[@]} tests)}; logs in $OUT_DIR"
echo "ci-image-widget-test: runs CI's step (run-widget-tests.sh --headless --retries 1, or widget-shards.py run) with cargo shimmed to the binary; it differs from the runner in kernel, cgroup driver and virtualisation"

failed_runs=0
for run in $(seq 1 "$RUNS"); do
    CONTAINER="lushtext-ci-widget-run-$$-$run"
    log="$OUT_DIR/run-$run.log"
    started=$SECONDS
    status=0
    {
        echo "== lane run $run: $LIMIT_LABEL, $(date -Is)"
        docker run --rm --name "$CONTAINER" --security-opt label=disable \
            "${LIMIT_ARGS[@]}" --memory "$MEMORY" --memory-swap "$MEMORY" \
            -v "$REPO_ROOT:$REPO_ROOT:ro" -v "$TARGET_DIR:$TARGET_DIR:ro" -v "$BUILD_DIR:$BUILD_DIR:ro" \
            -v "$OUT_DIR:$OUT_DIR" \
            -w "$REPO_ROOT" "${CONTAINER_ENV[@]}" "$LANE_IMAGE" \
            bash -c 'echo "nproc=$(nproc) fc-match monospace: $(fc-match monospace)"; ls /dev/dri 2>&1 | head -n1; exec timeout "$0" "$@"' \
            "$TIMEOUT_S" "${LANE_STEP[@]}"
    } >"$log" 2>&1 || status=$?
    CONTAINER=""
    flaky="$(grep -cE '^ *FLAKY(-SHARD)?: ' "$log" || true)"
    result="$(grep -m1 '^test result:' "$log" || echo "no test result (exit $status)")"
    selected="$(sed -n 's/^running \([0-9][0-9]*\) tests$/\1/p' "$log" | head -n1)"
    verdict="$(run_verdict "$status" "$flaky" "$selected" "${SHARD:+${#SHARD_NAMES[@]}}")"
    [[ "$verdict" == green ]] || failed_runs=$((failed_runs + 1))
    echo "run $run: $verdict, exit $status, $((SECONDS - started)) s, ${selected:-?} test(s) selected, $flaky FLAKY line(s): $result ($log)"
    grep -E '^(thread .* panicked|    FLAKY: )' "$log" | cut -c1-300 | sed 's/^/    /' || true
done
echo "ci-image-widget-test: $((RUNS - failed_runs)) of $RUNS run(s) green ($LIMIT_LABEL)"
[[ "$failed_runs" -eq 0 ]]
