#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd -- "$script_dir/.." && pwd)"
tmpdir="$(mktemp -d)"
expected_mime="MimeType=text/plain;application/x-zerosize;application/json;application/json5;application/toml;application/yaml;text/markdown;"

cleanup() {
    rm -rf "$tmpdir"
}
trap cleanup EXIT

mkdir -p "$tmpdir/bin" "$tmpdir/target/debug" "$tmpdir/xdg"
cp "$(command -v sleep)" "$tmpdir/target/debug/lushtext"
chmod +x "$tmpdir/target/debug/lushtext"

cat > "$tmpdir/bin/gtk-launch" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$1" > "$GTK_LAUNCH_LOG"
"$CARGO_TARGET_DIR/debug/lushtext" 1 &
EOF
chmod +x "$tmpdir/bin/gtk-launch"

cat > "$tmpdir/bin/update-desktop-database" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$UPDATE_DESKTOP_DATABASE_LOG"
exit 0
EOF
chmod +x "$tmpdir/bin/update-desktop-database"

cat > "$tmpdir/bin/gtk4-update-icon-cache" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$tmpdir/bin/gtk4-update-icon-cache"

cat > "$tmpdir/bin/gapplication" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >> "$GAPPLICATION_LOG"

case "$1" in
    list-apps)
        if [[ -e "$GAPPLICATION_RUNNING_MARKER" ]]; then
            printf 'dev.cominotti.lushtext\n'
        fi
        ;;
    action)
        if [[ "$2" == "dev.cominotti.lushtext" && "$3" == "quit" ]]; then
            if [[ "${GAPPLICATION_REFUSE_QUIT:-0}" == "1" ]]; then
                exit 0
            fi
            rm -f "$GAPPLICATION_RUNNING_MARKER"
        fi
        ;;
esac
EOF
chmod +x "$tmpdir/bin/gapplication"

export PATH="$tmpdir/bin:$PATH"
export XDG_DATA_HOME="$tmpdir/xdg"
export CARGO_TARGET_DIR="$tmpdir/target"
export GTK_LAUNCH_LOG="$tmpdir/gtk-launch.log"
export UPDATE_DESKTOP_DATABASE_LOG="$tmpdir/update-desktop-database.log"
export GAPPLICATION_LOG="$tmpdir/gapplication.log"
export GAPPLICATION_RUNNING_MARKER="$tmpdir/gapplication-running"

LUSHTEXT_DEV_RUN_NO_EXEC=1 "$repo_root/scripts/run-dev-app.sh"
if [[ -e "$tmpdir/xdg/applications/dev.cominotti.lushtext.desktop" ]]; then
    echo "normal no-exec staging left a production desktop entry behind" >&2
    exit 1
fi

LUSHTEXT_DEV_RUN_NO_EXEC=1 \
LUSHTEXT_DEV_RUN_KEEP_STAGED=1 \
    "$repo_root/scripts/run-dev-app.sh"
if [[ -e "$tmpdir/xdg/applications/dev.cominotti.lushtext.desktop" ]]; then
    echo "persistent staging used the production desktop ID" >&2
    exit 1
fi
if [[ ! -f "$tmpdir/xdg/applications/dev.cominotti.lushtext.Devel.desktop" ]]; then
    echo "persistent staging did not create the development desktop entry" >&2
    exit 1
fi
grep -q '^Name=LushText (Development)$' \
    "$tmpdir/xdg/applications/dev.cominotti.lushtext.Devel.desktop"
grep -Fxq "$expected_mime" \
    "$tmpdir/xdg/applications/dev.cominotti.lushtext.Devel.desktop"
if grep -Eq 'text/x-(csrc|chdr|python|rust)|jsonc|properties' \
    "$tmpdir/xdg/applications/dev.cominotti.lushtext.Devel.desktop"; then
    echo "persistent staging advertised a removed or deferred MIME type" >&2
    exit 1
fi
if find "$tmpdir/xdg" -name mimeapps.list -print -quit | grep -q .; then
    echo "development staging wrote MIME defaults" >&2
    exit 1
fi
if [[ "$(wc -l < "$UPDATE_DESKTOP_DATABASE_LOG")" -lt 3 ]]; then
    echo "development staging did not refresh the desktop database after staging and restore" >&2
    exit 1
fi

if LUSHTEXT_DEV_RUN_NO_EXEC=1 \
    LUSHTEXT_DEV_RUN_KEEP_STAGED=1 \
    LUSHTEXT_DEV_RUN_STAGED_APP_ID=dev.cominotti.lushtext \
    "$repo_root/scripts/run-dev-app.sh" > "$tmpdir/prod-id.log" 2>&1; then
    echo "production desktop ID was accepted for persistent staging" >&2
    exit 1
fi
grep -q "must not use production desktop ID" "$tmpdir/prod-id.log"

touch "$GAPPLICATION_RUNNING_MARKER"
LUSHTEXT_DEV_RUN_FORCE_RESTART=1 "$repo_root/scripts/run-dev-app.sh"
grep -Fxq "action dev.cominotti.lushtext quit" "$GAPPLICATION_LOG"
if [[ -e "$GAPPLICATION_RUNNING_MARKER" ]]; then
    echo "forced dev run did not ask the registered app owner to quit" >&2
    exit 1
fi
grep -Fxq "dev.cominotti.lushtext" "$GTK_LAUNCH_LOG"

: > "$GTK_LAUNCH_LOG"
touch "$GAPPLICATION_RUNNING_MARKER"
if GAPPLICATION_REFUSE_QUIT=1 \
    LUSHTEXT_DEV_RUN_FORCE_RESTART=1 \
    LUSHTEXT_DEV_RUN_RESTART_TIMEOUT_SECONDS=0 \
    "$repo_root/scripts/run-dev-app.sh" > "$tmpdir/refuse-quit.log" 2>&1; then
    echo "forced dev run succeeded even though the registered app owner refused to quit" >&2
    exit 1
fi
grep -q "still registered under dev.cominotti.lushtext" "$tmpdir/refuse-quit.log"
if [[ -s "$GTK_LAUNCH_LOG" ]]; then
    echo "forced dev run activated gtk-launch after the old app refused to quit" >&2
    exit 1
fi

echo "development desktop staging tests passed"
