#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if ! command -v rg >/dev/null 2>&1; then
  echo "Filesystem boundary audit requires ripgrep (rg)." >&2
  exit 1
fi

patterns='std::fs\b|use std::fs|std::os::unix::fs|std::os::unix::io|libc::|rustix::|\.canonicalize\(|\.exists\('

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
  rg -n -U '(^use[^\n;]*filesystem::sys|filesystem::\{[^;]*\bsys\b|crate::services::filesystem::sys|super::filesystem::sys)' crates/lushtext-core/src 2>/dev/null \
    | rg -v '(^crates/lushtext-core/src/services/filesystem/|^crates/lushtext-core/src/services/durable_write\.rs:)' \
    || true
)"

if [[ -n "$direct_sys_imports" ]]; then
  printf 'Private filesystem backend imports remain outside the approved implementation modules:\n\n' >&2
  printf '%s\n' "$direct_sys_imports" >&2
  exit 1
fi

direct_durable_imports="$(
  rg -n -U '(^use[^\n;]*(crate::services|super|services)::durable_write|(^use[^\n;]*(crate::services|super|services)::\{[^;]*\bdurable_write\b)|crate::services::durable_write|super::durable_write|services::durable_write|durable_write::)' crates/lushtext-core/src 2>/dev/null \
    | rg -v '(^crates/lushtext-core/src/services/filesystem/write\.rs:|^crates/lushtext-core/src/services/durable_write\.rs:)' \
    || true
)"

if [[ -n "$direct_durable_imports" ]]; then
  printf 'Production code imports the durable-write implementation instead of filesystem::write:\n\n' >&2
  printf '%s\n' "$direct_durable_imports" >&2
  exit 1
fi

status_probe_roots=(
  crates/lushtext-core/src
  crates/lushtext-core/tests
  crates/lushtext-core/benches
  crates/lushtext/src
  crates/lushtext/tests
  crates/lushtext/benches
)

status_probe_hits="$(
  {
    rg -n -U 'file_facts\([^\n;]*\)\s*\.is_(ok|err)(_and)?\(' "${status_probe_roots[@]}" --glob '*.rs' 2>/dev/null || true
    rg -n 'fn[[:space:]]+path_exists[[:space:]]*\(' "${status_probe_roots[@]}" --glob '*.rs' 2>/dev/null || true
  } | rg -v '^crates/lushtext-core/src/services/filesystem/sys\.rs:' || true
)"

if [[ -n "$status_probe_hits" ]]; then
  printf 'Status-only filesystem probes should use services::filesystem::metadata::{path_status, exists}:\n\n' >&2
  printf '%s\n' "$status_probe_hits" >&2
  exit 1
fi

unused_status_helpers=""
for helper in path_status exists; do
  declaration="$(
    rg -n "pub fn ${helper}[[:space:]]*\\(" crates/lushtext-core/src/services/filesystem/metadata.rs 2>/dev/null || true
  )"
  [[ -z "$declaration" ]] && continue

  uses="$(
    rg -n "(fs_metadata|metadata)::${helper}[[:space:]]*\\(" crates/lushtext-core/src --glob '*.rs' 2>/dev/null \
      | rg -v '^crates/lushtext-core/src/services/filesystem/metadata\.rs:' \
      || true
  )"
  if [[ -z "$uses" ]]; then
    unused_status_helpers+="${declaration}"$'\n'
  fi
done

if [[ -n "$unused_status_helpers" ]]; then
  printf 'Filesystem status helpers are declared without call sites outside metadata.rs:\n\n' >&2
  printf '%s\n' "$unused_status_helpers" >&2
  exit 1
fi

engine_adapter_hits="$(
  rg -n -U '(^use[^\n;]*(grep_searcher|ignore)::|grep_searcher::|ignore::|WalkBuilder::|SearcherBuilder::)' \
    crates/lushtext-core/src crates/lushtext/src --glob '*.rs' 2>/dev/null \
    | rg -v '^crates/lushtext-core/src/services/content_search/search\.rs:' \
    || true
)"

if [[ -n "$engine_adapter_hits" ]]; then
  printf 'Filesystem engine adapters are only approved in content_search/search.rs:\n\n' >&2
  printf '%s\n' "$engine_adapter_hits" >&2
  exit 1
fi

leftovers="$(
  {
    rg -n 'FileWriteLock|FilesystemError|filesystem::sidecar|pub mod sidecar;|pub mod error;|pub use error::|pub type FileWriteLock' \
      crates/lushtext-core/src crates/lushtext-core/tests crates/lushtext-core/benches \
      crates/lushtext/src crates/lushtext/tests AGENTS.md README.md .agents/rules .agents/skills 2>/dev/null || true
    rg -n 'rename_path' crates/lushtext-core/src/services/filesystem 2>/dev/null || true
    rg -n 'pub fn (write_bytes|sync_directory|symlink_facts)\b' \
      crates/lushtext-core/src/services/filesystem/metadata.rs \
      crates/lushtext-core/src/services/filesystem/tree.rs \
      crates/lushtext-core/src/services/filesystem/write.rs 2>/dev/null || true
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
  for manifest in Cargo.toml crates/*/Cargo.toml; do
    [[ -f "$manifest" ]] || continue
    declared="$(
      rg -n "^[[:space:]]*${crate}[[:space:]]*=|^\[(workspace\.)?(dependencies|dev-dependencies|build-dependencies)\.${crate}\]" \
        "$manifest" 2>/dev/null || true
    )"
    [[ -z "$declared" ]] && continue

    if [[ "$manifest" == "Cargo.toml" ]]; then
      source_roots=(crates)
    else
      crate_root="${manifest%/Cargo.toml}"
      source_roots=()
      for source_root in "$crate_root/src" "$crate_root/tests" "$crate_root/benches" "$crate_root/build.rs"; do
        [[ -e "$source_root" ]] && source_roots+=("$source_root")
      done
    fi

    used=""
    if [[ ${#source_roots[@]} -gt 0 ]]; then
      used="$(
        rg -n "\b${crate}::|use[[:space:]]+${crate}\b|extern[[:space:]]+crate[[:space:]]+${crate}\b" \
          "${source_roots[@]}" --glob '*.rs' 2>/dev/null || true
      )"
    fi

    if [[ -z "$used" ]]; then
      printf 'Controlled backend crate "%s" is declared but unused in %s:\n\n' "$crate" "$manifest" >&2
      printf '%s\n' "$declared" >&2
      printf '\nRemove the dependency or restore its backend usage in the declaring crate.\n' >&2
      exit 1
    fi
  done
done

printf 'Filesystem boundary audit passed.\n'
