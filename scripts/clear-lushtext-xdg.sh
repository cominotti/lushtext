#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

app_id="dev.cominotti.lushtext"
dev_app_id="$app_id.Devel"
app_data_dir_name="lushtext"

dry_run="${LUSHTEXT_CLEAR_DRY_RUN:-${DRY_RUN:-0}}"
include_flatpak="${LUSHTEXT_CLEAR_INCLUDE_FLATPAK:-${INCLUDE_FLATPAK:-1}}"
reset_gsettings="${LUSHTEXT_CLEAR_RESET_GSETTINGS:-${RESET_GSETTINGS:-1}}"
allow_running="${LUSHTEXT_CLEAR_ALLOW_RUNNING:-${ALLOW_RUNNING:-0}}"
assume_yes="${LUSHTEXT_CLEAR_ASSUME_YES:-${ASSUME_YES:-0}}"

declare -a removal_paths=()
declare -a removal_labels=()
declare -a removal_basenames=()

truthy() {
    case "${1:-}" in
        1 | true | TRUE | yes | YES | on | ON)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

fail() {
    echo "Error: $*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: scripts/clear-lushtext-xdg.sh [--yes] [--dry-run] [--help]

Remove LushText-owned XDG data/config/cache/state, development desktop/icon
staging, the Flatpak app-private XDG home, and reset the app's GSettings.

This is destructive: it deletes drafts, local history, sessions, and settings.
The exact list of paths is printed for confirmation before anything is removed.

Options:
  -y, --yes      Skip the interactive confirmation (required for automation).
      --dry-run  Print what would be removed without deleting anything.
  -h, --help     Print this help and exit without removing anything.

Environment overrides (all accept 1/true/yes/on):
  DRY_RUN / LUSHTEXT_CLEAR_DRY_RUN                  report only, remove nothing
  ASSUME_YES / LUSHTEXT_CLEAR_ASSUME_YES            skip the confirmation prompt
  INCLUDE_FLATPAK / LUSHTEXT_CLEAR_INCLUDE_FLATPAK  include ~/.var/app (default 1)
  RESET_GSETTINGS / LUSHTEXT_CLEAR_RESET_GSETTINGS  reset GSettings (default 1)
  ALLOW_RUNNING / LUSHTEXT_CLEAR_ALLOW_RUNNING      proceed while LushText runs
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -y | --yes)
            assume_yes=1
            shift
            ;;
        --dry-run)
            dry_run=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            usage >&2
            echo >&2
            echo "Error: unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

strip_trailing_slashes() {
    local value="$1"
    while [[ "$value" != "/" && "$value" == */ ]]; do
        value="${value%/}"
    done
    printf '%s\n' "$value"
}

require_absolute_base() {
    local label="$1"
    local value="$2"

    value="$(strip_trailing_slashes "$value")"
    [[ -n "$value" ]] || fail "$label resolved to an empty path."
    [[ "$value" == /* ]] || fail "$label must be absolute, got '$value'."
    [[ "$value" != "/" ]] || fail "$label resolved to '/', refusing to continue."

    printf '%s\n' "$value"
}

xdg_base() {
    local env_name="$1"
    local fallback="$2"
    local value="${!env_name:-}"

    if [[ -z "$value" ]]; then
        value="$fallback"
    fi

    require_absolute_base "$env_name" "$value"
}

queue_removal() {
    local label="$1"
    local path="$2"
    local expected_basename="$3"

    path="$(strip_trailing_slashes "$path")"
    [[ "$path" == /* ]] || fail "$label path must be absolute, got '$path'."
    [[ "$path" != "/" ]] || fail "$label path resolved to '/', refusing to continue."
    [[ "${path##*/}" == "$expected_basename" ]] ||
        fail "$label path '$path' does not end in '$expected_basename'."

    removal_labels+=("$label")
    removal_paths+=("$path")
    removal_basenames+=("$expected_basename")
}

check_lushtext_not_running() {
    truthy "$allow_running" && return 0
    truthy "$dry_run" && return 0
    command -v pgrep >/dev/null 2>&1 || return 0

    if pgrep -x lushtext >/dev/null 2>&1; then
        fail "LushText appears to be running. Close it first, or set ALLOW_RUNNING=1 if you intentionally want to clear data while it may recreate files."
    fi
}

remove_owned_path() {
    local label="$1"
    local path="$2"
    local expected_basename="$3"

    [[ "${path##*/}" == "$expected_basename" ]] ||
        fail "$label path '$path' no longer ends in '$expected_basename'."

    if [[ ! -e "$path" && ! -L "$path" ]]; then
        echo "Not present: $label ($path)"
        return 0
    fi

    if truthy "$dry_run"; then
        echo "Would remove: $label ($path)"
        return 0
    fi

    echo "Removing: $label ($path)"
    rm -rf -- "$path"
}

reset_lushtext_gsettings() {
    truthy "$reset_gsettings" || {
        echo "Skipping GSettings reset because RESET_GSETTINGS=0."
        return 0
    }

    if ! command -v gsettings >/dev/null 2>&1; then
        echo "gsettings not found; skipping $app_id settings reset."
        return 0
    fi

    if ! gsettings list-schemas 2>/dev/null | grep -Fxq "$app_id"; then
        echo "GSettings schema not visible: $app_id; skipping settings reset."
        return 0
    fi

    if truthy "$dry_run"; then
        echo "Would reset GSettings schema: $app_id"
        return 0
    fi

    echo "Resetting GSettings schema: $app_id"
    gsettings reset-recursively "$app_id"
}

refresh_desktop_metadata() {
    if truthy "$dry_run"; then
        echo "Would refresh desktop and icon metadata."
        return 0
    fi

    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q "$data_home/applications" >/dev/null 2>&1 || true
    fi

    if command -v gtk4-update-icon-cache >/dev/null 2>&1; then
        gtk4-update-icon-cache -qtf "$data_home/icons/hicolor" >/dev/null 2>&1 || true
    elif command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -qtf "$data_home/icons/hicolor" >/dev/null 2>&1 || true
    fi
}

home="$(require_absolute_base "HOME" "${HOME:-}")"
data_home="$(xdg_base "XDG_DATA_HOME" "$home/.local/share")"
config_home="$(xdg_base "XDG_CONFIG_HOME" "$home/.config")"
cache_home="$(xdg_base "XDG_CACHE_HOME" "$home/.cache")"
state_home="$(xdg_base "XDG_STATE_HOME" "$home/.local/state")"

queue_removal "\$XDG_DATA_HOME/$app_data_dir_name" "$data_home/$app_data_dir_name" "$app_data_dir_name"
queue_removal "\$XDG_CONFIG_HOME/$app_data_dir_name" "$config_home/$app_data_dir_name" "$app_data_dir_name"
queue_removal "\$XDG_CACHE_HOME/$app_data_dir_name" "$cache_home/$app_data_dir_name" "$app_data_dir_name"
queue_removal "\$XDG_STATE_HOME/$app_data_dir_name" "$state_home/$app_data_dir_name" "$app_data_dir_name"
queue_removal "same-ID local desktop shadow" "$data_home/applications/$app_id.desktop" "$app_id.desktop"
queue_removal "development desktop launcher" "$data_home/applications/$dev_app_id.desktop" "$dev_app_id.desktop"
queue_removal "development absolute icon staging" "$data_home/icons/lushtext-dev-run" "lushtext-dev-run"
queue_removal "development scalable icon" "$data_home/icons/hicolor/scalable/apps/$dev_app_id.svg" "$dev_app_id.svg"
queue_removal "development symbolic icon" "$data_home/icons/hicolor/symbolic/apps/$dev_app_id-symbolic.svg" "$dev_app_id-symbolic.svg"
queue_removal "development 32px icon" "$data_home/icons/hicolor/32x32/apps/$dev_app_id.png" "$dev_app_id.png"
queue_removal "development 64px icon" "$data_home/icons/hicolor/64x64/apps/$dev_app_id.png" "$dev_app_id.png"
queue_removal "development 128px icon" "$data_home/icons/hicolor/128x128/apps/$dev_app_id.png" "$dev_app_id.png"

if truthy "$include_flatpak"; then
    queue_removal "Flatpak app-private XDG home" "$home/.var/app/$app_id" "$app_id"
else
    echo "Skipping Flatpak app-private XDG home because INCLUDE_FLATPAK=0."
fi

confirm_removal() {
    truthy "$dry_run" && return 0
    truthy "$assume_yes" && return 0

    echo "About to permanently remove LushText-owned state, including drafts,"
    echo "local history, sessions, and settings:"
    for index in "${!removal_paths[@]}"; do
        printf '  - %s (%s)\n' "${removal_labels[$index]}" "${removal_paths[$index]}"
    done
    if truthy "$reset_gsettings"; then
        printf '  - GSettings schema %s (reset-recursively)\n' "$app_id"
    fi
    echo
    echo "Re-run with --dry-run to preview, or --yes to skip this prompt."

    if [[ ! -t 0 ]]; then
        fail "stdin is not a terminal and --yes was not given; refusing to remove anything."
    fi

    local reply=""
    read -r -p "Type 'yes' to continue: " reply || reply=""
    case "$reply" in
        y | Y | yes | YES | Yes)
            return 0
            ;;
        *)
            echo "Aborted; nothing was removed."
            exit 1
            ;;
    esac
}

check_lushtext_not_running
confirm_removal

echo "Clearing LushText-owned XDG/config state."
for index in "${!removal_paths[@]}"; do
    remove_owned_path "${removal_labels[$index]}" "${removal_paths[$index]}" "${removal_basenames[$index]}"
done
refresh_desktop_metadata
reset_lushtext_gsettings
