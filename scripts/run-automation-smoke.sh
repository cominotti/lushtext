#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/automation}"
BINARY="$REPO_ROOT/target/debug/lushtext"

usage() {
    cat <<'EOF'
Usage: scripts/run-automation-smoke.sh [--artifact-dir DIR] [--binary PATH]

Launch LushText in an isolated D-Bus session and headless Mutter compositor,
then verify the app-owned automation object, action catalog, snapshot,
WaitForIdle, and one parameterized GTK action from a real process.
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
smoke_require_command gdbus
smoke_require_command gsettings
smoke_require_command mutter

[[ -x /usr/bin/python3 ]] || smoke_skip "/usr/bin/python3 is not available."
/usr/bin/python3 -c 'import gi' >/dev/null 2>&1 || smoke_skip "system Python lacks PyGObject gi bindings."
[[ -x "$BINARY" ]] || smoke_skip "LushText debug binary is missing. Run 'make build-debug' first."

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
rm -rf "$ARTIFACT_DIR/assertions" "$ARTIFACT_DIR/fixtures" "$ARTIFACT_DIR/logs" "$ARTIFACT_DIR/state"
mkdir -p "$ARTIFACT_DIR/assertions" "$ARTIFACT_DIR/fixtures" "$ARTIFACT_DIR/logs"
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"

if ! /usr/bin/python3 "$REPO_ROOT/scripts/automation-smoke-driver.py" \
    --artifact-dir "$ARTIFACT_DIR" \
    --binary "$BINARY"; then
    smoke_fail "automation D-Bus smoke failed. Artifacts: $ARTIFACT_DIR"
fi
