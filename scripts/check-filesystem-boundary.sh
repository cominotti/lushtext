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

# Fixture gating. `write_draft` takes a `RegisteredDraft`, so a draft body can
# only be written for a registered id; the two fixture modules can write one
# without it (`draft_service::fixture::write_body`, and any
# `filesystem::fixture` writer aimed at `drafts/<id>.draft`). They must
# therefore exist only in test and `test-utils` builds, `test-utils` must not
# be a default feature, and no shipping build (release `[dependencies]`, Meson
# through build-aux/cargo.sh, Flatpak, Snap) may enable it, directly or through
# `manual-format-upgrade-fixtures`. The all-features Clippy gate enables
# `test-utils`, so it cannot see a production caller; CI's default-feature
# `cargo check -p lushtext --bins` is the build that would.
check_fixture_gating() {
  python3 - "$1" <<'PY'
import re
import sys
import tomllib
from pathlib import Path

root = Path(sys.argv[1])
GATE = '#[cfg(any(test, feature = "test-utils"))]'
FIXTURE_PARENTS = (
    "crates/lushtext-core/src/services/draft_service.rs",
    "crates/lushtext-core/src/services/filesystem/mod.rs",
)
ENABLERS = re.compile(r"\b(test-utils|manual-format-upgrade-fixtures)\b")
SHIPPING_FILES = (
    "build-aux/cargo.sh",
    "build-aux/dev.cominotti.lushtext.Flatpak.json",
    "snap/snapcraft.yaml",
    "meson.build",
    "meson_options.txt",
    "meson.options",
)
problems = []

for relative in FIXTURE_PARENTS:
    path = root / relative
    if not path.is_file():
        problems.append(f"{relative}: missing (fixture parent module)")
        continue
    lines = path.read_text(encoding="utf-8").splitlines()
    declarations = [i for i, line in enumerate(lines) if re.match(r"\s*pub mod fixture;", line)]
    if not declarations:
        problems.append(f"{relative}: no `pub mod fixture;` declaration found")
    for index in declarations:
        previous = lines[index - 1].strip() if index > 0 else ""
        if previous != GATE:
            problems.append(f"{relative}:{index + 1}: `pub mod fixture;` is not gated by {GATE}")

for manifest in sorted([root / "Cargo.toml", *root.glob("crates/**/Cargo.toml")]):
    if not manifest.is_file():
        continue
    relative = manifest.relative_to(root)
    data = tomllib.loads(manifest.read_text(encoding="utf-8"))
    default = data.get("features", {}).get("default", [])
    if any(ENABLERS.search(feature) for feature in default):
        problems.append(f"{relative}: `default` features enable test-utils: {default}")
    if relative.as_posix() == "crates/lushtext/Cargo.toml":
        for name, spec in data.get("dependencies", {}).items():
            features = spec.get("features", []) if isinstance(spec, dict) else []
            if any(ENABLERS.search(feature) for feature in features):
                problems.append(f"{relative}: [dependencies] {name} enables {features} in the shipped binary")

for relative in SHIPPING_FILES:
    path = root / relative
    if not path.is_file():
        continue
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if ENABLERS.search(line):
            problems.append(f"{relative}:{number}: a shipping build enables test-utils: {line.strip()}")

for problem in problems:
    print(f"fixture gating: {problem}", file=sys.stderr)
sys.exit(1 if problems else 0)
PY
}

fixture_gating_self_test() {
  local scratch
  scratch="$(mktemp -d)"
  make_tree() {
    local tree="$1"
    mkdir -p "$tree/crates/lushtext-core/src/services/filesystem" "$tree/crates/lushtext" "$tree/build-aux" "$tree/snap"
    printf '#[cfg(any(test, feature = "test-utils"))]\npub mod fixture;\n' \
      > "$tree/crates/lushtext-core/src/services/draft_service.rs"
    printf 'pub mod read;\n#[cfg(any(test, feature = "test-utils"))]\npub mod fixture;\n' \
      > "$tree/crates/lushtext-core/src/services/filesystem/mod.rs"
    printf '[features]\ntest-utils = []\n' > "$tree/crates/lushtext-core/Cargo.toml"
    printf '[features]\nmanual-format-upgrade-fixtures = ["lushtext-core/test-utils"]\n[dependencies]\nlushtext-core = { workspace = true }\n[dev-dependencies]\nlushtext-core = { workspace = true, features = ["test-utils"] }\n' \
      > "$tree/crates/lushtext/Cargo.toml"
    printf 'cargo build -p lushtext\n' > "$tree/build-aux/cargo.sh"
    printf 'parts: {}\n' > "$tree/snap/snapcraft.yaml"
  }
  expect() {
    local want="$1" name="$2" tree="$3"
    if check_fixture_gating "$tree" 2>/dev/null; then got=pass; else got=fail; fi
    if [[ "$got" != "$want" ]]; then
      printf 'fixture gating self-test %s: expected %s, got %s\n' "$name" "$want" "$got" >&2
      exit 1
    fi
  }
  make_tree "$scratch/good"
  expect pass good "$scratch/good"

  make_tree "$scratch/ungated-draft"
  printf 'pub mod fixture;\n' > "$scratch/ungated-draft/crates/lushtext-core/src/services/draft_service.rs"
  expect fail ungated-draft-fixture "$scratch/ungated-draft"

  make_tree "$scratch/ungated-fs"
  printf '#[cfg(test)]\npub mod fixture;\n' > "$scratch/ungated-fs/crates/lushtext-core/src/services/filesystem/mod.rs"
  expect fail wrongly-gated-filesystem-fixture "$scratch/ungated-fs"

  make_tree "$scratch/default"
  printf '[features]\ndefault = ["test-utils"]\ntest-utils = []\n' > "$scratch/default/crates/lushtext-core/Cargo.toml"
  expect fail default-feature "$scratch/default"

  make_tree "$scratch/shipped-dep"
  printf '[dependencies]\nlushtext-core = { workspace = true, features = ["test-utils"] }\n' > "$scratch/shipped-dep/crates/lushtext/Cargo.toml"
  expect fail shipped-dependency "$scratch/shipped-dep"

  make_tree "$scratch/meson"
  printf 'cargo build -p lushtext --features manual-format-upgrade-fixtures\n' > "$scratch/meson/build-aux/cargo.sh"
  expect fail meson-cargo-sh "$scratch/meson"

  make_tree "$scratch/snap"
  printf 'cargo build --features lushtext-core/test-utils\n' > "$scratch/snap/snap/snapcraft.yaml"
  expect fail snap "$scratch/snap"
  rm -rf "$scratch"
}

if [[ "${1:-}" == "--self-test" ]]; then
  fixture_gating_self_test
  printf 'Fixture gating self-test passed.\n'
fi

if ! check_fixture_gating "$repo_root"; then
  printf '\nGate both fixture modules with #[cfg(any(test, feature = "test-utils"))] and keep test-utils out of every shipping build.\n' >&2
  exit 1
fi

printf 'Filesystem boundary audit passed.\n'
