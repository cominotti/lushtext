#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
CURRENT_ROOT="$REPO_ROOT"
ARTIFACT_DIR="$REPO_ROOT/build/blueprint-visual-diff"
BASELINE_REF=""
STATE_MATRIX="visual-smoke"
VIEWPORT_MATRIX="visual-smoke"
ORIGINAL_ARGS=("$@")

usage() {
    cat <<'EOF'
Usage: scripts/compare-blueprint-visuals.sh --baseline-ref REF [options]

Capture a pre-change baseline and the current checkout through the same
Blueprint-sensitive visual smoke matrix, compare the screenshots, and write a
bounded report plus disposable image artifacts.

Options:
  --baseline-ref REF       Git ref or commit to capture as the baseline
  --current-root DIR       Checkout to compare against the baseline (default: repo root)
  --artifact-dir DIR       Output directory (default: build/blueprint-visual-diff)
  --state-matrix NAME      State matrix to capture (default: visual-smoke)
  --viewport-matrix NAME   Viewport matrix to capture (default: visual-smoke)
  -h, --help               Show this help
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --baseline-ref)
            [[ $# -lt 2 ]] && { echo "error: --baseline-ref requires a value" >&2; exit 2; }
            BASELINE_REF="$2"
            shift 2
            ;;
        --current-root)
            [[ $# -lt 2 ]] && { echo "error: --current-root requires a value" >&2; exit 2; }
            CURRENT_ROOT="$(cd "$2" && pwd)"
            shift 2
            ;;
        --artifact-dir)
            [[ $# -lt 2 ]] && { echo "error: --artifact-dir requires a value" >&2; exit 2; }
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        --state-matrix)
            [[ $# -lt 2 ]] && { echo "error: --state-matrix requires a value" >&2; exit 2; }
            STATE_MATRIX="$2"
            shift 2
            ;;
        --viewport-matrix)
            [[ $# -lt 2 ]] && { echo "error: --viewport-matrix requires a value" >&2; exit 2; }
            VIEWPORT_MATRIX="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "error: unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

[[ -n "$BASELINE_REF" ]] || { usage >&2; exit 2; }
[[ "$STATE_MATRIX" == "visual-smoke" ]] || { echo "error: only --state-matrix visual-smoke is supported" >&2; exit 2; }
[[ "$VIEWPORT_MATRIX" == "visual-smoke" ]] || { echo "error: only --viewport-matrix visual-smoke is supported" >&2; exit 2; }

BASELINE_COMMIT="$(git -C "$CURRENT_ROOT" rev-parse --verify "${BASELINE_REF}^{commit}")"
CURRENT_COMMIT="$(git -C "$CURRENT_ROOT" rev-parse HEAD)"
if git -C "$CURRENT_ROOT" diff --quiet && git -C "$CURRENT_ROOT" diff --cached --quiet && [[ -z "$(git -C "$CURRENT_ROOT" status --short --untracked-files=normal)" ]]; then
    CURRENT_TREE_STATUS="clean"
else
    CURRENT_TREE_STATUS="dirty"
fi
ARTIFACT_DIR="$(mkdir -p "$ARTIFACT_DIR" && cd "$ARTIFACT_DIR" && pwd)"
BASELINE_WT="$(mktemp -d /tmp/lushtext-blueprint-baseline.XXXXXX)"
BASELINE_CAPTURE_TMP="$(mktemp -d /tmp/lt-bvd-baseline.XXXXXX)"
CURRENT_CAPTURE_TMP="$(mktemp -d /tmp/lt-bvd-current.XXXXXX)"
FIXTURE_DIR="$(mktemp -d /tmp/lt-bvd-fixtures.XXXXXX)"
BLUEPRINT_COMPILER_BIN="${BLUEPRINT_COMPILER:-blueprint-compiler}"

cleanup() {
    if git -C "$CURRENT_ROOT" worktree list --porcelain | grep -Fxq "worktree $BASELINE_WT"; then
        git -C "$CURRENT_ROOT" worktree remove --force "$BASELINE_WT" >/dev/null 2>&1 || true
    elif [[ -d "$BASELINE_WT/.git" ]]; then
        rm -rf "$BASELINE_WT"
    fi
    rm -rf "$BASELINE_CAPTURE_TMP" "$CURRENT_CAPTURE_TMP" "$FIXTURE_DIR"
}
trap cleanup EXIT

# shellcheck source=scripts/smoke-common.sh
source "$CURRENT_ROOT/scripts/smoke-common.sh"

if ! command -v magick >/dev/null 2>&1; then
    echo "error: ImageMagick 'magick' is required for Blueprint visual comparison" >&2
    exit 1
fi
if ! command -v "$BLUEPRINT_COMPILER_BIN" >/dev/null 2>&1; then
    echo "error: blueprint-compiler is required for current-checkout drift validation" >&2
    echo "hint: set BLUEPRINT_COMPILER=/path/to/blueprint-compiler" >&2
    exit 1
fi

BLUEPRINT_VERSION="$("$BLUEPRINT_COMPILER_BIN" --version 2>/dev/null || printf 'unknown')"
TEXT_FIXTURE="$FIXTURE_DIR/visual-smoke.txt"
MARKDOWN_FIXTURE="$FIXTURE_DIR/visual-smoke.md"
smoke_create_text_fixture "$TEXT_FIXTURE"
cat >"$MARKDOWN_FIXTURE" <<'EOF'
# LushText visual smoke

This Markdown document exercises the rendered preview surface.

```rust
fn main() {
    println!("needle");
}
```

- narrow layout
- short layout
- preview geometry
EOF

scan_visual_logs() {
    local variant="$1"
    local name="$2"
    local root="$3"
    local capture_dir="$4"
    local report="$root/assertions/${name}-logs.txt"
    local matches="$root/assertions/${name}-warnings.txt"

    : >"$report"
    : >"$matches"
    shopt -s nullglob
    local log_paths=(
        "$root/${name}.session.log"
        "$capture_dir"/*.log
        "$capture_dir"/lushtext.stdout
        "$capture_dir"/lushtext.stderr
    )
    shopt -u nullglob

    for log_path in "${log_paths[@]}"; do
        [[ -f "$log_path" ]] || continue
        printf 'scanned=%s\n' "$log_path" >>"$report"
        grep -E -i \
            '(Gtk|Gdk|GSK|Adwaita|Libadwaita|AT-SPI|accessibility).*(warning|critical|error)|GLib-GObject-CRITICAL|gtk_[a-z0-9_]+.*assertion|gdk_[a-z0-9_]+.*assertion' \
            "$log_path" \
            | grep -E -v '^Gdk-Message: .*Error reading events from display: Broken pipe$' \
            >>"$matches" || true
    done

    if [[ -s "$matches" ]]; then
        cat "$matches" >&2
        echo "error: ${variant}/${name} emitted unexpected GTK/Adwaita/GDK/accessibility warnings" >&2
        exit 1
    fi
    echo "PASS: no unexpected GTK/Adwaita/GDK/accessibility warnings for ${variant}/${name}" >>"$report"
}

prepare_recovery_capture_state() {
    local capture_dir="$1"
    local data_dir="$capture_dir/data/lushtext"
    mkdir -p "$data_dir/drafts"

    printf '{ malformed session metadata\n' >"$data_dir/session.json"
    printf '{ malformed draft manifest\n' >"$data_dir/drafts/manifest.json"
    printf 'Visual smoke recovered draft body\n' >"$data_dir/drafts/untitled-visual-smoke.draft"
}

assert_recovery_capture_artifacts() {
    local name="$1"
    local root="$2"
    local capture_dir="$3"
    local data_dir="$capture_dir/data/lushtext"
    local tree_path="$root/assertions/${name}-atspi-tree.txt"
    local summary_path="$root/assertions/${name}-recovery-summary.txt"
    local quarantine_dir="$data_dir/recovery-quarantine"

    {
        echo "data_dir=$data_dir"
        if [[ -d "$quarantine_dir" ]]; then
            find "$quarantine_dir" -type f -printf '%P size=%s\n' | sort
        else
            echo "quarantine=<missing>"
        fi
    } >"$summary_path"

    if ! grep -q 'size=' "$summary_path"; then
        echo "error: recovery visual capture did not preserve a quarantine summary" >&2
        exit 1
    fi
    if ! grep -Eiq 'recovery|could not be loaded|draft|session' "$tree_path"; then
        echo "error: recovery visual capture did not expose recovery diagnostics in AT-SPI" >&2
        exit 1
    fi
}

run_capture() {
    local variant="$1"
    local source_root="$2"
    local binary="$3"
    local output_root="$4"
    local name="$5"
    local fixture="$6"
    local width="$7"
    local height="$8"
    local search="$9"
    local minimap="${10}"
    local color_scheme="${11}"
    shift 11
    local actions=("$@")
    local capture_dir="$output_root/captures/$name"
    local output="$output_root/screenshots/${name}.png"
    local session_log="$output_root/${name}.session.log"
    local manifest="$output_root/assertions/${name}-state.txt"

    mkdir -p "$capture_dir" "$output_root/screenshots" "$output_root/assertions"
    if [[ "$name" == "recovery-startup" ]]; then
        prepare_recovery_capture_state "$capture_dir"
    fi
    {
        echo "variant=$variant"
        echo "source_root=$source_root"
        echo "baseline_commit=$BASELINE_COMMIT"
        echo "current_commit=$CURRENT_COMMIT"
        echo "fixture=$fixture"
        echo "output=$output"
        echo "width=$width"
        echo "height=$height"
        echo "search=$search"
        echo "minimap=$minimap"
        echo "color_scheme=$color_scheme"
        printf 'actions='
        printf '%s ' "${actions[@]}"
        printf '\n'
    } >"$manifest"

    local capture_args=(
        "$source_root/.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py"
        --file "$fixture"
        --output "$output"
        --binary "$binary"
        --width "$width"
        --height "$height"
        --capture-artifact-dir "$capture_dir"
        --keep-artifacts
        --enable-atspi
    )
    if [[ -n "$search" ]]; then
        capture_args+=(--search "$search")
    fi
    if [[ "$minimap" == "1" ]]; then
        capture_args+=(--enable-minimap)
    fi
    if [[ "$color_scheme" != "default" ]]; then
        capture_args+=(--color-scheme "$color_scheme")
    fi
    if [[ "$name" == "recovery-startup" ]]; then
        capture_args+=(
            --atspi-tree-output "$output_root/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$output_root/assertions/${name}-atspi-focus.txt"
        )
    fi
    for action in "${actions[@]}"; do
        capture_args+=(--window-action "$action")
    done

    echo "capture: ${variant}/${name}"
    if ! /usr/bin/python3 "${capture_args[@]}" >"$session_log" 2>&1; then
        tail -n 160 "$session_log" >&2 || true
        echo "error: capture failed for ${variant}/${name}" >&2
        exit 1
    fi

    [[ -s "$output" ]] || { echo "error: screenshot is empty: $output" >&2; exit 1; }
    /usr/bin/python3 "$CURRENT_ROOT/scripts/assert-png-smoke.py" \
        "$output" \
        --max-width "$width" \
        --max-height "$height" \
        --require-top-band-detail \
        --require-bottom-band-detail \
        >"$output_root/assertions/${name}-png.txt"
    scan_visual_logs "$variant" "$name" "$output_root" "$capture_dir"
    if [[ "$name" == "recovery-startup" ]]; then
        assert_recovery_capture_artifacts "$name" "$output_root" "$capture_dir"
    fi
    if command -v file >/dev/null 2>&1; then
        file "$output" >"$output_root/assertions/${name}-file.txt" || true
    fi
}

run_matrix() {
    local variant="$1"
    local source_root="$2"
    local binary="$3"
    local output_root="$4"

    run_capture "$variant" "$source_root" "$binary" "$output_root" \
        "main-search-minimap" "$TEXT_FIXTURE" "1600" "1000" "needle" "1" "default"
    run_capture "$variant" "$source_root" "$binary" "$output_root" \
        "compact-properties" "$TEXT_FIXTURE" "760" "720" "" "0" "default" "toggle-properties"
    run_capture "$variant" "$source_root" "$binary" "$output_root" \
        "short-layout" "$TEXT_FIXTURE" "1200" "420" "" "0" "default"
    run_capture "$variant" "$source_root" "$binary" "$output_root" \
        "markdown-preview" "$MARKDOWN_FIXTURE" "1280" "860" "" "0" "default" "toggle-preview-mode"
    run_capture "$variant" "$source_root" "$binary" "$output_root" \
        "dark-style" "$TEXT_FIXTURE" "1600" "1000" "" "0" "force-dark"
    run_capture "$variant" "$source_root" "$binary" "$output_root" \
        "recovery-startup" "$TEXT_FIXTURE" "1280" "860" "" "0" "default"
}

compare_images() {
    local comparison_root="$ARTIFACT_DIR/comparison"
    local summary="$comparison_root/summary.tsv"
    local any_diff=0
    mkdir -p "$comparison_root"
    printf 'state\tbaseline_size\tcurrent_size\tabsolute_error_pixels\trmse\n' >"$summary"

    local states=(
        main-search-minimap
        compact-properties
        short-layout
        markdown-preview
        dark-style
        recovery-startup
    )

    for state in "${states[@]}"; do
        local baseline="$ARTIFACT_DIR/baseline/screenshots/${state}.png"
        local current="$ARTIFACT_DIR/current/screenshots/${state}.png"
        local paired="$comparison_root/${state}-baseline-current.png"
        local diff="$comparison_root/${state}-diff.png"
        local baseline_size
        local current_size
        local ae
        local rmse

        baseline_size="$(magick identify -format '%wx%h' "$baseline")"
        current_size="$(magick identify -format '%wx%h' "$current")"
        ae="$(magick compare -metric AE "$baseline" "$current" null: 2>&1 || true)"
        rmse="$(magick compare -metric RMSE "$baseline" "$current" null: 2>&1 || true)"
        magick compare "$baseline" "$current" "$diff" >/dev/null 2>&1 || true
        magick montage \
            -label "baseline: $state" "$baseline" \
            -label "current: $state" "$current" \
            -tile 2x1 -geometry +12+12 "$paired"
        printf '%s\t%s\t%s\t%s\t%s\n' "$state" "$baseline_size" "$current_size" "$ae" "$rmse" >>"$summary"

        if [[ "$baseline_size" != "$current_size" ]]; then
            any_diff=1
        fi
        if [[ "$ae" != "0" && "$ae" != "0 (0)" ]]; then
            any_diff=1
        fi
    done

    magick montage "$comparison_root"/*-baseline-current.png \
        -tile 1x6 -geometry +0+18 "$comparison_root/contact-sheet.png"
    return "$any_diff"
}

write_report() {
    local result="$1"
    local report="$ARTIFACT_DIR/report.md"
    local summary="$ARTIFACT_DIR/comparison/summary.tsv"

    {
        echo "# Blueprint Before/After Visual Proof"
        echo
        echo "Baseline ref: \`$BASELINE_REF\`"
        echo "Baseline commit: \`$BASELINE_COMMIT\`"
        echo "Current commit: \`$CURRENT_COMMIT\`"
        echo "Current checkout status: \`$CURRENT_TREE_STATUS\`"
        echo "Blueprint compiler: \`$BLUEPRINT_VERSION\`"
        echo "State matrix: \`$STATE_MATRIX\`"
        echo "Viewport matrix: \`$VIEWPORT_MATRIX\`"
        echo "Artifact directory: \`$ARTIFACT_DIR\`"
        printf 'Command: `'
        printf '%q ' "$0" "${ORIGINAL_ARGS[@]}"
        echo '`'
        echo
        if [[ "$result" == "PASS" ]]; then
            echo "Result: PASS"
            echo
            echo "Every screenshot pair was pixel-identical."
        else
            echo "Result: FAIL"
            echo
            echo "One or more screenshot pairs differed. Investigate or document each intentional difference before accepting the template change."
        fi
        echo
        echo "| State | Baseline Size | Current Size | AE | RMSE |"
        echo "| --- | --- | --- | --- | --- |"
        local header
        IFS= read -r header
        local state baseline_size current_size ae rmse
        while IFS=$'\t' read -r state baseline_size current_size ae rmse; do
            echo "| $state | $baseline_size | $current_size | $ae | $rmse |"
        done
        echo
        echo "Artifacts:"
        echo
        echo "- Baseline screenshots: \`$ARTIFACT_DIR/baseline/screenshots/\`"
        echo "- Current screenshots: \`$ARTIFACT_DIR/current/screenshots/\`"
        echo "- Pixel metrics: \`$ARTIFACT_DIR/comparison/summary.tsv\`"
        echo "- Side-by-side contact sheet: \`$ARTIFACT_DIR/comparison/contact-sheet.png\`"
    } <"$summary" >"$report"
}

rm -rf "$ARTIFACT_DIR/baseline" "$ARTIFACT_DIR/current" "$ARTIFACT_DIR/comparison"
mkdir -p "$ARTIFACT_DIR"

{
    echo "baseline_ref=$BASELINE_REF"
    echo "baseline_commit=$BASELINE_COMMIT"
    echo "current_commit=$CURRENT_COMMIT"
    echo "current_tree_status=$CURRENT_TREE_STATUS"
    echo "current_root=$CURRENT_ROOT"
    echo "baseline_worktree=$BASELINE_WT"
    echo "baseline_temp_artifacts=$BASELINE_CAPTURE_TMP"
    echo "current_temp_artifacts=$CURRENT_CAPTURE_TMP"
    echo "fixtures=$FIXTURE_DIR"
    echo "blueprint_compiler=$BLUEPRINT_COMPILER_BIN"
    echo "blueprint_compiler_version=$BLUEPRINT_VERSION"
    echo "state_matrix=$STATE_MATRIX"
    echo "viewport_matrix=$VIEWPORT_MATRIX"
} >"$ARTIFACT_DIR/run-manifest.txt"

echo "baseline: creating clean worktree at $BASELINE_WT"
git -C "$CURRENT_ROOT" worktree add --detach "$BASELINE_WT" "$BASELINE_COMMIT"

echo "baseline: building debug binary"
make -C "$BASELINE_WT" build-debug

echo "current: validating Blueprint drift and building debug binary"
BLUEPRINT_COMPILER="$BLUEPRINT_COMPILER_BIN" make -C "$CURRENT_ROOT" check-blueprint
make -C "$CURRENT_ROOT" build-debug

run_matrix "baseline" "$BASELINE_WT" "$BASELINE_WT/target/debug/lushtext" "$BASELINE_CAPTURE_TMP"
run_matrix "current" "$CURRENT_ROOT" "$CURRENT_ROOT/target/debug/lushtext" "$CURRENT_CAPTURE_TMP"

cp -a "$BASELINE_CAPTURE_TMP" "$ARTIFACT_DIR/baseline"
cp -a "$CURRENT_CAPTURE_TMP" "$ARTIFACT_DIR/current"
cp -a "$FIXTURE_DIR" "$ARTIFACT_DIR/fixtures"

comparison_failed=0
if ! compare_images; then
    comparison_failed=1
fi

find "$ARTIFACT_DIR" -maxdepth 3 -type f | sort >"$ARTIFACT_DIR/artifacts.txt"
if [[ "$comparison_failed" -eq 0 ]]; then
    write_report "PASS"
    echo "PASS: before/after visual comparison report written to $ARTIFACT_DIR/report.md"
else
    write_report "FAIL"
    echo "error: visual differences detected; report written to $ARTIFACT_DIR/report.md" >&2
    exit 1
fi
