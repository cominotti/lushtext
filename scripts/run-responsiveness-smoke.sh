#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Long-line responsiveness lane (`make responsiveness-smoke`).
#
# Runs the measurement build of LushText (built with the smoke-only
# `responsiveness-probes` feature into target/responsiveness/, never the
# ordinary target/debug binary) under a private `dbus-run-session` and
# `mutter --headless`, sweeps the synthetic long-line fixtures, and writes
# report.json and report.md with the derived thresholds and budget verdict.
# It never touches the developer's live desktop session.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/responsiveness}"
BINARY="${RESPONSIVENESS_BINARY:-$REPO_ROOT/target/responsiveness/release/lushtext}"
TIER="${RESPONSIVENESS_TIER:-quick}"
AXIOM_OBSERVATIONS="${RESPONSIVENESS_AXIOM_OBSERVATIONS:-}"
MAX_CPUS=16
CASES=()
DRIVER="$REPO_ROOT/scripts/responsiveness-smoke-driver.py"
PYTHON=/usr/bin/python3

usage() {
    cat <<'EOF'
Usage: scripts/run-responsiveness-smoke.sh [--artifact-dir DIR] [--binary PATH]
                                           [--tier quick|full] [--case PATTERN]...
                                           [--axiom-observations FILE]
                                           [--list-cases] [--self-test]

Environment: RESPONSIVENESS_TIER, RESPONSIVENESS_CASES (comma-separated
patterns), RESPONSIVENESS_CASE_CAP_S (per-interaction wall cap, default 20),
RESPONSIVENESS_INSTALL_CAP_S (default 180), RESPONSIVENESS_VARIANTS
(default plain,at,probe-im,ibus), RESPONSIVENESS_REPETITIONS,
RESPONSIVENESS_BISECT (0|1), RESPONSIVENESS_RENDERER (cairo|ngl),
RESPONSIVENESS_MONITOR (default 1280x800@120), RESPONSIVENESS_LENGTHS
(probe points per group, e.g. paragraph=2000,4500;dense=500) and
RESPONSIVENESS_PHASES (only these interactions; open and background always
run), both used by scripts/responsiveness-reference-sweep.py.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact-dir)
            [[ $# -lt 2 ]] && smoke_fail "--artifact-dir requires a value"
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        --binary)
            [[ $# -lt 2 ]] && smoke_fail "--binary requires a value"
            BINARY="$2"
            shift 2
            ;;
        --tier)
            [[ $# -lt 2 ]] && smoke_fail "--tier requires a value"
            TIER="$2"
            shift 2
            ;;
        --case)
            [[ $# -lt 2 ]] && smoke_fail "--case requires a value"
            CASES+=("$2")
            shift 2
            ;;
        --axiom-observations)
            [[ $# -lt 2 ]] && smoke_fail "--axiom-observations requires a value"
            AXIOM_OBSERVATIONS="$2"
            shift 2
            ;;
        --list-cases)
            exec "$PYTHON" "$DRIVER" list-cases --tier "$TIER"
            ;;
        --self-test)
            "$PYTHON" "$REPO_ROOT/scripts/generate-long-line-fixtures.py" --self-test
            "$PYTHON" "$REPO_ROOT/scripts/responsiveness-reference-sweep.py" --self-test
            exec "$PYTHON" "$DRIVER" self-test
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            smoke_fail "unknown argument: $1"
            ;;
    esac
done

case "$TIER" in
    quick | full) ;;
    *) smoke_fail "--tier must be quick or full" ;;
esac
if [[ -n "${RESPONSIVENESS_CASES:-}" ]]; then
    IFS=',' read -r -a extra_cases <<<"$RESPONSIVENESS_CASES"
    CASES+=("${extra_cases[@]}")
fi

smoke_require_command dbus-run-session
smoke_require_command mutter
smoke_require_command gdbus
[[ -x "$PYTHON" ]] || smoke_skip "/usr/bin/python3 is not available."
"$PYTHON" -c 'import gi; gi.require_version("Gio", "2.0")' 2>/dev/null || smoke_skip "PyGObject is not available."
[[ -x "$BINARY" ]] || smoke_fail "measurement binary is missing: $BINARY (make responsiveness-build builds it)"

# Never reach the live desktop: every GUI process below runs inside the
# private session this script creates, with these variables cleared.
unset WAYLAND_DISPLAY DISPLAY WAYLAND_SOCKET

# At most 16 CPUs, taken from the inherited affinity mask (never widened).
affinity_list() {
    "$PYTHON" -c '
import os, sys
cpus = sorted(os.sched_getaffinity(0))[: int(sys.argv[1])]
ranges = []
for cpu in cpus:
    if ranges and cpu == ranges[-1][1] + 1:
        ranges[-1][1] = cpu
    else:
        ranges.append([cpu, cpu])
print(",".join(f"{a}-{b}" if a != b else str(a) for a, b in ranges))
' "$MAX_CPUS"
}
INHERITED_AFFINITY="$("$PYTHON" -c 'import os; print(len(os.sched_getaffinity(0)))')"
AFFINITY="$(affinity_list)"
TASKSET=()
if [[ "$INHERITED_AFFINITY" -gt "$MAX_CPUS" ]]; then
    command -v taskset >/dev/null 2>&1 || smoke_fail "taskset is required to cap the lane at $MAX_CPUS CPUs"
    TASKSET=(taskset -c "$AFFINITY")
fi

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
# Start from a clean artifact root: stale case directories would be read as
# this run's evidence.
find "$ARTIFACT_DIR" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
mkdir -p "$ARTIFACT_DIR/logs"
export GSK_RENDERER="${RESPONSIVENESS_RENDERER:-cairo}"
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"
{
    echo "tier=$TIER"
    echo "binary=$BINARY"
    echo "inherited_cpus=$INHERITED_AFFINITY"
    echo "affinity=$AFFINITY"
    echo "taskset=${TASKSET[*]:-none}"
    echo "monitor=${RESPONSIVENESS_MONITOR:-1280x800@120}"
    echo "pango_version=$(pkg-config --modversion pango 2>/dev/null || true)"
    echo "mutter=$(mutter --version 2>/dev/null | head -n1 || true)"
    echo "cpu_model=$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //' || true)"
    echo "mem_available_kib=$(awk '/MemAvailable/ {print $2}' /proc/meminfo)"
} >>"$ARTIFACT_DIR/environment.txt"

CASE_ARGS=()
for pattern in "${CASES[@]}"; do
    CASE_ARGS+=(--case "$pattern")
done

set +e
"${TASKSET[@]}" "$REPO_ROOT/scripts/with-temp-scope.sh" --session-runtime responsiveness -- \
    dbus-run-session -- "$PYTHON" "$DRIVER" session \
    --artifact-dir "$ARTIFACT_DIR" --binary "$BINARY" --tier "$TIER" "${CASE_ARGS[@]}" \
    2>&1 | tee "$ARTIFACT_DIR/logs/lane.log"
status="${PIPESTATUS[0]}"
set -e

REPORT_ARGS=(report --artifact-dir "$ARTIFACT_DIR")
if [[ -n "$AXIOM_OBSERVATIONS" ]]; then
    REPORT_ARGS+=(--axiom-observations "$AXIOM_OBSERVATIONS")
fi
report_status=0
"$PYTHON" "$DRIVER" "${REPORT_ARGS[@]}" || report_status=$?

if [[ "$status" -ne 0 || "$report_status" -ne 0 ]]; then
    smoke_fail "responsiveness smoke failed (session $status, report $report_status). Artifacts: $ARTIFACT_DIR"
fi
echo "PASS: responsiveness smoke ($TIER) completed. Report: $ARTIFACT_DIR/report.md"
