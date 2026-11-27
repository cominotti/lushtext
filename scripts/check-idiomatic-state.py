#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Enforce idiomatic Rust state machines and block C/C++-style anti-patterns.

Checks production Rust sources under `crates/lushtext-model/src`,
`crates/lushtext-services/src`, and `crates/lushtext-ui/src` (excluding
`#[cfg(test)]` items and dedicated test/Kani/fuzzing files) for:
1. Oversized widget `imp.rs` files (`> 500` production lines).
2. Callback-farm `imp.rs` files (`> 2` `Box<dyn Fn...>` fields).
3. Flat `*State` / `*Session` / `*Progress` structs carrying `>= 4` flag/option
   fields (`bool`, `Cell<bool>`, `Option<_>`, `RefCell<Option<_>>`).
4. Manual state-bag clear/reset functions (`fn reset_*_state` / `fn clear_*_state`).
5. Raw `-1` / negative-count sentinels (`== -1`, `!= -1`, or `total < 0` in
   `policy.rs`) outside annotated FFI wrappers (`// ffi-sentinel:`).

Usage:
  scripts/check-idiomatic-state.py              # check against active ceilings
  scripts/check-idiomatic-state.py --strict     # require 0 violations everywhere
  scripts/check-idiomatic-state.py --list       # list every violation by category
  scripts/check-idiomatic-state.py --self-test  # run fixture regression tests
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

IMP_MAX_PRODUCTION_LINES = 500
IMP_MAX_BOX_CALLBACKS = 2
STATE_BAG_MIN_FIELDS = 4

ACTIVE_CEILINGS: dict[str, int] = {
    "oversized_imp": 0,
    "imp_callback_farm": 0,
    "flat_state_bag": 0,
    "manual_reset_fn": 0,
    "sentinel_outside_ffi": 0,
}

CFG_TEST_RE = re.compile(r"^\s*#\[cfg\(\s*test\s*\)\]")
BOX_FN_RE = re.compile(r"\bBox\s*<\s*dyn\s+Fn(?:Mut|Once)?\b")
STATE_STRUCT_RE = re.compile(
    r"\bstruct\s+([A-Za-z0-9_]*(?:State|Session|Progress)|State)\b"
)
FLAT_FIELD_RE = re.compile(
    r"^(?:pub(?:\([^)]*\))?\s+)?[a-zA-Z0-9_]+\s*:\s*"
    r"(?:(?:std|core)::cell::)?(?:Cell\s*<\s*bool\s*>|bool\b|RefCell\s*<\s*Option\s*<|Option\s*<)"
)
RESET_FN_RE = re.compile(r"\bfn\s+(?:reset_[a-z0-9_]*state|clear_[a-z0-9_]*state)\b")
SENTINEL_EQ_RE = re.compile(r"(?:==|!=)\s*-1\b|\b-1\s*(?:==|!=)")
POLICY_NEGATIVE_TOTAL_RE = re.compile(r"\btotal\s*<\s*0\b")

EXCLUDED_FILE_NAMES = {
    "tests.rs",
    "kani_proofs.rs",
    "fuzzing.rs",
}

STATE_DTO_SUFFIXES = ("Evidence", "Snapshot", "Facts")


def is_excluded_file(rel: str) -> bool:
    """Return True for dedicated test, Kani-proof, or fuzz-harness modules."""
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
    """Return `text` with `#[cfg(test)]` items removed (line-preserving)."""
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


def production_line_count(text: str) -> int:
    """Count lines before the first `#[cfg(test)]` (or non-empty trailing production text)."""
    lines = text.splitlines()
    limit = len(lines)
    for idx, line in enumerate(lines):
        if CFG_TEST_RE.match(line):
            limit = idx
            break
    while limit > 0 and lines[limit - 1] == "":
        limit -= 1
    return limit


def strip_comments_and_strings(text: str) -> str:
    """Blank comments and string literals while keeping line numbers unchanged."""
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
                while i < n and text[i] != "\n":
                    out.append(" ")
                    i += 1
                continue
            if nxt == "*":
                block_depth = 1
                out.extend("  ")
                i += 2
                continue
        if ch == '"':
            out.append(" ")
            i += 1
            while i < n:
                if text[i] == "\\":
                    if text[i + 1 : i + 2] == "\n":
                        out.extend([" ", "\n"])
                    else:
                        out.extend("  ")
                    i += 2
                elif text[i] == '"':
                    out.append(" ")
                    i += 1
                    break
                elif text[i] == "\n":
                    out.append("\n")
                    i += 1
                else:
                    out.append(" ")
                    i += 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def find_violations(root: Path) -> dict[str, list[str]]:
    """Return categorized findings across model, services, and ui production code."""
    hits: dict[str, list[str]] = {
        "oversized_imp": [],
        "imp_callback_farm": [],
        "flat_state_bag": [],
        "manual_reset_fn": [],
        "sentinel_outside_ffi": [],
    }

    ui_dir = root / "crates/lushtext-ui/src/ui"
    if ui_dir.is_dir():
        for path in sorted(ui_dir.rglob("imp.rs")):
            rel = path.relative_to(root).as_posix()
            raw = path.read_text(encoding="utf-8", errors="replace")
            prod_lines = production_line_count(raw)
            if prod_lines > IMP_MAX_PRODUCTION_LINES:
                hits["oversized_imp"].append(
                    f"{rel}: {prod_lines} production lines exceeds {IMP_MAX_PRODUCTION_LINES}"
                )
            prod = strip_comments_and_strings(strip_cfg_test_items(raw))
            cb_count = len(BOX_FN_RE.findall(prod))
            if cb_count > IMP_MAX_BOX_CALLBACKS:
                hits["imp_callback_farm"].append(
                    f"{rel}: {cb_count} Box<dyn Fn...> slots exceeds {IMP_MAX_BOX_CALLBACKS}"
                )

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
            if is_excluded_file(rel):
                continue
            raw = path.read_text(encoding="utf-8", errors="replace")
            raw_no_tests = strip_cfg_test_items(raw)
            prod = strip_comments_and_strings(raw_no_tests)
            raw_lines = raw_no_tests.splitlines()
            lines = prod.splitlines()

            # Rule 3: flat *State/*Session/*Progress bags with >= 4 bool/Option fields
            idx = 0
            while idx < len(lines):
                match = STATE_STRUCT_RE.search(lines[idx])
                if match:
                    name = match.group(1)
                    if not name.endswith(STATE_DTO_SUFFIXES):
                        j = idx
                        depth = 0
                        seen_brace = False
                        block: list[str] = []
                        while j < len(lines):
                            ln = lines[j]
                            block.append(ln)
                            depth += ln.count("{") - ln.count("}")
                            if "{" in ln:
                                seen_brace = True
                            if seen_brace and depth <= 0:
                                break
                            j += 1
                        flag_fields = sum(
                            1 for bl in block[1:] if FLAT_FIELD_RE.match(bl.strip())
                        )
                        if flag_fields >= STATE_BAG_MIN_FIELDS:
                            hits["flat_state_bag"].append(
                                f"{rel}:{idx + 1}: `struct {name}` has {flag_fields} flag/Option fields (>= {STATE_BAG_MIN_FIELDS})"
                            )
                        idx = j + 1
                        continue
                idx += 1

            # Rule 4: manual reset_*_state / clear_*_state helpers
            if rel != "crates/lushtext-ui/src/ui/accessibility.rs":
                for line_no, line in enumerate(lines, 1):
                    m = RESET_FN_RE.search(line)
                    if m:
                        hits["manual_reset_fn"].append(
                            f"{rel}:{line_no}: manual state clear/reset `{m.group(0)}`"
                        )

            # Rule 5: raw -1 sentinels outside explicitly annotated FFI wrappers
            is_policy = Path(rel).name == "policy.rs"
            for line_no, (raw_line, clean_line) in enumerate(zip(raw_lines, lines), 1):
                if "// ffi-sentinel:" in raw_line:
                    continue
                if SENTINEL_EQ_RE.search(clean_line):
                    hits["sentinel_outside_ffi"].append(
                        f"{rel}:{line_no}: raw `-1` sentinel comparison outside `// ffi-sentinel:` wrapper"
                    )
                elif is_policy and POLICY_NEGATIVE_TOTAL_RE.search(clean_line):
                    hits["sentinel_outside_ffi"].append(
                        f"{rel}:{line_no}: negative-total GTK sentinel branch in policy.rs"
                    )

    return hits


def check(root: Path, *, strict: bool = False) -> list[str]:
    hits = find_violations(root)
    findings: list[str] = []
    for category, items in hits.items():
        ceiling = 0 if strict else ACTIVE_CEILINGS[category]
        if len(items) > ceiling:
            findings.extend(items)
            findings.append(
                f"{category}: count ({len(items)}) exceeds ceiling ({ceiling})"
            )
    return findings


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="check-idiomatic-state-") as scratch:
        root = Path(scratch)
        (root / "crates/lushtext-ui/src/ui/widget_a").mkdir(parents=True)
        (root / "crates/lushtext-ui/src/ui/search_bar").mkdir(parents=True)

        # (a) oversized imp.rs (> 500 lines) + (b) callback farm (> 2 Box<dyn Fn>)
        filler = "\n".join(f"// line {i}" for i in range(505))
        (root / "crates/lushtext-ui/src/ui/widget_a/imp.rs").write_text(
            "pub struct Widget {\n"
            "    on_a: Box<dyn Fn()>,\n"
            "    on_b: Box<dyn FnMut(i32)>,\n"
            "    on_c: Box<dyn FnOnce()>,\n"
            "}\n"
            + filler
            + "\n",
            encoding="utf-8",
        )

        # (c) flat *State bag (>= 4 bool/Option fields) + (d) reset_*_state + (e) -1 sentinel
        (root / "crates/lushtext-ui/src/ui/widget_a/execution.rs").write_text(
            "pub struct LoadState {\n"
            "    active: Cell<bool>,\n"
            "    failed: bool,\n"
            "    token: Option<u32>,\n"
            "    pending: std::cell::RefCell<Option<String>>,\n"
            "}\n"
            "fn reset_load_state() {}\n"
            "fn check_height(h: i32) -> bool { h != -1 }\n",
            encoding="utf-8",
        )

        # (e2) policy.rs total < 0 sentinel
        (root / "crates/lushtext-ui/src/ui/search_bar/policy.rs").write_text(
            "pub fn label(total: i32) -> bool { total < 0 }\n",
            encoding="utf-8",
        )

        findings = check(root, strict=True)
        assert any("oversized_imp: count (1)" in f for f in findings), findings
        assert any("imp_callback_farm: count (1)" in f for f in findings), findings
        assert any("flat_state_bag: count (1)" in f for f in findings), findings
        assert any("manual_reset_fn: count (1)" in f for f in findings), findings
        assert any("sentinel_outside_ffi: count (2)" in f for f in findings), findings

        # Clean tree with phase enum, <= 500L imp.rs, <= 2 callbacks, Option in policy, FFI tag
        (root / "crates/lushtext-ui/src/ui/widget_a/imp.rs").write_text(
            "pub struct Widget {\n"
            "    on_event: Box<dyn Fn(Event)>,\n"
            "}\n",
            encoding="utf-8",
        )
        (root / "crates/lushtext-ui/src/ui/widget_a/execution.rs").write_text(
            "pub enum LoadPhase { Idle, Loading { token: u32 } }\n"
            "fn max_height(raw: i32) -> Option<i32> {\n"
            "    if raw == -1 { None } else { Some(raw) } // ffi-sentinel: GTK unset height\n"
            "}\n",
            encoding="utf-8",
        )
        (root / "crates/lushtext-ui/src/ui/search_bar/policy.rs").write_text(
            "pub fn label(total: Option<u32>) -> bool { total.is_none() }\n",
            encoding="utf-8",
        )
        assert check(root, strict=True) == [], "clean tree should pass strict check"

    print("check-idiomatic-state self-test passed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--strict", action="store_true")
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return 0

    hits = find_violations(REPO_ROOT)
    if args.list:
        total = 0
        for category, items in hits.items():
            for item in items:
                print(f"[{category}] {item}")
            total += len(items)
        summary = ", ".join(f"{k}={len(v)}" for k, v in hits.items())
        print(f"totals: {summary} (all={total})")
        return 0

    findings = check(REPO_ROOT, strict=args.strict)
    for finding in findings:
        print(f"check-idiomatic-state: {finding}", file=sys.stderr)
    if findings:
        return 1
    summary = ", ".join(f"{k}={len(v)}" for k, v in hits.items())
    mode = "strict 0" if args.strict else f"active ceilings; {summary}"
    print(f"check-idiomatic-state passed ({mode})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
