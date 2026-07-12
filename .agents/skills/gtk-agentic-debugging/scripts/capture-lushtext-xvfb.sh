#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: capture-lushtext-xvfb.sh --file PATH --output PATH [options]

Launch LushText on an isolated Xvfb display, optionally drive search, and
capture the root window to a PNG without touching the live desktop focus.

Options:
  --file PATH          File to open in the debug-owned LushText instance.
  --output PATH        PNG path to write.
  --search TEXT        Open in-tab search through D-Bus and type TEXT via xdotool.
  --enable-minimap     Enable show-minimap in an isolated GSettings keyfile.
  --binary PATH        LushText binary to launch (default: target/debug/lushtext).
  --repo-root PATH     Repository root (default: Git-discovered root).
  --app-id ID          Application D-Bus identity.
  --app-object-path P  Application D-Bus object path.
  --gsettings-schema S Application GSettings schema (default: app ID).
  --gsettings-schema-dir PATH
                       Compiled schema directory (default: REPO_ROOT/data).
  --width PX           Xvfb screen width (default: 1600).
  --height PX          Xvfb screen height (default: 1000).
  --keep-artifacts     Keep the temporary logs and xwd capture.
EOF
}

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="${LUSHTEXT_DEBUG_REPO_ROOT:-$(git -C "$script_dir" rev-parse --show-toplevel)}"

file_path=""
output_path=""
search_text=""
enable_minimap=0
binary_path="${LUSHTEXT_DEBUG_BINARY:-}"
app_id="${LUSHTEXT_DEBUG_APP_ID:-dev.cominotti.lushtext}"
app_object_path="${LUSHTEXT_DEBUG_APP_OBJECT_PATH:-}"
gsettings_schema="${LUSHTEXT_DEBUG_GSETTINGS_SCHEMA:-}"
gsettings_schema_dir="${LUSHTEXT_DEBUG_GSETTINGS_SCHEMA_DIR:-}"
screen_width=1600
screen_height=1000
keep_artifacts=0
internal_run=0

while (($#)); do
    case "$1" in
        --file)
            file_path="${2:-}"
            shift 2
            ;;
        --output)
            output_path="${2:-}"
            shift 2
            ;;
        --search)
            search_text="${2:-}"
            shift 2
            ;;
        --enable-minimap)
            enable_minimap=1
            shift
            ;;
        --binary)
            binary_path="${2:-}"
            shift 2
            ;;
        --repo-root)
            repo_root="${2:-}"
            shift 2
            ;;
        --app-id)
            app_id="${2:-}"
            shift 2
            ;;
        --app-object-path)
            app_object_path="${2:-}"
            shift 2
            ;;
        --gsettings-schema)
            gsettings_schema="${2:-}"
            shift 2
            ;;
        --gsettings-schema-dir)
            gsettings_schema_dir="${2:-}"
            shift 2
            ;;
        --width)
            screen_width="${2:-}"
            shift 2
            ;;
        --height)
            screen_height="${2:-}"
            shift 2
            ;;
        --keep-artifacts)
            keep_artifacts=1
            shift
            ;;
        --internal-run)
            internal_run=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

repo_root="$(cd -- "$repo_root" && pwd)"
binary_path="${binary_path:-$repo_root/target/debug/lushtext}"
app_object_path="${app_object_path:-/${app_id//./\/}}"
gsettings_schema="${gsettings_schema:-$app_id}"
gsettings_schema_dir="${gsettings_schema_dir:-$repo_root/data}"

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1" >&2
        echo "Run make dev-tools inside the Toolbx/container." >&2
        exit 1
    fi
}

validate_args() {
    if [[ -z "$file_path" || -z "$output_path" ]]; then
        usage >&2
        exit 2
    fi

    if [[ ! "$screen_width" =~ ^[0-9]+$ || ! "$screen_height" =~ ^[0-9]+$ ]]; then
        echo "--width and --height must be positive integers." >&2
        exit 2
    fi

    if [[ ! -f "$file_path" ]]; then
        echo "File to open does not exist: $file_path" >&2
        exit 1
    fi

    if [[ ! -x "$binary_path" ]]; then
        echo "LushText binary is not executable: $binary_path" >&2
        echo "Run make build-debug first, or pass --binary." >&2
        exit 1
    fi
}

choose_display() {
    local candidate

    for _ in {1..100}; do
        candidate=":$((RANDOM % 400 + 100))"
        if [[ ! -e "/tmp/.X11-unix/X${candidate#:}" ]]; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    echo "Could not find an unused X display number." >&2
    return 1
}

wait_for_xvfb() {
    for _ in {1..80}; do
        if xdotool getdisplaygeometry >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
    done

    echo "Xvfb did not become ready on DISPLAY=$DISPLAY." >&2
    return 1
}

wait_for_window() {
    local pid="$1"
    local window=""

    for _ in {1..120}; do
        window="$(xdotool search --onlyvisible --pid "$pid" . 2>/dev/null | head -n1 || true)"
        if [[ -n "$window" ]]; then
            printf '%s\n' "$window"
            return 0
        fi
        sleep 0.1
    done

    echo "No visible LushText window appeared for PID $pid." >&2
    return 1
}

wait_for_window_actions() {
    for _ in {1..120}; do
        if gdbus call \
            --session \
            --dest "$app_id" \
            --object-path "$app_object_path/window/1" \
            --method org.gtk.Actions.List >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
    done

    echo "LushText did not export window actions on the isolated D-Bus session." >&2
    return 1
}

activate_window_action() {
    local action_name="$1"

    gdbus call \
        --session \
        --dest "$app_id" \
        --object-path "$app_object_path/window/1" \
        --method org.gtk.Actions.Activate \
        "$action_name" \
        "[]" \
        "{}" >/dev/null
}

outer_run() {
    validate_args

    require_command dbus-run-session
    require_command gdbus
    require_command gsettings
    require_command magick
    require_command Xvfb
    require_command xdotool
    require_command xwd

    artifact_dir="$(mktemp -d /tmp/lushtext-xvfb-debug.XXXXXX)"
    mkdir -p "$artifact_dir/runtime" "$artifact_dir/data" "$artifact_dir/config" "$artifact_dir/cache"
    mkdir -p "$(dirname -- "$output_path")"

    export LUSHTEXT_XVFB_ARTIFACT_DIR="$artifact_dir"
    export LUSHTEXT_XVFB_BINARY="$binary_path"
    export LUSHTEXT_XVFB_ENABLE_MINIMAP="$enable_minimap"
    export LUSHTEXT_XVFB_FILE="$file_path"
    export LUSHTEXT_XVFB_KEEP_ARTIFACTS="$keep_artifacts"
    export LUSHTEXT_XVFB_OUTPUT="$output_path"
    export LUSHTEXT_XVFB_REPO_ROOT="$repo_root"
    export LUSHTEXT_XVFB_APP_ID="$app_id"
    export LUSHTEXT_XVFB_APP_OBJECT_PATH="$app_object_path"
    export LUSHTEXT_XVFB_GSETTINGS_SCHEMA="$gsettings_schema"
    export LUSHTEXT_XVFB_GSETTINGS_SCHEMA_DIR="$gsettings_schema_dir"
    export LUSHTEXT_XVFB_SCREEN_HEIGHT="$screen_height"
    export LUSHTEXT_XVFB_SCREEN_WIDTH="$screen_width"
    export LUSHTEXT_XVFB_SEARCH="$search_text"
    export LUSHTEXT_XVFB_DISPLAY
    LUSHTEXT_XVFB_DISPLAY="$(choose_display)"

    if dbus-run-session -- "$0" --internal-run >"$artifact_dir/session.log" 2>&1; then
        grep -E '^(.*: PNG image data|Launched PID:|Window ID:)' "$artifact_dir/session.log" || true
        echo "Screenshot saved to $output_path"
        if [[ "$keep_artifacts" == "1" ]]; then
            echo "Artifacts kept in $artifact_dir"
        else
            rm -rf -- "$artifact_dir" 2>/dev/null || true
        fi
        return 0
    else
        local status=$?
        echo "Xvfb capture failed. Artifacts kept in $artifact_dir" >&2
        echo "Last session log lines:" >&2
        tail -n 80 "$artifact_dir/session.log" >&2 || true
        return "$status"
    fi
}

inner_run() {
    local artifact_dir="${LUSHTEXT_XVFB_ARTIFACT_DIR:?}"
    local app_pid=""
    local xvfb_pid=""
    local window_id

    export DISPLAY="${LUSHTEXT_XVFB_DISPLAY:?}"
    export GDK_BACKEND=x11
    export GSETTINGS_BACKEND=keyfile
    export GSETTINGS_SCHEMA_DIR="${LUSHTEXT_XVFB_GSETTINGS_SCHEMA_DIR:?}"
    app_id="${LUSHTEXT_XVFB_APP_ID:?}"
    app_object_path="${LUSHTEXT_XVFB_APP_OBJECT_PATH:?}"
    export GSK_RENDERER="${GSK_RENDERER:-cairo}"
    export NO_AT_BRIDGE=1
    export XDG_CACHE_HOME="$artifact_dir/cache"
    export XDG_CONFIG_HOME="$artifact_dir/config"
    export XDG_DATA_HOME="$artifact_dir/data"
    export XDG_RUNTIME_DIR="$artifact_dir/runtime"

    cleanup() {
        if [[ -n "${app_pid:-}" ]]; then
            kill "$app_pid" >/dev/null 2>&1 || true
        fi
        if [[ -n "${xvfb_pid:-}" ]]; then
            kill "$xvfb_pid" >/dev/null 2>&1 || true
        fi
    }
    trap cleanup EXIT

    if [[ "${LUSHTEXT_XVFB_ENABLE_MINIMAP:?}" == "1" ]]; then
        gsettings set "${LUSHTEXT_XVFB_GSETTINGS_SCHEMA:?}" show-minimap true
    fi

    Xvfb "$DISPLAY" \
        -screen 0 "${LUSHTEXT_XVFB_SCREEN_WIDTH:?}x${LUSHTEXT_XVFB_SCREEN_HEIGHT:?}x24" \
        -nolisten tcp \
        >"$artifact_dir/xvfb.log" 2>&1 &
    xvfb_pid=$!

    wait_for_xvfb

    "${LUSHTEXT_XVFB_BINARY:?}" "${LUSHTEXT_XVFB_FILE:?}" \
        >"$artifact_dir/lushtext.log" 2>&1 &
    app_pid=$!

    window_id="$(wait_for_window "$app_pid")"
    wait_for_window_actions
    xdotool windowfocus "$window_id"

    if [[ -n "${LUSHTEXT_XVFB_SEARCH:-}" ]]; then
        activate_window_action begin-search
        sleep 0.3
        xdotool type --window "$window_id" --clearmodifiers --delay 1 "$LUSHTEXT_XVFB_SEARCH"
    fi

    sleep 0.8
    xwd -root -silent -out "$artifact_dir/root.xwd"
    magick "$artifact_dir/root.xwd" "${LUSHTEXT_XVFB_OUTPUT:?}"

    if command -v file >/dev/null 2>&1; then
        file "${LUSHTEXT_XVFB_OUTPUT:?}"
    fi
    echo "Launched PID: $app_pid"
    echo "Window ID: $window_id"

    cleanup
    trap - EXIT
}

if [[ "$internal_run" == "1" ]]; then
    inner_run
else
    outer_run
fi
