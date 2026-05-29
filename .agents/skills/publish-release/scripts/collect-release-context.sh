#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Collect raw release-diff context for a semantic LushText release analysis.

set -euo pipefail

usage() {
    cat >&2 <<'EOF'
Usage: collect-release-context.sh [base-ref] [head-ref]

Defaults:
  base-ref: previous v* tag reachable from head-ref
  head-ref: HEAD
EOF
    exit 2
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
fi

repo_root="${LUSHTEXT_RELEASE_REPO_ROOT:-$(git rev-parse --show-toplevel)}"
cd "$repo_root"

base_ref="${1:-${LUSHTEXT_RELEASE_BASE_REF:-}}"
head_ref="${2:-HEAD}"

git rev-parse --verify "$head_ref^{commit}" >/dev/null ||
    { echo "error: head ref not found: $head_ref" >&2; exit 1; }

if [[ -z "$base_ref" ]]; then
    base_ref="$(git describe --tags --abbrev=0 --match 'v[0-9]*' "$head_ref^" 2>/dev/null || true)"
    if [[ -z "$base_ref" ]]; then
        base_ref="$(git describe --tags --abbrev=0 --match 'v[0-9]*' "$head_ref" 2>/dev/null || true)"
    fi
fi

[[ -n "$base_ref" ]] ||
    { echo "error: could not determine previous v* tag; pass base-ref explicitly after confirming the release baseline" >&2; exit 1; }

git rev-parse --verify "$base_ref^{commit}" >/dev/null ||
    { echo "error: base ref not found: $base_ref" >&2; exit 1; }

base_sha="$(git rev-parse --short "$base_ref^{commit}")"
head_sha="$(git rev-parse --short "$head_ref^{commit}")"
base_date="$(git show -s --format=%cs "$base_ref^{commit}")"
head_date="$(git show -s --format=%cs "$head_ref^{commit}")"
changed_paths="$(git diff --name-only "$base_ref..$head_ref")"

section_if_paths() {
    local title="$1"
    local pattern="$2"

    if printf '%s\n' "$changed_paths" | grep -E "$pattern" >/dev/null; then
        printf -- '- %s\n' "$title"
        printf '%s\n' "$changed_paths" | grep -E "$pattern" | sed 's/^/  - /'
    fi
}

printf '# LushText Release Context\n\n'
printf -- '- Base: `%s` (%s, %s)\n' "$base_ref" "$base_sha" "$base_date"
printf -- '- Head: `%s` (%s, %s)\n\n' "$head_ref" "$head_sha" "$head_date"

printf '## Diff Stat\n\n'
git diff --stat "$base_ref..$head_ref" || true
printf '\n'

printf '## Changed Paths\n\n'
if [[ -n "$changed_paths" ]]; then
    git diff --name-status "$base_ref..$head_ref" | sed 's/^/- /'
else
    printf 'No changed paths.\n'
fi
printf '\n'

printf '## Commits\n\n'
git log --date=short --pretty=format:'- %h %cs %s' "$base_ref..$head_ref" || true
printf '\n\n'

printf '## User-Facing Surface Hints\n\n'
section_if_paths "Editor UI or workflow code changed" '^crates/lushtext-core/src/ui/'
section_if_paths "File I/O, drafts, session, notes, history, or search services changed" '^crates/lushtext-core/src/services/'
section_if_paths "Domain model or persisted data types changed" '^crates/lushtext-core/src/model/'
section_if_paths "Packaging, AppStream, desktop, icons, or Flatpak changed" '^(build-aux/|data/|meson|resources/|po/)'
section_if_paths "Release, CI, or automation changed" '^(\.github/|scripts/|Makefile$)'
section_if_paths "Dependencies or version surfaces changed" '(^Cargo\.|/Cargo\.toml$|Cargo\.lock$)'
printf '\n'

printf '## Release Metadata Snapshot\n\n'
printf 'Recent tags:\n\n'
git tag --list 'v*' --sort=-v:refname | head -10 | sed 's/^/- /'
printf '\nAppStream release entries:\n\n'
grep -n '<release version=' data/dev.cominotti.lushtext.metainfo.xml.in | sed 's/^/- /' || true
printf '\n'

printf '## Suggested Semantic Review Questions\n\n'
printf -- '- What changed for a person opening, editing, saving, searching, previewing, or organizing files?\n'
printf -- '- Did any default, shortcut, setting, file format, permission, or persistence behavior change?\n'
printf -- '- Are there manual upgrade actions, warnings, deprecations, or known risks?\n'
printf -- '- Which fixes are user-recognizable symptoms rather than internal refactors?\n'
printf -- '- Did packaging, AppStream, Flatpak, desktop identity, or release automation change?\n'
