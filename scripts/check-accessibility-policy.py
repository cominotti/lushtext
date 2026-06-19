#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Diff-aware policy checks for new GTK accessibility-sensitive UI edits."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


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
    args = parser.parse_args()

    if args.self_test:
        run_self_test()

    added_lines = collect_added_lines()
    findings = check_added_lines(added_lines, current_file_texts(added_lines))
    if findings:
        print("Accessibility policy check failed:", file=sys.stderr)
        for finding in findings:
            print(
                f"{finding.path}:{finding.line_no}: {finding.message}",
                file=sys.stderr,
            )
        return 1

    print(f"PASS: accessibility policy checked {len(added_lines)} added UI-sensitive lines")
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
    print("PASS: accessibility policy self-test")


if __name__ == "__main__":
    sys.exit(main())
