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

allow_re='(^crates/lushtext-build-support/src/lib\.rs:|^crates/lushtext-core/src/services/filesystem/(sys|fixture)\.rs:|^crates/lushtext-core/src/services/filesystem/mod\.rs:.*std::fs|^\.agents/skills/.*/scripts/.*\.py:)'

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

direct_sys_imports="$(
  rg -n '(^use .*filesystem::sys|crate::services::filesystem::sys|super::filesystem::sys)' crates/lushtext-core/src 2>/dev/null \
    | rg -v '(^crates/lushtext-core/src/services/filesystem/|^crates/lushtext-core/src/services/durable_write\.rs:)' \
    || true
)"

if [[ -n "$direct_sys_imports" ]]; then
  printf 'Private filesystem backend imports remain outside the approved implementation modules:\n\n' >&2
  printf '%s\n' "$direct_sys_imports" >&2
  exit 1
fi

direct_durable_imports="$(
  rg -n '(crate::services::durable_write|super::durable_write|services::durable_write|durable_write::)' crates/lushtext-core/src 2>/dev/null \
    | rg -v '(^crates/lushtext-core/src/services/filesystem/write\.rs:|^crates/lushtext-core/src/services/durable_write\.rs:)' \
    || true
)"

if [[ -n "$direct_durable_imports" ]]; then
  printf 'Production code imports the durable-write implementation instead of filesystem::write:\n\n' >&2
  printf '%s\n' "$direct_durable_imports" >&2
  exit 1
fi

leftovers="$(
  {
    rg -n 'FileWriteLock|FilesystemError|filesystem::sidecar|pub mod sidecar;|pub mod error;|pub use error::|pub type FileWriteLock' \
      crates/lushtext-core/src crates/lushtext-core/tests crates/lushtext-core/benches \
      crates/lushtext/src crates/lushtext/tests AGENTS.md README.md .agents/rules .agents/skills 2>/dev/null || true
    rg -n 'rename_path' crates/lushtext-core/src/services/filesystem 2>/dev/null || true
    find crates/lushtext-core/src/services/filesystem -maxdepth 1 \( -name error.rs -o -name sidecar.rs \) -print
  } | sed '/^$/d'
)"

if [[ -n "$leftovers" ]]; then
  printf 'Filesystem boundary leftovers remain after the rustix migration:\n\n' >&2
  printf '%s\n' "$leftovers" >&2
  exit 1
fi

# Controlled raw-backend crates the filesystem boundary owns. Each must stay used
# wherever it is declared: once the operations that needed it move to another
# backend (for example rustix replacing direct libc xattr calls), a lingering
# manifest declaration is a leftover the source-only patterns above cannot see.
controlled_backend_crates=(libc)

for crate in "${controlled_backend_crates[@]}"; do
  declared="$(
    rg -n "^[[:space:]]*${crate}[[:space:]]*=" Cargo.toml crates/*/Cargo.toml 2>/dev/null || true
  )"
  [[ -z "$declared" ]] && continue

  used="$(
    rg -n "\b${crate}::|use[[:space:]]+${crate}\b|extern[[:space:]]+crate[[:space:]]+${crate}\b" \
      crates --glob '*.rs' 2>/dev/null || true
  )"

  if [[ -z "$used" ]]; then
    printf 'Controlled backend crate "%s" is declared but unused:\n\n' "$crate" >&2
    printf '%s\n' "$declared" >&2
    printf '\nRemove the dependency or restore its backend usage.\n' >&2
    exit 1
  fi
done

printf 'Filesystem boundary audit passed.\n'
