#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

domain="${LUSHTEXT_FLATHUB_DOMAIN:-cominotti.dev}"
app_id="${LUSHTEXT_FLATHUB_APP_ID:-dev.cominotti.lushtext}"
token="${1:-${FLATHUB_VERIFICATION_TOKEN:-}}"
base_url="https://$domain"
token_url="$base_url/.well-known/org.flathub.VerifiedApps.txt"

fail() {
    echo "error: $*" >&2
    exit 1
}

command -v curl >/dev/null 2>&1 || fail "curl is required"

echo "Flathub app ID: $app_id"
echo "Verification domain: $domain"
echo "Verification URL: $token_url"

if ! curl --proto '=https' --tlsv1.2 -sSIL "$base_url" >/dev/null; then
    fail "HTTPS validation failed for $base_url. Fix DNS/TLS before requesting Flathub verification."
fi
echo "HTTPS certificate is valid for $domain."

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

if ! curl --proto '=https' --tlsv1.2 -fsSL "$token_url" -o "$tmp"; then
    fail "could not fetch $token_url. Publish the Flathub token file before verification."
fi
echo "Verification file is reachable."

if [[ -n "$token" ]]; then
    if grep -v '^[[:space:]]*#' "$tmp" |
        awk '{$1=$1; print}' |
        grep -Fxq "$token"; then
        echo "Expected Flathub verification token is present."
    else
        fail "expected Flathub verification token was not found as a non-comment line"
    fi
else
    echo "No expected token was provided; checked HTTPS and file reachability only."
fi
