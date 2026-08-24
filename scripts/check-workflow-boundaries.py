#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Enforce LushText's workflow readability boundary conventions.

Four mechanical guarantees, all derived from
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
KNOWN_STATUS_LABELS = (
    "pending",
    MIGRATED_STATUS,
    "partially-conforming",
    "exempt",
    "deferred",
    "cross-cutting",
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
ROLE_LINE_RE = re.compile(r"^-\s+([A-Za-z][A-Za-z ]*?):\s*(.+)$")
EXAMINE_GLOBS_RE = re.compile(r"^examine_globs\s*=\s*\[", re.MULTILINE)


@dataclass(frozen=True)
class MatrixRow:
    """One parsed `Product Matrix` row."""

    row_id: str
    line_number: int
    cells: tuple[str, ...]
    status: str


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
    return [entry for entry in re.findall(r'"([^"]+)"', body)]


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


def parse_matrix_rows(text: str) -> list[MatrixRow]:
    """Parse `Product Matrix` rows keyed by their stable row id."""
    rows: list[MatrixRow] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        cells = split_table_row(line)
        if len(cells) < 2 or not ROW_ID_RE.match(cells[0]):
            continue
        rows.append(
            MatrixRow(
                row_id=cells[0],
                line_number=line_number,
                cells=tuple(cells),
                status=parse_status(cells[-1]),
            )
        )
    return rows


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
    """Extract the single path a `facade:` role line claims, when it has one."""
    tokens = BACKTICKED_RE.findall(value)
    if len(tokens) != 1:
        return None
    return normalize_claim(tokens[0])


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


# --- Orchestration ----------------------------------------------------------


def check_tree(root: Path, matrix_path: Path, mutants_config: Path) -> list[str]:
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

    if not matrix_path.is_file():
        findings.append(f"missing workflow readability matrix: {display_path(matrix_path)}")
        return findings

    text = matrix_path.read_text(encoding="utf-8")
    rows = parse_matrix_rows(text)
    if not rows:
        findings.append(f"{display_path(matrix_path)}: no product matrix rows were parsed")
        return findings

    declarations = parse_role_declarations(text)
    findings.extend(status_findings(rows))
    findings.extend(role_findings(rows, declarations))
    findings.extend(evidence_findings(text, root))
    findings.extend(
        facade_size_findings(rows, declarations, parse_facade_budget(text), root)
    )
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


def run_self_test() -> None:
    """Prove each rule fires on a broken fixture and passes on a clean one."""
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

    findings = check_tree(REPO_ROOT, MATRIX_PATH, MUTANTS_CONFIG_PATH)
    if findings:
        print("workflow boundary policy violations:")
        for finding in findings:
            print(f"  - {finding}")
        return 1

    modules = policy_modules(REPO_ROOT)
    print(
        "workflow boundary policy passed: "
        f"{len(modules)} workflow policy module(s) are pure and mutation-scoped, "
        "and every migrated matrix row names complete, existing roles"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
