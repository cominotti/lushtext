#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later

set -euo pipefail

target_dir="${1:-build-aux/cominotti-flatpak/flatpak}"
max_file_bytes="${COMINOTTI_FLATPAK_PAGES_MAX_FILE_BYTES:-26214400}"
max_files="${COMINOTTI_FLATPAK_PAGES_MAX_FILES:-20000}"
sample_count="${COMINOTTI_FLATPAK_PAGES_LARGEST_COUNT:-10}"

fail() {
    echo "error: $*" >&2
    exit 1
}

[[ -d "$target_dir" ]] || fail "Cloudflare Pages staging directory does not exist: $target_dir"
[[ "$max_file_bytes" =~ ^[0-9]+$ && "$max_file_bytes" -gt 0 ]] ||
    fail "COMINOTTI_FLATPAK_PAGES_MAX_FILE_BYTES must be a positive integer"
[[ "$max_files" =~ ^[0-9]+$ && "$max_files" -gt 0 ]] ||
    fail "COMINOTTI_FLATPAK_PAGES_MAX_FILES must be a positive integer"
[[ "$sample_count" =~ ^[0-9]+$ && "$sample_count" -gt 0 ]] ||
    fail "COMINOTTI_FLATPAK_PAGES_LARGEST_COUNT must be a positive integer"

file_count="$(find "$target_dir" -type f | wc -l)"
largest_line="$(find "$target_dir" -type f -printf '%s %p\n' | sort -nr | sed -n '1p' || true)"
largest_bytes="${largest_line%% *}"

if [[ -z "$largest_line" ]]; then
    fail "Cloudflare Pages staging directory has no files: $target_dir"
fi

echo "Cloudflare Pages Flatpak preflight:"
echo "  staging directory: $target_dir"
echo "  files: $file_count / $max_files"
echo "  largest asset: $largest_bytes bytes / $max_file_bytes bytes"
echo "  largest assets:"
find "$target_dir" -type f -printf '    %s %p\n' | sort -nr | sed -n "1,${sample_count}p"

if (( file_count > max_files )); then
    fail "Cloudflare Pages file count limit exceeded ($file_count > $max_files). Use Cloudflare R2 behind flatpak.cominotti.dev before considering GitHub Pages or Netlify."
fi

oversized="$(find "$target_dir" -type f -size +"${max_file_bytes}"c -printf '%s %p\n' | sort -nr || true)"
if [[ -n "$oversized" ]]; then
    echo "Oversized assets:" >&2
    printf '%s\n' "$oversized" >&2
    fail "Cloudflare Pages static asset limit exceeded. Use Cloudflare R2 behind flatpak.cominotti.dev before considering GitHub Pages or Netlify."
fi

echo "Cloudflare Pages limits are satisfied for $target_dir"
