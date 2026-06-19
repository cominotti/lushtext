#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Diff-aware policy checks for new GTK accessibility-sensitive UI edits."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

from accessibility_source_fingerprint import source_fingerprint


REPO_ROOT = Path(__file__).resolve().parents[1]


@dataclass(frozen=True)
class AddedLine:
    """One added line from a tracked diff or untracked policy-sensitive file."""

    path: str
    line_no: int
    text: str


@dataclass(frozen=True)
class Finding:
    """One policy violation reported with a stable file and line."""

    path: str
    line_no: int
    message: str


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run script self-tests first")
    parser.add_argument(
        "--strict-current-tree",
        action="store_true",
        help="also inspect the current tree for helper bypasses and matrix/docs drift",
    )
    args = parser.parse_args()

    if args.self_test:
        run_self_test()

    added_lines = collect_added_lines()
    findings = check_added_lines(added_lines, current_file_texts(added_lines))
    if args.strict_current_tree:
        findings.extend(check_current_tree())
    if findings:
        print("Accessibility policy check failed:", file=sys.stderr)
        for finding in findings:
            print(
                f"{finding.path}:{finding.line_no}: {finding.message}",
                file=sys.stderr,
            )
        return 1

    current_tree = " and current-tree guardrails" if args.strict_current_tree else ""
    print(
        f"PASS: accessibility policy checked {len(added_lines)} added UI-sensitive lines"
        f"{current_tree}"
    )
    return 0


def run_git(args: list[str]) -> str:
    return subprocess.run(
        ["git", *args],
        cwd=REPO_ROOT,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    ).stdout


def collect_added_lines() -> list[AddedLine]:
    added = added_lines_from_diff(run_git(["diff", "--unified=0", "--no-ext-diff", "--"]))

    untracked = run_git(["ls-files", "--others", "--exclude-standard", "-z"])
    for raw_path in filter(None, untracked.split("\0")):
        if not is_policy_path(raw_path):
            continue
        path = REPO_ROOT / raw_path
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for line_no, line in enumerate(text.splitlines(), start=1):
            added.append(AddedLine(raw_path, line_no, line))

    return [line for line in added if is_policy_path(line.path)]


def added_lines_from_diff(diff: str) -> list[AddedLine]:
    added: list[AddedLine] = []
    current_path: str | None = None
    new_line_no = 0

    for line in diff.splitlines():
        if line.startswith("+++ b/"):
            current_path = line.removeprefix("+++ b/")
            continue
        if line.startswith("+++ /dev/null"):
            current_path = None
            continue

        hunk = re.match(r"@@ -\d+(?:,\d+)? \+(\d+)(?:,\d+)? @@", line)
        if hunk:
            new_line_no = int(hunk.group(1))
            continue

        if current_path is None:
            continue

        if line.startswith("+") and not line.startswith("+++"):
            added.append(AddedLine(current_path, new_line_no, line[1:]))
            new_line_no += 1
        elif line.startswith("-") and not line.startswith("---"):
            continue
        elif line:
            new_line_no += 1

    return added


def is_policy_path(path: str) -> bool:
    return (
        (path.startswith("crates/lushtext-core/src/ui/") and path.endswith(".rs"))
        or (path.startswith("crates/lushtext/tests/widget/") and path.endswith(".rs"))
        or (path.startswith("resources/ui/") and path.endswith((".blp", ".ui")))
        or path == "scripts/run-accessibility-smoke.sh"
    )


def current_file_texts(added_lines: list[AddedLine]) -> dict[str, str]:
    texts: dict[str, str] = {}
    for path in {line.path for line in added_lines}:
        full_path = REPO_ROOT / path
        if full_path.is_file():
            texts[path] = full_path.read_text(encoding="utf-8")
        else:
            texts[path] = ""
    return texts


def check_added_lines(added_lines: list[AddedLine], file_texts: dict[str, str]) -> list[Finding]:
    findings: list[Finding] = []
    by_file: dict[str, list[AddedLine]] = {}
    for line in added_lines:
        by_file.setdefault(line.path, []).append(line)

    for path, lines in by_file.items():
        file_text = file_texts.get(path, "")
        added_text = "\n".join(line.text for line in lines)
        check_direct_accessibility_calls(path, lines, findings)
        check_row_factory_policy(path, lines, added_text, file_text, findings)
        check_icon_hover_transient_policy(path, lines, added_text, file_text, findings)
        check_atspi_anchor_policy(path, lines, findings)

    return findings


def check_current_tree() -> list[Finding]:
    findings: list[Finding] = []
    check_current_direct_accessibility_calls(findings)
    check_current_row_factory_policy(findings)
    check_smoke_matrix_contracts(findings)
    check_manual_orca_contract(findings)
    check_smoke_summary_freshness(findings)
    return findings


def check_current_direct_accessibility_calls(findings: list[Finding]) -> None:
    """Keep app-owned metadata behind the helper in the current production UI."""

    ui_root = REPO_ROOT / "crates/lushtext-core/src/ui"
    direct_patterns = (
        "set_accessible_role",
        "gtk4::accessible::Property::",
        ".update_state(&",
        ".update_relation(&",
        ".announce(",
    )
    for path in sorted(ui_root.rglob("*.rs")):
        rel_path = path.relative_to(REPO_ROOT).as_posix()
        if rel_path.endswith("/accessibility.rs"):
            continue
        text = read_text(path)
        for line_no, line in enumerate(text.splitlines(), start=1):
            if any(pattern in line for pattern in direct_patterns):
                findings.append(
                    Finding(
                        rel_path,
                        line_no,
                        "current-tree direct GTK accessibility calls must route through ui::accessibility or be documented in a narrow allowlist",
                    )
                )


def check_current_row_factory_policy(findings: list[Finding]) -> None:
    """Require recycled GTK row factories to use the shared apply/clear helpers."""

    ui_root = REPO_ROOT / "crates/lushtext-core/src/ui"
    for path in sorted(ui_root.rglob("*.rs")):
        rel_path = path.relative_to(REPO_ROOT).as_posix()
        text = read_text(path)
        if "SignalListItemFactory" not in text and ".connect_bind(" not in text:
            continue
        if "RowAccessibility" in text and "clear_row_accessibility" in text:
            continue
        line_no = first_line_number(text, ("SignalListItemFactory", ".connect_bind("))
        findings.append(
            Finding(
                rel_path,
                line_no,
                "current-tree list factories must apply row accessibility metadata and clear stale row metadata on unbind/reuse",
            )
        )


def check_smoke_matrix_contracts(findings: list[Finding]) -> None:
    accessibility_smoke = REPO_ROOT / "scripts/run-accessibility-smoke.sh"
    visual_smoke = REPO_ROOT / "scripts/run-visual-smoke.sh"
    matrix_doc = REPO_ROOT / "docs/accessibility-matrix.md"
    automation_doc = REPO_ROOT / "docs/automation-reference.md"

    accessibility_text = read_text(accessibility_smoke)
    visual_text = read_text(visual_smoke)
    matrix_text = read_text(matrix_doc)
    automation_text = read_text(automation_doc)

    matrix_rows = set(re.findall(r"\bA11Y-[A-Z0-9-]+\b", matrix_text))
    accessibility_cases = parse_bash_array(accessibility_text, "ACCESSIBILITY_CASES")
    accessibility_rows_by_case = parse_matrix_rows_by_case(accessibility_text)
    visual_rows_by_case = parse_matrix_rows_by_case(visual_text)

    for case in accessibility_cases:
        rows = accessibility_rows_by_case.get(case, [])
        if not rows:
            findings.append(
                Finding(
                    "scripts/run-accessibility-smoke.sh",
                    line_for_case(accessibility_text, case),
                    f"accessibility smoke case `{case}` must declare accessibility matrix row ids",
                )
            )
        for row in rows:
            if row not in matrix_rows:
                findings.append(
                    Finding(
                        "scripts/run-accessibility-smoke.sh",
                        line_for_case(accessibility_text, case),
                        f"accessibility smoke case `{case}` references unknown matrix row `{row}`",
                    )
                )
        crosswalk_row = f"| `{case}` |"
        if crosswalk_row not in matrix_text:
            findings.append(
                Finding(
                    "docs/accessibility-matrix.md",
                    1,
                    f"accessibility smoke crosswalk is missing case `{case}`",
                )
            )

    for case, rows in visual_rows_by_case.items():
        for row in rows:
            if row not in matrix_rows:
                findings.append(
                    Finding(
                        "scripts/run-visual-smoke.sh",
                        line_for_case(visual_text, case),
                        f"visual smoke case `{case}` references unknown matrix row `{row}`",
                    )
                )

    required_summary_terms = (
        '"case_filters"',
        '"focused_run"',
        '"matrix_coverage"',
        '"source_fingerprint"',
        '"warnings"',
        '"unexpected_count"',
        '"fixture_data"',
        '"private_user_data"',
        '"host_caveats"',
    )
    for term in required_summary_terms:
        if term not in accessibility_text:
            findings.append(
                Finding(
                    "scripts/run-accessibility-smoke.sh",
                    1,
                    f"accessibility smoke manifests or summaries must include `{term}`",
                )
            )

    required_doc_terms = (
        "scenario-manifest-field-matrix-rows",
        "scenario-manifest-field-assertions",
        "scenario-manifest-field-anchor-scope",
        "scenario-manifest-field-artifact-boundary",
        "scenario-manifest-field-host-caveats",
        "--case PATTERN",
        "--list-cases",
    )
    for term in required_doc_terms:
        if term not in automation_text:
            findings.append(
                Finding(
                    "docs/automation-reference.md",
                    1,
                    f"automation reference is missing accessibility smoke contract term `{term}`",
                )
            )


def check_smoke_summary_freshness(findings: list[Finding]) -> None:
    current_fingerprint = source_fingerprint(REPO_ROOT)
    current_digest = current_fingerprint.get("sha256")
    summaries = (
        (
            REPO_ROOT / "build/smoke/accessibility/summary.json",
            "accessibility-smoke",
            "make accessibility-smoke",
        ),
        (
            REPO_ROOT / "build/smoke/visual/summary.json",
            "visual-smoke",
            "make visual-smoke",
        ),
    )
    for path, lane, command in summaries:
        if not path.is_file():
            continue
        summary = read_json_object(path)
        rel_path = path.relative_to(REPO_ROOT).as_posix()
        if summary is None:
            findings.append(Finding(rel_path, 1, "smoke summary is malformed JSON"))
            continue

        for message in smoke_summary_release_issues(summary, lane, command, current_digest):
            findings.append(Finding(rel_path, 1, message))


def smoke_summary_release_issues(
    summary: dict[str, object],
    lane: str,
    command: str,
    current_digest: object,
) -> list[str]:
    issues: list[str] = []
    if summary.get("lane") != lane:
        issues.append(f"smoke summary lane must be `{lane}` for release proof")
    if summary.get("status") != "passed":
        issues.append(f"smoke summary status is not passed; rerun `{command}`")
    if summary.get("case_filters") != ["all"]:
        issues.append(
            f"focused smoke summary cannot satisfy release proof; rerun unfiltered `{command}`"
        )

    matrix = summary.get("matrix_coverage")
    if not isinstance(matrix, dict) or matrix.get("focused_run") is not False:
        issues.append("smoke summary must record matrix_coverage.focused_run=false for release proof")
    warnings = summary.get("warnings")
    if not isinstance(warnings, dict) or warnings.get("unexpected_count") != 0:
        issues.append("smoke summary must report zero unexpected warnings")

    recorded = summary.get("source_fingerprint")
    recorded_digest = recorded.get("sha256") if isinstance(recorded, dict) else None
    if recorded_digest != current_digest:
        issues.append(
            "smoke summary source_fingerprint does not match the current accessibility-sensitive tree; rerun the smoke lane"
        )
    return issues


def check_manual_orca_contract(findings: list[Finding]) -> None:
    checklist = REPO_ROOT / "docs/accessibility-orca-checklist.md"
    accessibility_doc = REPO_ROOT / "docs/accessibility.md"
    coverage_doc = REPO_ROOT / "docs/end-user-coverage.md"

    if not checklist.is_file():
        findings.append(
            Finding(
                "docs/accessibility-orca-checklist.md",
                1,
                "manual Orca validation template is required for release-grade accessibility evidence",
            )
        )
        return

    checklist_text = read_text(checklist)
    required_terms = (
        "LushText build",
        "Install mode",
        "Operating system",
        "GNOME session",
        "Display backend",
        "Theme",
        "Text scale",
        "Orca version",
        "Matrix rows",
        "Automated artifacts",
        "Outcome",
        "Caveats",
        "Synthetic fixture",
        "Private user data",
    )
    for term in required_terms:
        if term not in checklist_text:
            findings.append(
                Finding(
                    "docs/accessibility-orca-checklist.md",
                    1,
                    f"manual Orca checklist is missing required field `{term}`",
                )
            )

    for doc in (accessibility_doc, coverage_doc):
        text = read_text(doc)
        if "accessibility-orca-checklist.md" not in text:
            findings.append(
                Finding(
                    doc.relative_to(REPO_ROOT).as_posix(),
                    1,
                    "accessibility release guidance must link the manual Orca checklist template",
                )
            )


def check_direct_accessibility_calls(
    path: str,
    lines: list[AddedLine],
    findings: list[Finding],
) -> None:
    if path.endswith("/accessibility.rs"):
        return

    direct_patterns = (
        "gtk4::accessible::Property::Label",
        "gtk4::accessible::Property::Description",
        ".update_state(&",
        ".update_relation(&",
        ".announce(",
    )
    for line in lines:
        if "test_accessible_" in line.text:
            continue
        if any(pattern in line.text for pattern in direct_patterns):
            findings.append(
                Finding(
                    line.path,
                    line.line_no,
                    "new accessible labels, descriptions, states, relations, and announcements must go through ui::accessibility helpers",
                )
            )


def check_row_factory_policy(
    path: str,
    lines: list[AddedLine],
    added_text: str,
    file_text: str,
    findings: list[Finding],
) -> None:
    if not path.startswith("crates/lushtext-core/src/ui/"):
        return
    if "SignalListItemFactory" not in added_text and "connect_bind" not in added_text:
        return
    if "RowAccessibility" in file_text and "clear_row_accessibility" in file_text:
        return
    first = first_matching(lines, ("SignalListItemFactory", "connect_bind"))
    findings.append(
        Finding(
            path,
            first.line_no if first else 1,
            "new or changed list factories must refresh row accessibility metadata on bind and clear it on unbind",
        )
    )


def check_icon_hover_transient_policy(
    path: str,
    lines: list[AddedLine],
    added_text: str,
    file_text: str,
    findings: list[Finding],
) -> None:
    if path.endswith("/accessibility.rs"):
        return

    accessibility_keywords = (
        "accessibility::",
        "accessible",
        "tooltip",
        "keyboard",
        "context menu",
        "Shortcut",
    )

    icon = first_matching(lines, ("set_icon_name", "from_icon_name", "icon-name", "GtkImage"))
    if icon and not contains_any(added_text, accessibility_keywords):
        findings.append(
            Finding(
                path,
                icon.line_no,
                "new icon-only controls need an accessible label/description or visible tooltip in the same change",
            )
        )

    hover = first_matching(lines, ("EventControllerMotion", "connect_enter", "connect_leave", "hover"))
    if hover and not contains_any(file_text, ("keyboard", "context menu", "accessib")):
        findings.append(
            Finding(
                path,
                hover.line_no,
                "new hover affordances need keyboard or context-menu parity plus accessible metadata",
            )
        )

    transient = first_matching_regex(
        lines,
        (
            r"\b(?:gtk4::|adw::)?(?:Popover|Dialog|Revealer)\b",
            r"\b(?:Popover|Dialog|Revealer)::",
            r"\bGtk(?:Popover|Dialog|Revealer)\b",
        ),
    )
    if transient and not contains_any(file_text, ("accessibility::", "accessible", "set_label")):
        findings.append(
            Finding(
                path,
                transient.line_no,
                "new transient surfaces need stable accessible names and dismissal/focus proof",
            )
        )


def check_atspi_anchor_policy(path: str, lines: list[AddedLine], findings: list[Finding]) -> None:
    if path != "scripts/run-accessibility-smoke.sh":
        return
    for line in lines:
        text = line.text.strip()
        if text.startswith("assert_anchor ") and len(re.findall(r'"[^"]+"', text)) < 4:
            findings.append(
                Finding(
                    path,
                    line.line_no,
                    "AT-SPI anchors must include capture, surface, role, and accessible name",
                )
            )


def first_matching(lines: list[AddedLine], needles: tuple[str, ...]) -> AddedLine | None:
    return next((line for line in lines if contains_any(line.text, needles)), None)


def first_matching_regex(lines: list[AddedLine], patterns: tuple[str, ...]) -> AddedLine | None:
    return next(
        (line for line in lines if any(re.search(pattern, line.text) for pattern in patterns)),
        None,
    )


def contains_any(text: str, needles: tuple[str, ...]) -> bool:
    lowered = text.lower()
    return any(needle.lower() in lowered for needle in needles)


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""


def read_json_object(path: Path) -> dict[str, object] | None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    return value if isinstance(value, dict) else None


def first_line_number(text: str, needles: tuple[str, ...]) -> int:
    for line_no, line in enumerate(text.splitlines(), start=1):
        if contains_any(line, needles):
            return line_no
    return 1


def line_for_case(text: str, case: str) -> int:
    return first_line_number(text, (f'"{case}"', case))


def parse_bash_array(text: str, name: str) -> list[str]:
    match = re.search(rf"^{re.escape(name)}=\(\n(?P<body>.*?)^\)", text, re.MULTILINE | re.DOTALL)
    if not match:
        return []
    values: list[str] = []
    for raw_line in match.group("body").splitlines():
        line = raw_line.split("#", 1)[0].strip().strip('"').strip("'")
        if line:
            values.append(line)
    return values


def parse_matrix_rows_by_case(text: str) -> dict[str, list[str]]:
    rows: dict[str, list[str]] = {}
    for match in re.finditer(r'"(?P<case>[^"]+)":\s*\[(?P<body>[^\]]*)\]', text):
        case = match.group("case")
        body = match.group("body")
        matrix_rows = re.findall(r'"(A11Y-[A-Z0-9-]+)"', body)
        if matrix_rows:
            rows[case] = matrix_rows
    return rows


def run_self_test() -> None:
    cases = [
        (
            "direct label fails",
            [AddedLine("crates/lushtext-core/src/ui/foo.rs", 10, 'button.update_property(&[gtk4::accessible::Property::Label("Run")]);')],
            {"crates/lushtext-core/src/ui/foo.rs": ""},
            1,
        ),
        (
            "helper file may use raw GTK accessible calls",
            [AddedLine("crates/lushtext-core/src/ui/accessibility.rs", 10, "widget.update_state(&[]);")],
            {"crates/lushtext-core/src/ui/accessibility.rs": "widget.update_state(&[]);"},
            0,
        ),
        (
            "row factory without cleanup fails",
            [AddedLine("crates/lushtext-core/src/ui/foo.rs", 20, "factory.connect_bind(|_, _| {});")],
            {"crates/lushtext-core/src/ui/foo.rs": "factory.connect_bind(|_, _| {});"},
            1,
        ),
        (
            "row factory with helper passes",
            [AddedLine("crates/lushtext-core/src/ui/foo.rs", 20, "factory.connect_bind(|_, _| {});")],
            {
                "crates/lushtext-core/src/ui/foo.rs": "RowAccessibility::new(\"Row\"); clear_row_accessibility(&row);",
            },
            0,
        ),
        (
            "icon-only control without metadata fails",
            [AddedLine("resources/ui/foo.blp", 30, 'icon-name: "window-close-symbolic";')],
            {"resources/ui/foo.blp": 'icon-name: "window-close-symbolic";'},
            1,
        ),
        (
            "AT-SPI anchor without full tuple fails",
            [AddedLine("scripts/run-accessibility-smoke.sh", 40, 'assert_anchor "shell" "button" "Open"')],
            {"scripts/run-accessibility-smoke.sh": ""},
            1,
        ),
    ]
    for name, lines, texts, expected_count in cases:
        count = len(check_added_lines(lines, texts))
        if count != expected_count:
            raise AssertionError(f"{name}: expected {expected_count} findings, saw {count}")

    bash_array = 'ACCESSIBILITY_CASES=(\n    shell\n    "editor"\n)\n'
    if parse_bash_array(bash_array, "ACCESSIBILITY_CASES") != ["shell", "editor"]:
        raise AssertionError("bash array parser did not return expected smoke cases")

    rows_by_case = parse_matrix_rows_by_case(
        'MATRIX_ROWS_BY_CASE = {\n'
        '    "shell": ["A11Y-SHELL-NO-CONTEXT", "A11Y-SHELL-REPRESENTATIVE"],\n'
        '}\n'
    )
    if rows_by_case != {
        "shell": ["A11Y-SHELL-NO-CONTEXT", "A11Y-SHELL-REPRESENTATIVE"]
    }:
        raise AssertionError("matrix row parser did not return expected mapping")

    release_summary = {
        "schema_version": 1,
        "lane": "accessibility-smoke",
        "status": "passed",
        "case_filters": ["all"],
        "matrix_coverage": {"focused_run": False},
        "warnings": {"unexpected_count": 0},
        "source_fingerprint": {"sha256": "digest"},
    }
    if smoke_summary_release_issues(
        release_summary, "accessibility-smoke", "make accessibility-smoke", "digest"
    ):
        raise AssertionError("release-grade smoke summary fixture failed freshness checks")
    focused_summary = dict(release_summary)
    focused_summary["case_filters"] = ["editor"]
    focused_summary["matrix_coverage"] = {"focused_run": True}
    if len(
        smoke_summary_release_issues(
            focused_summary, "accessibility-smoke", "make accessibility-smoke", "digest"
        )
    ) != 2:
        raise AssertionError("focused smoke summary fixture did not trip release checks")
    stale_summary = dict(release_summary)
    stale_summary["source_fingerprint"] = {"sha256": "stale"}
    if len(
        smoke_summary_release_issues(
            stale_summary, "accessibility-smoke", "make accessibility-smoke", "digest"
        )
    ) != 1:
        raise AssertionError("stale smoke summary fixture did not trip freshness check")
    print("PASS: accessibility policy self-test")


if __name__ == "__main__":
    sys.exit(main())
