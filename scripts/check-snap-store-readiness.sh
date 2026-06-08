#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Report the external Snap Store and platform gates for the LushText snap.
#
# This does not register, publish, or mutate store state. It gives release
# operators one local command that answers the next packaging questions:
#   - Which snap name does this checkout intend to use?
#   - Does the unauthenticated store API know that name?
#   - Does the logged-in Snapcraft account own that name, and is it Unlisted?
#   - Is a GNOME 50 platform snap visible yet?
#   - Does the GitHub CI gate match platform availability?
#   - Are CI store credentials present locally or configured in GitHub?

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO_ROOT/snap/snapcraft.yaml"
API_BASE="https://api.snapcraft.io/v2/snaps/info"
PLATFORM_CANDIDATES=(gnome-50-2604 gnome-50-2404 gnome-core26)

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

status() {
  printf '%-10s %s\n' "$1" "$2"
}

snap_api_head() {
  local name="$1"
  curl -fsS -H "Snap-Device-Series: 16" \
    "${API_BASE}/${name}?fields=name,base,channel-map" >/dev/null
}

check_store_credentials() {
  if [ -n "${SNAPCRAFT_STORE_CREDENTIALS:-}" ]; then
    status "OK" "SNAPCRAFT_STORE_CREDENTIALS is set in this environment."
    return 0
  fi

  if command -v gh >/dev/null 2>&1; then
    if gh auth status >/dev/null 2>&1; then
      if gh secret list 2>/dev/null | awk '$1 == "SNAPCRAFT_STORE_CREDENTIALS" { found = 1 } END { exit found ? 0 : 1 }'; then
        status "OK" "GitHub repository secret SNAPCRAFT_STORE_CREDENTIALS is configured."
        return 0
      fi
      status "WAIT" "GitHub secret SNAPCRAFT_STORE_CREDENTIALS is not listed for this repository."
      return 1
    fi
    status "WAIT" "gh is installed but not authenticated; CI secret presence cannot be checked."
    return 1
  fi

  status "WAIT" "SNAPCRAFT_STORE_CREDENTIALS is not set here; install/authenticate gh to check the repository secret."
  return 1
}

github_variable_value() {
  local variable_name="$1"
  gh variable list 2>/dev/null \
    | awk -v name="$variable_name" '$1 == name { print $2; exit }'
}

check_platform_ci_gate() {
  local platform_is_ready="$1"
  local value

  if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
    value="$(github_variable_value SNAP_PLATFORM_AVAILABLE)"
    if [ "$platform_is_ready" -eq 1 ]; then
      if [ "$value" = "true" ]; then
        status "OK" "GitHub variable SNAP_PLATFORM_AVAILABLE=true enables the gated build job."
        return 0
      fi
      status "WAIT" "GNOME 50 platform is visible, but SNAP_PLATFORM_AVAILABLE is '${value:-unset}'."
      return 1
    fi

    if [ "$value" = "true" ]; then
      status "WAIT" "SNAP_PLATFORM_AVAILABLE=true while no GNOME 50 platform candidate is visible."
      return 1
    fi
    status "OK" "SNAP_PLATFORM_AVAILABLE is '${value:-unset}' while the platform is unavailable."
    return 0
  fi

  if [ "$platform_is_ready" -eq 1 ]; then
    status "WAIT" "GNOME 50 platform is visible, but gh is unavailable/unauthenticated so the CI gate cannot be checked."
    return 1
  fi

  status "WAIT" "gh is unavailable/unauthenticated; CI gate cannot be checked before platform activation."
  return 0
}

[ -f "$MANIFEST" ] || fail "missing $MANIFEST"

SNAP_NAME="$(awk -F':' '$1 == "name" { gsub(/^[[:space:]]+|[[:space:]]+$/, "", $2); print $2; exit }' "$MANIFEST" | tr -d '"')"
[ -n "$SNAP_NAME" ] || fail "could not read snap name from $MANIFEST"

echo "Snap Store readiness for: $SNAP_NAME"
echo

store_ready=1
if snap_api_head "$SNAP_NAME" >/dev/null 2>&1; then
  status "OK" "public store API has snap info for '$SNAP_NAME'."
else
  status "WAIT" "public store API has no info for '$SNAP_NAME' (404 or unauthorized)."
  store_ready=0
fi

if command -v snapcraft >/dev/null 2>&1; then
  if snapcraft whoami >/dev/null 2>&1; then
    status "OK" "snapcraft is installed and authenticated."
    owned_line="$(snapcraft names 2>/dev/null | awk -v name="$SNAP_NAME" '$1 == name { print; exit }')"
    if [ -n "$owned_line" ]; then
      status "OK" "logged-in account lists '$SNAP_NAME': $owned_line"
      visibility="$(awk '{ print tolower($3) }' <<<"$owned_line")"
      if [ "$visibility" = "unlisted" ]; then
        status "OK" "'$SNAP_NAME' visibility is Unlisted."
      else
        status "WAIT" "'$SNAP_NAME' visibility is '${visibility:-unknown}', expected Unlisted."
        store_ready=0
      fi
    else
      status "WAIT" "logged-in account does not list '$SNAP_NAME'. Run: snapcraft register $SNAP_NAME"
      store_ready=0
    fi
  else
    status "WAIT" "snapcraft is installed but not authenticated. Run: snapcraft login"
    store_ready=0
  fi
else
  status "WAIT" "snapcraft is not installed; registration and credential export cannot be checked here."
  store_ready=0
fi

echo

if snap_api_head core26 >/dev/null 2>&1; then
  status "OK" "core26 base snap is visible in the store."
else
  status "WAIT" "core26 base snap is not visible from the store API."
  store_ready=0
fi

platform_ready=0
for candidate in "${PLATFORM_CANDIDATES[@]}"; do
  if snap_api_head "$candidate" >/dev/null 2>&1; then
    status "OK" "GNOME 50 platform candidate is visible: $candidate"
    platform_ready=1
    break
  fi
done

if [ "$platform_ready" -eq 0 ]; then
  status "WAIT" "no GNOME 50 platform candidate visible (${PLATFORM_CANDIDATES[*]})."
fi

echo

if ! check_platform_ci_gate "$platform_ready"; then
  store_ready=0
fi

echo

if ! check_store_credentials; then
  store_ready=0
fi

echo

if [ "$store_ready" -eq 1 ] && [ "$platform_ready" -eq 1 ]; then
  status "READY" "store registration/credentials and platform gate appear ready from this machine."
  exit 0
fi

status "PENDING" "one or more external gates still need operator action or platform publication."
exit 1
