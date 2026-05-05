#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Verify that GNOME/GLib can resolve the production LushText desktop entry as
# the installed Flatpak export instead of a stale development desktop file.

set -euo pipefail

app_id="${LUSHTEXT_FLATPAK_APP_ID:-dev.cominotti.lushtext}"
desktop_id="$app_id.desktop"
data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
user_app_dir="$data_home/applications"
user_export_dir="$data_home/flatpak/exports/share/applications"
system_export_dir="${LUSHTEXT_SYSTEM_FLATPAK_EXPORT_DIR:-/var/lib/flatpak/exports/share/applications}"
shadow_path="$user_app_dir/$desktop_id"

fail() {
    echo "error: $*" >&2
    exit 1
}

section() {
    printf '\n== %s ==\n' "$1"
}

desktop_has_flatpak_marker() {
    local desktop_file="$1"

    [[ -f "$desktop_file" ]] && grep -qx "X-Flatpak=$app_id" "$desktop_file"
}

find_flatpak_export() {
    local candidate

    for candidate in \
        "$user_export_dir/$desktop_id" \
        "$system_export_dir/$desktop_id"; do
        if desktop_has_flatpak_marker "$candidate"; then
            printf '%s\n' "$candidate"
            return 0
        fi
    done

    return 1
}

section "Flatpak metadata"
metadata="$(flatpak info --show-metadata "$app_id" 2>/dev/null)" ||
    fail "Flatpak app '$app_id' is not installed or its metadata cannot be read."
printf '%s\n' "$metadata" | grep -qx "name=$app_id" ||
    fail "Flatpak metadata does not identify application name '$app_id'."
printf '%s\n' "$metadata" | grep -qx "command=lushtext" ||
    fail "Flatpak metadata does not declare command 'lushtext'."
echo "Flatpak app: $app_id"
echo "Command: lushtext"

section "Flatpak desktop export"
export_path="$(find_flatpak_export)" ||
    fail "No Flatpak desktop export with X-Flatpak=$app_id found for $desktop_id."
echo "Export: $export_path"
echo "X-Flatpak: $app_id"

section "Development desktop shadow"
if [[ -e "$shadow_path" ]] && ! desktop_has_flatpak_marker "$shadow_path"; then
    fail "$shadow_path is a same-ID non-Flatpak desktop entry and can shadow $export_path."
fi
if [[ -e "$shadow_path" ]]; then
    echo "No non-Flatpak shadow: $shadow_path contains the Flatpak marker."
else
    echo "No non-Flatpak shadow: $shadow_path is absent."
fi

section "Effective Flatpak permissions"
flatpak info --show-permissions "$app_id"

section "MIME registration"
for mime in text/plain text/markdown application/x-zerosize; do
    mime_info="$(gio mime "$mime")"
    if grep -Fqx "	$desktop_id" <<< "$mime_info"; then
        echo "$mime: registered/recommended as $desktop_id"
    else
        fail "$desktop_id is not listed by 'gio mime $mime'."
    fi
done

section "Result"
echo "Flatpak desktop identity is usable for $desktop_id."
