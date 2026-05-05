#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
app_id="dev.cominotti.lushtext"
data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
desktop_dir="$data_home/applications"
icons_root="$data_home/icons/hicolor"
desktop_target="$desktop_dir/$app_id.desktop"
desktop_template="$repo_root/data/$app_id.desktop.in"
desktop_icon_source="$repo_root/data/icons/hicolor/128x128/apps/$app_id.png"
desktop_icon_hash="$(sha256sum "$desktop_icon_source" | awk '{ print substr($1, 1, 16) }')"
desktop_icon_dir="$data_home/icons/lushtext-dev-run"
desktop_icon_target="$desktop_icon_dir/$app_id-$desktop_icon_hash.png"
target_dir="${CARGO_TARGET_DIR:-$repo_root/target}"
build_target="${CARGO_BUILD_TARGET:-}"
keep_staged="${LUSHTEXT_DEV_RUN_KEEP_STAGED:-0}"
no_exec="${LUSHTEXT_DEV_RUN_NO_EXEC:-0}"
force_restart="${LUSHTEXT_DEV_RUN_FORCE_RESTART:-0}"

if [[ -n "$build_target" ]]; then
    binary="$target_dir/$build_target/debug/lushtext"
else
    binary="$target_dir/debug/lushtext"
fi

if [[ ! -x "$binary" ]]; then
    echo "Error: expected debug binary at $binary" >&2
    echo "Run 'make build-debug' first." >&2
    exit 1
fi

if [[ "$binary" =~ [[:space:]] ]]; then
    echo "Error: debug binary path contains whitespace and cannot be written safely into a desktop Exec line." >&2
    exit 1
fi

if ! command -v gtk-launch >/dev/null 2>&1; then
    echo "Error: gtk-launch is required so GNOME Shell can associate the dev run with the staged desktop entry." >&2
    exit 1
fi

backup_dir="$(mktemp -d)"
declare -a replaced_targets=()
declare -a replaced_backups=()
declare -a staged_targets=()

cleanup() {
    local status="${1:-$?}"

    trap - EXIT INT TERM

    if [[ "$keep_staged" == "1" ]]; then
        rm -rf "$backup_dir"
        exit "$status"
    fi

    local target
    local index
    for target in "${staged_targets[@]}"; do
        rm -f -- "$target"
    done

    for index in "${!replaced_targets[@]}"; do
        rm -f -- "${replaced_targets[$index]}"
        mv -- "${replaced_backups[$index]}" "${replaced_targets[$index]}"
        if [[ "${replaced_targets[$index]}" == "$desktop_target" ]]; then
            repair_desktop_entry_icon "${replaced_targets[$index]}"
        fi
    done

    refresh_shell_metadata
    rm -rf "$backup_dir"
    exit "$status"
}

trap 'cleanup $?' EXIT
trap 'cleanup 130' INT
trap 'cleanup 143' TERM

refresh_shell_metadata() {
    if command -v gtk4-update-icon-cache >/dev/null 2>&1; then
        gtk4-update-icon-cache -qtf "$icons_root" >/dev/null 2>&1 || true
    elif command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -qtf "$icons_root" >/dev/null 2>&1 || true
    fi
}

desktop_entry_icon_path() {
    local desktop_file="$1"

    awk -F= '$1 == "Icon" { print substr($0, index($0, "=") + 1); exit }' "$desktop_file"
}

install_desktop_icon_target() {
    mkdir -p -- "$desktop_icon_dir"
    install -m 0644 -- "$desktop_icon_source" "$desktop_icon_target"
}

repair_desktop_entry_icon() {
    local desktop_file="$1"
    local icon_path
    local repaired

    [[ -f "$desktop_file" ]] || return 0

    icon_path="$(desktop_entry_icon_path "$desktop_file")"
    if [[ "$icon_path" != /* || -e "$icon_path" ]]; then
        return 0
    fi

    repaired="$backup_dir/$(basename -- "$desktop_file").repaired.$RANDOM"
    if grep -q '^Icon=' "$desktop_file"; then
        sed -e "s|^Icon=.*$|Icon=$desktop_icon_target|" "$desktop_file" > "$repaired"
    else
        cp -- "$desktop_file" "$repaired"
        printf 'Icon=%s\n' "$desktop_icon_target" >> "$repaired"
    fi
    install -m 0644 -- "$repaired" "$desktop_file"
}

matching_binary_pids() {
    local pid
    local exe
    for pid in /proc/[0-9]*; do
        [[ -e "$pid/exe" ]] || continue
        exe="$(readlink -f -- "$pid/exe" 2>/dev/null || true)"
        [[ "$exe" == "$binary" ]] || continue
        basename -- "$pid"
    done
}

wait_for_pids_to_exit() {
    local deadline="$1"

    while (( SECONDS < deadline )); do
        mapfile -t current_pids < <(matching_binary_pids | sort -u)
        if (( ${#current_pids[@]} == 0 )); then
            return 0
        fi
        sleep 0.1
    done

    mapfile -t current_pids < <(matching_binary_pids | sort -u)
    (( ${#current_pids[@]} == 0 ))
}

stop_existing_instance() {
    echo "Stopping existing LushText instance so GNOME Shell picks up the refreshed dock icon." >&2

    if command -v gapplication >/dev/null 2>&1; then
        gapplication action "$app_id" quit >/dev/null 2>&1 || true
    fi

    if wait_for_pids_to_exit $((SECONDS + 5)); then
        return 0
    fi

    mapfile -t current_pids < <(matching_binary_pids | sort -u)
    if (( ${#current_pids[@]} > 0 )); then
        echo "Existing instance ignored app.quit; sending SIGTERM to refresh the dock icon cleanly." >&2
        kill "${current_pids[@]}" 2>/dev/null || true
    fi

    if wait_for_pids_to_exit $((SECONDS + 5)); then
        return 0
    fi

    echo "Error: existing LushText instance is still running; close it manually and retry." >&2
    exit 1
}

stage_file() {
    local src="$1"
    local dst="$2"

    mkdir -p -- "$(dirname -- "$dst")"

    if [[ -e "$dst" ]]; then
        local backup
        backup="$backup_dir/$(basename -- "$dst").$RANDOM.bak"
        mv -- "$dst" "$backup"
        replaced_targets+=("$dst")
        replaced_backups+=("$backup")
    else
        staged_targets+=("$dst")
    fi

    install -m 0644 -- "$src" "$dst"
}

install_desktop_icon_target

desktop_tmp="$backup_dir/$app_id.desktop"
sed \
    -e "s|^Exec=.*$|Exec=$binary %U|" \
    -e "/^Exec=/a TryExec=$binary" \
    -e "s|^Icon=.*$|Icon=$desktop_icon_target|" \
    "$desktop_template" > "$desktop_tmp"

repair_desktop_entry_icon "$desktop_target"
stage_file "$desktop_tmp" "$desktop_target"
stage_file "$repo_root/data/icons/dev.cominotti.lushtext.svg" \
    "$icons_root/scalable/apps/dev.cominotti.lushtext.svg"
stage_file "$repo_root/data/icons/dev.cominotti.lushtext-symbolic.svg" \
    "$icons_root/symbolic/apps/dev.cominotti.lushtext-symbolic.svg"
stage_file "$repo_root/data/icons/hicolor/32x32/apps/dev.cominotti.lushtext.png" \
    "$icons_root/32x32/apps/dev.cominotti.lushtext.png"
stage_file "$repo_root/data/icons/hicolor/64x64/apps/dev.cominotti.lushtext.png" \
    "$icons_root/64x64/apps/dev.cominotti.lushtext.png"
stage_file "$repo_root/data/icons/hicolor/128x128/apps/dev.cominotti.lushtext.png" \
    "$icons_root/128x128/apps/dev.cominotti.lushtext.png"

refresh_shell_metadata

if [[ "$no_exec" == "1" ]]; then
    echo "Prepared temporary GNOME desktop integration for $app_id"
    exit 0
fi

mapfile -t before_pids < <(matching_binary_pids | sort -u)
if (( ${#before_pids[@]} > 0 )); then
    if [[ "$force_restart" == "1" ]]; then
        stop_existing_instance
        before_pids=()
    else
        echo "Existing LushText instance detected; activating it through the staged desktop entry." >&2
        echo "If the dock item came from an older direct launch, close all LushText windows once and rerun make refresh-dock-icon for a fresh Shell association." >&2
    fi
fi
gtk-launch "$app_id" "$@"

new_pids=()
for _ in $(seq 1 100); do
    mapfile -t current_pids < <(matching_binary_pids | sort -u)
    new_pids=()
    for pid in "${current_pids[@]}"; do
        skip=0
        for old_pid in "${before_pids[@]}"; do
            if [[ "$pid" == "$old_pid" ]]; then
                skip=1
                break
            fi
        done
        if [[ "$skip" == "0" ]]; then
            new_pids+=("$pid")
        fi
    done

    if (( ${#new_pids[@]} > 0 )); then
        break
    fi
    sleep 0.1
done

if (( ${#new_pids[@]} == 0 )); then
    if (( ${#before_pids[@]} == 0 )); then
        exit 0
    fi
    new_pids=("${before_pids[@]}")
fi

while :; do
    any_running=0
    for pid in "${new_pids[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            any_running=1
            break
        fi
    done

    if [[ "$any_running" == "0" ]]; then
        break
    fi

    sleep 1
done
