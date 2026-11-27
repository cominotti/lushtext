#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Enforce LushText's zero-cost domain value types and primitive boundary discipline.

Checks:
1. Every `#[repr(transparent)]` wrapper struct under `crates/lushtext-model/src/values/`
   (and every `define_generation!` / `define_id!` macro definition) carries a
   compile-time `size_of` layout assertion (`const _: () = assert!(...size_of...);`).
2. Production structs under `crates/lushtext-{model,services,ui}/src` (excluding
   `#[cfg(test)]` items and dedicated test/fixture/proof files) do not declare
   raw integer `editor_id` or singular `generation` / `*_generation` fields.

Usage:
  scripts/check-domain-value-types.py              # check workspace against active ratchet
  scripts/check-domain-value-types.py --strict     # require 0 raw boundary fields
  scripts/check-domain-value-types.py --self-test  # run violation & pass fixture tests, then check
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Temporary ratchet matching main (`c43309a3`) before Task 3.1 migrates all
# generation and `editor_id` fields to typed `lushtext_model::values` wrappers.
# Task 3.1 removes this ceiling and enforces strictly 0 violations.
MIGRATION_CEILING: int | None = 152

TEST_ONLY_FILE_PARTS = {
    "tests.rs",
    "fixture.rs",
    "kani_proofs.rs",
}

RAW_INT_TY_RE = re.compile(
    r"^(?:(?:std::cell::|core::cell::|cell::)?Cell<\s*(?:u64|u32|usize|i64|i32)\s*>"
    r"|Option<\s*(?:u64|u32|usize|i64|i32)\s*>"
    r"|(?:std::sync::atomic::|core::sync::atomic::)?Atomic(?:U64|U32|Usize)"
    r"|u64|u32|usize|i64|i32)$"
)
STRUCT_RE = re.compile(
    r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)(?:<[^>]*>)?\s*\{([^}]*)\}"
)
FIELD_RE = re.compile(
    r"(?:^|[{\s,])(?:pub(?:\([^)]*\))?\s+)?((?:[a-z_]+_)?generation|editor_id)\s*:\s*([^,}\n]+)"
)
REPR_TRANSPARENT_STRUCT_RE = re.compile(
    r"#\[repr\(\s*transparent\s*\)\](?:\s*#\[[^\]]*\])*\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+(\w+)"
)
MACRO_DEF_RE = re.compile(
    r"macro_rules!\s+(define_\w+)\s*\{(.*?)^\}", re.S | re.M
)


def is_excluded_test_or_proof_file(rel: str) -> bool:
    """Return True for dedicated test, fixture, conformance, or Kani proof files."""
    parts = Path(rel).parts
    if parts[-1] in TEST_ONLY_FILE_PARTS or parts[-1].endswith("_tests.rs"):
        return True
    return any(p in {"tests", "kani_proofs", "conformance"} for p in parts)


def _scan_rust_delimiters(
    line: str, depth: int, seen_brace: bool, in_block_comment: bool
) -> tuple[int, bool, bool, bool]:
    """Advance brace depth and comment state across one line for `#[cfg(test)]` item stripping."""
    seen_semi_at_zero = False
    i = 0
    length = len(line)
    while i < length:
        if in_block_comment:
            end = line.find("*/", i)
            if end == -1:
                return depth, seen_brace, seen_semi_at_zero, True
            i = end + 2
            in_block_comment = False
            continue
        ch = line[i]
        if ch == "/" and i + 1 < length:
            nxt = line[i + 1]
            if nxt == "/":
                break
            if nxt == "*":
                in_block_comment = True
                i += 2
                continue
        if ch == '"':
            i += 1
            while i < length:
                if line[i] == "\\":
                    i += 2
                elif line[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            continue
        if ch == "'":
            if i + 2 < length and line[i + 1] == "\\":
                end_q = line.find("'", i + 3)
                if end_q != -1 and end_q - i <= 8:
                    i = end_q + 1
                    continue
            elif i + 2 < length and line[i + 2] == "'":
                i += 3
                continue
        if ch == "{":
            depth += 1
            seen_brace = True
        elif ch == "}":
            depth = max(0, depth - 1)
        elif ch == ";" and depth == 0 and not seen_brace:
            seen_semi_at_zero = True
        i += 1
    return depth, seen_brace, seen_semi_at_zero, in_block_comment


def strip_cfg_test_items(text: str) -> str:
    """Return `text` with every `#[cfg(test)]` item (`use`, `mod tests;`, `{ ... }`) removed."""
    lines = text.splitlines(keepends=True)
    kept: list[str] = []
    i = 0
    in_block_comment = False
    while i < len(lines):
        if re.match(r"^\s*#\[cfg\(\s*test\s*\)\]", lines[i]):
            depth = 0
            seen_brace = False
            blk = in_block_comment
            while i < len(lines):
                stripped = lines[i].strip()
                depth, seen_brace, semi0, blk = _scan_rust_delimiters(
                    lines[i], depth, seen_brace, blk
                )
                i += 1
                if not seen_brace and not semi0 and (
                    stripped.startswith("#[") or stripped.startswith("//") or stripped == ""
                ):
                    continue
                if semi0 or (seen_brace and depth == 0):
                    break
            in_block_comment = blk
            continue
        _, _, _, in_block_comment = _scan_rust_delimiters(
            lines[i], 0, False, in_block_comment
        )
        kept.append(lines[i])
        i += 1
    return "".join(kept)


def check_values_layout_assertions(root: Path) -> list[str]:
    """Ensure every #[repr(transparent)] value wrapper and value macro asserts size_of equality."""
    findings: list[str] = []
    values_dir = root / "crates/lushtext-model/src/values"
    if not values_dir.is_dir():
        return findings

    for path in sorted(values_dir.rglob("*.rs")):
        rel = path.relative_to(root).as_posix()
        if is_excluded_test_or_proof_file(rel):
            continue
        prod = strip_cfg_test_items(path.read_text(encoding="utf-8", errors="replace"))

        for m in REPR_TRANSPARENT_STRUCT_RE.finditer(prod):
            name = m.group(1)
            # Ignore macro template placeholders like `$name`
            if name.startswith("$"):
                continue
            assert_pat = re.compile(
                rf"size_of::<\s*{re.escape(name)}\s*>\(\)\s*==\s*(?:core::mem::|std::mem::)?size_of::<"
            )
            if not assert_pat.search(prod):
                findings.append(
                    f"{rel}: `#[repr(transparent)] struct {name}` is missing a compile-time `size_of::<{name}>()` layout assertion"
                )

        for m in MACRO_DEF_RE.finditer(prod):
            macro_name, body = m.group(1), m.group(2)
            if "#[repr(transparent)]" not in body:
                continue
            if "size_of::<$name>()" not in body or "assert!" not in body:
                findings.append(
                    f"{rel}: macro `{macro_name}!` defines `#[repr(transparent)]` types without asserting `size_of::<$name>()`"
                )
    return findings


def find_raw_boundary_fields(root: Path) -> list[tuple[str, int, str, str, str]]:
    """Return `(rel_path, line_no, struct_name, field_name, field_type)` for raw generation/editor_id fields."""
    hits: list[tuple[str, int, str, str, str]] = []
    for crate_dir in (
        "crates/lushtext-model/src",
        "crates/lushtext-services/src",
        "crates/lushtext-ui/src",
    ):
        base = root / crate_dir
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.rs")):
            rel = path.relative_to(root).as_posix()
            if is_excluded_test_or_proof_file(rel):
                continue
            prod = strip_cfg_test_items(
                path.read_text(encoding="utf-8", errors="replace")
            )
            for sm in STRUCT_RE.finditer(prod):
                struct_name, body = sm.group(1), sm.group(2)
                for fm in FIELD_RE.finditer(body):
                    field_name = fm.group(1)
                    field_ty = fm.group(2).strip().rstrip(",").strip()
                    if RAW_INT_TY_RE.match(field_ty):
                        abs_offset = sm.start(2) + fm.start(1)
                        line_no = prod.count("\n", 0, abs_offset) + 1
                        hits.append((rel, line_no, struct_name, field_name, field_ty))
    return hits


def check(root: Path, *, strict: bool = False) -> list[str]:
    findings = check_values_layout_assertions(root)
    raw_hits = find_raw_boundary_fields(root)
    ceiling = 0 if (strict or MIGRATION_CEILING is None) else MIGRATION_CEILING
    if len(raw_hits) > ceiling:
        for rel, line_no, struct_name, field_name, field_ty in raw_hits:
            findings.append(
                f"{rel}:{line_no}: `{struct_name}.{field_name}: {field_ty}` uses raw integer primitive instead of a `lushtext_model::values` wrapper"
            )
        findings.append(
            f"raw generation/editor_id field count ({len(raw_hits)}) exceeds ceiling ({ceiling})"
        )
    return findings


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="check-domain-value-types-") as scratch:
        root = Path(scratch)
        for rel in (
            "crates/lushtext-model/src/values",
            "crates/lushtext-services/src",
            "crates/lushtext-ui/src/ui/window",
        ):
            (root / rel).mkdir(parents=True)

        # 1. Valid `values` wrappers & boundary structs pass strict mode.
        valid_values = (
            "#[repr(transparent)]\n"
            "#[derive(Clone, Copy)]\n"
            "pub struct Bytes(u64);\n"
            "const _: () = assert!(core::mem::size_of::<Bytes>() == core::mem::size_of::<u64>());\n"
            "macro_rules! define_generation {\n"
            "    ($name:ident) => {\n"
            "        #[repr(transparent)]\n"
            "        pub struct $name(u64);\n"
            "        const _: () = assert!(core::mem::size_of::<$name>() == core::mem::size_of::<u64>());\n"
            "    };\n"
            "}\n"
        )
        (root / "crates/lushtext-model/src/values/mod.rs").write_text(
            valid_values, encoding="utf-8"
        )
        seams = root / "crates/lushtext-ui/src/ui/window/seams.rs"
        seams.write_text(
            "pub struct CleanTicket { pub generation: LoadGeneration, pub detached_generations: u64 }\n",
            encoding="utf-8",
        )
        assert check(root, strict=True) == [], "valid value wrappers and typed boundary structs must pass"

        # 2. Missing size_of assertion on unit wrapper in values/ fails.
        missing_assert = root / "crates/lushtext-model/src/values/units.rs"
        missing_assert.write_text(
            "#[repr(transparent)]\npub struct Sp(i32);\n", encoding="utf-8"
        )
        findings = check(root, strict=True)
        assert any("Sp" in f and "size_of::<Sp>()" in f for f in findings), findings
        missing_assert.unlink()

        # 3. Raw generation and editor_id fields in seams.rs / policy.rs / *State fail.
        seams.write_text(
            "pub struct BrokenTicket { pub load_generation: u64, pub editor_id: usize }\n",
            encoding="utf-8",
        )
        state = root / "crates/lushtext-ui/src/ui/window/state.rs"
        state.write_text(
            "pub struct WindowDraftState { pub dirty_generation: Cell<u64> }\n",
            encoding="utf-8",
        )
        findings = check(root, strict=True)
        assert any("seams.rs" in f and "load_generation" in f for f in findings), findings
        assert any("seams.rs" in f and "editor_id" in f for f in findings), findings
        assert any("state.rs" in f and "dirty_generation" in f for f in findings), findings
        state.unlink()

        # 4. Raw fields inside #[cfg(test)] do not fail.
        seams.write_text(
            "#[cfg(test)]\nmod tests {\n    struct TestTicket { generation: u64 }\n}\n",
            encoding="utf-8",
        )
        assert check(root, strict=True) == [], "raw generation inside #[cfg(test)] must be excluded"

    print("check-domain-value-types self-test passed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return 0

    if args.list:
        hits = find_raw_boundary_fields(REPO_ROOT)
        for rel, line_no, struct_name, field_name, field_ty in hits:
            print(f"{rel}:{line_no}\t{struct_name}.{field_name}: {field_ty}")
        print(f"total={len(hits)}")
        return 0

    findings = check(REPO_ROOT, strict=args.strict)
    for finding in findings:
        print(f"check-domain-value-types: {finding}", file=sys.stderr)
    if findings:
        return 1
    mode = "strict 0" if (args.strict or MIGRATION_CEILING is None) else f"ratchet <= {MIGRATION_CEILING}"
    print(f"check-domain-value-types passed ({mode})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
