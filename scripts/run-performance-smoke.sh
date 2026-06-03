#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/performance}"
FILTERS="${LUSHTEXT_PERFORMANCE_SMOKE_FILTER:-file_index_search file_index_rebuild content_search_smoke json_persistence editor_file_io replace_undo_workflows}"
SAMPLE_SIZE="${LUSHTEXT_PERFORMANCE_SMOKE_SAMPLE_SIZE:-10}"
MEASUREMENT_TIME="${LUSHTEXT_PERFORMANCE_SMOKE_MEASUREMENT_TIME:-1}"
WARM_UP_TIME="${LUSHTEXT_PERFORMANCE_SMOKE_WARM_UP_TIME:-1}"

usage() {
    cat <<'EOF'
Usage: scripts/run-performance-smoke.sh [--artifact-dir DIR] [--filter BENCH]

Run small Criterion smoke passes with coarse timing artifacts. This is distinct
from full benchmark reports and is intended as a quick sanity check. The default
filter set covers file indexing, command-palette search, workspace-wide content
search, persistence, editor file I/O, and replace/undo workflows.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact-dir)
            [[ $# -lt 2 ]] && smoke_fail "--artifact-dir requires a value"
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        --filter)
            [[ $# -lt 2 ]] && smoke_fail "--filter requires a value"
            FILTERS="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            smoke_fail "unknown argument: $1"
            ;;
    esac
done

smoke_require_command cargo

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"
{
    echo "filters=$FILTERS"
    echo "sample_size=$SAMPLE_SIZE"
    echo "warm_up_time=$WARM_UP_TIME"
    echo "measurement_time=$MEASUREMENT_TIME"
    echo "profile=bench"
} >"$ARTIFACT_DIR/config.txt"

cat >"$ARTIFACT_DIR/fixtures.txt" <<'EOF'
file_index_search: generated command-palette indexes at representative file counts
file_index_rebuild: generated workspace file lists at representative file counts
content_search_smoke: generated 200-file trees plus one 10k-line file
json_persistence: generated workspace/session JSON save and load fixtures
editor_file_io: generated text files for load, save, and Save As-equivalent explicit-path writes
replace_undo_workflows: generated disposable files for Replace All and undo restore
EOF

cat >"$ARTIFACT_DIR/thresholds.txt" <<'EOF'
This smoke lane is a coarse regression tripwire, not the release benchmark gate.

Coarse smoke thresholds:
- first-window readiness: target under 5s on a developer workstation; investigate over 10s
- representative small/medium file open: target under 500ms; investigate over 2s
- command-palette searches: target interactive-scale, not multi-second
- workspace/content search: must complete every generated fixture without stalling
- persistence, editor file I/O, Replace All, and undo restore: must complete every smoke sample successfully

Use make bench-report or make bench-report-full for enforceable release analysis.
EOF

cd "$REPO_ROOT"

: >"$ARTIFACT_DIR/summary.txt"
for filter in $FILTERS; do
    safe_filter="$(printf '%s' "$filter" | tr -c '[:alnum:]_.-' '_')"
    log_path="$ARTIFACT_DIR/criterion-$safe_filter.log"
    echo "Running performance smoke filter '$filter'..."
    if ! cargo bench -p lushtext-core --bench benchmarks -- "$filter" \
        --sample-size "$SAMPLE_SIZE" \
        --warm-up-time "$WARM_UP_TIME" \
        --measurement-time "$MEASUREMENT_TIME" \
        >"$log_path" 2>&1; then
        tail -n 120 "$log_path" >&2 || true
        smoke_fail "performance smoke failed for filter '$filter'. Artifacts: $ARTIFACT_DIR"
    fi
    {
        echo "## $filter"
        grep -E "^(Benchmarking|Analyzing|[[:space:]]*time:)" "$log_path" || true
        echo
    } >>"$ARTIFACT_DIR/summary.txt"
done

echo "PASS: performance smoke completed for filters '$FILTERS'. Artifacts: $ARTIFACT_DIR"
