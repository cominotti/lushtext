#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Deterministic identity/permission verification for the LushText Snap, analogous
# to scripts/verify-flatpak-identity.sh. Reports confinement type and effective
# plug connection state, and asserts the AppStream common-id linkage.
#
# Works against either:
#   - a built artifact:    scripts/verify-snap-identity.sh ./lushtext_*.snap
#   - the installed snap:  scripts/verify-snap-identity.sh   (no argument)
#
# Keys off the snap name declared in snap/snapcraft.yaml so it tracks the actual
# registered name once registration is finalized.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO_ROOT/snap/snapcraft.yaml"
EXPECTED_COMMON_ID="dev.cominotti.lushtext"
EXPECTED_DESKTOP="dev.cominotti.lushtext.desktop"

fail() { echo "FAIL: $*" >&2; exit 1; }
skip() { echo "SKIP: $*"; exit 0; }

[ -f "$MANIFEST" ] || fail "missing $MANIFEST"
SNAP_NAME="$(awk '/^name:/ { print $2; exit }' "$MANIFEST")"
[ -n "$SNAP_NAME" ] || fail "could not read snap name from $MANIFEST"
echo "Snap name (from manifest): $SNAP_NAME"

ARTIFACT="${1:-}"

assert_snap_yaml() {
  # $1 = path to an extracted meta/snap.yaml
  local yaml="$1"
  local confinement common_id
  confinement="$(awk '/^confinement:/ { print $2; exit }' "$yaml")"
  echo "Confinement: ${confinement:-<unset>}"
  [ "$confinement" = "strict" ] || fail "confinement is '${confinement}', expected 'strict'."

  common_id="$(awk '/common-id:/ { print $2; exit }' "$yaml" | tr -d '\"')"
  echo "common-id: ${common_id:-<unset>}"
  [ "$common_id" = "$EXPECTED_COMMON_ID" ] \
    || fail "common-id is '${common_id}', expected '${EXPECTED_COMMON_ID}'."

  echo "Declared plugs:"
  awk '/^plugs:|^    plugs:/{flag=1;next} /^[a-z]/{flag=0} flag' "$yaml" | sed 's/^/  /' || true
}

if [ -n "$ARTIFACT" ]; then
  # --- Inspect a built .snap artifact ---
  command -v unsquashfs >/dev/null 2>&1 || skip "unsquashfs not available."
  [ -f "$ARTIFACT" ] || fail "no such artifact: $ARTIFACT"
  TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
  unsquashfs -q -d "$TMP/squashfs" "$ARTIFACT" meta/snap.yaml \
    "meta/gui/${EXPECTED_DESKTOP}" "meta/gui/${SNAP_NAME}.desktop" >/dev/null 2>&1 || true
  [ -f "$TMP/squashfs/meta/snap.yaml" ] || fail "meta/snap.yaml not found in artifact."
  assert_snap_yaml "$TMP/squashfs/meta/snap.yaml"

  if ls "$TMP"/squashfs/meta/gui/*.desktop >/dev/null 2>&1; then
    echo "Desktop entry present in artifact: $(ls "$TMP"/squashfs/meta/gui/*.desktop)"
  else
    fail "no desktop entry found in the built snap's meta/gui."
  fi
  echo "PASS: artifact confinement=strict, common-id=${EXPECTED_COMMON_ID}, desktop entry present."
else
  # --- Inspect the installed snap ---
  command -v snap >/dev/null 2>&1 || skip "snap is not installed."
  snap list "$SNAP_NAME" >/dev/null 2>&1 || skip "snap '$SNAP_NAME' is not installed."

  echo "Installed snap info:"
  snap info "$SNAP_NAME" | grep -E "^(name|summary|tracking|refresh-date|installed):" || true
  echo "Effective plug connections:"
  snap connections "$SNAP_NAME" || true

  # Confirm the desktop entry the host sees is exported for this snap.
  if ls "/var/lib/snapd/desktop/applications/${SNAP_NAME}_"*.desktop >/dev/null 2>&1; then
    echo "Host-exported desktop entry: $(ls /var/lib/snapd/desktop/applications/${SNAP_NAME}_*.desktop)"
  else
    fail "no host-exported desktop entry for snap '${SNAP_NAME}'."
  fi
  echo "PASS: installed snap reports confinement/plugs and an exported desktop entry."
fi
