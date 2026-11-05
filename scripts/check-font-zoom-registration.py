#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Every application monospace widget follows the editor font and zoom.

`scope-font-zoom-restyle` keys the font rules by a zoom class that
`ui/font_zoom.rs` puts on the widgets it has registered, so a widget that
carries `.monospace` without registering keeps GTK's default font and stops
following the zoom and the custom font (design D2, D3). This check fails on
any way of adding `.monospace` outside the registering helper:

- Rust, under `crates/lushtext-core/src` (comments stripped):
  `add_css_class("monospace")`, `set_monospace(true)`, and a
  `css_classes([...])` list naming `"monospace"`, except inside
  `crates/lushtext-core/src/ui/font_zoom.rs`;
- Blueprint, under `resources/ui` (comments stripped): `monospace: true` and a
  `styles [...]` list naming `"monospace"`, except for a widget listed in
  `BLUEPRINT_SITES` whose Rust construction site calls `follow_editor_font`
  (or `follow_editor_font_monospace`) on that template child.

Each finding names the file and line. `--self-test` proves the check red on
deliberate violations and green on the allowed forms.
"""

from __future__ import annotations

import re
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
RUST_ROOT = "crates/lushtext-core/src"
BLUEPRINT_ROOT = "resources/ui"
REGISTRY = "crates/lushtext-core/src/ui/font_zoom.rs"

# (Blueprint file, widget id) -> the Rust file that registers it.
BLUEPRINT_SITES = {
    ("resources/ui/line-inspector.blp", "window_view"): (
        "crates/lushtext-core/src/ui/editor_page/protected_view/inspector.rs"
    ),
}

RUST_PATTERNS = (
    (re.compile(r"\.add_css_class\(\s*\"monospace\"\s*\)"), 'add_css_class("monospace")'),
    (re.compile(r"\.set_monospace\(\s*true\s*\)"), "set_monospace(true)"),
    (re.compile(r"css_classes\(\s*\[[^\]]*\"monospace\"", re.S), 'css_classes([.."monospace"..])'),
)
BLUEPRINT_PATTERNS = (
    (re.compile(r"\bmonospace\s*:\s*true\b"), "monospace: true"),
    (re.compile(r"\bstyles\s*\[[^\]]*\"monospace\"", re.S), 'styles ["monospace"]'),
)
BLUEPRINT_OBJECT = re.compile(r"(?:^|[\s:])(\$?[A-Z][\w.]*)\s+(\w+)$")


def strip_line_comments(text: str) -> str:
    """Drop `//` comments (outside string literals) and `/* */` blocks,
    keeping line numbers."""
    out = []
    in_block = False
    for line in text.splitlines():
        result = []
        i = 0
        in_string = False
        while i < len(line):
            if in_block:
                end = line.find("*/", i)
                if end < 0:
                    i = len(line)
                    break
                in_block = False
                i = end + 2
                continue
            char = line[i]
            if in_string:
                result.append(char)
                if char == "\\" and i + 1 < len(line):
                    result.append(line[i + 1])
                    i += 2
                    continue
                if char == '"':
                    in_string = False
                i += 1
                continue
            if line.startswith("//", i):
                break
            if line.startswith("/*", i):
                in_block = True
                i += 2
                continue
            if char == '"':
                in_string = True
            result.append(char)
            i += 1
        out.append("".join(result))
    return "\n".join(out)


def line_of(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def rust_findings(path: str, text: str) -> list[str]:
    if path == REGISTRY:
        return []
    code = strip_line_comments(text)
    found = []
    for pattern, label in RUST_PATTERNS:
        for match in pattern.finditer(code):
            found.append(
                f"{path}:{line_of(code, match.start())}: {label} outside ui/font_zoom.rs; "
                "use font_zoom::follow_editor_font_monospace so the widget follows the zoom"
            )
    return found


def enclosing_object(code: str, offset: int) -> str | None:
    """The id of the innermost named Blueprint object open at `offset`."""
    stack: list[str | None] = []
    for match in re.finditer(r"[{}]", code[:offset]):
        if match.group() == "{":
            head = code[: match.start()].rstrip()
            named = BLUEPRINT_OBJECT.search(head)
            stack.append(named.group(2) if named else None)
        elif stack:
            stack.pop()
    for name in reversed(stack):
        if name:
            return name
    return None


def registers(rust_text: str, widget_id: str) -> bool:
    code = strip_line_comments(rust_text)
    return re.search(
        rf"follow_editor_font(?:_monospace)?\(\s*&\*?\s*(?:self|imp)\.{re.escape(widget_id)}\b",
        code,
    ) is not None


def blueprint_findings(path: str, text: str, read) -> list[str]:
    code = strip_line_comments(text)
    found = []
    for pattern, label in BLUEPRINT_PATTERNS:
        for match in pattern.finditer(code):
            widget = enclosing_object(code, match.start())
            where = f"{path}:{line_of(code, match.start())}"
            site = BLUEPRINT_SITES.get((path, widget or ""))
            if site is None:
                found.append(
                    f"{where}: {label} on {widget or 'an unnamed widget'} is not mapped to a "
                    "registering Rust site in BLUEPRINT_SITES"
                )
            elif not registers(read(site), widget or ""):
                found.append(
                    f"{where}: {label} on {widget}: {site} does not call "
                    f"follow_editor_font on {widget}"
                )
    return found


def scan(root: Path) -> list[str]:
    def read(relative: str) -> str:
        target = root / relative
        return target.read_text(encoding="utf-8") if target.exists() else ""

    findings = []
    for path in sorted((root / RUST_ROOT).rglob("*.rs")):
        relative = path.relative_to(root).as_posix()
        findings += rust_findings(relative, path.read_text(encoding="utf-8"))
    for path in sorted((root / BLUEPRINT_ROOT).glob("*.blp")):
        relative = path.relative_to(root).as_posix()
        findings += blueprint_findings(relative, path.read_text(encoding="utf-8"), read)
    return findings


def self_test() -> int:
    failures = []

    def expect(name: str, files: dict[str, str], red: bool) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative, text in files.items():
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(text, encoding="utf-8")
            (root / RUST_ROOT).mkdir(parents=True, exist_ok=True)
            (root / BLUEPRINT_ROOT).mkdir(parents=True, exist_ok=True)
            findings = scan(root)
            if bool(findings) != red:
                failures.append(f"{name}: expected {'red' if red else 'green'}, got {findings}")
            if red and findings and ":" not in findings[0]:
                failures.append(f"{name}: the finding names no call site: {findings}")

    site = f"{RUST_ROOT}/ui/x.rs"
    expect("add_css_class", {site: 'label.add_css_class("monospace");\n'}, True)
    expect("set_monospace", {site: "view.set_monospace(true);\n"}, True)
    expect("css_classes", {site: 'Label::builder().css_classes(["dim", "monospace"]);\n'}, True)
    expect("comment", {site: '// label.add_css_class("monospace");\n'}, False)
    expect("other class", {site: 'label.add_css_class("monospace-ish");\n'}, False)
    expect("helper", {site: "font_zoom::follow_editor_font_monospace(&label);\n"}, False)
    expect("registry", {REGISTRY: 'widget.add_css_class("monospace");\n'}, False)
    expect("set false", {site: "view.set_monospace(false);\n"}, False)
    blueprint = "resources/ui/line-inspector.blp"
    template = "template $T : Adw.Dialog {\n  child: TextView window_view {\n    monospace: true;\n  };\n}\n"
    registered = "fn constructed(&self) { crate::ui::font_zoom::follow_editor_font(&*self.window_view); }\n"
    expect(
        "blueprint mapped and registered",
        {blueprint: template, BLUEPRINT_SITES[(blueprint, "window_view")]: registered},
        False,
    )
    expect(
        "blueprint mapped but not registered",
        {blueprint: template, BLUEPRINT_SITES[(blueprint, "window_view")]: "fn constructed(&self) {}\n"},
        True,
    )
    expect(
        "blueprint unmapped widget",
        {"resources/ui/other.blp": "Box {\n  Label path {\n    styles [\n      \"monospace\",\n    ]\n  }\n}\n"},
        True,
    )
    expect(
        "blueprint false",
        {"resources/ui/other.blp": "TextView v {\n  monospace: false;\n}\n"},
        False,
    )
    if failures:
        print("check-font-zoom-registration self-test failed:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print("check-font-zoom-registration self-test passed")
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    findings = scan(REPO_ROOT)
    if findings:
        print("monospace widgets that do not follow the editor font:", file=sys.stderr)
        for finding in findings:
            print(f"  {finding}", file=sys.stderr)
        return 1
    print("every application monospace widget follows the editor font")
    return 0


if __name__ == "__main__":
    sys.exit(main())
