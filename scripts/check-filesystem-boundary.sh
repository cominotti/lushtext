#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

patterns='std::fs::|use std::fs|std::os::unix::fs|std::os::unix::io|libc::|rustix::|\.canonicalize\(|\.exists\('

scan_roots=(
  AGENTS.md
  crates/lushtext-build-support/src
  crates/lushtext/build.rs
  crates/lushtext-core/AGENTS.md
  crates/lushtext-core/build.rs
  crates/lushtext/AGENTS.md
  crates/lushtext-core/src
  crates/lushtext-core/tests
  crates/lushtext-core/benches
  crates/lushtext/src
  crates/lushtext/tests
  crates/lushtext/benches
  .agents/rules
  .agents/skills
)

allow_re='(^crates/lushtext-build-support/src/lib\.rs:|^crates/lushtext-core/src/services/filesystem/(sys|fixture)\.rs:|^crates/lushtext-core/src/services/durable_write\.rs:|^crates/lushtext-core/src/services/filesystem/mod\.rs:.*std::fs|^\.agents/skills/.*/scripts/.*\.py:)'

hits="$(
  rg -n "$patterns" "${scan_roots[@]}" 2>/dev/null || true
)"

violations="$(
  printf '%s\n' "$hits" | rg -v "$allow_re" || true
)"

if [[ -n "$violations" ]]; then
  printf 'Direct filesystem access remains outside the approved boundary:\n\n' >&2
  printf '%s\n' "$violations" >&2
  exit 1
fi

printf 'Filesystem boundary audit passed.\n'
