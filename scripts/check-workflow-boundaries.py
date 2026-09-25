#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Enforce LushText's workflow readability boundary conventions.

Ten mechanical guarantees, all derived from
`openspec/specs/workflow-readability-boundaries/spec.md`,
`openspec/specs/mutation-testing/spec.md`, and the completion rule in
`docs/workflow-readability-matrix.md`:

1. Purity: a workflow `policy.rs` under `crates/lushtext-core/src/` must not
   import or reference `gtk4`, `glib`, `gio`, `libadwaita`, or `sourceview5`.
2. Mutation reach: every such `policy.rs` must be matched by an `examine_globs`
   entry in `.cargo/mutants.toml`, so relocating pure policy beside its workflow
   cannot silently drop mutation coverage.
3. Role completeness: every matrix row marked `migrated` must declare its
   facade, coordination, policy, evidence, and mutation-parity roles in the
   matrix's `Migrated Workflow Roles` section. Every row's status must also be
   one of the documented labels, so a typo cannot silently disable this rule.
4. Evidence presence: every repository path the matrix claims as existing
   evidence must exist on disk. Planned relocation targets are exempt only on
   the line that writes `relocates to <path>`; the same path named as a role in
   the `Migrated Workflow Roles` section is always a claim about the tree.
5. Facade size budget: when the matrix's `Facade size budget` section declares
   a normative budget as `- normative facade line budget: <integer>`, no
   `migrated` row's declared facade file may exceed it. The budget is set by the
   first migration change after the exemplar, so while no line declares one this
   rule is inert rather than assuming a default.
6. Programme-record agreement: the slot ledger in
   `docs/next/workflow-readability.md` and the matrix must tell the same story
   about what is done and what is outstanding. Each ledger line reads
   `- slot <n> (complete|outstanding): <WFR-ID>[ (partial)][, ...]`, and the
   check fails when a `complete` slot names a row the matrix does not mark
   `migrated`, when an `outstanding` slot names a `migrated` row without the
   `(partial)` marker, when a named row id is absent from the matrix, or when a
   matrix row that is neither `migrated` nor `exempt` and carries a migration
   slot appears in no `outstanding` ledger line. `(partial)` on a `complete` line
   means the slot's share of an incremental row is done while the row continues
   later, so that entry is exempt from the `migrated` requirement. A caller that passes no record
   path leaves this rule inert (the fixtures that exercise other rules do so);
   the real-tree entry point always passes the canonical path, so a missing
   record is reported rather than silently skipped.
7. Role-home declaration: every `.rs` file in a `migrated` row's role home is
   the facade, the GTK subclass state file, a fixed role name, a bounded or
   stage-qualified coordination role, or a file that row's own matrix text names
   by a backticked repository path. A role home is the directory holding the
   declared facade plus any subdirectory of it the row itself names through a
   declared role path -- the nested home the convention permits. Enumeration is
   non-recursive in each, so a workflow is never made responsible for a
   neighbour's directory. A module in the one directory the convention claims to
   have classified must not be unclassified, and the declaration has to be
   machine-readable: a bare stem or a brace expansion classifies a file for a
   human reader while leaving this check blind.
8. Test-seam ratchet: the count of externally reachable `*_for_test`
   declarations under `crates/lushtext-core/src` must not exceed the ceiling the
   matrix records. Only excess fails. A count below the ceiling passes without a
   finding, because forcing the figure down on every cleanup would train a reader
   to treat it as a number to adjust rather than a ceiling to stay under.
   `cfg(feature = "test-utils")` sites are deliberately NOT ratcheted: that
   population rises as workflows gain gated evidence surfaces, so ratcheting it
   would penalise the convention being followed. An unparsed ceiling is itself a
   finding when the caller requires one -- which the real-tree entry point does --
   because a rule that checked nothing would otherwise retire the programme's
   headline ratchet while exiting 0.
9. Kani harness modules: a file named `kani_proofs.rs` is verification code,
   not decision logic (rule 3's discovery) or an undeclared role-home module
   (rule 7), but only when its parent module declares it
   `#[cfg(kani)] mod kani_proofs;`. Any `kani_proofs.rs` under `crates/` whose
   parent does not gate it, or that has no parent module file, is a finding.
10. Whole-pixel geometry policy: every module in `WHOLE_PIXEL_POLICY_MODULES`,
   and every `policy.rs` under a directory in `GEOMETRY_ROLE_HOMES` (so a new
   one is covered without an edit here), is protected. A module whose recorded
   ceiling of admitted functions is 0 carries the literal inner attribute
   `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]`, which
   nothing beneath it can lower; one that still admits fractional functions
   carries the pair as `deny`. Matched on comment- and literal-stripped code,
   in the module and its child files (`x.rs` -> `x/**/*.rs`, less a gated
   `kani_proofs.rs`), any attribute that lowers either lint or a group holding
   one (`allow`, `warn`, `cfg_attr`, an inner `expect`, or an `expect` on
   anything but a `fn`), a `#[path]` remap, or an `include!` is a finding, as
   is a count of `expect`-admitted functions above the module's ceiling. At the
   real-tree entry point a listed module missing from the tree is a finding, so
   a rename cannot drop it, and so is any disagreement between the list and the
   table in the "Whole-pixel geometry policy" section of
   `.agents/rules/workflow-convention.md`.
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
MATRIX_PATH = REPO_ROOT / "docs/workflow-readability-matrix.md"
RECORD_PATH = REPO_ROOT / "docs/next/workflow-readability.md"
MUTANTS_CONFIG_PATH = REPO_ROOT / ".cargo/mutants.toml"
CORE_SRC = Path("crates/lushtext-core/src")

# The GTK family a pure policy module must never reach for. Keep this list in
# sync with the same list in the capability spec and `.agents/rules/rust.md`.
GTK_FAMILY_CRATES = ("gtk4", "glib", "gio", "libadwaita", "sourceview5")

POLICY_MODULE_NAME = "policy.rs"

# Layer-relative shorthand the matrix uses for crate-internal modules, e.g.
# `model/plain_disposal.rs` rather than the full crate path.
CORE_LAYER_PREFIXES = ("ui/", "model/", "services/")
# Repository-rooted prefixes that identify a backticked token as a path claim
# rather than a Rust identifier, action name, or accelerator.
REPO_PATH_PREFIXES = (
    ".agents/",
    ".cargo/",
    ".github/",
    "build-aux/",
    "crates/",
    "data/",
    "docs/",
    "fuzz/",
    "openspec/",
    "resources/",
    "scripts/",
    "snap/",
)

MIGRATED_STATUS = "migrated"
# The label set documented in the matrix's `Status Labels` section. An unknown
# label must fail loudly: a silently unrecognized status would exempt its row
# from the migrated-role rule instead of enforcing it.
SUPERSEDED_STATUS = "superseded"
KNOWN_STATUS_LABELS = (
    "pending",
    MIGRATED_STATUS,
    "partially-conforming",
    "exempt",
    "deferred",
    "cross-cutting",
    # A row that was *replaced* rather than migrated, exempted, or shared.
    # `WFR-SHELL-LAYOUT` is the first: slot 7a resolved it was never one
    # workflow, and slot 7b replaced it with seven rows that each own one. None
    # of the other five labels describes that history — `exempt` and
    # `cross-cutting` both claim the row's code is deliberately unmigrated, and
    # `deferred` is transitional — so the vocabulary gained a terminal label
    # rather than the row taking a label that misdescribes it.
    SUPERSEDED_STATUS,
)
ROLES_SECTION_HEADING = "## Migrated Workflow Roles"
FACADE_BUDGET_SECTION_HEADING = "### Facade size budget"
# The machine-readable declaration documented beside it in that matrix section.
# Absent means "not set yet", which is the exemplar's recorded state.
FACADE_BUDGET_RE = re.compile(r"^-\s+normative facade line budget:\s*(\d+)\s*$")
REQUIRED_ROLES = ("facade", "coordination", "policy", "evidence", "mutation parity")
# Roles whose value may be the literal `none` because not every migrated
# workflow owns pure policy, a coordination module, or a relocation.
OPTIONAL_ROLE_VALUES = {"coordination", "policy", "mutation parity"}

STRING_LITERAL_RE = re.compile(r'"(?:[^"\\]|\\.)*"')
BACKTICKED_RE = re.compile(r"`([^`]+)`")
RELOCATION_TARGET_RE = re.compile(r"relocates? to `([^`]+)`")
ROW_ID_RE = re.compile(r"^WFR-[A-Z0-9-]+$")
# The programme record's machine-readable slot ledger, documented beside itself
# in `docs/next/workflow-readability.md`.
SLOT_LEDGER_RE = re.compile(
    r"^-\s+slot\s+(\S+)\s+\((complete|outstanding)\):\s*(\S.*)$", re.IGNORECASE
)
SLOT_ENTRY_RE = re.compile(r"(WFR-[A-Z0-9-]+)(\s*\(partial\))?", re.IGNORECASE)
# The ledger's *shape* without its vocabulary, used only to tell a malformed
# ledger line from ordinary prose that happens to mention a slot. A line matching
# this but not `SLOT_LEDGER_RE` is a dropped claim rather than a sentence.
#
# Every quantifier below is disjoint from what follows it -- `\s+` is followed by
# a literal, and `[^:]*` is followed by the single character it excludes -- so the
# pattern is deterministic and cannot backtrack super-linearly. The earlier form
# ended `\s*\(?[^)]*\)?\s*:`, three overlapping runs whose failing case was
# quadratic. This form accepts a strict *superset* of that one, so the
# malformed-line detector cannot have been narrowed: enumerating every suffix
# over `- s(l)o:t7` up to length 6, plus 200k random strings, found zero lines
# the old pattern matched and this one does not. It additionally catches shapes
# such as `- slot 7b (a) (b): x`, which the old pattern let pass as prose even
# though it is plainly a dropped claim.
SLOT_LEDGER_SHAPE_RE = re.compile(r"^-\s+slot\s+\S[^:]*:", re.IGNORECASE)
# Statuses that owe no outstanding-slot entry: the work is either done or the row
# is deliberately never migrated.
SETTLED_STATUSES = ("migrated", "exempt", SUPERSEDED_STATUS)
# Statuses that mean "this row's shape is not settled yet". The capability
# delta landed by slot 7b states they must not survive the change that closes
# the migration programme, and the mechanical half of that is below: once the
# ledger declares no slot outstanding, the programme is closed, and a row still
# carrying one of these is a finding rather than a review question.
TRANSITIONAL_STATUSES = ("pending", "deferred", "partially-conforming")
# Terminal statuses a row can hold that are *not* `migrated`. A slot may declare
# its work on such a row complete; it may not claim the row became `migrated`,
# because these labels mean it never will.
TERMINAL_NON_MIGRATING_STATUSES = ("cross-cutting", "exempt", SUPERSEDED_STATUS)
# Both this pattern and `SLOT_LEDGER_RE` end their value group with
# `\s*(\S.*)$` rather than `\s*(.+)$`: the sets `\s` and `.` overlap, so the
# looser form is an ambiguous adjacent-quantifier pair a scanner reports as
# super-linear backtracking. Callers match against an already-stripped line and
# strip each captured group, so requiring a non-space first character is
# behaviour-identical.
ROLE_LINE_RE = re.compile(r"^-\s+([A-Za-z][A-Za-z ]*?):\s*(\S.*)$")
# Stems the convention assigns no role name to but that every role home carries:
# the facade and the GTK subclass state file. `imp.rs` is a called presentation
# surface in every migrated row, so requiring each row to re-declare it would add
# 22 identical lines that carry no information.
ROLE_HOME_EXEMPT_STEMS = ("mod", "imp")
# The ceiling's own subsection, read exactly like `Facade size budget`: only that
# heading's body is scanned, so a figure cannot be smuggled in from prose
# elsewhere. Keying on the parent `## Measurement Definitions` instead looks
# equivalent and is not -- the parser resets on any `#` line, so the real tree's
# `###` subsection heading closed the section and the rule read as inert while a
# flatter fixture passed. That is the shape of fail-open this file closes
# elsewhere, so the real-tree entry point now also reports an unparsed ceiling.
MEASUREMENT_SECTION_HEADING = "### Externally reachable test-seam ceiling"
# The machine-readable ceiling declared inside it, in the same shape as the facade
# budget.
SEAM_CEILING_RE = re.compile(
    r"^-\s+externally reachable `\\?\*_for_test` declaration ceiling:\s*(\d+)\s*$"
)
# `pub fn` / `pub(crate) fn` whose name ends `_for_test`, which is the predicate
# `docs/next/workflow-readability.md` records this figure against. `pub(super)`
# declarations are outside it by design.
FOR_TEST_DECLARATION_RE = re.compile(
    r"(?<![A-Za-z0-9_])pub(?:\(crate\))?\s+fn\s+[A-Za-z0-9_]*_for_test(?![A-Za-z0-9_])"
)
EXAMINE_GLOBS_RE = re.compile(r"^examine_globs\s*=\s*\[", re.MULTILINE)


@dataclass(frozen=True)
class MatrixRow:
    """One parsed `Product Matrix` row."""

    row_id: str
    line_number: int
    cells: tuple[str, ...]
    status: str
    # The row's migration slot, or None when the table declares no `Slot` column.
    # `none` is a real value: it marks a row that is never migrated.
    slot: str | None


@dataclass(frozen=True)
class RoleDeclaration:
    """The declared roles of one migrated workflow."""

    row_id: str
    line_number: int
    roles: dict[str, str]


def display_path(path: Path) -> str:
    """Render a path relative to the repository root when possible."""
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


# --- Check 1: policy purity -------------------------------------------------


def policy_modules(root: Path) -> list[Path]:
    """List workflow policy modules in the crate the convention governs."""
    core = root / CORE_SRC
    if not core.is_dir():
        return []
    return sorted(core.rglob(POLICY_MODULE_NAME))


def code_lines(text: str) -> list[tuple[int, str]]:
    """Yield `(line_number, code)` with comments and string literals removed.

    Only enough Rust lexing to keep prose out of the purity scan: doc comments,
    line comments, block comments, and double-quoted literals cannot introduce a
    real GTK dependency, but they do mention GTK types. Exotic forms such as raw
    strings containing `"` may survive stripping; that errs toward reporting a
    finding, which is the safe direction for a policy gate.
    """
    result: list[tuple[int, str]] = []
    in_block = False
    for line_number, raw_line in enumerate(text.splitlines(), start=1):
        line = raw_line
        if in_block:
            close = line.find("*/")
            if close < 0:
                continue
            line = line[close + 2 :]
            in_block = False
        line = STRING_LITERAL_RE.sub('""', line)
        while True:
            block = line.find("/*")
            comment = line.find("//")
            if comment >= 0 and (block < 0 or comment < block):
                line = line[:comment]
                break
            if block < 0:
                break
            close = line.find("*/", block + 2)
            if close < 0:
                line = line[:block]
                in_block = True
                break
            line = line[:block] + " " + line[close + 2 :]
        stripped = line.strip()
        if stripped:
            result.append((line_number, stripped))
    return result


def gtk_reference_findings(path: Path, root: Path) -> list[str]:
    """Return purity findings for one policy module."""
    findings: list[str] = []
    relative = display_path(path) if root == REPO_ROOT else str(path.relative_to(root))
    for line_number, line in code_lines(path.read_text(encoding="utf-8")):
        if line.startswith("#!"):
            continue
        for crate in GTK_FAMILY_CRATES:
            # `use gtk4::...` and bare `gtk4::Widget` paths are both violations;
            # a suffix match such as `my_glib::x` is not.
            pattern = rf"(?<![A-Za-z0-9_]){re.escape(crate)}\s*::"
            use_pattern = rf"^use\s+{re.escape(crate)}\b"
            if re.search(pattern, line) or re.search(use_pattern, line):
                findings.append(
                    f"{relative}:{line_number} references `{crate}`; "
                    "workflow policy modules must stay free of GTK-family imports"
                )
                break
    return findings


# --- Check 2: mutation scope reach ------------------------------------------


def parse_examine_globs(config_path: Path) -> list[str]:
    """Read `examine_globs` entries without depending on a TOML parser."""
    text = config_path.read_text(encoding="utf-8")
    match = EXAMINE_GLOBS_RE.search(text)
    if match is None:
        return []
    tail = text[match.end() :]
    end = tail.find("]")
    if end < 0:
        return []
    body = tail[:end]
    return re.findall(r'"([^"]+)"', body)


def glob_to_regex(pattern: str) -> re.Pattern[str]:
    """Translate a globset-style pattern into an anchored regex.

    `**` spans path separators and is treated as matching zero or more path
    components, which is the permissive reading. That matters only for a
    hypothetical `crates/.../ui/policy.rs` sitting directly in a layer root; the
    convention places policy modules one or more directories deep, where both
    readings agree.
    """
    parts: list[str] = []
    index = 0
    while index < len(pattern):
        char = pattern[index]
        if pattern.startswith("**/", index):
            parts.append(r"(?:[^/]+/)*")
            index += 3
        elif pattern.startswith("**", index):
            parts.append(r".*")
            index += 2
        elif char == "*":
            parts.append(r"[^/]*")
            index += 1
        elif char == "?":
            parts.append(r"[^/]")
            index += 1
        else:
            parts.append(re.escape(char))
            index += 1
    return re.compile("^" + "".join(parts) + "$")


def mutation_reach_findings(paths: list[str], globs: list[str]) -> list[str]:
    """Return findings for policy modules the mutation scope cannot reach."""
    if not globs:
        return [
            f"{display_path(MUTANTS_CONFIG_PATH)}: no examine_globs entries were parsed"
        ]
    matchers = [glob_to_regex(glob) for glob in globs]
    findings: list[str] = []
    for relative in paths:
        if not any(matcher.match(relative) for matcher in matchers):
            findings.append(
                f"{relative} is not matched by any .cargo/mutants.toml examine_globs "
                "entry, so relocating pure policy here would drop mutation coverage"
            )
    return findings


# --- Check 3: inclusion-side discovery of pure `ui/` modules ----------------
#
# Check 2 asks "is every `policy.rs` reachable by a mutation glob?". It can only
# ever inspect files the naming convention already selects, so it cannot see the
# converse defect: pure decision logic sitting in `ui/` under some *other* file
# name is silently outside the mutation scope while every command exits 0. This
# check closes that half by discovering GTK-free `ui/` modules and requiring each
# to carry a declared workflow role.
#
# The classification is role-based rather than a content escape. The tree
# legitimately contains many GTK-free non-policy modules — narrative facades,
# `seams.rs` value-object modules, bounded coordination roles, `test_policy.rs`,
# `evidence.rs` — and a content-shaped escape list would go red on all of them.
# Two channels satisfy the check:
#
#   1. the file name is one the convention already assigns a role to, or
#   2. the module doc *declares* the role, which is the recorded-reason escape.
#
# The escape's limit is that the declaration must name a role or an ownership —
# "Role: ...", "cross-cutting", or an owning `WFR-*` row. A comment merely
# asserting the module is fine does not classify it.

# File names the convention itself assigns a role to.
FACADE_MODULE_NAME = "mod.rs"
FIXED_ROLE_MODULE_NAMES = (
    POLICY_MODULE_NAME,
    "evidence.rs",
    "seams.rs",
    "test_policy.rs",
)
# The bounded coordination role set. A module may qualify one of these with the
# stage order it serves (`query_execution.rs`), so both forms are accepted.
BOUNDED_COORDINATION_ROLES = (
    "admission",
    "execution",
    "retirement",
    "watch",
    "journal",
)
# Module-doc tokens that declare a role or an ownership. The lookbehind is a
# non-word character rather than whitespace because these tokens are routinely
# written inside Markdown emphasis or backticks (`**cross-cutting**`,
# `` `WFR-SHELL-LAYOUT` ``), which a whitespace-anchored pattern misses — a real
# false positive this check hit on `ui/sidebar/width_preset.rs`.
ROLE_DECLARATION_RE = re.compile(
    r"(?<![A-Za-z0-9_])(?:[Rr]ole:|cross-cutting|WFR-[A-Z0-9-]+)",
)
UI_SUBTREE = "ui"

# Kani harness modules are verification code, not workflow modules: a harness
# over a `ui/**/policy.rs` holds functions (so it would read as unclassified
# decision logic) and may sit in a role home (so it would read as an undeclared
# module). The name is mandatory for harness modules, and the mutation scope
# excludes it by the same name (`crates/**/kani_proofs.rs`). Both rules skip
# such a file only when its parent module declares it under `#[cfg(kani)]`,
# because that gate is what keeps it out of every ordinary build; an ungated or
# orphaned `kani_proofs.rs` is a finding.
KANI_HARNESS_MODULE_NAME = "kani_proofs.rs"
KANI_HARNESS_GATE_RE = re.compile(
    r"#\[cfg\(kani\)\][ \t]*\n[ \t]*(?:pub(?:\([^)\n]*\))?[ \t]+)?mod[ \t]+kani_proofs[ \t]*;"
)


def harness_parent_module(path: Path) -> Path | None:
    """Return the module file that declares `path`, if one exists.

    `policy/kani_proofs.rs` is declared by `policy.rs`; `x/kani_proofs.rs` by
    `x/mod.rs` or `x.rs`; a crate-root `src/kani_proofs.rs` by `src/lib.rs` or
    `src/main.rs`.
    """
    directory = path.parent
    if directory.name == "src":
        candidates = [directory / "lib.rs", directory / "main.rs"]
    else:
        candidates = [directory / "mod.rs", directory.parent / f"{directory.name}.rs"]
    return next((candidate for candidate in candidates if candidate.is_file()), None)


def parent_gates_kani_harness(parent: Path | None) -> bool:
    """Return whether `parent` declares `#[cfg(kani)] mod kani_proofs;`."""
    return parent is not None and KANI_HARNESS_GATE_RE.search(parent.read_text(encoding="utf-8")) is not None


def is_gated_kani_harness(path: Path) -> bool:
    """Return whether `path` is a `kani_proofs.rs` its parent gates on `cfg(kani)`."""
    return path.name == KANI_HARNESS_MODULE_NAME and parent_gates_kani_harness(harness_parent_module(path))


def kani_harness_findings(root: Path) -> list[str]:
    """Return findings for `kani_proofs.rs` files no parent gates on `cfg(kani)`."""
    findings: list[str] = []
    crates = root / "crates"
    if not crates.is_dir():
        return findings
    for path in sorted(crates.rglob(KANI_HARNESS_MODULE_NAME)):
        parent = harness_parent_module(path)
        if parent_gates_kani_harness(parent):
            continue
        relative_path = (
            display_path(path) if root == REPO_ROOT else str(path.relative_to(root))
        )
        where = (
            "has no parent module file"
            if parent is None
            else "is not declared `#[cfg(kani)] mod kani_proofs;` by its parent"
        )
        findings.append(
            f"{relative_path} is a Kani harness module but {where}, so ordinary "
            "builds would compile it as production code"
        )
    return findings


# Rule 10. The pure geometry and budget policies take and return whole pixels
# wherever the value is not inherently fractional. The compiler holds that line
# and this rule holds the compiler to every module that must carry it. See the
# "Whole-pixel geometry policy" section of `.agents/rules/workflow-convention.md`.
#
# Each entry maps a module (relative to `crates/lushtext-core/src`) to its
# ceiling of function-level admissions: the number of
# `#[expect(clippy::float_arithmetic | clippy::disallowed_methods, ...)]`
# attributes it and its child files may carry. A ceiling of 0 means the module
# needs no admission, so it must carry the literal
# `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]`, which no
# attribute anywhere beneath it can lower (rustc rejects it, E0453). A module
# that still admits fractional functions carries the same pair as `deny`, and
# this rule is then the only thing standing between it and a lint-lowering
# attribute that Clippy's own gates miss: `allow_attributes` ignores inner
# attributes and `cfg_attr`, and neither it nor `allow_attributes_without_reason`
# notices a reasoned `expect` on an `impl`, a `mod`, or a whole child file. The
# convention's "Whole-pixel geometry policy" table must list exactly these
# modules and ceilings (checked below); raising a ceiling is a reviewed edit to
# both. Only excess fails, as for the `*_for_test` ceiling.
WHOLE_PIXEL_POLICY_MODULES: dict[str, int] = {
    "ui/window/geometry/policy.rs": 0,
    "ui/editor_page/minimap/policy.rs": 8,
    "ui/markdown_preview/policy.rs": 0,
    "ui/sidebar/width_preset.rs": 0,
    "model/editor_memory.rs": 0,
    "ui/window/local_history/policy.rs": 0,
    "ui/window/focus_mode/policy.rs": 0,
}
# Role homes whose `policy.rs` files hold geometry decisions: any `policy.rs`
# beneath them, present or future, is protected too, with a ceiling of 0 (so it
# must forbid) unless the list above names it.
GEOMETRY_ROLE_HOMES = (
    "ui/window/geometry",
    "ui/editor_page/minimap",
    "ui/markdown_preview",
)
CONVENTION_PATH = REPO_ROOT / ".agents/rules/workflow-convention.md"
WHOLE_PIXEL_SECTION_HEADING = "### Whole-pixel geometry policy"
WHOLE_PIXEL_TABLE_ROW_RE = re.compile(
    r"^\|\s*`([^`]+)`\s*\|\s*`?(forbid|deny)`?\s*\|\s*(\d+)\s*\|"
)
# The two lints a protected module raises, spelled as its top attribute must.
WHOLE_PIXEL_LINTS = ("clippy::float_arithmetic", "clippy::disallowed_methods")
# Any lint name whose level change reaches either lint: the lints themselves
# (and `disallowed_method`, the renamed alias rustc still honours), the
# `restriction` group `float_arithmetic` belongs to, the `style` group
# `disallowed_methods` belongs to, `all` (which contains `style`), and
# `blanket_clippy_restriction_lints`, which a group-level expect needs beside it.
WHOLE_PIXEL_LOWERING_LINT_RE = re.compile(
    r"clippy::(?:float_arithmetic|disallowed_methods?|restriction|"
    r"blanket_clippy_restriction_lints|style|all)\b"
)
ATTRIBUTE_START_RE = re.compile(r"#\s*(?:(!)\s*)?\[")
ATTRIBUTE_NAME_RE = re.compile(r"\s*([A-Za-z_][A-Za-z0-9_:]*)")
# What may follow an admitting `expect` before the item it admits: more outer
# attributes (skipped separately), then a function's qualifiers and `fn`.
FN_ITEM_RE = re.compile(
    r"\s*(?:pub(?:\s*\([^)]*\))?\s+)?"
    r"(?:(?:const|async|unsafe|safe|default|extern(?:\s*\"\")?)\s+)*fn\b"
)
TOP_LEVEL_INNER_ATTRIBUTE_RE = re.compile(r"^#!\[\s*(forbid|deny)\s*\(([^\]]*)\)\s*\]", re.MULTILINE)
INCLUDE_MACRO_RE = re.compile(r"\binclude!\s*\(")


def strip_rust_comments_and_strings(text: str) -> str:
    """Blank out comments and literals, keeping every newline and offset.

    Unlike `code_lines`, this lexes nested block comments, raw strings, and
    character literals (so `'"'` does not open a string), because rule 10 must
    not be satisfied by an attribute written inside a comment, nor misled by one
    written inside a literal.
    """
    out = list(text)
    length = len(text)

    def blank(start: int, end: int) -> None:
        for index in range(start, min(end, length)):
            if out[index] != "\n":
                out[index] = " "

    index = 0
    while index < length:
        char = text[index]
        if text.startswith("//", index):
            end = text.find("\n", index)
            end = length if end < 0 else end
            blank(index, end)
            index = end
        elif text.startswith("/*", index):
            depth = 0
            cursor = index
            while cursor < length:
                if text.startswith("/*", cursor):
                    depth += 1
                    cursor += 2
                elif text.startswith("*/", cursor):
                    depth -= 1
                    cursor += 2
                    if depth == 0:
                        break
                else:
                    cursor += 1
            blank(index, cursor)
            index = cursor
        elif char in "rb" and re.match(r"(?:br|rb|r)(#*)\"", text[index:index + 260]) and (
            index == 0 or not (text[index - 1].isalnum() or text[index - 1] == "_")
        ):
            match = re.match(r"(?:br|rb|r)(#*)\"", text[index:])
            assert match is not None
            closing = '"' + match.group(1)
            end = text.find(closing, index + match.end())
            end = length if end < 0 else end + len(closing)
            blank(index + 1, end - 1)
            index = end
        elif char == '"':
            cursor = index + 1
            while cursor < length and text[cursor] != '"':
                cursor += 2 if text[cursor] == "\\" else 1
            blank(index + 1, cursor)
            index = cursor + 1
        elif char == "'":
            # A character literal, or a lifetime/label (left alone).
            if text.startswith("'\\", index):
                end = text.find("'", index + 2)
                end = length if end < 0 else end + 1
                blank(index + 1, end - 1)
                index = end
            elif index + 2 < length and text[index + 2] == "'":
                blank(index + 1, index + 2)
                index += 3
            else:
                index += 1
        else:
            index += 1
    return "".join(out)


@dataclass(frozen=True)
class RustAttribute:
    """One `#[...]` or `#![...]` attribute found in comment-stripped code."""

    line: int
    inner: bool
    name: str
    body: str
    end: int


def rust_attributes(code: str) -> list[RustAttribute]:
    """Return every attribute in comment- and literal-stripped `code`."""
    attributes: list[RustAttribute] = []
    for match in ATTRIBUTE_START_RE.finditer(code):
        depth = 1
        cursor = match.end()
        while cursor < len(code) and depth:
            if code[cursor] == "[":
                depth += 1
            elif code[cursor] == "]":
                depth -= 1
            cursor += 1
        body = code[match.end() : cursor - 1]
        name_match = ATTRIBUTE_NAME_RE.match(body)
        attributes.append(
            RustAttribute(
                line=code.count("\n", 0, match.start()) + 1,
                inner=match.group(1) is not None,
                name=name_match.group(1) if name_match else "",
                body=re.sub(r"\s+", "", body),
                end=cursor,
            )
        )
    return attributes


def admitted_fn_position(code: str, position: int) -> int | None:
    """Return where the `fn` after the attribute ending at `position` starts, if it is one.

    Further outer attributes are skipped, so two `expect`s on one function
    resolve to the same position and count as one admission.
    """
    while True:
        rest = code[position:]
        start = ATTRIBUTE_START_RE.match(rest.lstrip())
        if start is None or start.group(1) is not None:
            break
        position += len(rest) - len(rest.lstrip()) + start.end()
        depth = 1
        while position < len(code) and depth:
            if code[position] == "[":
                depth += 1
            elif code[position] == "]":
                depth -= 1
            position += 1
    match = FN_ITEM_RE.match(code, position)
    return match.end() if match else None


def whole_pixel_policy_modules(root: Path) -> dict[Path, int]:
    """Return every protected module with its function-level admission ceiling."""
    core = root / CORE_SRC
    modules = {core / path: ceiling for path, ceiling in WHOLE_PIXEL_POLICY_MODULES.items()}
    for home in GEOMETRY_ROLE_HOMES:
        directory = core / home
        if directory.is_dir():
            for path in directory.rglob(POLICY_MODULE_NAME):
                modules.setdefault(path, 0)
    return dict(sorted(modules.items()))


def whole_pixel_child_files(module: Path) -> list[Path]:
    """Return the module's child files (`x.rs` -> `x/**/*.rs`), less gated Kani harnesses."""
    directory = module.parent if module.name == "mod.rs" else module.with_suffix("")
    if not directory.is_dir():
        return []
    return sorted(
        path
        for path in directory.rglob("*.rs")
        if path != module and not is_gated_kani_harness(path)
    )


def whole_pixel_attribute_findings(relative_path: str, code: str) -> tuple[list[str], int]:
    """Return lint-lowering findings for one file and its count of admitted functions."""
    findings: list[str] = []
    admitted_functions: set[int] = set()
    for attribute in rust_attributes(code):
        where = f"{relative_path}:{attribute.line}"
        if attribute.name == "path":
            findings.append(
                f"{where} remaps a module with `#[path]`; a whole-pixel module's children "
                "must live in its own child directory, where this rule scans them"
            )
            continue
        if WHOLE_PIXEL_LOWERING_LINT_RE.search(attribute.body) is None:
            continue
        if attribute.name in {"deny", "forbid"}:
            continue
        if attribute.name == "cfg_attr":
            findings.append(
                f"{where} changes a whole-pixel lint level through `cfg_attr`; admit a "
                "fractional function with a plain function-level `#[expect]` instead"
            )
        elif attribute.name in {"allow", "warn"}:
            findings.append(
                f"{where} lowers a whole-pixel lint with `{attribute.name}`; admit a "
                "fractional function with a function-level `#[expect(..., reason = ...)]`"
            )
        elif attribute.name == "expect" and attribute.inner:
            findings.append(
                f"{where} expects a whole-pixel lint module-wide; put the expectation on "
                "the function that needs it"
            )
        elif attribute.name == "expect" and admitted_fn_position(code, attribute.end) is None:
            findings.append(
                f"{where} expects a whole-pixel lint on an item that is not a function "
                "(an `impl`, `mod`, `trait`, statement, or expression); put the "
                "expectation on the one function that computes the fractional value"
            )
        elif attribute.name == "expect":
            position = admitted_fn_position(code, attribute.end)
            assert position is not None
            admitted_functions.add(position)
        else:
            findings.append(
                f"{where} names a whole-pixel lint in an unrecognised `{attribute.name}` "
                "attribute"
            )
    if INCLUDE_MACRO_RE.search(code):
        findings.append(
            f"{relative_path} uses `include!`, whose text this rule cannot scan"
        )
    return findings, len(admitted_functions)


def whole_pixel_top_attribute(code: str) -> str | None:
    """Return `forbid` or `deny` when the file raises both whole-pixel lints at top level."""
    for match in TOP_LEVEL_INNER_ATTRIBUTE_RE.finditer(code):
        lints = {lint.strip() for lint in re.sub(r"\s+", "", match.group(2)).split(",")}
        if all(lint in lints for lint in WHOLE_PIXEL_LINTS):
            return match.group(1)
    return None


def whole_pixel_findings(root: Path, *, real_tree: bool) -> list[str]:
    """Return findings for whole-pixel policy modules and their child files."""
    findings: list[str] = []
    lints = ", ".join(WHOLE_PIXEL_LINTS)
    for path, ceiling in whole_pixel_policy_modules(root).items():
        relative_path = display_path(path) if root == REPO_ROOT else str(path.relative_to(root))
        if not path.is_file():
            if real_tree:
                findings.append(
                    f"{relative_path} is listed in WHOLE_PIXEL_POLICY_MODULES but does not exist; "
                    "update the list when a geometry policy moves"
                )
            continue
        code = strip_rust_comments_and_strings(path.read_text(encoding="utf-8"))
        level = whole_pixel_top_attribute(code)
        if ceiling == 0 and level != "forbid":
            findings.append(
                f"{relative_path} is a whole-pixel policy module needing no admission but "
                f"lacks `#![forbid({lints})]`"
            )
        elif ceiling > 0 and level is None:
            findings.append(
                f"{relative_path} is a whole-pixel policy module but lacks "
                f"`#![deny({lints})]`"
            )
        file_findings, admissions = whole_pixel_attribute_findings(relative_path, code)
        findings.extend(file_findings)
        for child in whole_pixel_child_files(path):
            child_relative = (
                display_path(child) if root == REPO_ROOT else str(child.relative_to(root))
            )
            child_code = strip_rust_comments_and_strings(child.read_text(encoding="utf-8"))
            child_findings, child_admissions = whole_pixel_attribute_findings(
                child_relative, child_code
            )
            findings.extend(child_findings)
            admissions += child_admissions
        if admissions > ceiling:
            findings.append(
                f"{relative_path} admits {admissions} functions with a whole-pixel `expect`, "
                f"above its recorded ceiling of {ceiling}. Make a function whole-pixel "
                "instead; or, deliberately, raise the ceiling in WHOLE_PIXEL_POLICY_MODULES "
                "and the convention's whole-pixel table together"
            )
    return findings


def parse_whole_pixel_table(text: str) -> dict[str, tuple[str, int]] | None:
    """Read the convention's whole-pixel table, or None when it is absent."""
    in_section = False
    rows: dict[str, tuple[str, int]] = {}
    for line in text.splitlines():
        if line.startswith("#"):
            if in_section and rows:
                break
            in_section = line.strip() == WHOLE_PIXEL_SECTION_HEADING
            continue
        if not in_section:
            continue
        match = WHOLE_PIXEL_TABLE_ROW_RE.match(line.strip())
        if match:
            rows[match.group(1)] = (match.group(2), int(match.group(3)))
    return rows or None


def whole_pixel_ledger_findings(convention_path: Path) -> list[str]:
    """Return findings when the convention's whole-pixel table disagrees with the script."""
    where = display_path(convention_path)
    if not convention_path.is_file():
        return [f"missing workflow convention: {where}"]
    table = parse_whole_pixel_table(convention_path.read_text(encoding="utf-8"))
    if table is None:
        return [
            f"{where}: the `{WHOLE_PIXEL_SECTION_HEADING}` section declares no module table "
            "(`| `path` | `forbid` or `deny` | ceiling |`)"
        ]
    findings: list[str] = []
    for path, ceiling in WHOLE_PIXEL_POLICY_MODULES.items():
        expected = ("forbid" if ceiling == 0 else "deny", ceiling)
        if path not in table:
            findings.append(f"{where}: the whole-pixel table omits `{path}`, which rule 10 protects")
        elif table[path] != expected:
            findings.append(
                f"{where}: the whole-pixel table records `{path}` as {table[path][0]} / "
                f"{table[path][1]}, but rule 10 enforces {expected[0]} / {expected[1]}"
            )
    for path in table:
        if path not in WHOLE_PIXEL_POLICY_MODULES:
            findings.append(
                f"{where}: the whole-pixel table lists `{path}`, which WHOLE_PIXEL_POLICY_MODULES "
                "does not protect"
            )
    return findings


def module_doc(text: str) -> str:
    """Return the file's leading `//!` module documentation block."""
    doc: list[str] = []
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if line.startswith("//!"):
            doc.append(line[3:].strip())
            continue
        if not line or line.startswith("//"):
            # Blank lines and the SPDX header precede the module doc.
            continue
        break
    return "\n".join(doc)


def has_convention_role_name(name: str) -> bool:
    """Return whether the file name is one the convention assigns a role to."""
    if name == FACADE_MODULE_NAME or name in FIXED_ROLE_MODULE_NAMES:
        return True
    stem = name[:-3] if name.endswith(".rs") else name
    return any(
        stem == role or stem.endswith(f"_{role}") for role in BOUNDED_COORDINATION_ROLES
    )


def holds_decision_logic(lines: list[tuple[int, str]]) -> bool:
    """Return whether the module declares any function.

    A pure module with no functions is a type or re-export module and carries no
    decision to mutate. Requiring a richer content test here would reintroduce
    the content escape this check deliberately rejects.
    """
    return any(re.search(r"(?<![A-Za-z0-9_])fn\s+[A-Za-z_]", line) for _, line in lines)


def gtk_free_ui_modules(root: Path) -> list[Path]:
    """List GTK-free modules under the crate's `ui/` subtree.

    Purity uses the same predicate as Check 1 — `code_lines()` stripping plus the
    GTK-family reference scan — so a doc comment mentioning `gio::ListStore` does
    not make a pure module look impure. Stating the predicate once and sharing it
    is the point: a discovery check whose purity test lives in whoever runs a
    grep is not mechanical.
    """
    ui_root = root / CORE_SRC / UI_SUBTREE
    if not ui_root.is_dir():
        return []
    pure: list[Path] = []
    for path in sorted(ui_root.rglob("*.rs")):
        if not gtk_reference_findings(path, root):
            pure.append(path)
    return pure


# Pure `ui/` modules that hold decision logic, are classified only by a **prose**
# role declaration in their module doc, and are therefore outside `ui/**/policy.rs`
# — so they generate zero mutants. Each is a deliberate, recorded disposition, not
# an oversight, and listing them here makes the silence visible in the gate rather
# than only in the module's own comment. A prose-classified module that is *not*
# listed is a finding: the escape must be explicit somewhere a reviewer reads.
PROSE_CLASSIFIED_UNMUTATED = {
    "crates/lushtext-core/src/ui/sidebar/width_preset.rs": (
        "cross-cutting value owned by WFR-SHELL-GEOMETRY (WFR-SHELL-LAYOUT until "
        "slot 7b superseded it) and consumed by Preferences and the window shell. "
        "That row is now migrated and its own policy is mutation-covered at "
        "ui/window/geometry/policy.rs; this value type stays a prose-classified "
        "unmutated module. It now holds one decision, from_fraction (nearest "
        "preset, non-finite to the default), which extend-kani-to-pure-policies "
        "fixed failing-first and which Kani proves with the clamps and "
        "round-trips (width_preset/kani_proofs.rs); it remains outside the "
        "mutation scope because moving it into the role home would put a "
        "Preferences-facing type behind a workflow directory"
    ),
    "crates/lushtext-core/src/ui/sidebar/workspace_section/watch_targets.rs": (
        "stateful data structure owned by WFR-WORKSPACE-TREE's `watch` role, "
        "recorded in that row's Migrated Workflow Roles subsection as neither a "
        "role nor a called presentation surface"
    ),
}


def unclassified_pure_module_findings(root: Path) -> list[str]:
    """Return findings for pure `ui/` modules that carry no declared role."""
    findings: list[str] = []
    for path in gtk_free_ui_modules(root):
        if has_convention_role_name(path.name) or is_gated_kani_harness(path):
            continue
        text = path.read_text(encoding="utf-8")
        if not holds_decision_logic(code_lines(text)):
            continue
        relative_path = (
            display_path(path) if root == REPO_ROOT else str(path.relative_to(root))
        )
        if ROLE_DECLARATION_RE.search(module_doc(text)):
            # Classified by prose. That is permitted, but a prose-classified
            # module holding decision logic generates no mutants, so the
            # disposition has to be recorded here too.
            #
            # Only the real tree carries that ledger: `PROSE_CLASSIFIED_UNMUTATED`
            # holds repo-relative paths, so a fixture tree has nothing to consult
            # and must not be asked to register its modules.
            if root != REPO_ROOT:
                continue
            if relative_path not in PROSE_CLASSIFIED_UNMUTATED:
                findings.append(
                    f"{relative_path} is GTK-free, holds decision logic, and is "
                    "classified only by a prose role declaration, so it generates no "
                    "mutants. Record its disposition in PROSE_CLASSIFIED_UNMUTATED "
                    "or rename it into the `policy.rs` convention"
                )
            continue
        findings.append(
            f"{relative_path} is GTK-free and holds decision logic but carries no declared "
            "workflow role, so the `ui/**/policy.rs` mutation convention cannot see it. "
            "Rename it into the convention or declare its role in the module doc"
        )
    return findings


# --- Matrix parsing ---------------------------------------------------------


def split_table_row(line: str) -> list[str]:
    """Split one Markdown table row into trimmed cells."""
    stripped = line.strip()
    if not stripped.startswith("|"):
        return []
    return [cell.strip() for cell in stripped.strip("|").split("|")]


def parse_status(cell: str) -> str:
    """Reduce a status cell to its leading label token.

    The matrix writes statuses as a bare label, as `label — see <section>`, or
    as `label (<note>)`. Both suffix forms are dropped so the label itself can be
    validated; anything else left over stays part of the token and therefore
    fails validation instead of quietly bypassing the migrated-role rule.
    """
    return cell.split("—")[0].split("(")[0].strip().strip("`*_").strip().lower()


def status_findings(rows: list[MatrixRow]) -> list[str]:
    """Return findings for rows whose status is not a documented label."""
    findings: list[str] = []
    for row in rows:
        if row.status not in KNOWN_STATUS_LABELS:
            findings.append(
                f"{display_path(MATRIX_PATH)}:{row.line_number} row {row.row_id} has status "
                f"`{row.cells[-1]}`, whose label `{row.status}` is not one of "
                f"{', '.join(KNOWN_STATUS_LABELS)}; an unrecognized label would silently "
                "exempt the row from the migrated-role rule"
            )
    return findings


def parse_matrix_rows(text: str) -> tuple[list[MatrixRow], bool]:
    """Parse `Product Matrix` rows keyed by their stable row id.

    Returns the rows and whether a `Slot` column was actually located. The
    second value exists because locating that column by name is a fail-open: the
    defence against a positional shift silently disappears when the header is
    reworded. With no located column every row's `slot` is `None`, every
    slot-dependent finding is skipped, and the gate exits 0 while checking
    nothing. The caller reports that as a finding wherever the slot is consumed;
    a matrix that carries no `Slot` column and is reconciled against no record
    is not a fail-open, so the check belongs at the consumer rather than here.
    """
    rows: list[MatrixRow] = []
    slot_index: int | None = None
    for line_number, line in enumerate(text.splitlines(), start=1):
        cells = split_table_row(line)
        if len(cells) < 2:
            continue
        if not ROW_ID_RE.match(cells[0]):
            # Track the enclosing table's header so the `Slot` column is located
            # by name rather than by a positional guess that a later column
            # insertion would silently shift.
            lowered = [cell.lower() for cell in cells]
            if "row id" in lowered:
                slot_index = lowered.index("slot") if "slot" in lowered else None
            continue
        slot = (
            cells[slot_index]
            if slot_index is not None and slot_index < len(cells)
            else None
        )
        rows.append(
            MatrixRow(
                row_id=cells[0],
                line_number=line_number,
                cells=tuple(cells),
                status=parse_status(cells[-1]),
                slot=slot,
            )
        )

    return rows, slot_index is not None


def parse_role_declarations(text: str) -> dict[str, RoleDeclaration]:
    """Parse the `Migrated Workflow Roles` section into per-row role maps."""
    declarations: dict[str, RoleDeclaration] = {}
    lines = text.splitlines()
    in_section = False
    current: str | None = None
    current_line = 0
    roles: dict[str, str] = {}

    def flush() -> None:
        nonlocal current, roles
        if current is not None:
            declarations[current] = RoleDeclaration(
                row_id=current, line_number=current_line, roles=dict(roles)
            )
        current = None
        roles = {}

    in_fence = False
    for line_number, line in enumerate(lines, start=1):
        if line.lstrip().startswith("```"):
            # Fenced blocks document the format; they are not declarations.
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        if line.startswith("## "):
            if in_section:
                flush()
                in_section = False
            in_section = line.strip() == ROLES_SECTION_HEADING
            continue
        if not in_section:
            continue
        if line.startswith("### "):
            flush()
            current = line.removeprefix("### ").strip()
            current_line = line_number
            continue
        role_match = ROLE_LINE_RE.match(line.strip())
        if role_match and current is not None:
            roles[role_match.group(1).strip().lower()] = role_match.group(2).strip()

    if in_section:
        flush()
    return declarations


def role_findings(rows: list[MatrixRow], declarations: dict[str, RoleDeclaration]) -> list[str]:
    """Return findings for migrated rows whose roles are incomplete."""
    findings: list[str] = []
    migrated = [row for row in rows if row.status == MIGRATED_STATUS]
    for row in migrated:
        declaration = declarations.get(row.row_id)
        if declaration is None:
            findings.append(
                f"{display_path(MATRIX_PATH)}:{row.line_number} row {row.row_id} is marked "
                f"`{MIGRATED_STATUS}` but has no `{ROLES_SECTION_HEADING.removeprefix('## ')}` "
                "entry naming its facade, coordination, policy, evidence, and mutation "
                "parity roles"
            )
            continue
        for role in REQUIRED_ROLES:
            value = declaration.roles.get(role)
            if not value:
                findings.append(
                    f"{display_path(MATRIX_PATH)}:{declaration.line_number} row {row.row_id} "
                    f"is marked `{MIGRATED_STATUS}` but does not name its `{role}` role"
                )
                continue
            if value.lower() == "none" and role not in OPTIONAL_ROLE_VALUES:
                findings.append(
                    f"{display_path(MATRIX_PATH)}:{declaration.line_number} row {row.row_id} "
                    f"declares `{role}: none`, which the completion rule does not permit"
                )
    for row_id, declaration in sorted(declarations.items()):
        if row_id not in {row.row_id for row in migrated}:
            findings.append(
                f"{display_path(MATRIX_PATH)}:{declaration.line_number} declares roles for "
                f"{row_id}, which is not marked `{MIGRATED_STATUS}` in the product matrix"
            )
    return findings


# --- Check 4: claimed evidence exists ---------------------------------------


def normalize_claim(token: str) -> str | None:
    """Map a backticked matrix token to a repository-relative path, or None."""
    candidate = token.strip()
    # Drop `path.rs:123` line references and glob tails such as `dir/**`.
    candidate = re.sub(r":\d+(:\d+)?$", "", candidate)
    candidate = re.sub(r"/\*\*/?\*?(\.\w+)?$", "", candidate)
    candidate = candidate.rstrip("/")
    if not candidate or "*" in candidate or " " in candidate:
        return None
    if candidate.startswith(REPO_PATH_PREFIXES):
        return candidate
    if candidate.startswith(CORE_LAYER_PREFIXES):
        return str(CORE_SRC / candidate)
    return None


def claim_exists(claim: str, root: Path) -> bool:
    """Report whether one normalized path claim is satisfied by the checkout.

    Two OpenSpec lifecycle moves are tolerated because the claim's meaning
    survives them:

    * A capability spec claim under `openspec/specs/` is also satisfied by the
      same capability inside an unarchived change, because OpenSpec moves delta
      specs into `openspec/specs/` only at archive time.
    * A claim under `openspec/changes/<name>/` is also satisfied by
      `openspec/changes/archive/*-<name>/`, because archiving prefixes the
      change directory with its archive date.

    Both keep the name-bearing part of the claim, so a fabricated capability or
    change name still fails.
    """
    if (root / claim).exists():
        return True
    if claim.startswith("openspec/specs/"):
        tail = claim.removeprefix("openspec/specs/")
        return any((root / "openspec/changes").glob(f"*/specs/{tail}"))
    if claim.startswith("openspec/changes/"):
        name, _, tail = claim.removeprefix("openspec/changes/").partition("/")
        if not name or name == "archive":
            return False
        pattern = f"*-{name}/{tail}" if tail else f"*-{name}"
        return any((root / "openspec/changes/archive").glob(pattern))
    return False


def evidence_findings(text: str, root: Path) -> list[str]:
    """Return findings for matrix path claims that do not exist on disk."""
    findings: list[str] = []
    seen: set[str] = set()
    in_fence = False
    for line_number, line in enumerate(text.splitlines(), start=1):
        # The relocation exemption is line-scoped on purpose. A planned target is
        # only a target where the matrix says `relocates to <path>`; the same
        # path named as a role in `Migrated Workflow Roles` is a claim that the
        # relocation happened, and must be existence-checked.
        planned = set(RELOCATION_TARGET_RE.findall(line))
        if line.lstrip().startswith("```"):
            # Fenced blocks hold format documentation and reproduction commands,
            # whose placeholder paths are examples rather than evidence claims.
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        for token in BACKTICKED_RE.findall(line):
            if token in planned:
                continue
            claim = normalize_claim(token)
            if claim is None or claim in seen:
                continue
            if not claim_exists(claim, root):
                seen.add(claim)
                findings.append(
                    f"{display_path(MATRIX_PATH)}:{line_number} claims `{token}`, "
                    f"but {claim} does not exist"
                )
    return findings


# --- Check 5: facade size budget --------------------------------------------


def parse_facade_budget(text: str) -> int | None:
    """Read the normative facade line budget, or None while it is unset.

    Only the `Facade size budget` section is read, and only its first
    declaration line, so a budget cannot be smuggled in from prose elsewhere or
    declared twice with different numbers.
    """
    in_section = False
    in_fence = False
    for line in text.splitlines():
        if line.lstrip().startswith("```"):
            # The section documents the declaration's own format in a fence.
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        if line.startswith("#"):
            in_section = line.strip() == FACADE_BUDGET_SECTION_HEADING
            continue
        if not in_section:
            continue
        match = FACADE_BUDGET_RE.match(line.strip())
        if match is not None:
            return int(match.group(1))
    return None


def declared_facade_path(value: str) -> str | None:
    """Extract the path a `facade:` role line claims, when it has one.

    Fail-open 4 of 4. This used to require *exactly one* backticked token, to
    avoid guessing between two candidate paths. But the established way to write
    this line is `` `path` — **N** physical lines of 370, down from a
    pre-convention `old_name.rs` ``, and that second token made the whole
    facade-budget rule inert for the row: no claim, no path, no size check, and
    the gate exits 0. Three of sixteen migrated rows were unchecked this way.

    The line's shape resolves the ambiguity without guessing: the claim is the
    **first** token, and only when it looks like a Rust source path. Prose that
    mentions another `.rs` file after the em dash cannot displace it, and a line
    whose first token is not a path still declines rather than inventing one.
    """
    tokens = BACKTICKED_RE.findall(value)
    if not tokens:
        return None
    first = tokens[0].strip()
    if not first.endswith(".rs") or "/" not in first:
        return None
    return normalize_claim(first)


def facade_size_findings(
    rows: list[MatrixRow],
    declarations: dict[str, RoleDeclaration],
    budget: int | None,
    root: Path,
) -> list[str]:
    """Return findings for migrated facades that exceed the declared budget."""
    if budget is None:
        return []
    findings: list[str] = []
    for row in rows:
        if row.status != MIGRATED_STATUS:
            continue
        declaration = declarations.get(row.row_id)
        if declaration is None:
            # Rule 3 already reports the missing declaration.
            continue
        claim = declared_facade_path(declaration.roles.get("facade", ""))
        if claim is None:
            continue
        path = root / claim
        if not path.is_file():
            # Rule 4 already reports the absent path.
            continue
        size = len(path.read_text(encoding="utf-8").splitlines())
        if size > budget:
            findings.append(
                f"{display_path(MATRIX_PATH)}:{declaration.line_number} row {row.row_id} "
                f"declares facade `{claim}`, which is {size} lines and exceeds the "
                f"normative facade line budget of {budget}"
            )
    return findings


# --- Check 6: programme record agreement ------------------------------------


@dataclass(frozen=True)
class SlotClaim:
    """One machine-readable slot ledger line from the programme record."""

    slot: str
    line_number: int
    complete: bool
    # Row ids claimed by this slot, paired with their `(partial)` marker.
    entries: tuple[tuple[str, bool], ...]


def parse_slot_ledger(text: str) -> tuple[list[SlotClaim], list[tuple[int, str]]]:
    """Parse the programme record's slot ledger lines, ignoring fenced examples.

    Returns the parsed claims and the lines that *look* like ledger entries but
    do not parse. The second value closes a fail-open: a non-matching line used
    to be skipped with the only guard being the all-lines-failed case, so a
    single typo'd verb among eleven lines dropped that slot's whole claim
    silently. A dropped `outstanding` line is exactly how a row with remaining
    work comes to read as settled.
    """
    claims: list[SlotClaim] = []
    malformed: list[tuple[int, str]] = []
    in_fence = False
    for line_number, line in enumerate(text.splitlines(), start=1):
        if line.lstrip().startswith("```"):
            # The record documents the ledger's own format in a fence.
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        stripped = line.strip()
        match = SLOT_LEDGER_RE.match(stripped)
        if match is None:
            # Narrow enough to avoid every ordinary `- slot ...` mention in
            # prose: the line must open a list item and name a slot with a
            # colon, which is the ledger's own shape.
            if SLOT_LEDGER_SHAPE_RE.match(stripped):
                malformed.append((line_number, stripped))
            continue
        entries = tuple(
            (row_id.upper(), bool(partial))
            for row_id, partial in SLOT_ENTRY_RE.findall(match.group(3))
        )
        claims.append(
            SlotClaim(
                slot=match.group(1),
                line_number=line_number,
                complete=match.group(2).lower() == "complete",
                entries=entries,
            )
        )
    return claims, malformed


# The sentence in a `superseded` row that names the rows taking its scope. It
# ends at the first *sentence-terminating* period -- one followed by whitespace
# or the end of the cell -- so a backticked path such as
# `ui/window/tab_strip/mod.rs` inside the sentence cannot truncate the capture at
# its own dot and silently drop every replacement named after it. The earlier
# form, `\s*(.+?)(?:\.|\Z)`, stopped at the *first* period of any kind.
#
# The two alternatives are disjoint on their first character and each consumes
# exactly one, so the loop is deterministic and needs no reluctant quantifier.
# `re.S` is gone because no `.` metacharacter remains for it to widen; `[^.]`
# already spans newlines, so the old DOTALL reach is preserved.
SUPERSEDED_REPLACEMENT_RE = re.compile(r"\*\*Superseded by:\*\*((?:[^.]|\.(?!\s|\Z))*)")


def superseded_findings(rows: list[MatrixRow]) -> list[str]:
    """Verify every `superseded` row names replacements the matrix carries.

    The capability delta that introduced `superseded` pairs it with this
    obligation deliberately. A terminal label on its own *exempts* its row from
    every role requirement, so a row could be marked `superseded` and simply
    stop being checked. Requiring it to name its replacements, and requiring
    each named replacement to exist as a row, means the scope it gave up is
    provably carried by rows that are themselves checked.
    """
    findings: list[str] = []
    known = {row.row_id for row in rows}
    for row in rows:
        if row.status != SUPERSEDED_STATUS:
            continue
        joined = " | ".join(row.cells)
        match = SUPERSEDED_REPLACEMENT_RE.search(joined)
        named = (
            {token for token in BACKTICKED_RE.findall(match.group(1)) if token.startswith("WFR-")}
            if match
            else set()
        )
        if not named:
            findings.append(
                f"{display_path(MATRIX_PATH)}:{row.line_number} row {row.row_id} is "
                f"`{SUPERSEDED_STATUS}` but names no replacement row; a terminal label "
                "that exempts its row from every role obligation must say which rows "
                "carry the scope it gave up"
            )
            continue
        for replacement in sorted(named - known):
            findings.append(
                f"{display_path(MATRIX_PATH)}:{row.line_number} row {row.row_id} is "
                f"`{SUPERSEDED_STATUS}` and names replacement {replacement}, which the "
                "matrix does not carry as a row"
            )
    return findings


# --- Check 7: every module in a migrated role home is declared --------------


def parse_role_sections(text: str) -> dict[str, str]:
    """Return the raw body of each `### WFR-*` subsection of the roles section.

    `parse_role_declarations` keeps only lines matching the `- role: value` shape,
    which is right for the role rules but wrong here: a row may classify a module
    in ordinary prose inside the same subsection, and that prose is still the
    row's own text.
    """
    sections: dict[str, str] = {}
    current: str | None = None
    buffer: list[str] = []
    in_section = False
    in_fence = False

    def flush() -> None:
        nonlocal current, buffer
        if current is not None:
            sections[current] = "\n".join(buffer)
        current = None
        buffer = []

    for line in text.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            if in_section and current is not None:
                buffer.append(line)
            continue
        if not in_fence and line.startswith("## "):
            flush()
            in_section = line.strip() == ROLES_SECTION_HEADING
            continue
        if not in_section:
            continue
        if not in_fence and line.startswith("### "):
            flush()
            current = line.removeprefix("### ").strip()
            continue
        if current is not None:
            buffer.append(line)
    flush()
    return sections


def named_paths(text: str) -> set[str]:
    """Return every backticked token in `text` that resolves to a repo path."""
    resolved: set[str] = set()
    for token in BACKTICKED_RE.findall(text):
        claim = normalize_claim(token)
        if claim is not None:
            resolved.add(claim)
    return resolved


def role_home_directories(
    declaration: RoleDeclaration, facade: str
) -> list[str]:
    """Return the row's role home directories, closest-first.

    The facade's directory is always a home. A second home is admitted only when
    it is a **subdirectory of it** that the row names through a declared role
    path -- the convention's nested role home. Admitting every directory holding a
    declared role would make a row that declares one coordination module in a
    shared directory responsible for every other workflow's files there.
    """
    facade_dir = Path(facade).parent
    homes = {facade_dir}
    for value in declaration.roles.values():
        for claim in named_paths(value):
            candidate = Path(claim).parent
            if candidate != facade_dir and facade_dir in candidate.parents:
                homes.add(candidate)
    return [str(home) for home in sorted(homes)]


def role_home_findings(
    rows: list[MatrixRow],
    declarations: dict[str, RoleDeclaration],
    sections: dict[str, str],
    root: Path,
) -> list[str]:
    """Return findings for undeclared modules in a migrated row's role home."""
    findings: list[str] = []
    for row in rows:
        if row.status != MIGRATED_STATUS:
            continue
        declaration = declarations.get(row.row_id)
        if declaration is None:
            # Rule 3 already reports the missing declaration.
            continue
        facade = declared_facade_path(declaration.roles.get("facade", ""))
        if facade is None or not (root / facade).is_file():
            # Rules 3 and 4 already report an unreadable or absent facade claim.
            continue
        declared = named_paths(" | ".join(row.cells)) | named_paths(
            sections.get(row.row_id, "")
        )
        for home in role_home_directories(declaration, facade):
            for path in sorted((root / home).glob("*.rs")):
                stem = path.stem
                if (
                    stem in ROLE_HOME_EXEMPT_STEMS
                    or has_convention_role_name(path.name)
                    or is_gated_kani_harness(path)
                ):
                    continue
                relative = f"{home}/{path.name}"
                if relative in declared:
                    continue
                findings.append(
                    f"{display_path(MATRIX_PATH)}:{declaration.line_number} row "
                    f"{row.row_id} has an undeclared module in its role home: "
                    f"{relative} carries no convention role name and this row's "
                    "matrix text names it nowhere. Declare it as a called "
                    "presentation surface or coordination role in the matrix row, "
                    "as a backticked repository path"
                )
    return findings


# --- Check 8: the externally reachable test-seam ratchet ---------------------


def parse_seam_ceiling(text: str) -> int | None:
    """Read the recorded `*_for_test` declaration ceiling, or None while unset."""
    in_section = False
    in_fence = False
    for line in text.splitlines():
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        if line.startswith("#"):
            in_section = line.strip() == MEASUREMENT_SECTION_HEADING
            continue
        if not in_section:
            continue
        match = SEAM_CEILING_RE.match(line.strip())
        if match is not None:
            return int(match.group(1))
    return None


def count_for_test_declarations(root: Path) -> int:
    """Count `pub`/`pub(crate)` `*_for_test` declarations in the governed crate.

    Counted over comment- and string-stripped code so a doc comment naming a
    retired getter cannot inflate the figure the ratchet compares.
    """
    core = root / CORE_SRC
    if not core.is_dir():
        return 0
    total = 0
    for path in sorted(core.rglob("*.rs")):
        for _, line in code_lines(path.read_text(encoding="utf-8")):
            total += len(FOR_TEST_DECLARATION_RE.findall(line))
    return total


def seam_ratchet_findings(text: str, root: Path, *, required: bool) -> list[str]:
    """Return a finding when the seam count exceeds the recorded ceiling.

    `required` is set by the real-tree entry point. A fixture exercising another
    rule carries no ceiling and must leave this inert, but the real matrix losing
    its declaration -- by a reworded heading, say -- would silently retire the
    programme's headline ratchet while the gate kept exiting 0. The flag is
    explicit rather than inferred from another rule's argument, so a fixture for
    one rule cannot turn a second rule on by accident.
    """
    ceiling = parse_seam_ceiling(text)
    if ceiling is None:
        if required:
            return [
                f"{display_path(MATRIX_PATH)}: no "
                f"`{MEASUREMENT_SECTION_HEADING.removeprefix('### ')}` declaration was "
                "parsed, so the `*_for_test` ratchet checked nothing; restore the "
                "section's `- externally reachable `*_for_test` declaration ceiling: "
                "<integer>` line or update this check together with it"
            ]
        return []
    actual = count_for_test_declarations(root)
    if actual <= ceiling:
        return []
    return [
        f"{display_path(MATRIX_PATH)}: {actual} externally reachable `*_for_test` "
        f"declarations under {CORE_SRC} exceed the recorded ceiling of {ceiling}. "
        "Extend the owning workflow's evidence surface with the fact the test "
        "needs and delete the getter; or, deliberately, raise the ceiling in the "
        "matrix's Measurement Definitions in this same change with a stated reason"
    ]


def record_findings(
    rows: list[MatrixRow], record_path: Path, slot_column_located: bool
) -> list[str]:
    """Return findings where the programme record and the matrix disagree."""
    record = display_path(record_path)
    if not record_path.is_file():
        return [
            f"missing programme record: {record}; the matrix's status is only half "
            "the story without it"
        ]

    findings: list[str] = []
    if not slot_column_located:
        # Fail-open 1 of 3. `parse_matrix_rows` locates the `Slot` column by
        # name, deliberately, so a later column insertion cannot shift a
        # positional guess. The cost is that rewording the header makes every
        # row's slot `None`, which skips the whole outstanding-slot sweep below
        # and exits 0. Reconciling against a record without that column is the
        # exact state where the skip is invisible, so it is reported here rather
        # than at the parse, where a Slot-less fixture matrix is legitimate.
        findings.append(
            f"{display_path(MATRIX_PATH)}: the Product Matrix `Slot` column could not "
            f"be located, so every row parsed with no slot and the outstanding-slot "
            f"half of the reconciliation against {record} was skipped; restore the "
            "`Slot` header or update this check together with it"
        )

    text = record_path.read_text(encoding="utf-8")
    claims, malformed = parse_slot_ledger(text)
    for line_number, line in malformed:
        # Fail-open 2 of 3. A line that looks like a ledger line but does not
        # parse used to be `continue`d, guarded only by the all-lines-failed
        # case. One typo'd verb among eleven lines was invisible — and a dropped
        # `outstanding` line is precisely how a row with remaining work reads as
        # settled.
        findings.append(
            f"{record}:{line_number}: this line opens like a slot ledger entry but "
            f"does not parse as `- slot <n> (complete|outstanding): <WFR-ID>, ...`, so "
            f"its claim was silently dropped: {line!r}"
        )

    if not claims:
        findings.append(
            f"{record}: no `- slot <n> (complete|outstanding): <WFR-ID>` ledger lines "
            "were parsed, so the record makes no checkable claim about remaining scope"
        )
        return findings

    status_by_id = {row.row_id: row for row in rows}
    claimed_outstanding: set[str] = set()
    claimed_complete: set[str] = set()

    for claim in claims:
        for row_id, partial in claim.entries:
            row = status_by_id.get(row_id)
            if row is None:
                findings.append(
                    f"{record}:{claim.line_number} slot {claim.slot} names {row_id}, "
                    f"which has no row in {display_path(MATRIX_PATH)}"
                )
                continue
            if claim.complete and partial:
                # `(partial)` on a complete line means this slot's share of the
                # row is done while the row continues in a later slot, which is
                # how an incremental row such as WFR-AUTOMATION-SPINE avoids
                # having to be falsely marked `migrated` to satisfy the gate.
                continue
            if claim.complete:
                claimed_complete.add(row_id)
            if claim.complete and row.status in TERMINAL_NON_MIGRATING_STATUSES:
                # A cross-cutting lane or an exempt row can never be `migrated`
                # — that is what the label means — so a slot that finishes its
                # work cannot express the fact by marking it migrated. Requiring
                # that would demand the impossible, and the alternative the gate
                # used to force was worse: listing a terminal row as
                # *outstanding* so the reconciliation would pass, which is how
                # `WFR-PLAIN-DISPOSAL` came to be recorded as unfinished work in
                # a slot that had settled it.
                continue
            if claim.complete and row.status != MIGRATED_STATUS:
                findings.append(
                    f"{record}:{claim.line_number} slot {claim.slot} is declared "
                    f"complete but {row_id} is `{row.status}` in "
                    f"{display_path(MATRIX_PATH)}:{row.line_number}, not "
                    f"`{MIGRATED_STATUS}`"
                )
                continue
            if not claim.complete:
                claimed_outstanding.add(row_id)
                if row.status == MIGRATED_STATUS and not partial:
                    findings.append(
                        f"{record}:{claim.line_number} slot {claim.slot} lists {row_id} "
                        f"as outstanding, but {display_path(MATRIX_PATH)}:"
                        f"{row.line_number} marks it `{MIGRATED_STATUS}`; a migrated row "
                        "with remaining scope must be written as "
                        f"`{row_id} (partial)`"
                    )

    # Delta 1's mechanical half, landed by slot 7b. A ledger with no
    # `outstanding` slot is the machine-readable statement that the programme is
    # closed. A transitional row surviving that is exactly the drift the delta
    # forbids, and it is invisible to every other check here: each of those only
    # asks whether the ledger and the matrix *agree*, and they agree perfectly
    # when a `pending` row is listed as outstanding in a slot nobody will run.
    if all(claim.complete for claim in claims):
        for row in rows:
            if row.status in TRANSITIONAL_STATUSES:
                findings.append(
                    f"{display_path(MATRIX_PATH)}:{row.line_number} row {row.row_id} is "
                    f"`{row.status}`, which is transitional, but {record} declares no "
                    "outstanding slot; a transitional status must not survive the change "
                    "that closes the migration programme"
                )

    for row in rows:
        if row.status in SETTLED_STATUSES:
            continue
        slot_cell = "" if row.slot is None else row.slot.strip().lower()
        if slot_cell == "none":
            # Fail-open 3 of 3. `none` is a real value meaning "never migrated",
            # which is correct for a row whose terminal status says so. It is not
            # correct for a *transitional* row: nothing distinguished
            # "deliberately never migrated" from "has remaining work and no slot
            # to do it in", so the latter escaped the sweep below entirely.
            if row.status not in TERMINAL_NON_MIGRATING_STATUSES:
                findings.append(
                    f"{display_path(MATRIX_PATH)}:{row.line_number} row {row.row_id} is "
                    f"`{row.status}` with migration slot `none`; `none` means the row is "
                    "never migrated, which only a terminal status may claim, so this row "
                    "is exempt from the outstanding-slot rule while still carrying "
                    "unfinished work"
                )
            continue
        if not slot_cell:
            continue
        # Named on a complete line is also being accounted for. Only a row the
        # ledger mentions nowhere is a disagreement.
        if row.row_id not in claimed_outstanding and row.row_id not in claimed_complete:
            findings.append(
                f"{display_path(MATRIX_PATH)}:{row.line_number} row {row.row_id} is "
                f"`{row.status}` with migration slot `{row.slot}`, but {record} lists it "
                "in no outstanding slot; the remaining-scope ledger and the matrix "
                "disagree about what is left"
            )
    return findings


# --- Orchestration ----------------------------------------------------------


def check_tree(
    root: Path,
    matrix_path: Path,
    mutants_config: Path,
    record_path: Path | None = None,
    *,
    require_seam_ceiling: bool = False,
) -> list[str]:
    """Return every workflow boundary finding for one checkout root."""
    findings: list[str] = []

    modules = policy_modules(root)
    for module in modules:
        findings.extend(gtk_reference_findings(module, root))

    relative_modules = [str(module.relative_to(root)) for module in modules]
    if mutants_config.is_file():
        findings.extend(
            mutation_reach_findings(relative_modules, parse_examine_globs(mutants_config))
        )
    else:
        findings.append(f"missing mutation configuration: {display_path(mutants_config)}")

    # Check 3 needs no matrix data, so it runs before the matrix early return:
    # a missing matrix must not silently disarm the discovery half.
    findings.extend(unclassified_pure_module_findings(root))
    findings.extend(kani_harness_findings(root))
    findings.extend(whole_pixel_findings(root, real_tree=require_seam_ceiling))
    if require_seam_ceiling:
        findings.extend(whole_pixel_ledger_findings(root / CONVENTION_PATH.relative_to(REPO_ROOT)))

    if not matrix_path.is_file():
        findings.append(f"missing workflow readability matrix: {display_path(matrix_path)}")
        return findings

    text = matrix_path.read_text(encoding="utf-8")
    rows, slot_column_located = parse_matrix_rows(text)
    if not rows:
        findings.append(f"{display_path(matrix_path)}: no product matrix rows were parsed")
        return findings

    declarations = parse_role_declarations(text)
    findings.extend(status_findings(rows))
    findings.extend(superseded_findings(rows))
    findings.extend(role_findings(rows, declarations))
    findings.extend(evidence_findings(text, root))
    findings.extend(
        facade_size_findings(rows, declarations, parse_facade_budget(text), root)
    )
    findings.extend(
        role_home_findings(rows, declarations, parse_role_sections(text), root)
    )
    findings.extend(seam_ratchet_findings(text, root, required=require_seam_ceiling))
    if record_path is not None:
        findings.extend(record_findings(rows, record_path, slot_column_located))
    return findings


def write(path: Path, content: str) -> None:
    """Write a self-test fixture file, creating parent directories."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


MINIMAL_MUTANTS_CONFIG = """
test_tool = "nextest"

examine_globs = [
    "crates/lushtext-core/src/model/**/*.rs",
    "crates/lushtext-core/src/services/**/*.rs",
    "crates/lushtext-core/src/ui/**/policy.rs",
]
""".lstrip()

MATRIX_HEADER = """
# Fixture Matrix

## Product Matrix

| Row id | Workflow | Owned pure policy | Status |
| --- | --- | --- | --- |
""".lstrip()


def facade_budget_section(budget: int | None) -> str:
    """Render the matrix's facade-budget section for a fixture.

    Passing None renders the section without a declaration, which is the
    exemplar's recorded state and must leave the rule inert.
    """
    declaration = "" if budget is None else f"- normative facade line budget: {budget}\n"
    return (
        "\n## Conventions\n\n### Facade size budget\n\n"
        "Declared as:\n\n```\n- normative facade line budget: <integer>\n```\n\n"
        f"{declaration}"
    )


def build_fixture(
    root: Path,
    *,
    matrix_body: str,
    roles: str = "",
    budget_section: str = "",
) -> tuple[Path, Path]:
    """Create a minimal checkout-shaped fixture and return its two inputs."""
    write(root / ".cargo/mutants.toml", MINIMAL_MUTANTS_CONFIG)
    matrix = root / "docs/workflow-readability-matrix.md"
    write(matrix, MATRIX_HEADER + matrix_body + budget_section + roles)
    return matrix, root / ".cargo/mutants.toml"


# --- Rule 10 self-test fixtures ---------------------------------------------
#
# One table so every escape the rule closes is named once, and so the same
# fixtures can be replayed against an older revision of this script to show
# each case failing first. `files` maps paths under `crates/lushtext-core/src`
# to contents; `finding` is whether rule 10 must report it.

WHOLE_PIXEL_FORBID = "#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]\n"
WHOLE_PIXEL_DENY = "#![deny(clippy::float_arithmetic, clippy::disallowed_methods)]\n"
WHOLE_PIXEL_DENY_MODULE = "ui/editor_page/minimap/policy.rs"
WHOLE_PIXEL_FORBID_MODULE = "ui/window/geometry/policy.rs"
WHOLE_PIXEL_REASONED_FN = (
    '#[expect(clippy::float_arithmetic, reason = "a widget coordinate is fractional")]\n'
    "pub fn half(value: f64) -> f64 { value / 2.0 }\n"
)


@dataclass(frozen=True)
class WholePixelCase:
    """One rule-10 fixture and whether it must be a finding."""

    name: str
    files: dict[str, str]
    finding: bool
    real_tree: bool = False


def whole_pixel_cases() -> list[WholePixelCase]:
    """Return every rule-10 fixture, passing and failing."""
    deny_module = WHOLE_PIXEL_DENY_MODULE
    deny_children = deny_module[: -len(".rs")]
    forbid_module = WHOLE_PIXEL_FORBID_MODULE
    new_home_policy = GEOMETRY_ROLE_HOMES[0] + "/nested/policy.rs"

    def deny(body: str, **children: str) -> dict[str, str]:
        files = {deny_module: "//! Pure.\n\n" + WHOLE_PIXEL_DENY + body}
        files.update({f"{deny_children}/{name}.rs": text for name, text in children.items()})
        return files

    fns = "".join(
        f'#[expect(clippy::float_arithmetic, reason = "fractional {n}")]\n'
        f"pub fn f{n}(v: f64) -> f64 {{ v / 2.0 }}\n"
        for n in range(WHOLE_PIXEL_POLICY_MODULES[deny_module] + 1)
    )
    at_ceiling = fns[: fns.rindex("#[expect")]
    return [
        # Passing shapes.
        WholePixelCase("a forbidding module passes", {forbid_module: "//! Pure.\n" + WHOLE_PIXEL_FORBID}, False),
        WholePixelCase("a denying module with a reasoned fn-level expect passes", deny(WHOLE_PIXEL_REASONED_FN), False),
        WholePixelCase(
            "an expect on a method, a const fn, and through other attributes passes",
            deny(
                "impl Span {\n"
                '    #[expect(clippy::float_arithmetic, reason = "a fractional height")]\n'
                "    #[must_use]\n"
                "    pub(crate) const fn height(&self) -> f64 { self.b - self.a }\n"
                "}\n"
            ),
            False,
        ),
        WholePixelCase(
            "two expects on one function count as one admission",
            deny(
                at_ceiling[: at_ceiling.rindex("pub fn")]
                + '#[expect(clippy::disallowed_methods, reason = "f64::midpoint")]\n'
                + at_ceiling[at_ceiling.rindex("pub fn") :]
            ),
            False,
        ),
        WholePixelCase("admissions at the ceiling pass", deny(at_ceiling), False),
        WholePixelCase(
            "a gated Kani harness child is not scanned",
            deny(
                "#[cfg(kani)]\nmod kani_proofs;\n",
                kani_proofs='#![allow(clippy::float_arithmetic, reason = "harness")]\n',
            ),
            False,
        ),
        WholePixelCase("a new geometry policy.rs that forbids passes", {new_home_policy: "//! Pure.\n" + WHOLE_PIXEL_FORBID}, False),
        WholePixelCase("fixtures without the listed modules pass off the real tree", {"model/other.rs": "pub fn ok() {}\n"}, False),
        WholePixelCase(
            "attributes inside literals are ignored",
            deny('pub const S: &str = "#![allow(clippy::float_arithmetic)]";\npub const C: char = \'"\';\n'),
            False,
        ),
        # Findings: missing or wrong top attribute.
        WholePixelCase("a listed module without the attribute", {forbid_module: "//! Pure.\npub fn ok() {}\n"}, True),
        WholePixelCase("a module needing no admission that only denies", {forbid_module: "//! Pure.\n" + WHOLE_PIXEL_DENY}, True),
        WholePixelCase(
            "a forbid only inside a line comment",
            {forbid_module: "//! Pure.\n// " + WHOLE_PIXEL_FORBID},
            True,
        ),
        WholePixelCase(
            "a deny only inside a block comment",
            {deny_module: "//! Pure.\n/*\n" + WHOLE_PIXEL_DENY + "*/\npub fn ok() {}\n"},
            True,
        ),
        WholePixelCase(
            "a deny of float arithmetic without disallowed_methods",
            {deny_module: "//! Pure.\n#![deny(clippy::float_arithmetic)]\n"},
            True,
        ),
        WholePixelCase("a new geometry policy.rs without the attribute", {new_home_policy: "//! Pure.\npub fn ok() {}\n"}, True),
        WholePixelCase("a listed module missing from the real tree", {"model/other.rs": "pub fn ok() {}\n"}, True, real_tree=True),
        # Findings: lint-lowering attributes the workspace Clippy lints miss.
        WholePixelCase(
            "a module-level inner allow",
            deny('#![allow(clippy::float_arithmetic, reason = "x")]\n'),
            True,
        ),
        WholePixelCase(
            "a module-wide expect",
            deny('#![expect(clippy::float_arithmetic, reason = "x")]\n'),
            True,
        ),
        WholePixelCase(
            "a reasoned expect on an impl block",
            deny(
                '#[expect(clippy::float_arithmetic, reason = "x")]\n'
                "impl Span {\n    pub fn height(&self) -> f64 { self.b - self.a }\n}\n"
            ),
            True,
        ),
        WholePixelCase(
            "a reasoned expect on a `mod x;` line",
            deny(
                '#[expect(clippy::float_arithmetic, reason = "x")]\nmod helpers;\n',
                helpers="pub fn half(v: f64) -> f64 { v / 2.0 }\n",
            ),
            True,
        ),
        WholePixelCase(
            "a reasoned expect on an inline mod",
            deny(
                '#[expect(clippy::float_arithmetic, reason = "x")]\n'
                "mod inner {\n    pub fn half(v: f64) -> f64 { v / 2.0 }\n}\n"
            ),
            True,
        ),
        WholePixelCase(
            "a reasoned expect on a statement",
            deny(
                "pub fn half(v: f64) -> f64 {\n"
                '    #[expect(clippy::float_arithmetic, reason = "x")]\n'
                "    let h = v / 2.0;\n    h\n}\n"
            ),
            True,
        ),
        WholePixelCase(
            "a module-level inner allow in an inline mod",
            deny(
                "pub mod inner {\n"
                '    #![allow(clippy::float_arithmetic, reason = "x")]\n'
                "    pub fn half(v: f64) -> f64 { v / 2.0 }\n}\n"
            ),
            True,
        ),
        WholePixelCase(
            "a cfg_attr allow",
            deny('#![cfg_attr(all(), allow(clippy::float_arithmetic, reason = "x"))]\n'),
            True,
        ),
        WholePixelCase(
            "a cfg_attr expect on a function",
            deny(
                '#[cfg_attr(all(), expect(clippy::float_arithmetic, reason = "x"))]\n'
                "pub fn half(v: f64) -> f64 { v / 2.0 }\n"
            ),
            True,
        ),
        WholePixelCase(
            "a group-level expect of restriction",
            deny(
                "#![expect(clippy::restriction, clippy::blanket_clippy_restriction_lints, "
                'reason = "x")]\n'
            ),
            True,
        ),
        WholePixelCase(
            "an allow of the style group, which holds disallowed_methods",
            deny('#[allow(clippy::style, reason = "x")]\npub fn mid(a: f64, b: f64) -> f64 { f64::midpoint(a, b) }\n'),
            True,
        ),
        WholePixelCase(
            "a warn of float arithmetic",
            deny('#![warn(clippy::float_arithmetic)]\n'),
            True,
        ),
        WholePixelCase(
            "a child file that expects the lint module-wide",
            deny(
                "mod helpers;\n",
                helpers='#![expect(clippy::float_arithmetic, reason = "x")]\npub fn half(v: f64) -> f64 { v / 2.0 }\n',
            ),
            True,
        ),
        WholePixelCase(
            "a child file of a forbidding module that allows the lint",
            {
                forbid_module: "//! Pure.\n" + WHOLE_PIXEL_FORBID + "mod helpers;\n",
                forbid_module[: -len(".rs")] + "/helpers.rs": '#![allow(clippy::float_arithmetic, reason = "x")]\n',
            },
            True,
        ),
        WholePixelCase("admissions above the ceiling", deny(fns), True),
        WholePixelCase(
            "child-file admissions count toward the ceiling",
            deny(
                at_ceiling + "mod helpers;\n",
                helpers=WHOLE_PIXEL_REASONED_FN,
            ),
            True,
        ),
        WholePixelCase(
            "a module remapped with #[path]",
            deny('#[path = "../elsewhere.rs"]\nmod helpers;\n'),
            True,
        ),
        WholePixelCase("an include! of unscanned text", deny('include!("../elsewhere.rs");\n'), True),
    ]


def run_whole_pixel_case(case: WholePixelCase, findings_of) -> list[str]:
    """Return what `findings_of(root, real_tree=...)` reports for one fixture."""
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        for relative, content in case.files.items():
            write(root / CORE_SRC / relative, content)
        return findings_of(root, real_tree=case.real_tree)


def whole_pixel_self_test_failures() -> list[tuple[str, str]]:
    """Return `(case, problem)` for every rule-10 fixture the rule gets wrong."""
    failures: list[tuple[str, str]] = []
    for case in whole_pixel_cases():
        findings = run_whole_pixel_case(case, whole_pixel_findings)
        if case.finding and not findings:
            failures.append((case.name, "expected a finding, got none"))
        if not case.finding and findings:
            failures.append((case.name, f"expected no finding, got {findings}"))

    table_header = (
        f"{WHOLE_PIXEL_SECTION_HEADING}\n\n| Module | Level | Admitted functions (ceiling) |\n"
        "| --- | --- | --- |\n"
    )

    def table(rows: dict[str, int]) -> str:
        return table_header + "".join(
            f"| `{path}` | `{'forbid' if ceiling == 0 else 'deny'}` | {ceiling} |\n"
            for path, ceiling in rows.items()
        )

    def ledger(text: str | None) -> list[str]:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "workflow-convention.md"
            if text is not None:
                path.write_text(text, encoding="utf-8")
            return whole_pixel_ledger_findings(path)

    exact = dict(WHOLE_PIXEL_POLICY_MODULES)
    if ledger(table(exact)):
        failures.append(("an agreeing whole-pixel table", f"got {ledger(table(exact))}"))
    first = next(iter(exact))
    for name, rows in [
        ("a table omitting a protected module", {k: v for k, v in exact.items() if k != first}),
        ("a table with a different ceiling", {**exact, WHOLE_PIXEL_DENY_MODULE: 99}),
        ("a table listing an unprotected module", {**exact, "model/other.rs": 0}),
    ]:
        if not ledger(table(rows)):
            failures.append((name, "expected a finding, got none"))
    wrong_level = table(exact).replace(f"| `{first}` | `forbid` |", f"| `{first}` | `deny` |")
    if not ledger(wrong_level):
        failures.append(("a table with the wrong level", "expected a finding, got none"))
    if not ledger("# Convention\n\nNo table.\n"):
        failures.append(("a convention without the table", "expected a finding, got none"))
    if not ledger(None):
        failures.append(("a missing convention", "expected a finding, got none"))
    return failures


def run_self_test() -> None:
    """Prove each rule fires on a broken fixture and passes on a clean one."""
    # Every known status must be classified by at least one of the three rule
    # sets below, and a transitional label must not also read as terminal. This
    # is the one invariant over the status vocabulary that no fixture can reach:
    # a label added to `KNOWN_STATUS_LABELS` and classified nowhere passes
    # `status_findings` as recognized, then falls through the settled, terminal,
    # and transitional tests alike — silently exempting its rows from the
    # outstanding-slot sweep and the programme-closed rule. That is the same
    # fail-open shape this file closes four times elsewhere, one level up in the
    # vocabulary rather than in the parsing.
    classified = set(SETTLED_STATUSES) | set(TERMINAL_NON_MIGRATING_STATUSES)
    unclassified = set(KNOWN_STATUS_LABELS) - classified - set(TRANSITIONAL_STATUSES)
    if unclassified:
        raise AssertionError(
            f"status label(s) {sorted(unclassified)} are in KNOWN_STATUS_LABELS but "
            "classified as neither settled, terminal-non-migrating, nor transitional; "
            "rows holding them would escape the outstanding-slot and programme-closed "
            "rules"
        )
    both = set(TRANSITIONAL_STATUSES) & classified
    if both:
        raise AssertionError(
            f"status label(s) {sorted(both)} are classified both transitional and "
            "terminal/settled; the programme-closed rule and the outstanding-slot "
            "exemption would disagree about the same row"
        )

    clean_row = "| WFR-EXAMPLE | Example | `model/example_policy.rs` | pending |\n"

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        matrix, config = build_fixture(root, matrix_body=clean_row)
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write(
            root / CORE_SRC / "ui/search_panel/policy.rs",
            "//! gtk4::Widget in a doc comment is fine.\npub fn decide() -> bool { true }\n",
        )
        findings = check_tree(root, matrix, config)
        if findings:
            raise AssertionError(f"expected clean fixture to pass, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        matrix, config = build_fixture(root, matrix_body=clean_row)
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write(
            root / CORE_SRC / "ui/search_panel/policy.rs",
            "use gtk4::prelude::*;\npub fn decide(w: &gtk4::Widget) {}\n",
        )
        findings = check_tree(root, matrix, config)
        if not any("references `gtk4`" in finding for finding in findings):
            raise AssertionError(f"expected a GTK purity finding, got {findings}")

    # --- Check 3: inclusion-side discovery ---------------------------------
    #
    # The red arm and the green arm are both proved, because a discovery check
    # that has only ever been seen passing is a check nobody has run, and one
    # that goes red on a conforming tree is a check that must be suppressed to
    # pass. The green arm therefore uses the module shapes the convention
    # actually blesses: a GTK-free facade, a `seams.rs`, a bounded
    # `retirement.rs`, a qualified coordination role, and a module that declares
    # cross-cutting ownership in prose.
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        matrix, config = build_fixture(root, matrix_body=clean_row)
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write(
            root / CORE_SRC / "ui/example/unclassified.rs",
            "//! Some prose that names no role at all.\npub fn decide() -> bool { true }\n",
        )
        findings = check_tree(root, matrix, config)
        if not any("carries no declared workflow role" in f for f in findings):
            raise AssertionError(f"expected a discovery finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        matrix, config = build_fixture(root, matrix_body=clean_row)
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        # Every one of these is correctly named or correctly declared under the
        # convention and MUST NOT be reported.
        write(
            root / CORE_SRC / "ui/example/mod.rs",
            "//! The example workflow facade.\npub fn run() -> bool { true }\n",
        )
        write(
            root / CORE_SRC / "ui/example/seams.rs",
            "//! Seam value objects.\npub fn make() -> bool { true }\n",
        )
        write(
            root / CORE_SRC / "ui/example/retirement.rs",
            "//! Deferred destruction.\npub fn retire() -> bool { true }\n",
        )
        write(
            root / CORE_SRC / "ui/example/query_execution.rs",
            "//! A qualified bounded role.\npub fn execute() -> bool { true }\n",
        )
        write(
            root / CORE_SRC / "ui/example/test_policy.rs",
            "//! Test policy.\npub fn value() -> u64 { 1 }\n",
        )
        write(
            root / CORE_SRC / "ui/example/evidence.rs",
            "//! Evidence surface.\npub fn read() -> bool { true }\n",
        )
        write(
            root / CORE_SRC / "ui/example/preset.rs",
            "//! A value that is **cross-cutting**, owned by `WFR-EXAMPLE`.\n"
            "pub fn clamp(v: u64) -> u64 { v }\n",
        )
        write(
            root / CORE_SRC / "ui/example/types_only.rs",
            "//! Types with no decision to mutate.\npub struct Thing;\n",
        )
        findings = check_tree(root, matrix, config)
        if any("carries no declared workflow role" in f for f in findings):
            raise AssertionError(
                f"discovery check went red on a conforming tree: {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        matrix, config = build_fixture(root, matrix_body=clean_row)
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        # A crate-root `policy.rs` is outside every real `examine_globs` entry:
        # the globs reach `model/**`, `services/**`, and `ui/**/policy.rs` only.
        # (`/**/` matches zero segments in globset, so `ui/policy.rs` *is*
        # reachable and would not exercise this rule.)
        write(root / CORE_SRC / "policy.rs", "pub fn decide() {}\n")
        findings = check_tree(root, matrix, config)
        if not any("examine_globs" in finding for finding in findings):
            raise AssertionError(f"expected a mutation reach finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        matrix, config = build_fixture(
            root,
            matrix_body="| WFR-EXAMPLE | Example | none | migrated |\n",
        )
        findings = check_tree(root, matrix, config)
        if not any("has no `Migrated Workflow Roles` entry" in f for f in findings):
            raise AssertionError(f"expected a missing-roles finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        matrix, config = build_fixture(
            root,
            matrix_body="| WFR-EXAMPLE | Example | none | migrated |\n",
            roles=(
                "\n## Migrated Workflow Roles\n\n### WFR-EXAMPLE\n\n"
                "- facade: `ui/search_panel/mod.rs`\n"
                "- coordination: none\n"
                "- policy: none\n"
                "- mutation parity: none\n"
            ),
        )
        write(root / CORE_SRC / "ui/search_panel/mod.rs", "pub struct Panel;\n")
        findings = check_tree(root, matrix, config)
        if not any("does not name its `evidence` role" in f for f in findings):
            raise AssertionError(f"expected a missing-role finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        matrix, config = build_fixture(
            root,
            matrix_body="| WFR-EXAMPLE | Example | none | migrated |\n",
            roles=(
                "\n## Migrated Workflow Roles\n\n### WFR-EXAMPLE\n\n"
                "- facade: `ui/search_panel/mod.rs`\n"
                "- coordination: `ui/search_panel/execution.rs`\n"
                "- policy: none\n"
                "- evidence: `ui/search_panel/evidence.rs`\n"
                "- mutation parity: none\n"
            ),
        )
        write(root / CORE_SRC / "ui/search_panel/mod.rs", "pub struct Panel;\n")
        write(root / CORE_SRC / "ui/search_panel/execution.rs", "pub struct Run;\n")
        findings = check_tree(root, matrix, config)
        if not any("evidence.rs does not exist" in f for f in findings):
            raise AssertionError(f"expected an absent-evidence finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A planned relocation target is a target, not an evidence claim.
        matrix, config = build_fixture(
            root,
            matrix_body=(
                "| WFR-EXAMPLE | Example | `model/example_policy.rs` "
                "(1 workflow, relocates to `ui/search_panel/policy.rs`) | pending |\n"
            ),
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        findings = check_tree(root, matrix, config)
        if findings:
            raise AssertionError(f"expected planned target to be exempt, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        matrix, config = build_fixture(root, matrix_body=clean_row)
        findings = check_tree(root, matrix, config)
        if not any("example_policy.rs does not exist" in f for f in findings):
            raise AssertionError(f"expected a missing-module finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Fenced format documentation must not be read as a declaration or a claim.
        matrix, config = build_fixture(
            root,
            matrix_body=clean_row,
            roles=(
                "\n## Migrated Workflow Roles\n\n"
                "```\n### WFR-EXAMPLE\n\n- facade: `ui/example/mod.rs`\n```\n"
            ),
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        findings = check_tree(root, matrix, config)
        if findings:
            raise AssertionError(f"expected fenced documentation to be inert, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # An unarchived capability spec satisfies its `openspec/specs/` claim;
        # a capability that exists nowhere still fails.
        matrix, config = build_fixture(
            root,
            matrix_body=clean_row,
            roles=(
                "\nSee `openspec/specs/live-capability/spec.md` and "
                "`openspec/specs/absent-capability/spec.md`.\n"
            ),
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write(root / "openspec/changes/in-flight/specs/live-capability/spec.md", "# spec\n")
        findings = check_tree(root, matrix, config)
        if any("live-capability" in finding for finding in findings):
            raise AssertionError(f"expected unarchived capability to pass, got {findings}")
        if not any("absent-capability" in finding for finding in findings):
            raise AssertionError(f"expected absent capability to fail, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # An archived change satisfies an `openspec/changes/<name>/` claim, but a
        # change name that exists nowhere still fails.
        matrix, config = build_fixture(
            root,
            matrix_body=clean_row,
            roles=(
                "\nSee `openspec/changes/live-change/evidence/parity.md` and "
                "`openspec/changes/bogus-change/evidence/parity.md`.\n"
            ),
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write(
            root / "openspec/changes/archive/2026-01-02-live-change/evidence/parity.md",
            "# parity\n",
        )
        findings = check_tree(root, matrix, config)
        if any("live-change" in finding for finding in findings):
            raise AssertionError(f"expected archived change to pass, got {findings}")
        if not any("bogus-change" in finding for finding in findings):
            raise AssertionError(f"expected absent change to fail, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A relocation target is exempt only on the `relocates to` line. Once the
        # roles section claims the same path, it must exist.
        matrix, config = build_fixture(
            root,
            matrix_body=(
                "| WFR-EXAMPLE | Example | `model/example_policy.rs` "
                "(relocates to `ui/search_panel/policy.rs`) | migrated |\n"
            ),
            roles=(
                "\n## Migrated Workflow Roles\n\n### WFR-EXAMPLE\n\n"
                "- facade: `ui/search_panel/mod.rs`\n"
                "- coordination: none\n"
                "- policy: `ui/search_panel/policy.rs`\n"
                "- evidence: `ui/search_panel/evidence.rs`\n"
                "- mutation parity: none\n"
            ),
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write(root / CORE_SRC / "ui/search_panel/mod.rs", "pub struct Panel;\n")
        write(root / CORE_SRC / "ui/search_panel/evidence.rs", "pub struct Facts;\n")
        findings = check_tree(root, matrix, config)
        if not any("search_panel/policy.rs does not exist" in f for f in findings):
            raise AssertionError(
                f"expected a roles-section relocation claim to be checked, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Roles declared for a row that is not marked `migrated` is reverse drift.
        matrix, config = build_fixture(
            root,
            matrix_body=clean_row,
            roles=(
                "\n## Migrated Workflow Roles\n\n### WFR-EXAMPLE\n\n"
                "- facade: `ui/search_panel/mod.rs`\n"
                "- coordination: none\n"
                "- policy: none\n"
                "- evidence: `ui/search_panel/evidence.rs`\n"
                "- mutation parity: none\n"
            ),
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write(root / CORE_SRC / "ui/search_panel/mod.rs", "pub struct Panel;\n")
        write(root / CORE_SRC / "ui/search_panel/evidence.rs", "pub struct Facts;\n")
        findings = check_tree(root, matrix, config)
        if not any(f"is not marked `{MIGRATED_STATUS}`" in f for f in findings):
            raise AssertionError(f"expected a reverse-drift finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # `facade` and `evidence` may never be the literal `none`.
        matrix, config = build_fixture(
            root,
            matrix_body="| WFR-EXAMPLE | Example | none | migrated |\n",
            roles=(
                "\n## Migrated Workflow Roles\n\n### WFR-EXAMPLE\n\n"
                "- facade: none\n"
                "- coordination: none\n"
                "- policy: none\n"
                "- evidence: none\n"
                "- mutation parity: none\n"
            ),
        )
        findings = check_tree(root, matrix, config)
        for role in ("facade", "evidence"):
            if not any(f"declares `{role}: none`" in f for f in findings):
                raise AssertionError(
                    f"expected `{role}: none` to be rejected, got {findings}"
                )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A status label the matrix does not document must fail loudly instead of
        # exempting its row from the migrated-role rule.
        matrix, config = build_fixture(
            root,
            matrix_body="| WFR-EXAMPLE | Example | none | migrated ✓ |\n",
        )
        findings = check_tree(root, matrix, config)
        if not any("is not one of" in finding for finding in findings):
            raise AssertionError(f"expected an unknown-status finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A documented label with a parenthetical suffix still selects rule 3.
        matrix, config = build_fixture(
            root,
            matrix_body="| WFR-EXAMPLE | Example | none | migrated (slot 1) |\n",
        )
        findings = check_tree(root, matrix, config)
        if any("is not one of" in finding for finding in findings):
            raise AssertionError(f"expected `migrated (slot 1)` to parse, got {findings}")
        if not any("has no `Migrated Workflow Roles` entry" in f for f in findings):
            raise AssertionError(
                f"expected a suffixed `migrated` status to require roles, got {findings}"
            )

    migrated_row = "| WFR-EXAMPLE | Example | none | migrated |\n"
    migrated_roles = (
        "\n## Migrated Workflow Roles\n\n### WFR-EXAMPLE\n\n"
        "- facade: `ui/search_panel/mod.rs`\n"
        "- coordination: none\n"
        "- policy: none\n"
        "- evidence: `ui/search_panel/evidence.rs`\n"
        "- mutation parity: none\n"
    )

    def write_migrated_workflow(root: Path, *, facade_lines: int) -> None:
        write(
            root / CORE_SRC / "ui/search_panel/mod.rs",
            "".join(f"// line {index}\n" for index in range(facade_lines)),
        )
        write(root / CORE_SRC / "ui/search_panel/evidence.rs", "pub struct Facts;\n")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A declared budget the facade respects must pass.
        matrix, config = build_fixture(
            root,
            matrix_body=migrated_row,
            budget_section=facade_budget_section(400),
            roles=migrated_roles,
        )
        write_migrated_workflow(root, facade_lines=400)
        findings = check_tree(root, matrix, config)
        if findings:
            raise AssertionError(f"expected a respected budget to pass, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # One line over the declared budget must fail and name the numbers.
        matrix, config = build_fixture(
            root,
            matrix_body=migrated_row,
            budget_section=facade_budget_section(400),
            roles=migrated_roles,
        )
        write_migrated_workflow(root, facade_lines=401)
        findings = check_tree(root, matrix, config)
        if not any(
            "is 401 lines and exceeds the normative facade line budget of 400" in finding
            for finding in findings
        ):
            raise AssertionError(f"expected a facade budget finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # With no declaration the rule is inert, however large the facade is.
        matrix, config = build_fixture(
            root,
            matrix_body=migrated_row,
            budget_section=facade_budget_section(None),
            roles=migrated_roles,
        )
        write_migrated_workflow(root, facade_lines=5000)
        findings = check_tree(root, matrix, config)
        if findings:
            raise AssertionError(f"expected an undeclared budget to be inert, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Fail-open 4 of 4: an annotated `facade:` line. The established way to
        # write this line names the measured size and the pre-convention file, so
        # it carries a second backticked token. Requiring exactly one token made
        # the whole rule inert for such a row — three of sixteen migrated rows
        # were unchecked this way, and the gate exited 0.
        annotated_roles = migrated_roles.replace(
            "- facade: `ui/search_panel/mod.rs`\n",
            "- facade: `ui/search_panel/mod.rs` — **401** physical lines of 400, "
            "down from a pre-convention `search_panel.rs`\n",
        )
        matrix, config = build_fixture(
            root,
            matrix_body=migrated_row,
            budget_section=facade_budget_section(400),
            roles=annotated_roles,
        )
        write_migrated_workflow(root, facade_lines=401)
        findings = check_tree(root, matrix, config)
        if not any(
            "is 401 lines and exceeds the normative facade line budget of 400" in finding
            for finding in findings
        ):
            raise AssertionError(
                f"expected an annotated facade line to still be checked, got {findings}"
            )

        # A `facade:` line whose first token is not a source path still declines,
        # rather than guessing a path out of prose.
        matrix, config = build_fixture(
            root,
            matrix_body=migrated_row,
            budget_section=facade_budget_section(400),
            roles=migrated_roles.replace(
                "- facade: `ui/search_panel/mod.rs`\n",
                "- facade: `none` — this workflow owns no facade\n",
            ),
        )
        write_migrated_workflow(root, facade_lines=5000)
        findings = check_tree(root, matrix, config)
        if any("facade line budget" in finding for finding in findings):
            raise AssertionError(
                f"expected a non-path facade claim to decline, got {findings}"
            )

    # Rule 6 needs a matrix with a `Slot` column, because an outstanding row is
    # one that carries a migration slot and is not settled.
    slotted_header = (
        "\n## Product Matrix\n\n"
        "| Row id | Workflow | Risk | Slot | Status |\n"
        "| --- | --- | --- | --- | --- |\n"
    )

    def slotted_fixture(root: Path, body: str, ledger: str | None) -> tuple[Path, Path, Path]:
        """Build a matrix with a `Slot` column plus an optional programme record."""
        write(root / ".cargo/mutants.toml", MINIMAL_MUTANTS_CONFIG)
        matrix = root / "docs/workflow-readability-matrix.md"
        write(matrix, "# Fixture Matrix\n" + slotted_header + body + migrated_roles)
        write(root / CORE_SRC / "ui/search_panel/mod.rs", "pub struct Panel;\n")
        write(root / CORE_SRC / "ui/search_panel/evidence.rs", "pub struct Facts;\n")
        record = root / "docs/next/workflow-readability.md"
        if ledger is not None:
            write(
                record,
                "# Fixture Record\n\nDeclared as:\n\n```\n"
                "- slot <n> (complete|outstanding): <WFR-ID>\n```\n\n" + ledger,
            )
        return matrix, root / ".cargo/mutants.toml", record

    agreeing_body = (
        "| WFR-EXAMPLE | Example | tier-2 | 1 | migrated |\n"
        "| WFR-PENDING | Pending | tier-3 | 2 | pending |\n"
        "| WFR-SHARED | Shared | tier-3 | none | cross-cutting |\n"
    )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Agreement passes: the complete slot names the migrated row, the
        # outstanding slot names every unsettled slotted row, and a `none`-slot
        # row owes no ledger entry.
        matrix, config, record = slotted_fixture(
            root,
            agreeing_body,
            "- slot 1 (complete): WFR-EXAMPLE\n- slot 2 (outstanding): WFR-PENDING\n",
        )
        findings = check_tree(root, matrix, config, record)
        if findings:
            raise AssertionError(f"expected an agreeing record to pass, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A slot declared complete whose row the matrix still lists as pending.
        matrix, config, record = slotted_fixture(
            root,
            agreeing_body,
            "- slot 1 (complete): WFR-EXAMPLE, WFR-PENDING\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any("is declared complete but WFR-PENDING is `pending`" in f for f in findings):
            raise AssertionError(
                f"expected a complete-but-pending finding, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A terminal non-migrating row may be named on a *complete* line. The
        # gate used to demand `migrated` here, which such a row can never reach,
        # so the only way to pass was to record settled work as outstanding.
        cross_cutting_body = (
            "| WFR-EXAMPLE | Example | `model/example_policy.rs` | migrated |\n"
            "| WFR-LANE | Lane | 1 | cross-cutting |\n"
        )
        matrix, config, record = slotted_fixture(
            root,
            cross_cutting_body,
            "- slot 1 (complete): WFR-EXAMPLE, WFR-LANE\n",
        )
        findings = check_tree(root, matrix, config, record)
        if any("is declared complete but WFR-LANE" in f for f in findings):
            raise AssertionError(
                f"a cross-cutting lane must be nameable on a complete line, got {findings}"
            )
        if any("WFR-LANE" in f and "no outstanding slot" in f for f in findings):
            raise AssertionError(
                f"a row named on a complete line is accounted for, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # But a *pending* row on a complete line is still a false claim.
        pending_on_complete = (
            "| WFR-EXAMPLE | Example | `model/example_policy.rs` | migrated |\n"
            "| WFR-PENDING | Pending | 1 | pending |\n"
        )
        matrix, config, record = slotted_fixture(
            root,
            pending_on_complete,
            "- slot 1 (complete): WFR-EXAMPLE, WFR-PENDING\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any("is declared complete but WFR-PENDING is `pending`" in f for f in findings):
            raise AssertionError(
                f"a pending row on a complete line must still fail, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # The matrix has an unsettled slotted row the ledger never mentions.
        matrix, config, record = slotted_fixture(
            root, agreeing_body, "- slot 1 (complete): WFR-EXAMPLE\n"
        )
        findings = check_tree(root, matrix, config, record)
        if not any("lists it in no outstanding slot" in f for f in findings):
            raise AssertionError(
                f"expected a missing-outstanding finding, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A migrated row may be listed outstanding only as `(partial)`.
        matrix, config, record = slotted_fixture(
            root,
            agreeing_body,
            "- slot 2 (outstanding): WFR-EXAMPLE, WFR-PENDING\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any("must be written as `WFR-EXAMPLE (partial)`" in f for f in findings):
            raise AssertionError(f"expected an unmarked-partial finding, got {findings}")

        matrix, config, record = slotted_fixture(
            root,
            agreeing_body,
            "- slot 1 (complete): WFR-EXAMPLE\n"
            "- slot 2 (outstanding): WFR-EXAMPLE (partial), WFR-PENDING\n",
        )
        findings = check_tree(root, matrix, config, record)
        if findings:
            raise AssertionError(
                f"expected a `(partial)` migrated row to pass, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # `(partial)` on a complete line exempts an incremental row from the
        # `migrated` requirement, so a row whose scope spans slots does not have
        # to be falsely marked migrated. It must still be listed outstanding.
        matrix, config, record = slotted_fixture(
            root,
            agreeing_body,
            "- slot 1 (complete): WFR-EXAMPLE, WFR-PENDING (partial)\n"
            "- slot 2 (outstanding): WFR-PENDING\n",
        )
        findings = check_tree(root, matrix, config, record)
        if findings:
            raise AssertionError(
                f"expected `(partial)` on a complete line to pass, got {findings}"
            )

        # A split slot keeps its number and takes a letter suffix, so the slot
        # label is not restricted to an integer.
        matrix, config, record = slotted_fixture(
            root,
            agreeing_body,
            "- slot 1 (complete): WFR-EXAMPLE\n"
            "- slot 2a (outstanding): WFR-PENDING\n",
        )
        findings = check_tree(root, matrix, config, record)
        if findings:
            raise AssertionError(f"expected a `2a` slot label to parse, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A ledger row id that no matrix row defines is a typo, not scope.
        matrix, config, record = slotted_fixture(
            root,
            agreeing_body,
            "- slot 1 (complete): WFR-EXAMPLE\n"
            "- slot 2 (outstanding): WFR-PENDING, WFR-TYPO\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any("WFR-TYPO, which has no row" in f for f in findings):
            raise AssertionError(f"expected an unknown-row finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A caller that passes a record path expects a record: absent must fail,
        # and a record with no ledger claims nothing checkable.
        matrix, config, record = slotted_fixture(root, agreeing_body, None)
        findings = check_tree(root, matrix, config, record)
        if not any("missing programme record" in f for f in findings):
            raise AssertionError(f"expected an absent-record finding, got {findings}")
        # Passing no record path at all leaves the rule inert, which is what the
        # fixtures above rely on while exercising rules 1 through 5.
        if any("programme record" in f for f in check_tree(root, matrix, config)):
            raise AssertionError("expected an omitted record path to be inert")

        write(record, "# Fixture Record\n\nNo ledger here.\n")
        findings = check_tree(root, matrix, config, record)
        if not any("no `- slot" in f for f in findings):
            raise AssertionError(f"expected an empty-ledger finding, got {findings}")

    # --- The three parsing-path fail-opens -----------------------------------
    #
    # These three arms exist because the rules above were implemented correctly
    # while the code deciding *what they see* could fail open. Each arm is the
    # deliberate red for one hole: before the fix, each fixture below passed
    # with zero findings while the rule it defeats was skipped entirely.

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Fail-open 1: reword the `Slot` header. Every row then parses with no
        # slot, so the outstanding-slot sweep is skipped and the ledger's
        # omission of WFR-PENDING goes unreported.
        matrix, config, record = slotted_fixture(
            root, agreeing_body, "- slot 1 (complete): WFR-EXAMPLE\n"
        )
        matrix.write_text(
            matrix.read_text(encoding="utf-8").replace(
                "| Row id | Workflow | Risk | Slot | Status |",
                "| Row id | Workflow | Risk | Migration slot | Status |",
            ),
            encoding="utf-8",
        )
        findings = check_tree(root, matrix, config, record)
        if not any("`Slot` column could not be located" in f for f in findings):
            raise AssertionError(
                f"expected a renamed `Slot` header to be a finding, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Fail-open 2: a typo'd verb drops that slot's whole claim. Here the
        # dropped line is the one that would have reported WFR-PENDING as
        # outstanding, so without the fix the matrix and ledger disagree at
        # exit 0.
        matrix, config, record = slotted_fixture(
            root,
            agreeing_body,
            "- slot 1 (complete): WFR-EXAMPLE\n"
            "- slot 2 (outstandng): WFR-PENDING\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any("does not parse as" in f for f in findings):
            raise AssertionError(
                f"expected a malformed ledger line to be a finding, got {findings}"
            )
        # Ordinary prose that mentions a slot must stay inert, or the fix trades
        # a fail-open for a false positive on every sentence in the record.
        matrix, config, record = slotted_fixture(
            root,
            agreeing_body,
            "- slot 1 (complete): WFR-EXAMPLE\n"
            "- slot 2 (outstanding): WFR-PENDING\n\n"
            "Prose: slot 2 covers the pending row, and slot 3 does not exist.\n"
            "- slots are numbered without letters until a split occurs.\n",
        )
        findings = check_tree(root, matrix, config, record)
        if findings:
            raise AssertionError(
                f"expected prose mentioning slots to stay inert, got {findings}"
            )
        # Punctuation, not only vocabulary: a ledger line can drop its claim by
        # losing its parenthetical or by growing a second one. Both parse as
        # prose under `SLOT_LEDGER_RE`, so only the shape detector stands
        # between them and a silently dropped slot. The doubled parenthetical is
        # also the arm that pins the shape rewrite's deliberate broadening --
        # the previous `\s*\(?[^)]*\)?\s*:` tail let it pass.
        for dropped in (
            "- slot 2 outstanding: WFR-PENDING\n",
            "- slot 2 (outstanding) (again): WFR-PENDING\n",
        ):
            matrix, config, record = slotted_fixture(
                root,
                agreeing_body,
                "- slot 1 (complete): WFR-EXAMPLE\n" + dropped,
            )
            findings = check_tree(root, matrix, config, record)
            if not any("does not parse as" in f for f in findings):
                raise AssertionError(
                    f"expected `{dropped.strip()}` to be reported as a dropped "
                    f"claim, got {findings}"
                )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Fail-open 3: `none` is legitimate for a terminal row (WFR-SHARED in
        # `agreeing_body` relies on it) but must not exempt a transitional one.
        matrix, config, record = slotted_fixture(
            root,
            "| WFR-EXAMPLE | Example | tier-2 | 1 | migrated |\n"
            "| WFR-ORPHAN | Orphan | tier-3 | none | pending |\n",
            "- slot 1 (complete): WFR-EXAMPLE\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any(
            "WFR-ORPHAN" in f and "migration slot `none`" in f for f in findings
        ):
            raise AssertionError(
                f"expected a `none`-slot transitional row to be a finding, got {findings}"
            )
        # And the legitimate case still passes: `none` on a terminal row.
        matrix, config, record = slotted_fixture(
            root, agreeing_body, "- slot 1 (complete): WFR-EXAMPLE\n"
            "- slot 2 (outstanding): WFR-PENDING\n"
        )
        findings = check_tree(root, matrix, config, record)
        if findings:
            raise AssertionError(
                f"expected `none` on a terminal row to stay legitimate, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Delta 1's mechanical half (slot 7b): a transitional status must not
        # survive the change that closes the programme. A ledger with no
        # `outstanding` slot is that closing statement. Deliberate red: the
        # same fixture with an `outstanding` slot present passes, so the arm
        # is testing the close condition rather than the row.
        matrix, config, record = slotted_fixture(
            root,
            "| WFR-EXAMPLE | Example | tier-2 | 1 | migrated |\n"
            "| WFR-LEFTOVER | Leftover | tier-3 | 1 | pending |\n",
            "- slot 1 (complete): WFR-EXAMPLE\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any(
            "WFR-LEFTOVER" in f and "transitional" in f and "closes the migration" in f
            for f in findings
        ):
            raise AssertionError(
                f"expected a transitional row to be a finding at programme close, got {findings}"
            )
        # With an outstanding slot naming it, the same row is legitimate: the
        # programme is still open and the ledger says where the work lives.
        matrix, config, record = slotted_fixture(
            root,
            "| WFR-EXAMPLE | Example | tier-2 | 1 | migrated |\n"
            "| WFR-LEFTOVER | Leftover | tier-3 | 1 | pending |\n",
            "- slot 1 (complete): WFR-EXAMPLE\n"
            "- slot 2 (outstanding): WFR-LEFTOVER\n",
        )
        findings = check_tree(root, matrix, config, record)
        if findings:
            raise AssertionError(
                f"expected a transitional row with an outstanding slot to pass, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # `superseded`'s counterpart obligation: the label exempts its row from
        # every role requirement, so it must name the rows that took the scope,
        # and each must exist. Without this, `superseded` is a way to stop being
        # checked.
        matrix, config, record = slotted_fixture(
            root,
            "| WFR-EXAMPLE | Example | tier-2 | 1 | migrated |\n"
            "| WFR-REPLACED | Replaced | tier-3 | none | superseded — **Superseded by:** `WFR-EXAMPLE`. |\n",
            "- slot 1 (complete): WFR-EXAMPLE\n",
        )
        findings = check_tree(root, matrix, config, record)
        if findings:
            raise AssertionError(
                f"expected a superseded row naming an existing replacement to pass, got {findings}"
            )
        # Names nothing -> finding.
        matrix, config, record = slotted_fixture(
            root,
            "| WFR-EXAMPLE | Example | tier-2 | 1 | migrated |\n"
            "| WFR-REPLACED | Replaced | tier-3 | none | superseded |\n",
            "- slot 1 (complete): WFR-EXAMPLE\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any("names no replacement row" in f for f in findings):
            raise AssertionError(
                f"expected a superseded row naming no replacement to be a finding, got {findings}"
            )
        # Names a replacement the matrix does not carry -> finding.
        matrix, config, record = slotted_fixture(
            root,
            "| WFR-EXAMPLE | Example | tier-2 | 1 | migrated |\n"
            "| WFR-REPLACED | Replaced | tier-3 | none | superseded — **Superseded by:** `WFR-GHOST`. |\n",
            "- slot 1 (complete): WFR-EXAMPLE\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any("WFR-GHOST" in f and "does not carry" in f for f in findings):
            raise AssertionError(
                f"expected an unknown replacement row to be a finding, got {findings}"
            )
        # A replacement sentence may name a path, and a path carries dots. The
        # capture has to reach the end of the sentence: here the ghost row is
        # named *after* `mod.rs`, so a capture that stopped at the first period
        # of any kind would verify only the first replacement and report
        # nothing. The `mod.rs` file is created because the matrix's own
        # evidence check existence-tests backticked path claims.
        write(root / CORE_SRC / "ui/window/tab_strip/mod.rs", "pub struct TabStrip;\n")
        matrix, config, record = slotted_fixture(
            root,
            "| WFR-EXAMPLE | Example | tier-2 | 1 | migrated |\n"
            "| WFR-REPLACED | Replaced | tier-3 | none | superseded — "
            "**Superseded by:** `WFR-EXAMPLE`, whose role home is "
            "`ui/window/tab_strip/mod.rs`, and `WFR-GHOST`. The row is kept. |\n",
            "- slot 1 (complete): WFR-EXAMPLE\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any("WFR-GHOST" in f and "does not carry" in f for f in findings):
            raise AssertionError(
                "expected a replacement named after a dotted path to be captured, "
                f"got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # `superseded` (slot 7b): a row that was *replaced* rather than migrated,
        # exempted, or shared. It must be accepted as a label, treated as
        # settled so the migrated-role rule does not fire on it, and treated as
        # terminal so a `none` slot on it is legitimate. Before the label was
        # added, the same row produced an unrecognized-label finding, which is
        # the deliberate red this arm reproduces from the other side.
        matrix, config, record = slotted_fixture(
            root,
            "| WFR-EXAMPLE | Example | tier-2 | 1 | migrated |\n"
            "| WFR-REPLACED | Replaced | tier-3 | none | superseded — **Superseded by:** `WFR-EXAMPLE`. |\n",
            "- slot 1 (complete): WFR-EXAMPLE\n",
        )
        findings = check_tree(root, matrix, config, record)
        if findings:
            raise AssertionError(
                f"expected a `superseded` row with slot `none` to pass, got {findings}"
            )
        # And the label must still be *checked*: a near-miss spelling is a
        # finding, not a silent exemption from the migrated-role rule.
        matrix, config, record = slotted_fixture(
            root,
            "| WFR-EXAMPLE | Example | tier-2 | 1 | migrated |\n"
            "| WFR-REPLACED | Replaced | tier-3 | none | supersceded — **Superseded by:** `WFR-EXAMPLE`. |\n",
            "- slot 1 (complete): WFR-EXAMPLE\n",
        )
        findings = check_tree(root, matrix, config, record)
        if not any("WFR-REPLACED" in f and "unrecognized label" in f for f in findings):
            raise AssertionError(
                f"expected a misspelled terminal label to be a finding, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # String literals and block comments name GTK types without importing them.
        matrix, config = build_fixture(root, matrix_body=clean_row)
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write(
            root / CORE_SRC / "ui/search_panel/policy.rs",
            '/* gtk4::Widget in a block comment is fine. */\n'
            'pub const HINT: &str = "gtk4::Widget in a literal is fine";\n'
            "pub fn decide() -> bool { true }\n",
        )
        findings = check_tree(root, matrix, config)
        if findings:
            raise AssertionError(f"expected non-code GTK mentions to pass, got {findings}")

    # --- Check 7: role-home declaration ------------------------------------

    home_row = "| WFR-EXAMPLE | Example | none | migrated |\n"

    def home_roles(extra: str = "") -> str:
        return (
            "\n## Migrated Workflow Roles\n\n### WFR-EXAMPLE\n\n"
            "- facade: `ui/search_panel/mod.rs`\n"
            "- coordination: `ui/search_panel/execution.rs`\n"
            "- policy: `ui/search_panel/policy.rs`\n"
            "- evidence: `ui/search_panel/evidence.rs`\n"
            "- mutation parity: none\n" + extra
        )

    def write_home(root: Path, *, extra_modules: tuple[str, ...] = ()) -> None:
        panel = root / CORE_SRC / "ui/search_panel"
        write(panel / "mod.rs", "//! Facade.\n")
        write(panel / "imp.rs", "//! Subclass state.\n")
        write(panel / "execution.rs", "//! Coordination.\n")
        write(panel / "policy.rs", "//! Pure.\npub fn decide() -> bool { true }\n")
        write(panel / "evidence.rs", "//! Surface.\npub struct Facts;\n")
        write(panel / "seams.rs", "//! Seams.\npub struct Ticket;\n")
        write(panel / "test_policy.rs", "//! Test policy.\npub struct Policy;\n")
        write(panel / "replace_execution.rs", "//! Stage-qualified coordination.\n")
        for name in extra_modules:
            write(panel / name, "//! A called presentation surface.\n")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Negative: a home holding only convention names plus `mod`/`imp` passes.
        matrix, config = build_fixture(root, matrix_body=home_row, roles=home_roles())
        write_home(root)
        findings = check_tree(root, matrix, config)
        if any("undeclared module in its role home" in f for f in findings):
            raise AssertionError(f"expected a conventional home to pass, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Positive: an extra module the row names nowhere is a finding.
        matrix, config = build_fixture(root, matrix_body=home_row, roles=home_roles())
        write_home(root, extra_modules=("list_factory.rs",))
        findings = check_tree(root, matrix, config)
        if not any(
            "undeclared module in its role home" in f and "list_factory.rs" in f
            for f in findings
        ):
            raise AssertionError(f"expected an undeclared-module finding, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Declaring it by a backticked repository path clears the finding, and a
        # bare stem does not -- the declaration has to be machine-readable.
        matrix, config = build_fixture(
            root,
            matrix_body=home_row,
            roles=home_roles(
                "- called presentation surfaces: `ui/search_panel/list_factory.rs`\n"
            ),
        )
        write_home(root, extra_modules=("list_factory.rs", "results.rs"))
        findings = check_tree(root, matrix, config)
        if any("list_factory.rs" in f for f in findings):
            raise AssertionError(f"expected a declared path to pass, got {findings}")
        if not any("results.rs" in f for f in findings):
            raise AssertionError(
                f"expected the still-undeclared sibling to fail, got {findings}"
            )

        matrix, config = build_fixture(
            root,
            matrix_body=home_row,
            roles=home_roles("- called presentation surfaces: `list_factory.rs`\n"),
        )
        findings = check_tree(root, matrix, config)
        if not any("list_factory.rs" in f for f in findings):
            raise AssertionError(
                f"expected a bare stem to leave the module undeclared, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # A subdirectory the row names through a declared role IS a nested home;
        # one it never names is outside this row's responsibility.
        matrix, config = build_fixture(
            root,
            matrix_body=home_row,
            roles=home_roles(
                "- watch: `ui/search_panel/section/watch.rs`\n"
            ),
        )
        write_home(root)
        write(root / CORE_SRC / "ui/search_panel/section/watch.rs", "//! Watch.\n")
        write(root / CORE_SRC / "ui/search_panel/section/row_factory.rs", "//! Rows.\n")
        write(root / CORE_SRC / "ui/search_panel/neighbour/other.rs", "//! Elsewhere.\n")
        findings = check_tree(root, matrix, config)
        if not any("section/row_factory.rs" in f for f in findings):
            raise AssertionError(
                f"expected the named nested home to be enumerated, got {findings}"
            )
        if any("neighbour/other.rs" in f for f in findings):
            raise AssertionError(
                f"expected an unnamed subdirectory to stay out of scope, got {findings}"
            )

    # --- Check 8: the externally reachable test-seam ratchet ----------------

    def measurement_section(ceiling: int | None) -> str:
        declaration = (
            ""
            if ceiling is None
            else f"- externally reachable `\\*_for_test` declaration ceiling: {ceiling}\n"
        )
        return (
            "\n## Measurement Definitions\n\n"
            "### Externally reachable test-seam ceiling\n\nDeclared as:\n\n```\n"
            "- externally reachable `\\*_for_test` declaration ceiling: <integer>\n"
            f"```\n\n{declaration}"
        )

    def write_seams(root: Path, count: int) -> None:
        # The fixture declares a role in its module doc so Check 3's discovery
        # half does not also fire on a GTK-free file full of `fn`s.
        #
        # The visibilities are mixed on purpose, and the count is asserted rather
        # than assumed. A fixture built only from `pub fn` would still produce
        # every expected verdict if the predicate were narrowed to `pub\s+fn`, so
        # the ratchet would read as proven while silently ignoring every
        # `pub(crate)` seam; and `pub(super)` is outside the predicate by design,
        # so one must be present to prove it is *not* counted.
        declarations = [
            f"pub fn probe_{index}_for_test() -> bool {{ true }}"
            for index in range(count - 1)
        ]
        declarations.append(
            f"pub(crate) fn probe_{count - 1}_for_test() -> bool {{ true }}"
        )
        body = (
            "//! Role: called presentation surface.\n"
            + "".join(f"{line}\n" for line in declarations)
            + "pub(super) fn uncounted_for_test() -> bool { true }\n"
        )
        write(root / CORE_SRC / "ui/search_panel/imp.rs", body)
        observed = count_for_test_declarations(root)
        if observed != count:
            raise AssertionError(
                f"fixture must declare exactly {count} externally reachable seams "
                f"({count - 1} `pub` plus 1 `pub(crate)`, alongside 1 uncounted "
                f"`pub(super)`), but the predicate counted {observed}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Equal to the ceiling passes; the ratchet fails only on excess.
        matrix, config = build_fixture(
            root, matrix_body=clean_row, budget_section=measurement_section(2)
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write_seams(root, 2)
        findings = check_tree(root, matrix, config)
        if any("exceed the recorded ceiling" in f for f in findings):
            raise AssertionError(f"expected an at-ceiling count to pass, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Below the ceiling passes without a finding: a cleanup must not be
        # required to also edit the matrix, or the figure becomes a number to
        # adjust rather than a ceiling to stay under.
        matrix, config = build_fixture(
            root, matrix_body=clean_row, budget_section=measurement_section(5)
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write_seams(root, 1)
        findings = check_tree(root, matrix, config)
        if findings:
            raise AssertionError(f"expected a below-ceiling count to pass, got {findings}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # Above the ceiling fails, and the message names both remedies in order.
        matrix, config = build_fixture(
            root, matrix_body=clean_row, budget_section=measurement_section(2)
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write_seams(root, 3)
        findings = check_tree(root, matrix, config)
        breach = [f for f in findings if "exceed the recorded ceiling" in f]
        if not breach:
            raise AssertionError(f"expected an over-ceiling finding, got {findings}")
        if "evidence surface" not in breach[0] or "raise the ceiling" not in breach[0]:
            raise AssertionError(f"expected both remedies named, got {breach[0]!r}")

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # `pub(super)` is outside the predicate, and `cfg(feature = "test-utils")`
        # sites are deliberately not ratcheted -- that population rises as
        # workflows gain gated evidence surfaces.
        matrix, config = build_fixture(
            root, matrix_body=clean_row, budget_section=measurement_section(0)
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write(
            root / CORE_SRC / "ui/search_panel/imp.rs",
            "//! Role: called presentation surface.\n"
            '#[cfg(feature = "test-utils")]\n'
            "pub(super) fn hidden_for_test() -> bool { true }\n"
            '#[cfg(feature = "test-utils")]\n'
            "pub(super) fn another_for_test() -> bool { true }\n",
        )
        findings = check_tree(root, matrix, config)
        if findings:
            raise AssertionError(
                f"expected `pub(super)` seams and gate sites to be outside the "
                f"ratchet, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # No declaration leaves the rule inert for a fixture exercising another
        # rule -- but the real-tree entry point, which always passes a record
        # path, reports it rather than silently retiring the ratchet.
        matrix, config = build_fixture(root, matrix_body=clean_row)
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write_seams(root, 99)
        findings = check_tree(root, matrix, config)
        if findings:
            raise AssertionError(f"expected an undeclared ceiling to be inert, got {findings}")

        findings = check_tree(root, matrix, config, require_seam_ceiling=True)
        if not any("ratchet checked nothing" in finding for finding in findings):
            raise AssertionError(
                f"expected a missing ceiling to be reported on the real-tree entry "
                f"point, got {findings}"
            )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        # And the declaration must be read from its own subsection: keying on the
        # parent `## Measurement Definitions` looks equivalent and silently made
        # the rule inert on the real matrix, whose declaration sits under a `###`.
        matrix, config = build_fixture(
            root,
            matrix_body=clean_row,
            budget_section=(
                "\n## Measurement Definitions\n\n"
                "- externally reachable `\\*_for_test` declaration ceiling: 1\n"
            ),
        )
        write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
        write_seams(root, 3)
        findings = check_tree(root, matrix, config, require_seam_ceiling=True)
        if not any("ratchet checked nothing" in finding for finding in findings):
            raise AssertionError(
                f"expected a declaration outside the ceiling's own subsection to be "
                f"unparsed and reported, got {findings}"
            )

    # --- Kani harness modules ----------------------------------------------
    #
    # A `kani_proofs.rs` is verification code: its parent declares it
    # `#[cfg(kani)] mod kani_proofs;`, so no ordinary build compiles it. Both the
    # discovery check and the role-home check recognise it, but only when that
    # gate is really there; an ungated or orphaned harness file, and a
    # same-content file under any other name, stay findings.
    harness = (
        "//! Kani harnesses over the example policy.\n"
        "use super::*;\n#[kani::proof]\nfn decision_holds() { assert!(decide()); }\n"
    )

    def harness_findings(parent: str | None, name: str = "kani_proofs.rs") -> list[str]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            matrix, config = build_fixture(root, matrix_body=clean_row)
            write(root / CORE_SRC / "model/example_policy.rs", "pub fn ok() {}\n")
            policy = root / CORE_SRC / "ui/example/policy.rs"
            if parent is not None:
                write(policy, "//! Pure.\npub fn decide() -> bool { true }\n" + parent)
            write(root / CORE_SRC / "ui/example/policy" / name, harness)
            return [f for f in check_tree(root, matrix, config) if name in f]

    gated = harness_findings("#[cfg(kani)]\nmod kani_proofs;\n")
    if gated:
        raise AssertionError(f"expected a cfg(kani)-gated harness to pass, got {gated}")
    if not harness_findings("mod kani_proofs;\n"):
        raise AssertionError("expected an ungated kani_proofs.rs to be a finding")
    if not harness_findings(None):
        raise AssertionError("expected a kani_proofs.rs with no parent module to be a finding")
    if not harness_findings("#[cfg(kani)]\nmod proofs;\n", name="proofs.rs"):
        raise AssertionError("expected harness content under another name to stay a finding")

    def home_harness_findings(facade: str) -> list[str]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            matrix, config = build_fixture(root, matrix_body=home_row, roles=home_roles())
            write_home(root)
            panel = root / CORE_SRC / "ui/search_panel"
            write(panel / "mod.rs", "//! Facade.\n" + facade)
            write(panel / "kani_proofs.rs", harness)
            return [
                f
                for f in check_tree(root, matrix, config)
                if "kani_proofs.rs" in f
            ]

    for name, message in whole_pixel_self_test_failures():
        raise AssertionError(f"whole-pixel self-test `{name}`: {message}")

    home_gated = home_harness_findings("#[cfg(kani)]\nmod kani_proofs;\n")
    if home_gated:
        raise AssertionError(f"expected a gated harness in a role home to pass, got {home_gated}")
    if not home_harness_findings("mod kani_proofs;\n"):
        raise AssertionError("expected an ungated harness in a role home to be a finding")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="run built-in fixture tests before checking the current tree",
    )
    args = parser.parse_args()

    if args.self_test:
        run_self_test()

    findings = check_tree(
        REPO_ROOT,
        MATRIX_PATH,
        MUTANTS_CONFIG_PATH,
        RECORD_PATH,
        require_seam_ceiling=True,
    )
    if findings:
        print("workflow boundary policy violations:")
        for finding in findings:
            print(f"  - {finding}")
        return 1

    modules = policy_modules(REPO_ROOT)
    print(
        "workflow boundary policy passed: "
        f"{len(modules)} workflow policy module(s) are pure and mutation-scoped, "
        "every migrated matrix row names complete, existing roles, and the programme "
        "record's slot ledger agrees with the matrix"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
