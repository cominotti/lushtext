#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Enforce Two-Zone Arc safety and cold UI generic-Fn ratchets.

Scans production Rust sources under `crates/lushtext-model/src`,
`crates/lushtext-services/src`, and `crates/lushtext-ui/src` (excluding
`#[cfg(test)]` items and dedicated test, Kani, fuzzing, conformance, and
fixture modules) for:
1. Naked document-sized `Arc<String>` / `Arc<Vec<u8>>` payloads outside
   `DisposalOwned<T>`.
2. Double-indirected `Arc<PathBuf>` types (`SharedPath(Arc<Path>)` replaces them).
3. `Arc` usage inside `ui/**/policy.rs`, `lushtext-model/src/values/**` Kani
   proofs, or Kani-checked core modules (except documented non-Kani
   `DocumentProfile::fold_plan` in `model/document/paragraph_boundary.rs` and
   `#[cfg(not(kani))]` items).
4. Cold generic `fn <name><...Fn(?:Mut|Once)...>` functions in
   `crates/lushtext-ui/src/ui/**` above the ratcheted ceiling (`<= 25`),
   excluding thin one-line delegates to `Box<dyn Fn...>` / non-generic inner
   helpers.

Usage:
  scripts/check-arc-safety.py              # check against active ceilings
  scripts/check-arc-safety.py --strict     # enforce final programme ceilings
  scripts/check-arc-safety.py --list       # list every finding by category
  scripts/check-arc-safety.py --self-test  # run fixture regression tests
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

STRICT_CEILINGS: dict[str, int] = {
    "naked_arc_document_payload": 0,
    "double_indirected_arc_pathbuf": 0,
    "arc_in_policy_or_kani": 0,
    "cold_generic_fn_ui": 25,
}

# Final strict programme ceilings enforced on every tree.
ACTIVE_CEILINGS: dict[str, int] = dict(STRICT_CEILINGS)

CFG_TEST_RE = re.compile(r"^\s*#\[cfg\(\s*test\s*\)\]")
CFG_NOT_KANI_RE = re.compile(r"^\s*#\[cfg\(\s*not\(\s*kani\s*\)\s*\)\]")
KANI_MODULE_RE = re.compile(r"\bmod\s+kani_proofs\b|#\[cfg\(\s*kani\s*\)\]")

NAKED_ARC_PAYLOAD_RE = re.compile(r"\bArc\s*<\s*(?:String|Vec\s*<\s*u8\s*>)\s*>")
ARC_PATHBUF_RE = re.compile(r"\bArc\s*<\s*PathBuf\s*>")
ARC_USE_OR_TYPE_RE = re.compile(r"\bArc\b")
SHARED_IMPORT_RE = re.compile(r"\b(?:SharedPath|SharedStr|SharedPaths|values::shared)\b")
GENERIC_FN_RE = re.compile(r"\bfn\s+([a-zA-Z0-9_]+)\s*<([^>]*)>")
FN_TRAIT_BOUND_RE = re.compile(r"\bFn(?:Mut|Once)?\b")
THIN_DELEGATE_RE = re.compile(
    r"\bBox::new\b|[a-z0-9_]_(?:inner|boxed)\(|\bregister_[a-z0-9]"
)

EXCLUDED_FILE_NAMES = {
    "tests.rs",
    "kani_proofs.rs",
    "fuzzing.rs",
    "fixture.rs",
}

THIN_FACADE_MAX_BODY_LINES = 8


def is_excluded_file(rel: str) -> bool:
    """Return True for dedicated test, Kani-proof, fuzzing, conformance, or fixture files."""
    parts = Path(rel).parts
    if parts[-1] in EXCLUDED_FILE_NAMES or parts[-1].endswith("_tests.rs"):
        return True
    return any(part in {"tests", "kani_proofs", "conformance"} for part in parts)


def _scan_string(text: str, i: int) -> tuple[str, int]:
    """Consume a double-quoted Rust string literal starting at `i`."""
    out = [" "]
    i += 1
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "\\":
            out.append(" \n" if text[i + 1 : i + 2] == "\n" else "  ")
            i += 2
        elif ch == '"':
            out.append(" ")
            return "".join(out), i + 1
        elif ch == "\n":
            out.append("\n")
            i += 1
        else:
            out.append(" ")
            i += 1
    return "".join(out), n


def strip_comments_and_strings(text: str) -> str:
    """Blank comments and string literals while keeping exact line count."""
    out: list[str] = []
    i = 0
    n = len(text)
    block_depth = 0
    while i < n:
        ch = text[i]
        if block_depth > 0:
            if ch == "/" and text[i + 1 : i + 2] == "*":
                block_depth += 1
                out.append("  ")
                i += 2
            elif ch == "*" and text[i + 1 : i + 2] == "/":
                block_depth -= 1
                out.append("  ")
                i += 2
            else:
                out.append("\n" if ch == "\n" else " ")
                i += 1
            continue
        if ch == "/" and text[i + 1 : i + 2] == "/":
            while i < n and text[i] != "\n":
                out.append(" ")
                i += 1
            continue
        if ch == "/" and text[i + 1 : i + 2] == "*":
            block_depth = 1
            out.append("  ")
            i += 2
            continue
        if ch == '"':
            chunk, i = _scan_string(text, i)
            out.append(chunk)
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def _line_brace_delta(line: str) -> tuple[int, bool, bool]:
    """Compute net `{` - `}` delta outside quotes and line comments."""
    cleaned = strip_comments_and_strings(line)
    opens = cleaned.count("{")
    closes = cleaned.count("}")
    return opens - closes, opens > 0, ";" in cleaned


def strip_matching_attr_items(text: str, attr_re: re.Pattern[str]) -> str:
    """Remove top-level or nested Rust items annotated with `attr_re` (line-preserving)."""
    lines = text.splitlines(keepends=True)
    kept: list[str] = []
    i = 0
    while i < len(lines):
        if not attr_re.match(lines[i]):
            kept.append(lines[i])
            i += 1
            continue
        depth = 0
        seen_brace = False
        while i < len(lines):
            stripped = lines[i].strip()
            delta, has_open, has_semi = _line_brace_delta(lines[i])
            if has_open:
                seen_brace = True
            depth = max(0, depth + delta)
            kept.append("\n" if lines[i].endswith("\n") else "")
            i += 1
            if (
                not seen_brace
                and not has_semi
                and (stripped.startswith("#[") or stripped.startswith("//") or stripped == "")
            ):
                continue
            if (not seen_brace and has_semi) or (seen_brace and depth == 0):
                break
    return "".join(kept)


def _collect_fn_body(lines: list[str], start_idx: int) -> list[str]:
    """Collect lines from a `fn` signature until its closing brace."""
    body: list[str] = []
    depth = 0
    seen_brace = False
    for line in lines[start_idx:]:
        body.append(line)
        opens = line.count("{")
        closes = line.count("}")
        if opens > 0:
            seen_brace = True
        depth += opens - closes
        if seen_brace and depth <= 0:
            break
    return body


def _is_thin_delegate_facade(lines: list[str], start_idx: int) -> bool:
    """Return True if a generic `fn` is a tiny wrapper calling a boxed inner helper."""
    body = _collect_fn_body(lines, start_idx)
    if len(body) > THIN_FACADE_MAX_BODY_LINES:
        return False
    joined = "\n".join(body)
    return bool(THIN_DELEGATE_RE.search(joined))


def _scan_file_arc_rules(
    rel: str,
    raw_lines: list[str],
    clean_lines: list[str],
    is_policy_or_kani: bool,
    hits: dict[str, list[str]],
) -> None:
    """Inspect one production file for Arc safety and policy-purity violations."""
    allow_fold_plan = rel == "crates/lushtext-model/src/document/paragraph_boundary.rs"
    is_shared_module = rel == "crates/lushtext-model/src/values/shared.rs"

    for line_no, (raw_line, clean_line) in enumerate(zip(raw_lines, clean_lines), 1):
        for _ in NAKED_ARC_PAYLOAD_RE.finditer(clean_line):
            hits["naked_arc_document_payload"].append(
                f"{rel}:{line_no}: naked Arc<String>/Arc<Vec<u8>>; wrap document payloads in DisposalOwned<T>"
            )
        for _ in ARC_PATHBUF_RE.finditer(clean_line):
            hits["double_indirected_arc_pathbuf"].append(
                f"{rel}:{line_no}: double-indirected Arc<PathBuf>; use lushtext_model::values::SharedPath"
            )
        if is_policy_or_kani and not is_shared_module:
            if allow_fold_plan and ("fold_plan" in raw_line or "use std::sync::Arc" in raw_line):
                continue
            if ARC_USE_OR_TYPE_RE.search(clean_line) or SHARED_IMPORT_RE.search(clean_line):
                hits["arc_in_policy_or_kani"].append(
                    f"{rel}:{line_no}: Arc/Shared metadata disallowed inside ui/**/policy.rs and Kani-checked cores"
                )


def _scan_ui_generic_fns(
    rel: str,
    clean_lines: list[str],
    hits: dict[str, list[str]],
) -> None:
    """Count cold generic `<...Fn*>` UI functions excluding thin boxed delegates."""
    if not rel.startswith("crates/lushtext-ui/src/ui/"):
        return
    for idx, line in enumerate(clean_lines):
        m = GENERIC_FN_RE.search(line)
        if not m or not FN_TRAIT_BOUND_RE.search(m.group(2)):
            continue
        if _is_thin_delegate_facade(clean_lines, idx):
            continue
        hits["cold_generic_fn_ui"].append(
            f"{rel}:{idx + 1}: generic Fn signature `fn {m.group(1)}<{m.group(2)}>`"
        )


def _check_kani_proofs_no_shared(root: Path, hits: dict[str, list[str]]) -> None:
    """Verify no `kani_proofs.rs` imports `SharedPath`/`SharedStr`/`SharedPaths`."""
    for crate_dir in ("crates/lushtext-model/src", "crates/lushtext-services/src", "crates/lushtext-ui/src"):
        base = root / crate_dir
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("kani_proofs*.rs")):
            rel = path.relative_to(root).as_posix()
            clean = strip_comments_and_strings(path.read_text(encoding="utf-8", errors="replace"))
            for line_no, line in enumerate(clean.splitlines(), 1):
                if SHARED_IMPORT_RE.search(line) or ARC_USE_OR_TYPE_RE.search(line):
                    hits["arc_in_policy_or_kani"].append(
                        f"{rel}:{line_no}: Kani harness must stay isolated from Arc and SharedPath/SharedStr"
                    )


def find_violations(root: Path) -> dict[str, list[str]]:
    """Scan production Rust code and return categorized Arc-safety findings."""
    hits: dict[str, list[str]] = {category: [] for category in STRICT_CEILINGS}

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
            is_kani_module = bool(KANI_MODULE_RE.search(raw))
            is_policy = rel.startswith("crates/lushtext-ui/src/ui/") and Path(rel).name == "policy.rs"

            no_tests = strip_matching_attr_items(raw, CFG_TEST_RE)
            prod_for_arc = strip_matching_attr_items(no_tests, CFG_NOT_KANI_RE) if is_kani_module else no_tests
            clean_arc = strip_comments_and_strings(prod_for_arc)
            _scan_file_arc_rules(
                rel,
                prod_for_arc.splitlines(),
                clean_arc.splitlines(),
                is_policy or is_kani_module,
                hits,
            )

            clean_ui = strip_comments_and_strings(no_tests)
            _scan_ui_generic_fns(rel, clean_ui.splitlines(), hits)

    _check_kani_proofs_no_shared(root, hits)
    return hits


def check(root: Path, *, strict: bool = False) -> list[str]:
    """Compare measured counts against strict or active ceilings."""
    hits = find_violations(root)
    ceilings = STRICT_CEILINGS if strict else ACTIVE_CEILINGS
    findings: list[str] = []
    for category, items in hits.items():
        ceiling = ceilings[category]
        if len(items) > ceiling:
            findings.extend(items)
            findings.append(
                f"{category}: count ({len(items)}) exceeds ceiling ({ceiling})"
            )
    return findings


def self_test() -> None:
    """Exercise every rule and safe exemption against a scratch directory."""
    with tempfile.TemporaryDirectory(prefix="check-arc-safety-") as scratch:
        root = Path(scratch)
        (root / "crates/lushtext-model/src/document").mkdir(parents=True)
        (root / "crates/lushtext-services/src/palette/index").mkdir(parents=True)
        (root / "crates/lushtext-ui/src/ui/notes").mkdir(parents=True)
        (root / "crates/lushtext-ui/src/ui/sample").mkdir(parents=True)

        (root / "crates/lushtext-ui/src/ui/notes/install_execution.rs").write_text(
            "use std::sync::Arc;\npub struct Body { raw: Arc<String> }\n",
            encoding="utf-8",
        )
        (root / "crates/lushtext-services/src/palette/index/arena.rs").write_text(
            "use std::sync::Arc;\npub struct Arena { folder: Arc<PathBuf> }\n",
            encoding="utf-8",
        )
        (root / "crates/lushtext-ui/src/ui/sample/policy.rs").write_text(
            "use std::sync::Arc;\npub struct Policy { items: Arc<[PathBuf]> }\n",
            encoding="utf-8",
        )
        (root / "crates/lushtext-model/src/document/rule.rs").write_text(
            "use std::sync::Arc;\nmod kani_proofs {}\npub fn f() -> Arc<u32> { Arc::new(1) }\n",
            encoding="utf-8",
        )
        # Write 26 generic UI functions to exceed ceiling 25 cleanly.
        gen_fns = "\n".join(
            f"pub fn g{i}<F: FnOnce() + 'static>(cb: F) {{\n    let x = {i};\n    let _ = x;\n    cb();\n}}"
            for i in range(26)
        )
        (root / "crates/lushtext-ui/src/ui/sample/mod.rs").write_text(gen_fns + "\n", encoding="utf-8")

        findings = check(root, strict=True)
        assert any("naked_arc_document_payload: count (1)" in f for f in findings), findings
        assert any("double_indirected_arc_pathbuf: count (1)" in f for f in findings), findings
        assert any("arc_in_policy_or_kani: count (4)" in f for f in findings), findings
        assert any("cold_generic_fn_ui: count (26)" in f for f in findings), findings

        # Replace with safe patterns: DisposalOwned<String>, SharedPath, non-Arc policy, thin delegates.
        (root / "crates/lushtext-ui/src/ui/notes/install_execution.rs").write_text(
            "use std::sync::Arc;\npub struct Body { raw: Arc<DisposalOwned<String>> }\n",
            encoding="utf-8",
        )
        (root / "crates/lushtext-services/src/palette/index/arena.rs").write_text(
            "pub struct Arena { folder: SharedPath }\n",
            encoding="utf-8",
        )
        (root / "crates/lushtext-ui/src/ui/sample/policy.rs").write_text(
            "pub struct Policy { count: usize }\n",
            encoding="utf-8",
        )
        (root / "crates/lushtext-model/src/document/rule.rs").write_text(
            "mod kani_proofs {}\n#[cfg(not(kani))]\nuse std::sync::Arc;\npub fn f() -> u32 { 1 }\n",
            encoding="utf-8",
        )
        thin_fns = "\n".join(
            [
                f"pub fn g{i}<F: FnOnce() + 'static>(cb: F) {{\n    do_inner(Box::new(cb));\n}}"
                for i in range(26)
            ]
            + [
                "pub fn h_inner<F: Fn() + 'static>(cb: F) {\n    run_inner(Rc::new(cb));\n}",
                "pub fn h_boxed<F: Fn() + 'static>(cb: F) {\n    run_boxed(Box::new(cb));\n}",
                "pub fn h_reg<F: Fn() + 'static>(cb: F) {\n    register_slot(Rc::new(cb));\n}",
            ]
        )
        (root / "crates/lushtext-ui/src/ui/sample/mod.rs").write_text(thin_fns + "\n", encoding="utf-8")

        strict_findings = check(root, strict=True)
        assert strict_findings == [], f"clean tree failed strict check: {strict_findings}"

    print("check-arc-safety self-test passed")


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
        print(f"check-arc-safety: {finding}", file=sys.stderr)
    if findings:
        return 1
    summary = ", ".join(f"{k}={len(v)}" for k, v in hits.items())
    mode = "strict" if args.strict else f"active ceilings; {summary}"
    print(f"check-arc-safety passed ({mode})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
