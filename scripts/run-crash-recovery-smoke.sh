#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/smoke-common.sh
source "$REPO_ROOT/scripts/smoke-common.sh"

ARTIFACT_DIR="${LUSHTEXT_SMOKE_ARTIFACT_DIR:-build/smoke/crash-recovery}"
BINARY="$REPO_ROOT/target/debug/lushtext"

usage() {
    cat <<'EOF'
Usage: scripts/run-crash-recovery-smoke.sh [--artifact-dir DIR] [--binary PATH]

Launch LushText in isolated app state, create draft/session recovery state
through the real GTK process, terminate it with SIGKILL, relaunch, and preserve
metadata, logs, assertions, and screenshots as artifacts.
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

[[ -x /usr/bin/python3 ]] || smoke_skip "/usr/bin/python3 is not available."
[[ -x "$BINARY" ]] || smoke_skip "LushText debug binary is missing. Run 'make build-debug' first."

if [[ ! -x /usr/libexec/at-spi2-registryd ]]; then
    smoke_skip "at-spi2-registryd is not available."
fi
if ! /usr/bin/python3 -c 'import gi, pyatspi' >/dev/null 2>&1; then
    smoke_skip "Python AT-SPI bindings are not available."
fi

ARTIFACT_DIR="$(smoke_artifact_dir "$ARTIFACT_DIR")"
smoke_write_environment_report "$ARTIFACT_DIR/environment.txt"

if ! /usr/bin/python3 "$REPO_ROOT/scripts/crash-recovery-smoke-driver.py" \
    --artifact-dir "$ARTIFACT_DIR" \
    --binary "$BINARY"; then
    smoke_fail "crash recovery smoke failed. Artifacts: $ARTIFACT_DIR"
fi
