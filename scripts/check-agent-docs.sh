#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Validate the agent-facing guidance that keeps future implementation work
# aligned with the repo's rules and skill contracts.

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)

cd "$REPO_ROOT"

tmp_expected=$(mktemp)
tmp_actual=$(mktemp)
trap 'rm -f "$tmp_expected" "$tmp_actual"' EXIT

find .agents/rules -maxdepth 1 -type f -name '*.md' -printf '%f\n' | sort >"$tmp_expected"
awk '
  /^## Rules Index/ { in_rules = 1; next }
  /^## / && in_rules { in_rules = 0 }
  in_rules { print }
' AGENTS.md | sed -n 's/^- `\([^`]*\.md\)` .*/\1/p' | sort >"$tmp_actual"

if ! diff -u "$tmp_expected" "$tmp_actual"; then
  echo "AGENTS.md Rules Index is out of sync with .agents/rules/*.md" >&2
  exit 1
fi

required_files=(
  AGENTS.md
  .agents/rules/rust.md
  .agents/rules/build.md
  .agents/skills/data-safety/SKILL.md
  .agents/skills/gtk-perf-review/SKILL.md
  .agents/skills/gtk-perf-rust-optimize/SKILL.md
  .agents/skills/gtk-perf-scale/SKILL.md
  .agents/skills/gtk-responsiveness/SKILL.md
  .agents/skills/rust-comments/SKILL.md
  .agents/skills/rust-hex-arch/SKILL.md
)

for path in "${required_files[@]}"; do
  if ! grep -q 'services::filesystem' "$path"; then
    echo "$path must mention services::filesystem after filesystem-boundary changes" >&2
    exit 1
  fi
done

"$REPO_ROOT/scripts/check-filesystem-boundary.sh"

echo "Agent documentation check passed."
