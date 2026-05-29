#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Local confined smoke test for the LushText Snap.
#
# Native and Flatpak tests cannot catch confinement-only failures (a baked path
# that the snap `layout:` does not map, a missing schema, a blocked workspace
# root, an AppArmor/seccomp denial). This script builds the strict-confined snap,
# installs it, launches it headlessly, confirms it loads its resources and opens
# a file, and fails if any AppArmor/seccomp denial is observed.
#
# It SKIPS cleanly (exit 0 with a clear message) when the tooling or the GNOME 50
# platform snap required to build a GTK 4.22 snap is unavailable, so it is safe to
# wire into `make` and CI before the platform dependency lands.
#
# Usage: scripts/run-snap-smoke.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SNAP_NAME="lushtext"

skip() {
  echo "SKIP: $*"
  exit 0
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# --- Gate 1: tooling -------------------------------------------------------
command -v snapcraft >/dev/null 2>&1 || skip "snapcraft is not installed."
command -v snap >/dev/null 2>&1 || skip "snapd/snap is not installed."

# --- Gate 2: GNOME 50 platform snap availability ---------------------------
# LushText needs GTK 4.22 (GNOME 50). The gnome extension currently provides
# only gnome-46-2404 (GTK 4.14). Probe the store for a GNOME 50 platform snap;
# if none exists yet, building cannot satisfy the floor, so skip.
platform_snap_available() {
  local candidate
  for candidate in gnome-50-2604 gnome-50-2404 gnome-core26; do
    if curl -fsS -H "Snap-Device-Series: 16" \
        "https://api.snapcraft.io/v2/snaps/info/${candidate}?fields=base" \
        >/dev/null 2>&1; then
      echo "$candidate"
      return 0
    fi
  done
  return 1
}

if [ "${LUSHTEXT_SNAP_FORCE:-0}" != "1" ]; then
  if ! PLATFORM_SNAP="$(platform_snap_available)"; then
    skip "no published GNOME 50 platform snap (gnome-50-2604 etc.) yet — \
GTK 4.22 build is gated. Set LUSHTEXT_SNAP_FORCE=1 to attempt anyway."
  fi
  echo "Found GNOME 50 platform snap: ${PLATFORM_SNAP}"
fi

# --- Build -----------------------------------------------------------------
cd "$REPO_ROOT"
echo "Building snap (LXD backend)..."
snapcraft pack --use-lxd

SNAP_FILE="$(ls -t "${SNAP_NAME}"_*.snap 2>/dev/null | head -n1 || true)"
[ -n "$SNAP_FILE" ] || fail "no .snap artifact produced by snapcraft."
echo "Built: $SNAP_FILE"

# --- Install ---------------------------------------------------------------
echo "Installing $SNAP_FILE (dangerous, unsigned local build)..."
sudo snap install --dangerous "$SNAP_FILE"
trap 'sudo snap remove --purge "$SNAP_NAME" >/dev/null 2>&1 || true' EXIT
# Connect manual-connect plugs the smoke test exercises.
sudo snap connect "${SNAP_NAME}:removable-media" >/dev/null 2>&1 || true

# --- Headless launch + resource/file-open assertions -----------------------
# Prepare a HOME-rooted file the confined app is allowed to read.
WORK_DIR="$(mktemp -d "${HOME}/.lushtext-snap-smoke.XXXXXX")"
SAMPLE="$WORK_DIR/sample.txt"
echo "hello from snap smoke test" > "$SAMPLE"
trap 'rm -rf "$WORK_DIR"; sudo snap remove --purge "$SNAP_NAME" >/dev/null 2>&1 || true' EXIT

LOG="$(mktemp)"
echo "Launching confined app headlessly..."
# A headless compositor (mutter --headless / xvfb) must be running; the runner
# (or CI) provides it. Give the app a moment to register resources and open the
# file, then quit it.
if command -v xvfb-run >/dev/null 2>&1; then
  RUNNER=(xvfb-run -a)
else
  RUNNER=()
fi

set +e
timeout 30s "${RUNNER[@]}" snap run "$SNAP_NAME" "$SAMPLE" >"$LOG" 2>&1 &
APP_PID=$!
sleep 12
kill "$APP_PID" >/dev/null 2>&1 || true
wait "$APP_PID" 2>/dev/null
set -e

# The app panics (non-zero, with a clear message) if GResource or the GSettings
# schema fail to load under confinement. Treat those messages as hard failures.
if grep -qiE "failed to load installed GResource|failed to load GResource|Settings schema .* is not installed" "$LOG"; then
  echo "----- app output -----"; cat "$LOG"; echo "----------------------"
  fail "confined app could not load its GResource bundle or GSettings schema."
fi

# --- AppArmor / seccomp denial check ---------------------------------------
echo "Checking for AppArmor/seccomp denials..."
DENIALS=""
if command -v snappy-debug >/dev/null 2>&1; then
  DENIALS="$(sudo snappy-debug --elapsed 1 2>/dev/null | grep -iE "DENIED|denied" || true)"
else
  # Fallback: scan the kernel/journal log for apparmor denials naming this snap.
  DENIALS="$(journalctl -k --since '1 min ago' 2>/dev/null \
    | grep -iE "apparmor=\"DENIED\".*snap.${SNAP_NAME}" || true)"
fi

if [ -n "$DENIALS" ]; then
  echo "----- denials -----"; echo "$DENIALS"; echo "-------------------"
  fail "AppArmor/seccomp denials observed for snap.${SNAP_NAME}."
fi

echo "PASS: confined snap launched, loaded resources, opened a HOME file, no denials."
