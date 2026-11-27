#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Enforce typed error contracts on production model, service, and UI boundaries.

Checks that production Rust sources under `crates/lushtext-model/src`,
`crates/lushtext-services/src`, and `crates/lushtext-ui/src` (excluding
`#[cfg(test)]` items and dedicated test/Kani-proof files) do not declare or use:
1. `anyhow` (`use anyhow` / `anyhow::`)
2. `Result<..., String>` (single-line or multi-line, including nested generics)

Usage:
  scripts/check-error-contracts.py              # check workspace against active ceiling
  scripts/check-error-contracts.py --strict     # require 0 violations
  scripts/check-error-contracts.py --list       # list every violation and total
  scripts/check-error-contracts.py --self-test  # run fixture tests
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# Temporary ratchet during Tasks 2.x; reset to None (strict 0) in Task 3.1.
MIGRATION_CEILING: int | None = 153

ANYHOW_RE = re.compile(r"\buse\s+anyhow\b|\banyhow::")
RESULT_START_RE = re.compile(r"\bResult\s*<")
CFG_TEST_RE = re.compile(r"^\s*#\[cfg\(\s*test\s*\)\]")

EXCLUDED_FILE_NAMES = {
    "tests.rs",
    "kani_proofs.rs",
}


def is_excluded_test_or_proof_file(rel: str) -> bool:
    """Return True for dedicated unit-test or Kani-proof modules."""
    parts = Path(rel).parts
    if parts[-1] in EXCLUDED_FILE_NAMES or parts[-1].endswith("_tests.rs"):
        return True
    return any(part in {"tests", "kani_proofs"} for part in parts)


def _scan_rust_delimiters(
    line: str, depth: int, seen_brace: bool, in_block_comment: bool
) -> tuple[int, bool, bool, bool]:
    """Track brace depth across one line so whole `#[cfg(test)]` items can be dropped."""
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
    """Return `text` with `#[cfg(test)]` items blanked by line-preserving removal."""
    lines = text.splitlines(keepends=True)
    kept: list[str] = []
    i = 0
    in_block_comment = False
    while i < len(lines):
        if CFG_TEST_RE.match(lines[i]):
            depth = 0
            seen_brace = False
            blk = in_block_comment
            while i < len(lines):
                stripped = lines[i].strip()
                depth, seen_brace, semi0, blk = _scan_rust_delimiters(
                    lines[i], depth, seen_brace, blk
                )
                kept.append("\n" if lines[i].endswith("\n") else "")
                i += 1
                if (
                    not seen_brace
                    and not semi0
                    and (stripped.startswith("#[") or stripped.startswith("//") or stripped == "")
                ):
                    continue
                if semi0 or (seen_brace and depth == 0):
                    break
            in_block_comment = blk
            continue
        _, _, _, in_block_comment = _scan_rust_delimiters(lines[i], 0, False, in_block_comment)
        kept.append(lines[i])
        i += 1
    return "".join(kept)


def strip_comments_and_strings(text: str) -> str:
    """Blank comments and string literals with spaces while preserving line breaks."""
    out: list[str] = []
    i = 0
    n = len(text)
    block_depth = 0
    while i < n:
        ch = text[i]
        if block_depth > 0:
            if ch == "/" and i + 1 < n and text[i + 1] == "*":
                block_depth += 1
                out.extend("  ")
                i += 2
            elif ch == "*" and i + 1 < n and text[i + 1] == "/":
                block_depth -= 1
                out.extend("  ")
                i += 2
            elif ch == "\n":
                out.append("\n")
                i += 1
            else:
                out.append(" ")
                i += 1
            continue
        if ch == "/" and i + 1 < n:
            nxt = text[i + 1]
            if nxt == "/":
                end = text.find("\n", i)
                if end == -1:
                    out.append(" " * (n - i))
                    break
                out.append(" " * (end - i))
                i = end
                continue
            if nxt == "*":
                block_depth = 1
                out.extend("  ")
                i += 2
                continue
        if ch == "r" and i + 1 < n and text[i + 1] in ('"', "#"):
            hashes = 0
            j = i + 1
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                out.append(" " * (j + 1 - i))
                i = j + 1
                closing = '"' + ("#" * hashes)
                while i < n:
                    if text.startswith(closing, i):
                        out.append(" " * len(closing))
                        i += len(closing)
                        break
                    out.append("\n" if text[i] == "\n" else " ")
                    i += 1
                continue
        if ch == '"':
            out.append(" ")
            i += 1
            while i < n:
                if text[i] == "\\":
                    if i + 1 < n and text[i + 1] == "\n":
                        out.append(" \n")
                    else:
                        out.extend("  ")
                    i += 2
                elif text[i] == '"':
                    out.append(" ")
                    i += 1
                    break
                else:
                    out.append("\n" if text[i] == "\n" else " ")
                    i += 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def find_result_string_spans(code: str) -> list[int]:
    """Return byte offsets of `Result<..., String>` occurrences in comment-stripped Rust code."""
    offsets: list[int] = []
    for match in RESULT_START_RE.finditer(code):
        i = match.end()
        angle_depth = 1
        paren_depth = 0
        bracket_depth = 0
        args: list[str] = []
        current_start = i
        n = len(code)
        while i < n and angle_depth > 0:
            ch = code[i]
            if ch in ";{}":
                break
            if ch == "<":
                angle_depth += 1
            elif ch == ">":
                angle_depth -= 1
                if angle_depth == 0:
                    args.append(code[current_start:i])
                    break
            elif ch == "(":
                paren_depth += 1
            elif ch == ")":
                paren_depth = max(0, paren_depth - 1)
            elif ch == "[":
                bracket_depth += 1
            elif ch == "]":
                bracket_depth = max(0, bracket_depth - 1)
            elif ch == "," and angle_depth == 1 and paren_depth == 0 and bracket_depth == 0:
                args.append(code[current_start:i])
                current_start = i + 1
            i += 1
        if angle_depth == 0 and len(args) >= 2:
            err_ty = "".join(args[1].split()).rstrip(",")
            if err_ty in {"String", "std::string::String", "alloc::string::String"}:
                offsets.append(match.start())
    return offsets


def find_violations(root: Path) -> list[tuple[str, int, str]]:
    """Return `(rel_path, line_no, reason)` for every error-contract violation."""
    hits: list[tuple[str, int, str]] = []
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
            raw = path.read_text(encoding="utf-8", errors="replace")
            prod = strip_comments_and_strings(strip_cfg_test_items(raw))
            file_hits: list[tuple[int, str]] = []
            for m in ANYHOW_RE.finditer(prod):
                line_no = prod.count("\n", 0, m.start()) + 1
                file_hits.append(
                    (line_no, "uses `anyhow` instead of a typed domain `thiserror` error enum")
                )
            for offset in find_result_string_spans(prod):
                line_no = prod.count("\n", 0, offset) + 1
                file_hits.append(
                    (
                        line_no,
                        "uses `Result<_, String>` instead of a typed `thiserror` error enum",
                    )
                )
            for line_no, reason in sorted(file_hits):
                hits.append((rel, line_no, reason))
    return hits


def check(root: Path, *, strict: bool = False) -> list[str]:
    hits = find_violations(root)
    ceiling = 0 if (strict or MIGRATION_CEILING is None) else MIGRATION_CEILING
    if len(hits) <= ceiling:
        return []
    findings = [f"{rel}:{line_no}: {reason}" for rel, line_no, reason in hits]
    findings.append(
        f"error-contract violation count ({len(hits)}) exceeds ceiling ({ceiling})"
    )
    return findings


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="check-error-contracts-") as scratch:
        root = Path(scratch)
        for rel in (
            "crates/lushtext-services/src",
            "crates/lushtext-ui/src",
        ):
            (root / rel).mkdir(parents=True)

        svc = root / "crates/lushtext-services/src/sample.rs"
        svc.write_text(
            "use anyhow::Context;\n"
            "pub fn foo() -> anyhow::Result<()> { Ok(()) }\n"
            "pub(crate) fn bar() -> Result<(), String> { Ok(()) }\n"
            "pub fn multi() -> Result<\n    Vec<(u8, String)>,\n    String,\n> { Ok(Vec::new()) }\n",
            encoding="utf-8",
        )
        findings = check(root, strict=True)
        assert any("sample.rs:1:" in f and "anyhow" in f for f in findings), findings
        assert any("sample.rs:2:" in f and "anyhow" in f for f in findings), findings
        assert any("sample.rs:3:" in f and "Result<_, String>" in f for f in findings), findings
        assert any("sample.rs:4:" in f and "Result<_, String>" in f for f in findings), findings

        svc.write_text(
            "#[derive(Debug, thiserror::Error)]\n"
            "pub enum SampleServiceError { #[error(\"boom\")] Failed }\n"
            "// Not anyhow::Result or Result<(), String> in a comment\n"
            "pub fn foo() -> Result<(), SampleServiceError> {\n"
            "    let _msg = \"anyhow::Result or Result<(), String>\";\n"
            "    Ok(())\n"
            "}\n"
            "#[cfg(test)]\n"
            "mod tests {\n"
            "    fn test_helper() -> anyhow::Result<()> { Ok(()) }\n"
            "    fn str_helper() -> Result<(), String> { Ok(()) }\n"
            "}\n",
            encoding="utf-8",
        )
        assert check(root, strict=True) == [], "clean typed errors + comments/tests must pass"

    print("check-error-contracts self-test passed")


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
        hits = find_violations(REPO_ROOT)
        for rel, line_no, reason in hits:
            print(f"{rel}:{line_no}\t{reason}")
        print(f"total={len(hits)}")
        return 0

    findings = check(REPO_ROOT, strict=args.strict)
    for finding in findings:
        print(f"check-error-contracts: {finding}", file=sys.stderr)
    if findings:
        return 1
    mode = "strict 0" if (args.strict or MIGRATION_CEILING is None) else f"ratchet <= {MIGRATION_CEILING}"
    print(f"check-error-contracts passed ({mode})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
