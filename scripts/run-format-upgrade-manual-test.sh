#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

mode="${1:-}"
if [[ "$mode" != "newer" && "$mode" != "older" ]]; then
    echo "Usage: $0 newer|older" >&2
    exit 2
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
marker_name=".lushtext-format-upgrade-manual-test"
data_home="${FORMAT_UPGRADE_TEST_HOME:-}"

if [[ -z "$data_home" ]]; then
    data_home="$(mktemp -d -t "lushtext-format-upgrade-$mode.XXXXXX")"
else
    mkdir -p -- "$data_home"
fi

app_home="$data_home/lushtext"
marker="$app_home/$marker_name"

if [[ -e "$app_home" ]]; then
    if [[ -f "$marker" ]]; then
        rm -rf -- "$app_home"
    elif [[ -n "$(find "$app_home" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
        echo "Error: $app_home already contains data not created by this manual test." >&2
        echo "Choose an empty FORMAT_UPGRADE_TEST_HOME or remove the directory yourself." >&2
        exit 1
    fi
fi

mkdir -p -- "$app_home"
{
    printf 'mode=%s\n' "$mode"
    printf 'created_by=%s\n' "$(basename -- "$0")"
} > "$marker"

case "$mode" in
    newer)
        version="${FORMAT_UPGRADE_TEST_VERSION:-999}"
        expected="newer-data dialog with Quit and Start Fresh, no Convert"
        after_action="After Start Fresh, inspect: $app_home/format-upgrade-backups"
        ;;
    older)
        version="${FORMAT_UPGRADE_TEST_VERSION:-0}"
        if [[ "$version" != "0" ]]; then
            echo "Error: the older manual fixture only supports FORMAT_UPGRADE_TEST_VERSION=0." >&2
            exit 1
        fi
        export LUSHTEXT_FORMAT_UPGRADE_MANUAL_SESSION_V0=1
        expected="older-data dialog with Convert, Start Fresh, and Quit"
        after_action="After Convert, inspect session.json for version 1 and backups under: $app_home/format-upgrade-backups"
        ;;
esac

if [[ ! "$version" =~ ^[0-9]+$ ]]; then
    echo "Error: FORMAT_UPGRADE_TEST_VERSION must be a non-negative integer." >&2
    exit 1
fi

printf '%s\n' \
    '{' \
    '  "kind": "dev.cominotti.lushtext.session",' \
    "  \"version\": $version," \
    '  "data": { "tabs": [], "active_tab_index": null }' \
    '}' > "$app_home/session.json"

if [[ "$mode" == "newer" ]]; then
    mkdir -p -- "$app_home/drafts"
    printf '%s\n' 'manual draft body preserved by Start Fresh' > "$app_home/drafts/manual-test.draft"
fi

echo "Prepared isolated format-upgrade test data:"
echo "  Scenario: $mode"
echo "  XDG_DATA_HOME=$data_home"
echo "  App data: $app_home"
echo "Expected startup behavior: $expected."
echo "$after_action"

export XDG_DATA_HOME="$data_home"
export LUSHTEXT_DEV_RUN_FORCE_RESTART=1
exec "$repo_root/scripts/run-dev-app.sh"
