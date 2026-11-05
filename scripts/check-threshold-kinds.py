#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Every integer constant is classified, and every threshold is a power of two.

`.agents/rules/rust.md` ("Thresholds are powers of two, rounded the
conservative way") and `power-of-two-thresholds` (design D1, D2, D10) make the
classification a gate. `docs/thresholds.md` is the committed inventory: one row
per integer `const`/`static` in the production crates' `src/` trees, excluding
`#[cfg(test)]` modules and files, `kani_proofs.rs`, and the `gtk-lush-axioms`
probe crate. Each row gives the file, the module-qualified name, the value, the
kind, and the rounding or the reason.

This check:

1. finds every such constant, keyed by file and module-qualified name (inline
   `mod`, `impl` and `fn` scopes are part of the key, so two same-named
   constants in two inline modules of one file are two rows), including
   declarations that span several lines;
2. evaluates simple literal expressions: integer literals with separators and
   suffixes, `+ - * / % << >>`, parentheses, `as` casts, `T::MAX`/`T::MIN`,
   `T::from(..)`, and references to other constants (same scope first, then
   the same file, then a crate-unique name);
3. fails on an unlisted constant, a stale row, or a value mismatch;
4. fails on an in-scope constant that is not a power of two unless its kind is
   `derived`, `external fact`, `measurement record`, or it is a recorded
   exception;
5. checks that an in-scope constant's rustdoc carries a `Kind:` line naming the
   same kind (and "recorded exception" for an exception);
6. checks a `presentation cap` (design D12: a cap on what the user sees or
   gets, never rounded down) against the former value its row records as
   `up from <value>`: the constant must be the power of two at or above it, or,
   for `presentation cap, exception`, that value exactly. The retired kind
   `text cap` is refused (its rows are presentation caps).

Out-of-scope rows name their D1 class as `out: <class>`; they need no `Kind:`
line and may hold any value.

`--self-test` proves the check red on each failure class, on a fixture tree.
`--write` regenerates `docs/thresholds.md`, keeping the kind and reason of
every row that still exists (for maintenance after a deliberate change;
review the diff).
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
INVENTORY = "docs/thresholds.md"
INT_TYPES = "u8|u16|u32|u64|u128|usize|i8|i16|i32|i64|i128|isize"
TYPE_MAX = {
    "u8": 2**8 - 1,
    "u16": 2**16 - 1,
    "u32": 2**32 - 1,
    "u64": 2**64 - 1,
    "u128": 2**128 - 1,
    "usize": 2**64 - 1,
    "i8": 2**7 - 1,
    "i16": 2**15 - 1,
    "i32": 2**31 - 1,
    "i64": 2**63 - 1,
    "i128": 2**127 - 1,
    "isize": 2**63 - 1,
}

IN_SCOPE_KINDS = (
    "cap",
    "presentation cap",
    "coverage window",
    "policy tier",
    "retention tier",
    "retry budget",
    "external fact",
    "derived",
    "measurement record",
)
# Retired kinds (design D12): accessible-text and announcement lengths are
# characters announced, so they are presentation caps.
RETIRED_KINDS = {"text cap": "presentation cap"}
# Temporary class while the presentation-cap amendment converts a row (task
# 9.2); removed when every row is converted (task 9.9).
PENDING = ("pending-presentation",)
# Kinds allowed off a power of two without being an exception (design D2).
EXACT_KINDS = ("external fact", "derived", "measurement record")
OUT_CLASSES = (
    "geometry",
    "percent",
    "priority",
    "code",
    "time",
    "factor",
    "rate",
    "structural",
    "presentation",
    "identifier",
    "tool",
)


# --------------------------------------------------------------------------
# Lexing: blank comments and literals, keeping offsets and line numbers.


def blank(text: str) -> str:
    """Return `text` with comments and string/char literal contents replaced by
    spaces (newlines kept), so braces and keywords can be scanned safely."""
    out = list(text)
    i, n = 0, len(text)

    def wipe(a: int, b: int) -> None:
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = n if j < 0 else j
            wipe(i, j)
            i = j
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            wipe(i, j)
            i = j
        elif c == "r" and re.match(r'r#*"', text[i:]) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            m = re.match(r'r(#*)"', text[i:])
            hashes = m.group(1)
            end = text.find('"' + hashes, i + len(m.group(0)))
            end = n if end < 0 else end + 1 + len(hashes)
            wipe(i + 1, end)
            i = end
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            wipe(i + 1, j)
            i = j + 1
        elif c == "'":
            m = re.match(r"'(\\(x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'])'", text[i:], re.S)
            if m:
                wipe(i + 1, i + len(m.group(0)) - 1)
                i += len(m.group(0))
            else:
                i += 1
        else:
            i += 1
    return "".join(out)


@dataclass
class Const:
    path: str
    name: str  # module-qualified within the file, e.g. `attach_threshold::JSON`
    leaf: str
    ty: str
    expr: str
    line: int
    doc: str
    scope: tuple[str, ...]
    value: int | None = None


TEST_MOD = re.compile(
    r"#\[cfg\(\s*test\s*\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*(\{|;)"
)
CONST_DECL = re.compile(
    rf"(?<![\w])(?:pub(?:\s*\([^)]*\))?\s+)?(?:const|static)\s+(?:mut\s+)?([A-Z][A-Z0-9_]*)\s*:\s*({INT_TYPES})\s*="
)
SCOPE_OPEN = re.compile(
    r"(?:\bmod\s+(\w+)|\bimpl\b(?:\s*<[^{;]*?>)?\s+(?:[\w:<>, &']+\s+for\s+)?([\w:]+)[^{;]*|\bfn\s+(\w+)[^{;]*|\btrait\s+(\w+)[^{;]*)$"
)


def doc_before(raw_lines: list[str], line_index: int) -> str:
    """The `///` doc block above the item at `line_index` (0-based), skipping
    attribute lines between the doc and the item."""
    docs: list[str] = []
    k = line_index - 1
    while k >= 0:
        s = raw_lines[k].strip()
        if s.startswith("///"):
            docs.append(s[3:].strip())
        elif s.startswith("#[") or (s.startswith("//") and not s.startswith("///")) or (docs == [] and s.endswith(")]")):
            pass  # attributes and plain comments between the doc and the item
        else:
            break
        k -= 1
    return "\n".join(reversed(docs))


def scan_file(path: Path, rel: str) -> tuple[list[Const], set[str]]:
    """Constants of one file, and the module files it declares `#[cfg(test)]`."""
    raw = path.read_text(encoding="utf-8")
    text = blank(raw)
    raw_lines = raw.split("\n")
    excluded_spans: list[tuple[int, int]] = []
    test_files: set[str] = set()
    for m in TEST_MOD.finditer(text):
        if m.group(2) == ";":
            test_files.add(m.group(1))
            continue
        start = m.end() - 1
        depth, j = 0, start
        while j < len(text):
            if text[j] == "{":
                depth += 1
            elif text[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        excluded_spans.append((start, j))

    # Brace scopes with labels.
    labels: list[tuple[int, str | None]] = []  # (open offset, label)
    scope_at: dict[int, tuple[str, ...]] = {}
    stack: list[str | None] = []
    last_stmt = 0
    consts: list[Const] = []
    decls = {m.start(): m for m in CONST_DECL.finditer(text)}
    i = 0
    while i < len(text):
        c = text[i]
        if c == "{":
            header = text[last_stmt:i].strip().replace("\n", " ")
            header = re.sub(r"#\[[^\]]*\]", " ", header).strip()
            m = SCOPE_OPEN.search(header)
            label = None
            if m:
                if m.group(1):
                    label = m.group(1)
                elif m.group(2):
                    label = m.group(2).split("::")[-1]
                elif m.group(3):
                    label = m.group(3)
                elif m.group(4):
                    label = m.group(4)
            stack.append(label)
            last_stmt = i + 1
        elif c == "}":
            if stack:
                stack.pop()
            last_stmt = i + 1
        elif c == ";":
            last_stmt = i + 1
        if i in decls:
            m = decls[i]
            if not any(a <= i <= b for a, b in excluded_spans):
                line_no = text.count("\n", 0, i)
                # Skip an item-level `#[cfg(test)]` const.
                attr_window = "\n".join(raw_lines[max(0, line_no - 3) : line_no + 1])
                pre = text[max(0, i - 200) : i]
                if not re.search(r"#\[cfg\(\s*test\s*\)\]\s*$", pre):
                    end = text.find(";", m.end())
                    expr = re.sub(r"\s+", " ", text[m.end() : end]).strip()
                    scope = tuple(s for s in stack if s)
                    name = "::".join(scope + (m.group(1),))
                    # A constant declared once per `cfg` (a production value and
                    # a Kani bound) is one row per variant.
                    cfg = re.search(r"#\[cfg\(([^\]]*)\)\]\s*(?:#\[[^\]]*\]\s*)*$", raw[max(0, i - 200) : i])
                    if cfg:
                        name += f" #[cfg({re.sub(r'\s+', '', cfg.group(1))})]"
                    consts.append(
                        Const(
                            path=rel,
                            name=name,
                            leaf=m.group(1),
                            ty=m.group(2),
                            expr=expr,
                            line=line_no + 1,
                            doc=doc_before(raw_lines, line_no),
                            scope=scope,
                        )
                    )
                    del attr_window
        i += 1
    return consts, test_files


def production_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for src in sorted(root.glob("crates/**/src")):
        rel = src.relative_to(root).as_posix()
        if rel.startswith("crates/gtk-lush/axioms") or "/tests/" in rel + "/":
            continue
        files.extend(sorted(p for p in src.rglob("*.rs")))
    return files


def module_file_candidates(path: Path, name: str) -> list[Path]:
    """Where a `mod name;` declared in `path` lives."""
    base = path.parent if path.name in ("mod.rs", "lib.rs", "main.rs") else path.with_suffix("")
    return [base / f"{name}.rs", base / name / "mod.rs"]


def scan(root: Path) -> list[Const]:
    files = production_files(root)
    excluded: set[Path] = set()
    per_file: dict[Path, list[Const]] = {}
    for f in files:
        consts, test_mods = scan_file(f, f.relative_to(root).as_posix())
        per_file[f] = consts
        for name in test_mods:
            for cand in module_file_candidates(f, name):
                excluded.add(cand.resolve())
    # A `#[cfg(test)] mod x;` excludes x.rs and everything under x/.
    def is_excluded(p: Path) -> bool:
        rp = p.resolve()
        if p.name == "kani_proofs.rs":
            return True
        for e in excluded:
            if rp == e:
                return True
            base = e.parent if e.name == "mod.rs" else e.with_suffix("")
            if base in rp.parents:
                return True
        return False

    out: list[Const] = []
    for f in files:
        if is_excluded(f):
            continue
        out.extend(per_file[f])
    evaluate(out)
    return out


# --------------------------------------------------------------------------
# Evaluation.


def evaluate(consts: list[Const]) -> None:
    by_leaf: dict[str, list[Const]] = {}
    for c in consts:
        by_leaf.setdefault(c.leaf, []).append(c)

    def resolve(ref: str, ctx: Const) -> int | None:
        parts = ref.split("::")
        leaf = parts[-1]
        cands = by_leaf.get(leaf, [])
        if len(parts) > 1:
            qual = [c for c in cands if c.name.endswith("::".join(parts[-2:])) or c.name == leaf]
            qual = [c for c in qual if c.name.split("::")[:-1][-1:] == parts[-2:-1]] or qual
            cands = qual or cands
        same_scope = [c for c in cands if c.path == ctx.path and c.scope == ctx.scope]
        same_file = [c for c in cands if c.path == ctx.path]
        for group in (same_scope, same_file, cands):
            vals = {c.value for c in group if c.value is not None}
            if len(group) and len(vals) == 1 and all(c.value is not None for c in group):
                return vals.pop()
            if len(group) == 1:
                return group[0].value
        return None

    for _ in range(8):
        changed = False
        for c in consts:
            if c.value is not None:
                continue
            v = eval_expr(c.expr, lambda r, c=c: resolve(r, c))
            if v is not None:
                c.value = v
                changed = True
        if not changed:
            break


def eval_expr(expr: str, resolve) -> int | None:
    x = expr
    x = re.sub(r"\b(\w+)::MAX\b", lambda m: str(TYPE_MAX[m.group(1)]) if m.group(1) in TYPE_MAX else m.group(0), x)
    x = re.sub(r"\b(\w+)::MIN\b", lambda m: ("0" if m.group(1).startswith("u") else str(-TYPE_MAX[m.group(1)] - 1)) if m.group(1) in TYPE_MAX else m.group(0), x)
    x = re.sub(r"\b(?:%s)::from\(" % INT_TYPES, "(", x)
    x = re.sub(r"\bas\s+(?:%s)\b" % INT_TYPES, "", x)
    x = re.sub(r"\b0x([0-9a-fA-F_]+)(?:%s)?\b" % INT_TYPES, lambda m: str(int(m.group(1).replace("_", ""), 16)), x)
    x = re.sub(r"\b(\d[\d_]*)(?:%s)?\b" % INT_TYPES, lambda m: m.group(1).replace("_", ""), x)
    failed = False

    def sub_ref(m: re.Match) -> str:
        nonlocal failed
        v = resolve(m.group(0))
        if v is None:
            failed = True
            return "0"
        return f"({v})"

    x = re.sub(r"\b(?:[a-z_][\w]*::)*[A-Z][A-Z0-9_]+\b", sub_ref, x)
    if failed:
        return None
    if not re.fullmatch(r"[\d\s\*\+\-<>\(\)/%]+", x):
        return None
    try:
        return int(eval(x.replace("/", "//"), {"__builtins__": {}}))  # noqa: S307 - digits and operators only
    except Exception:
        return None


# --------------------------------------------------------------------------
# The inventory.


@dataclass
class Row:
    path: str
    name: str
    value: str
    kind: str
    reason: str
    line: int


ROW = re.compile(r"^\|\s*`([^`]+)`\s*\|\s*`([^`]+)`\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|\s*(.*?)\s*\|\s*$")


def read_inventory(path: Path) -> list[Row]:
    rows: list[Row] = []
    for n, line in enumerate(path.read_text(encoding="utf-8").split("\n"), 1):
        m = ROW.match(line)
        if m:
            rows.append(Row(m.group(1), m.group(2), m.group(3).replace("`", ""), m.group(4), m.group(5), n))
    return rows


def fmt_value(c: Const) -> str:
    return f"{c.value:,}" if c.value is not None else f"expr: {c.expr}"


def parse_value(s: str) -> str:
    s = s.strip()
    return s.replace(",", "") if re.fullmatch(r"-?[\d,]+", s) else s


def is_pow2(v: int) -> bool:
    return v > 0 and v & (v - 1) == 0


def pow2_at_or_above(v: int) -> int:
    return 1 if v <= 1 else 1 << (v - 1).bit_length()


UP_FROM = re.compile(r"\bup from\s+(-?[\d,]+)")


def split_kind(kind: str) -> tuple[str, bool]:
    """`cap, exception` -> (`cap`, True)."""
    k = kind.strip()
    exc = False
    if k.endswith(", exception"):
        k, exc = k[: -len(", exception")].strip(), True
    return k, exc


DOC_KIND = re.compile(r"^Kind:\s*([^;.\n(,:]+?)\s*(?:[;.(,:]|$)", re.M)


def check(root: Path) -> list[str]:
    consts = scan(root)
    errors: list[str] = []
    inv_path = root / INVENTORY
    if not inv_path.exists():
        return [f"{INVENTORY}: missing"]
    rows = read_inventory(inv_path)
    by_key = {(c.path, c.name): c for c in consts}
    dup = {}
    for c in consts:
        dup.setdefault((c.path, c.name), []).append(c)
    for key, group in dup.items():
        if len(group) > 1:
            errors.append(f"{key[0]}:{group[1].line}: `{key[1]}` declared twice in one scope (cfg variants?); qualify it")
    row_keys = {}
    for r in rows:
        if (r.path, r.name) in row_keys:
            errors.append(f"{INVENTORY}:{r.line}: duplicate row for `{r.path}` `{r.name}`")
        row_keys[(r.path, r.name)] = r
    for key, c in sorted(by_key.items()):
        if key not in row_keys:
            errors.append(f"{c.path}:{c.line}: `{c.name}` = {fmt_value(c)} is not listed in {INVENTORY}")
    for key, r in row_keys.items():
        c = by_key.get(key)
        if c is None:
            errors.append(f"{INVENTORY}:{r.line}: stale row `{r.path}` `{r.name}` (no such constant)")
            continue
        want = parse_value(fmt_value(c))
        if parse_value(r.value) != want:
            errors.append(f"{c.path}:{c.line}: `{c.name}` is {fmt_value(c)}, {INVENTORY}:{r.line} says {r.value}")
        kind = r.kind.strip()
        if kind.startswith("out:"):
            cls = kind[4:].strip()
            if cls not in OUT_CLASSES:
                errors.append(f"{INVENTORY}:{r.line}: unknown out-of-scope class `{cls}`")
            continue
        if kind in PENDING:
            continue
        base, exc = split_kind(kind)
        if base in RETIRED_KINDS:
            errors.append(
                f"{INVENTORY}:{r.line}: `{base}` is retired; `{r.name}` is a {RETIRED_KINDS[base]} (design D12)"
            )
            continue
        if base not in IN_SCOPE_KINDS:
            errors.append(f"{INVENTORY}:{r.line}: unknown kind `{kind}`")
            continue
        if base == "presentation cap":
            up = UP_FROM.search(r.reason)
            if not up:
                errors.append(
                    f"{INVENTORY}:{r.line}: `{r.name}` is a presentation cap but its row records no "
                    f"`up from <value>` (its former value)"
                )
            elif c.value is not None:
                former = int(up.group(1).replace(",", ""))
                want_value = former if exc else pow2_at_or_above(former)
                if c.value != want_value:
                    if exc:
                        errors.append(
                            f"{c.path}:{c.line}: `{c.name}` = {c.value:,} is a presentation cap exception "
                            f"kept at {former:,}, its recorded value"
                        )
                    else:
                        errors.append(
                            f"{c.path}:{c.line}: `{c.name}` = {c.value:,} is a presentation cap up from "
                            f"{former:,}, so it must be {want_value:,} (the power of two at or above it; "
                            f"never rounded down)"
                        )
        if c.value is not None and not is_pow2(c.value) and base not in EXACT_KINDS and not exc:
            errors.append(
                f"{c.path}:{c.line}: `{c.name}` = {c.value:,} is a {base} but not a power of two "
                f"(round it the conservative way, or record an exception)"
            )
        if c.value is None and base not in EXACT_KINDS and not exc:
            errors.append(f"{c.path}:{c.line}: `{c.name}` could not be evaluated; a {base} must be a literal power of two")
        m = DOC_KIND.search(c.doc)
        if not m:
            errors.append(f"{c.path}:{c.line}: `{c.name}` has no `Kind:` line in its rustdoc (inventory: {kind})")
            continue
        doc_base = re.sub(r"^(?:an?|the)\s+", "", m.group(1).replace("*", "").strip())
        if doc_base != base:
            errors.append(f"{c.path}:{c.line}: `{c.name}` rustdoc says `Kind: {doc_base}`, {INVENTORY} says `{base}`")
        kind_paragraph = c.doc[m.start() :].split("\n\n")[0]
        doc_exc = "recorded exception" in kind_paragraph
        if doc_exc != exc:
            errors.append(
                f"{c.path}:{c.line}: `{c.name}` is {'' if exc else 'not '}a recorded exception in {INVENTORY} "
                f"but its rustdoc {'does not say' if exc else 'says'} so"
            )
    return errors


def write(root: Path) -> None:
    consts = scan(root)
    inv_path = root / INVENTORY
    old = {(r.path, r.name): r for r in read_inventory(inv_path)} if inv_path.exists() else {}
    text = inv_path.read_text(encoding="utf-8") if inv_path.exists() else ""
    head = text.split("<!-- inventory -->")[0] if "<!-- inventory -->" in text else "# Thresholds\n\n"
    lines = [head.rstrip("\n"), "", "<!-- inventory -->", ""]
    current_file = None
    for c in sorted(consts, key=lambda c: (c.path, c.line)):
        if c.path != current_file:
            current_file = c.path
            lines += ["", f"### `{c.path}`", "", "| File | Constant | Value | Kind | Rounding or reason |", "|---|---|---|---|---|"]
        r = old.get((c.path, c.name))
        kind = r.kind if r else "UNCLASSIFIED"
        reason = r.reason if r else ""
        shown = fmt_value(c) if c.value is not None else f"`{fmt_value(c)}`"
        lines.append(f"| `{c.path}` | `{c.name}` | {shown} | {kind} | {reason} |")
    inv_path.write_text("\n".join(lines).replace("\n\n\n", "\n\n") + "\n", encoding="utf-8")


# --------------------------------------------------------------------------
# Self-test.

FIXTURE_LIB = """\
/// Kind: cap.
pub const GOOD_CAP: usize = 8 * 1024;

/// Kind: policy tier.
pub const SPLIT_TIER: u64 =
    64 * 1024 * 1024;

pub(crate) mod attach_threshold {
    /// Kind: cap; recorded exception (`power-of-two-thresholds`).
    pub const JSON: usize = 24 * 1024;
}

pub(crate) mod typing_threshold {
    /// Kind: cap; recorded exception (`power-of-two-thresholds`).
    pub const JSON: usize = 20 * 1024;
}

/// Kind: derived.
pub const DERIVED: usize = GOOD_CAP / 3 + attach_threshold::JSON;

pub const PADDING_PX: i32 = 24;

/// Kind: presentation cap; up from 10,000.
pub const SHOWN_CAP: usize = 16_384;

/// Kind: presentation cap; recorded exception (measured).
pub const SHOWN_EXC: usize = 10_000;

#[cfg(test)]
mod tests {
    const IGNORED: usize = 7;
}
"""

FIXTURE_INVENTORY = """\
# Thresholds

<!-- inventory -->

| File | Constant | Value | Kind | Rounding or reason |
|---|---|---|---|---|
| `crates/demo/src/lib.rs` | `GOOD_CAP` | 8,192 | cap | rounded down |
| `crates/demo/src/lib.rs` | `SPLIT_TIER` | 67,108,864 | policy tier | 64 MiB |
| `crates/demo/src/lib.rs` | `attach_threshold::JSON` | 24,576 | cap, exception | measured loss |
| `crates/demo/src/lib.rs` | `typing_threshold::JSON` | 20,480 | cap, exception | measured loss |
| `crates/demo/src/lib.rs` | `DERIVED` | 27,306 | derived | GOOD_CAP / 3 + JSON |
| `crates/demo/src/lib.rs` | `PADDING_PX` | 24 | out: geometry | |
| `crates/demo/src/lib.rs` | `SHOWN_CAP` | 16,384 | presentation cap | up from 10,000 |
| `crates/demo/src/lib.rs` | `SHOWN_EXC` | 10,000 | presentation cap, exception | up from 10,000; 16,384 measured over budget |
"""


def self_test() -> int:
    failures = 0

    def run(name: str, lib: str, inv: str, expect: str | None) -> None:
        nonlocal failures
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "crates/demo/src").mkdir(parents=True)
            (root / "crates/demo/src/lib.rs").write_text(lib, encoding="utf-8")
            (root / "docs").mkdir()
            (root / INVENTORY).write_text(inv, encoding="utf-8")
            errs = check(root)
        if expect is None:
            ok = not errs
        else:
            ok = any(expect in e for e in errs)
        if not ok:
            failures += 1
            print(f"self-test FAILED: {name}: expected {expect!r}, got {errs}", file=sys.stderr)

    run("clean fixture", FIXTURE_LIB, FIXTURE_INVENTORY, None)
    run(
        "unlisted constant",
        FIXTURE_LIB + "/// Kind: cap.\npub const NEW_CAP: usize = 64;\n",
        FIXTURE_INVENTORY,
        "`NEW_CAP` = 64 is not listed",
    )
    run(
        "stale row",
        FIXTURE_LIB,
        FIXTURE_INVENTORY + "| `crates/demo/src/lib.rs` | `GONE` | 4 | cap | |\n",
        "stale row",
    )
    run(
        "value mismatch",
        FIXTURE_LIB.replace("8 * 1024;", "16 * 1024;"),
        FIXTURE_INVENTORY,
        "`GOOD_CAP` is 16,384",
    )
    run(
        "non-power-of-two cap",
        FIXTURE_LIB.replace("8 * 1024;", "10_000;"),
        FIXTURE_INVENTORY.replace("| 8,192 | cap |", "| 10,000 | cap |").replace("| 27,306 |", "| 27,909 |"),
        "is a cap but not a power of two",
    )
    run(
        "missing Kind line",
        FIXTURE_LIB.replace("/// Kind: cap.\npub const GOOD_CAP", "pub const GOOD_CAP"),
        FIXTURE_INVENTORY,
        "`GOOD_CAP` has no `Kind:` line",
    )
    run(
        "disagreeing Kind line",
        FIXTURE_LIB.replace("/// Kind: cap.\npub const GOOD_CAP", "/// Kind: coverage window.\npub const GOOD_CAP"),
        FIXTURE_INVENTORY,
        "rustdoc says `Kind: coverage window`",
    )
    run(
        "exception without its rustdoc",
        FIXTURE_LIB.replace(
            "    /// Kind: cap; recorded exception (`power-of-two-thresholds`).\n    pub const JSON: usize = 20",
            "    /// Kind: cap.\n    pub const JSON: usize = 20",
        ),
        FIXTURE_INVENTORY,
        "`typing_threshold::JSON` is a recorded exception",
    )
    # The spec-stage scan keyed by bare name and merged these two.
    run(
        "same-named constants in two inline modules, one changed",
        FIXTURE_LIB.replace("pub const JSON: usize = 20 * 1024;", "pub const JSON: usize = 16 * 1024;"),
        FIXTURE_INVENTORY,
        "`typing_threshold::JSON` is 16,384",
    )
    run(
        "a declaration spanning two lines, changed",
        FIXTURE_LIB.replace("    64 * 1024 * 1024;", "    32 * 1024 * 1024;"),
        FIXTURE_INVENTORY,
        "`SPLIT_TIER` is 33,554,432",
    )
    run(
        "a cfg(test) module is not scanned",
        FIXTURE_LIB.replace("const IGNORED: usize = 7;", "const IGNORED: usize = 9;"),
        FIXTURE_INVENTORY,
        None,
    )
    # The presentation-cap amendment (design D12): never rounded down, rounded
    # up to exactly the power of two at or above the former value, and an
    # exception keeps that value exactly.
    run(
        "presentation cap rounded down",
        FIXTURE_LIB.replace("SHOWN_CAP: usize = 16_384;", "SHOWN_CAP: usize = 8_192;"),
        FIXTURE_INVENTORY.replace("| `SHOWN_CAP` | 16,384 |", "| `SHOWN_CAP` | 8,192 |"),
        "`SHOWN_CAP` = 8,192 is a presentation cap up from 10,000, so it must be 16,384",
    )
    run(
        "presentation cap rounded up past the next power of two",
        FIXTURE_LIB.replace("SHOWN_CAP: usize = 16_384;", "SHOWN_CAP: usize = 32_768;"),
        FIXTURE_INVENTORY.replace("| `SHOWN_CAP` | 16,384 |", "| `SHOWN_CAP` | 32,768 |"),
        "`SHOWN_CAP` = 32,768 is a presentation cap up from 10,000, so it must be 16,384",
    )
    run(
        "presentation cap exception that differs from its recorded value",
        FIXTURE_LIB.replace("SHOWN_EXC: usize = 10_000;", "SHOWN_EXC: usize = 9_000;"),
        FIXTURE_INVENTORY.replace("| `SHOWN_EXC` | 10,000 |", "| `SHOWN_EXC` | 9,000 |"),
        "`SHOWN_EXC` = 9,000 is a presentation cap exception kept at 10,000",
    )
    run(
        "presentation cap without its former value",
        FIXTURE_LIB,
        FIXTURE_INVENTORY.replace("| up from 10,000 |", "| rounded up |"),
        "`SHOWN_CAP` is a presentation cap but its row records no `up from <value>`",
    )
    run(
        "a retired text cap row",
        FIXTURE_LIB.replace("/// Kind: cap.\npub const GOOD_CAP", "/// Kind: text cap.\npub const GOOD_CAP"),
        FIXTURE_INVENTORY.replace("| 8,192 | cap |", "| 8,192 | text cap |"),
        "`text cap` is retired",
    )
    if failures:
        return 1
    print("check-threshold-kinds self-test passed")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--list", action="store_true", help="print every scanned constant")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if args.list:
        for c in scan(REPO_ROOT):
            print(f"{c.path}:{c.line}\t{c.name}\t{fmt_value(c)}\t{c.expr}")
        return 0
    if args.write:
        write(REPO_ROOT)
        return 0
    errors = check(REPO_ROOT)
    for e in errors:
        print(e, file=sys.stderr)
    if errors:
        print(f"check-threshold-kinds: {len(errors)} finding(s); see {INVENTORY} and .agents/rules/rust.md", file=sys.stderr)
        return 1
    print(f"check-threshold-kinds: {len(read_inventory(REPO_ROOT / INVENTORY))} constants classified")
    return 0


if __name__ == "__main__":
    sys.exit(main())
