#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/visual}"
BINARY="$REPO_ROOT/target/debug/lushtext"
WIDTH="${LUSHTEXT_VISUAL_SMOKE_WIDTH:-1600}"
HEIGHT="${LUSHTEXT_VISUAL_SMOKE_HEIGHT:-1000}"

usage() {
    cat <<'EOF'
Usage: scripts/run-visual-smoke.sh [--artifact-dir DIR] [--binary PATH]

Launch LushText in isolated headless Mutter sessions, capture representative
geometry-sensitive desktop states, and preserve screenshots/log artifacts.
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
        -h|--help)
            usage
            exit 0
            ;;
        *)
            smoke_fail "unknown argument: $1"
            ;;
    esac
done

smoke_require_command dbus-run-session
smoke_require_command mutter
smoke_require_command gdbus
smoke_require_command gsettings
smoke_require_command gst-launch-1.0
smoke_require_command pipewire
smoke_require_command pw-dump
smoke_require_command wireplumber

[[ -x /usr/bin/python3 ]] || smoke_skip "/usr/bin/python3 is not available."
[[ -x "$BINARY" ]] || smoke_skip "LushText debug binary is missing. Run 'make build-debug' first."

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"

rm -rf "$ARTIFACT_DIR/screenshots" "$ARTIFACT_DIR/captures" "$ARTIFACT_DIR/assertions"
mkdir -p "$ARTIFACT_DIR/fixtures" "$ARTIFACT_DIR/screenshots" "$ARTIFACT_DIR/captures" "$ARTIFACT_DIR/assertions"

TEXT_FIXTURE="$ARTIFACT_DIR/fixtures/visual-smoke.txt"
MARKDOWN_FIXTURE="$ARTIFACT_DIR/fixtures/visual-smoke.md"
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
    local name="$1"
    local capture_dir="$2"
    local report="$ARTIFACT_DIR/assertions/${name}-logs.txt"
    local matches="$ARTIFACT_DIR/assertions/${name}-warnings.txt"

    : >"$report"
    : >"$matches"
    shopt -s nullglob
    local log_paths=(
        "$ARTIFACT_DIR/${name}.session.log"
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
        smoke_fail "visual smoke '${name}' emitted unexpected GTK/Adwaita/GDK/accessibility warnings"
    fi
    echo "PASS: no unexpected GTK/Adwaita/GDK/accessibility warnings for ${name}" >>"$report"
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
    local capture_dir="$2"
    local data_dir="$capture_dir/data/lushtext"
    local tree_path="$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
    local summary_path="$ARTIFACT_DIR/assertions/${name}-recovery-summary.txt"
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
        smoke_fail "recovery visual smoke did not preserve a quarantine summary"
    fi
    if ! grep -Eiq 'recovery|could not be loaded|draft|session' "$tree_path"; then
        smoke_fail "recovery visual smoke did not expose recovery diagnostics in the AT-SPI tree"
    fi
}

run_capture() {
    local name="$1"
    local fixture="$2"
    local output="$3"
    local width="$4"
    local height="$5"
    local search="$6"
    local minimap="$7"
    local color_scheme="$8"
    shift 8
    local actions=("$@")
    local capture_dir="$ARTIFACT_DIR/captures/$name"
    local session_log="$ARTIFACT_DIR/${name}.session.log"
    local manifest="$ARTIFACT_DIR/assertions/${name}-state.txt"

    mkdir -p "$capture_dir"
    if [[ "$name" == "recovery-startup" ]]; then
        prepare_recovery_capture_state "$capture_dir"
    fi
    {
        echo "name=$name"
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
        "$REPO_ROOT/.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py"
        --file "$fixture"
        --output "$output"
        --binary "$BINARY"
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
            --atspi-tree-output "$ARTIFACT_DIR/assertions/${name}-atspi-tree.txt"
            --atspi-focus-output "$ARTIFACT_DIR/assertions/${name}-atspi-focus.txt"
        )
    fi
    for action in "${actions[@]}"; do
        capture_args+=(--window-action "$action")
    done

    if ! /usr/bin/python3 "${capture_args[@]}" >"$session_log" 2>&1; then
        if grep -qE 'AT-SPI registry did not register|Missing at-spi2-registryd|PipeWire did not become ready|Missing required command' "$session_log"; then
            tail -n 120 "$session_log" >&2 || true
            smoke_skip "visual smoke host support unavailable during '${name}'. Artifacts: $ARTIFACT_DIR"
        fi
        tail -n 120 "$session_log" >&2 || true
        smoke_fail "visual smoke capture '${name}' failed. Artifacts: $ARTIFACT_DIR"
    fi

    [[ -s "$output" ]] || smoke_fail "visual smoke screenshot is empty: $output"
    /usr/bin/python3 "$REPO_ROOT/scripts/assert-png-smoke.py" \
        "$output" \
        --max-width "$width" \
        --max-height "$height" \
        --require-top-band-detail \
        --require-bottom-band-detail \
        >"$ARTIFACT_DIR/assertions/${name}-png.txt"
    scan_visual_logs "$name" "$capture_dir"
    if [[ "$name" == "recovery-startup" ]]; then
        assert_recovery_capture_artifacts "$name" "$capture_dir"
    fi
    if command -v file >/dev/null 2>&1; then
        file "$output" >"$ARTIFACT_DIR/assertions/${name}-file.txt" || true
    fi
    echo "PASS: visual smoke '${name}' captured at $output"
}

run_capture "main-search-minimap" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/main-search-minimap.png" "$WIDTH" "$HEIGHT" "needle" "1" "default"
run_capture "compact-properties" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/compact-properties.png" "760" "720" "" "0" "default" "toggle-properties"
run_capture "short-layout" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/short-layout.png" "1200" "420" "" "0" "default"
run_capture "markdown-preview" "$MARKDOWN_FIXTURE" "$ARTIFACT_DIR/screenshots/markdown-preview.png" "1280" "860" "" "0" "default" "toggle-preview-mode"
run_capture "dark-style" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/dark-style.png" "$WIDTH" "$HEIGHT" "" "0" "force-dark"
run_capture "recovery-startup" "$TEXT_FIXTURE" "$ARTIFACT_DIR/screenshots/recovery-startup.png" "1280" "860" "" "0" "default"

{
    echo "screenshots=$ARTIFACT_DIR/screenshots"
    echo "captures=$ARTIFACT_DIR/captures"
    echo "assertions=$ARTIFACT_DIR/assertions"
    find "$ARTIFACT_DIR/screenshots" -maxdepth 1 -type f -name '*.png' -print | sort
} >"$ARTIFACT_DIR/summary.txt"

echo "PASS: visual smoke screenshots and artifacts captured under $ARTIFACT_DIR"
