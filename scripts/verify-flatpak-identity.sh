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
required_mime_types=(
    text/plain
    application/x-zerosize
    application/json
    application/json5
    application/toml
    application/yaml
    text/markdown
)
removed_source_mime_types=(
    text/x-csrc
    text/x-chdr
    text/x-python
    text/x-rust
)
expected_mime_line="MimeType=$(IFS=';'; printf '%s;' "${required_mime_types[*]}")"

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

desktop_mime_line() {
    local desktop_file="$1"

    awk -F= '$1 == "MimeType" { print $0; exit }' "$desktop_file"
}

mime_section_contains_desktop() {
    local mime_info="$1"
    local section="$2"

    awk -v section="$section" -v desktop_id="$desktop_id" '
        $0 == section ":" { in_section = 1; next }
        in_section && $0 !~ /^\t/ { in_section = 0 }
        in_section && $0 == "\t" desktop_id { found = 1 }
        END { exit found ? 0 : 1 }
    ' <<< "$mime_info"
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

section "Flatpak desktop MIME allowlist"
actual_mime_line="$(desktop_mime_line "$export_path")"
if [[ "$actual_mime_line" != "$expected_mime_line" ]]; then
    fail "$export_path has '$actual_mime_line', expected '$expected_mime_line'."
fi
echo "$actual_mime_line"

for mime in "${removed_source_mime_types[@]}"; do
    if grep -Fq "$mime" <<< "$actual_mime_line"; then
        fail "$export_path still advertises removed source MIME type '$mime'."
    fi
done
echo "Removed source MIME types are absent from the Flatpak export."

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
for mime in "${required_mime_types[@]}"; do
    mime_info="$(gio mime "$mime")"
    if ! mime_section_contains_desktop "$mime_info" "Registered applications"; then
        fail "$desktop_id is not registered by 'gio mime $mime'."
    fi
    if ! mime_section_contains_desktop "$mime_info" "Recommended applications"; then
        fail "$desktop_id is not recommended by 'gio mime $mime'."
    fi
    echo "$mime: registered and recommended as $desktop_id"
done

section "Result"
echo "Flatpak desktop identity is usable for $desktop_id."
