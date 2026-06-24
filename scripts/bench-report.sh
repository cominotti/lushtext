#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

# Generate a markdown benchmark report from Criterion results.
#
# Usage: scripts/bench-report.sh [--mode short|full] [--scope release|diagnostic] [--filter <name>] [--out-dir <dir>] [--baseline <name>]
#
# Prerequisites: cargo, jq

set -euo pipefail

MODE="short"
SCOPE=""
OUT_DIR="docs/benchmarks"
BASELINE=""
CRITERION_DIR="target/criterion"
FILTERS=()
RELEASE_FILTERS=(
    file_index_incremental
    content_search_smoke
    json_persistence
    replace_preview_generation
    recovery_performance
)

usage() {
    cat <<'EOF'
Usage: scripts/bench-report.sh [--mode short|full] [--scope release|diagnostic] [--filter <name>] [--out-dir <dir>] [--baseline <name>]

Runs Criterion benchmarks and generates a markdown report.

Options:
  --mode      short (default) or full — controls Criterion sample size
  --scope     release (bounded release-safe report) or diagnostic (deeper analysis)
  --filter    Criterion benchmark group/name filter; may be passed multiple times
  --out-dir   output directory for markdown reports (default: docs/benchmarks)
  --baseline  name of a saved baseline for comparison (e.g., "main")
  -h, --help  show this help text
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --mode)
            [[ $# -lt 2 ]] && { echo "Error: --mode requires a value." >&2; exit 1; }
            MODE="$2"; shift 2 ;;
        --scope)
            [[ $# -lt 2 ]] && { echo "Error: --scope requires a value." >&2; exit 1; }
            SCOPE="$2"; shift 2 ;;
        --filter)
            [[ $# -lt 2 ]] && { echo "Error: --filter requires a value." >&2; exit 1; }
            FILTERS+=("$2"); shift 2 ;;
        --out-dir)
            [[ $# -lt 2 ]] && { echo "Error: --out-dir requires a value." >&2; exit 1; }
            OUT_DIR="$2"; shift 2 ;;
        --baseline)
            [[ $# -lt 2 ]] && { echo "Error: --baseline requires a value." >&2; exit 1; }
            BASELINE="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Error: unknown argument: $1" >&2; usage; exit 1 ;;
    esac
done

if [[ "$MODE" != "short" && "$MODE" != "full" ]]; then
    echo "Error: invalid --mode '$MODE'. Expected 'short' or 'full'." >&2
    exit 1
fi

if [[ -z "$SCOPE" ]]; then
    if [[ "$MODE" == "short" ]]; then
        SCOPE="release"
    else
        SCOPE="diagnostic"
    fi
fi

if [[ "$SCOPE" != "release" && "$SCOPE" != "diagnostic" ]]; then
    echo "Error: invalid --scope '$SCOPE'. Expected 'release' or 'diagnostic'." >&2
    exit 1
fi

if [[ "$SCOPE" == "release" && "$MODE" != "short" ]]; then
    echo "Error: --scope release requires --mode short; release reports must stay bounded." >&2
    exit 1
fi

if [[ "$SCOPE" == "release" && "${#FILTERS[@]}" -eq 0 ]]; then
    FILTERS=("${RELEASE_FILTERS[@]}")
fi

# Check prerequisites
for cmd in cargo jq; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Error: '$cmd' not found. Please install it." >&2
        exit 1
    fi
done

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
mkdir -p "$OUT_DIR"

clean_previous_results() {
    if [[ ! -d "$CRITERION_DIR" ]]; then
        return
    fi

    if [[ -n "$BASELINE" ]]; then
        find "$CRITERION_DIR" -type d -name new -prune -exec rm -rf {} +
        return
    fi

    rm -rf "$CRITERION_DIR"
}

# ─── Step 1: Run benchmarks ────────────────────────────────────────────

join_by() {
    local separator="$1"
    shift
    local joined=""
    local item
    for item in "$@"; do
        if [[ -n "$joined" ]]; then
            joined+="$separator"
        fi
        joined+="$item"
    done
    printf '%s\n' "$joined"
}

if [[ "${#FILTERS[@]}" -eq 0 ]]; then
    filters_display="all registered Criterion benchmarks"
else
    filters_display="$(join_by ", " "${FILTERS[@]}")"
fi

echo "Running Criterion benchmarks (mode: $MODE, scope: $SCOPE, filters: $filters_display)..."

bench_args=(-p lushtext-core --bench benchmarks)
criterion_args=()

if [[ "$MODE" == "short" ]]; then
    if [[ "$SCOPE" == "release" ]]; then
        criterion_args+=(--sample-size 10 --warm-up-time 1 --measurement-time 2)
    else
        criterion_args+=(--sample-size 30)
    fi
fi

if [[ -n "$BASELINE" ]]; then
    criterion_args+=(--baseline "$BASELINE")
fi

clean_previous_results
if [[ "${#FILTERS[@]}" -eq 0 ]]; then
    cargo bench "${bench_args[@]}" -- "${criterion_args[@]}"
else
    for filter in "${FILTERS[@]}"; do
        cargo bench "${bench_args[@]}" -- "$filter" "${criterion_args[@]}"
    done
fi

# ─── Step 2: Parse Criterion JSON output ───────────────────────────────

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

results_file="$tmp_dir/results.tsv"
: >"$results_file"
has_any_baseline=false

# Walk all estimates.json files from the latest run
while IFS= read -r estimates_path; do
    # Extract group/benchmark from path:
    #   target/criterion/<group>/<bench>/new/estimates.json
    rel="${estimates_path#"$CRITERION_DIR/"}"
    bench_path="${rel%/new/estimates.json}"

    # Parse mean, median, std_dev in a single jq call (values are in nanoseconds)
    read -r mean_ns median_ns std_dev_ns < <(
        jq -r '[.mean.point_estimate, .median.point_estimate, .std_dev.point_estimate]
               | map(. // "") | @tsv' "$estimates_path" 2>/dev/null || echo "")

    if [[ -z "$mean_ns" ]]; then
        continue
    fi

    # Check for baseline comparison (only when explicitly requested via --baseline)
    base_mean_ns=""
    delta_pct=""
    base_estimates="${estimates_path/\/new\//\/base\/}"
    if [[ -n "$BASELINE" && -f "$base_estimates" ]]; then
        base_mean_ns=$(jq -r '.mean.point_estimate // empty' "$base_estimates" 2>/dev/null || echo "")
        if [[ -n "$base_mean_ns" ]]; then
            delta_pct=$(awk -v new="$mean_ns" -v old="$base_mean_ns" \
                'BEGIN { if (old > 0) printf "%.2f", ((new - old) / old) * 100; else print "-" }')
            has_any_baseline=true
        fi
    fi

    printf "%s\t%s\t%s\t%s\t%s\t%s\n" \
        "$bench_path" "$mean_ns" "$median_ns" "$std_dev_ns" "$base_mean_ns" "$delta_pct" \
        >>"$results_file"

done < <(find "$CRITERION_DIR" -path "*/new/estimates.json" -type f 2>/dev/null | sort)

if [[ ! -s "$results_file" ]]; then
    echo "Error: no Criterion results found in $CRITERION_DIR." >&2
    echo "Hint: benchmarks may have failed to run." >&2
    exit 1
fi

# ─── Step 3: Generate markdown ─────────────────────────────────────────

# Format nanoseconds to human-readable units (batched: 3 values in one awk call)
format_ns_triple() {
    awk -v a="$1" -v b="$2" -v c="$3" '
    function fmt(ns) {
        if (ns >= 1e9)      return sprintf("%.3f s",  ns / 1e9)
        else if (ns >= 1e6) return sprintf("%.3f ms", ns / 1e6)
        else if (ns >= 1e3) return sprintf("%.3f us", ns / 1e3)
        else                return sprintf("%.1f ns", ns)
    }
    BEGIN { printf "%s\t%s\t%s\n", fmt(a), fmt(b), fmt(c) }'
}

generated_at_utc="$(date -u +"%Y-%m-%d %H:%M:%S UTC")"
report_stamp="$(date -u +"%Y-%m-%d_%H-%M-%S")"
report_path="$OUT_DIR/$report_stamp.md"
git_branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")"
git_commit="$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")"
rustc_version="$(rustc --version 2>/dev/null || echo "unknown")"
platform="$(uname -srm)"
has_baseline="no"
if [[ -n "$BASELINE" ]]; then
    has_baseline="yes ($BASELINE)"
fi
if [[ "${#criterion_args[@]}" -eq 0 ]]; then
    criterion_args_display="default Criterion settings"
else
    criterion_args_display="$(join_by " " "${criterion_args[@]}")"
fi
if [[ "$SCOPE" == "release" ]]; then
    report_scope="release-safe bounded report"
    fixture_scope="curated release filters: $filters_display"
else
    report_scope="deep diagnostic report"
    fixture_scope="diagnostic filters: $filters_display"
fi
release_tag="${BENCH_REPORT_RELEASE_TAG:-n/a}"
release_commit="${BENCH_REPORT_RELEASE_COMMIT:-n/a}"

{
    echo "# LushText Benchmark Report"
    echo ""
    echo "> Generated at: $generated_at_utc"
    echo ""
    echo "## Run Metadata"
    echo ""
    echo "| Field | Value |"
    echo "|---|---|"
    echo "| Mode | \`$MODE\` |"
    echo "| Report Scope | \`$report_scope\` |"
    echo "| Criterion Filters | \`$filters_display\` |"
    echo "| Fixture Scope | \`$fixture_scope\` |"
    echo "| Criterion Args | \`$criterion_args_display\` |"
    echo "| Baseline Comparison | \`$has_baseline\` |"
    echo "| Release Tag | \`$release_tag\` |"
    echo "| Release Source Commit | \`$release_commit\` |"
    echo "| Branch | \`$git_branch\` |"
    echo "| Commit | \`$git_commit\` |"
    echo "| Platform | \`$platform\` |"
    echo "| Rust Version | \`$rustc_version\` |"
    echo ""

    # Group results by benchmark group (first path component)
    current_group=""

    while IFS=$'\t' read -r bench_path mean_ns median_ns std_dev_ns base_mean_ns delta_pct; do
        # Extract group name (everything before the last /)
        group="${bench_path%/*}"
        bench_name="${bench_path##*/}"

        if [[ "$group" != "$current_group" ]]; then
            if [[ -n "$current_group" ]]; then
                echo ""
            fi
            echo "## $group"
            echo ""
            if [[ "$has_any_baseline" == "true" ]]; then
                echo "| Benchmark | Mean | Median | Std Dev | vs Baseline |"
                echo "|---|---:|---:|---:|---:|"
            else
                echo "| Benchmark | Mean | Median | Std Dev |"
                echo "|---|---:|---:|---:|"
            fi
            current_group="$group"
        fi

        IFS=$'\t' read -r mean_fmt median_fmt std_fmt < <(format_ns_triple "$mean_ns" "$median_ns" "$std_dev_ns")

        if [[ "$has_any_baseline" == "true" ]]; then
            if [[ -n "$delta_pct" ]]; then
                if awk -v d="$delta_pct" 'BEGIN { exit (d + 0 > 2) ? 0 : 1 }'; then
                    delta_cell="+${delta_pct}% (regression)"
                elif awk -v d="$delta_pct" 'BEGIN { exit (d + 0 < -2) ? 0 : 1 }'; then
                    delta_cell="${delta_pct}% (improvement)"
                else
                    delta_cell="${delta_pct}% (within noise)"
                fi
            else
                delta_cell="-"
            fi
            printf "| \`%s\` | %s | %s | %s | %s |\n" \
                "$bench_name" "$mean_fmt" "$median_fmt" "$std_fmt" "$delta_cell"
        else
            printf "| \`%s\` | %s | %s | %s |\n" \
                "$bench_name" "$mean_fmt" "$median_fmt" "$std_fmt"
        fi
    done <"$results_file"

    echo ""
    echo "---"
    echo ""
    echo "<details>"
    echo "<summary>Raw results (TSV)</summary>"
    echo ""
    echo '```tsv'
    echo "benchmark	mean_ns	median_ns	std_dev_ns	base_mean_ns	delta_pct"
    cat "$results_file"
    echo '```'
    echo ""
    echo "</details>"
} >"$report_path"

echo ""
echo "Benchmark report written to: $report_path"
