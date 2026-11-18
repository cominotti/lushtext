#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Every string-named signal connection or emission in lushtext-core goes
through `ui/signal_connect.rs`, which checks the handler's return type against
the signal's declared return type when the handler is connected.

glib-rs's string-named `connect_local` and its family take a closure that
returns `Option<glib::Value>` and check that answer only inside the emission,
in GLib's C marshaller, where a panic cannot unwind: the process aborts. A
handler on `GtkTextView::extend-selection` (`gboolean`) that answered `None`
outside Protected View aborted LushText on every double click
(`fix-extend-selection-abort`). This check fails on any of these calls under
`crates/lushtext-core/src` (comments stripped, call spread over lines
included), except inside `crates/lushtext-core/src/ui/signal_connect.rs`:

- `connect`, `connect_id`, `connect_local`, `connect_local_id`,
  `connect_closure`, `connect_closure_id`, `connect_unsafe`,
  `connect_unsafe_id`, method or path form (`ObjectExt::connect_local(`);
- `emit_by_name`, `emit_by_name_with_values`, `emit_by_name_with_details`,
  `emit_by_name_with_details_and_values`, `emit_with_details`,
  `emit_with_values`, with or without a turbofish.

Typed `connect_*` methods (`connect_changed`, `connect_notify_local`, ...)
carry their return type in their closure signature and are allowed. Tests
under `crates/lushtext/tests/**` and the GTK axiom probes under
`crates/gtk-lush/axioms/**` are outside the scope: they emit and connect by
name on purpose, to drive the toolkit directly.

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
HELPER = "crates/lushtext-core/src/ui/signal_connect.rs"

# A method call (`.connect_local(`, also split over lines) or a path call
# (`ObjectExt::connect_local(`): a local closure that happens to be named
# `connect` is not one. The name must end there, so `disconnect(` and longer
# names (`connect_local_history_requested(`) stay out. The turbofish is lazy,
# so `emit_by_name::<()>(` and `emit_by_name::<bool>(` both match.
TURBOFISH = r"(?:\s*::\s*<[^;{}]*?>)?"
BANNED = re.compile(
    r"(?:\.|::)\s*(?P<name>"
    r"connect(?:_local|_closure|_unsafe)?(?:_id)?"
    r"|emit_by_name(?:_with_values|_with_details(?:_and_values)?)?"
    r"|emit_with_(?:details|values)"
    r")" + TURBOFISH + r"\s*\(",
)


def strip_comments(text: str) -> str:
    """Drop `//` comments (outside string literals) and `/* */` blocks, and
    blank string literals' contents, keeping line numbers."""
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
                if char == "\\" and i + 1 < len(line):
                    result.append("  ")
                    i += 2
                    continue
                if char == '"':
                    in_string = False
                    result.append(char)
                else:
                    result.append(" ")
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


def findings_in(path: str, text: str) -> list[str]:
    if path == HELPER:
        return []
    code = strip_comments(text)
    return [
        f"{path}:{line_of(code, match.start())}: string-named `{match.group('name')}` outside "
        "ui/signal_connect.rs; use signal_connect::connect_void / connect_returning / "
        "emit_void / emit_returning, which check the signal's declared return type"
        for match in BANNED.finditer(code)
    ]


def scan(root: Path) -> list[str]:
    findings = []
    for path in sorted((root / RUST_ROOT).rglob("*.rs")):
        relative = path.relative_to(root).as_posix()
        findings += findings_in(relative, path.read_text(encoding="utf-8"))
    return findings


def self_test() -> int:
    failures = []

    def expect(name: str, files: dict[str, str], red: bool) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / RUST_ROOT).mkdir(parents=True, exist_ok=True)
            for relative, text in files.items():
                target = root / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(text, encoding="utf-8")
            findings = scan(root)
            if bool(findings) != red:
                failures.append(f"{name}: expected {'red' if red else 'green'}, got {findings}")
            if red and findings and ":" not in findings[0]:
                failures.append(f"{name}: the finding names no call site: {findings}")

    site = f"{RUST_ROOT}/ui/x.rs"
    red_cases = {
        "connect_local": 'view.connect_local("extend-selection", false, |_| None);\n',
        "connect_local_id": "view.connect_local_id(id, None, false, |_| None);\n",
        "connect": 'view.connect("x", false, |_| None);\n',
        "connect_id": "view.connect_id(id, None, false, |_| None);\n",
        "connect_closure multi-line": (
            "let id = clock.connect_closure(\n    \"layout\",\n    true,\n"
            "    glib::closure_local!(move |_: FrameClock| ()),\n);\n"
        ),
        "connect_closure_id": "clock.connect_closure_id(id, None, true, closure);\n",
        "connect_unsafe": 'unsafe { view.connect_unsafe("x", false, |_| None) };\n',
        "connect_unsafe_id": "unsafe { view.connect_unsafe_id(id, None, false, |_| None) };\n",
        "path form": 'glib::prelude::ObjectExt::connect_local(&view, "x", false, |_| None);\n',
        "emit_by_name turbofish": 'self.emit_by_name::<()>("content-changed", &[]);\n',
        "emit_by_name split": 'self.obj()\n    .emit_by_name::<bool>(\n        "retrieve-surrounding",\n        &[],\n    );\n',
        "emit_by_name_with_values": 'obj.emit_by_name_with_values("x", &[]);\n',
        "emit_by_name_with_details": 'obj.emit_by_name_with_details::<()>("x", detail, &[]);\n',
        "emit_with_details": "obj.emit_with_details::<()>(id, detail, &[]);\n",
        "emit_with_values": "obj.emit_with_values(id, &[]);\n",
    }
    for name, text in red_cases.items():
        expect(name, {site: text}, True)
    green_cases = {
        "comment": '// view.connect_local("x", false, |_| None);\n',
        "block comment": '/* self.emit_by_name::<()>("x", &[]); */\n',
        "string literal": 'let doc = "call connect_local(name) here";\n',
        "typed connect": "buffer.connect_changed(|_| {});\nview.connect_notify_local(Some(\"x\"), |_, _| {});\n",
        "longer name": "sidebar.connect_local_history_requested(move |path| {});\n",
        "disconnect": "view.disconnect(handler);\n",
        "local closure named connect": "let connect = |b: &Button| {};\nconnect(&imp.next_button);\n",
        "helpers": (
            'signal_connect::connect_void(&view, "backspace", false, |_| {});\n'
            'signal_connect::connect_returning(&view, "extend-selection", false, |_| true);\n'
            'signal_connect::emit_void(self, "content-changed", &[]);\n'
        ),
    }
    for name, text in green_cases.items():
        expect(name, {site: text}, False)
    expect("helper module is exempt", {HELPER: 'object.connect_local(signal, after, |_| None);\n'}, False)
    expect(
        "outside the scope",
        {"crates/lushtext/tests/widget/x.rs": 'view.connect_local("x", false, |_| None);\n'},
        False,
    )
    if failures:
        print("check-signal-connections self-test failed:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1
    print(
        f"check-signal-connections self-test passed "
        f"({len(red_cases) + len(green_cases) + 2} cases)."
    )
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    findings = scan(REPO_ROOT)
    if findings:
        print("string-named signal connections or emissions outside the helper:", file=sys.stderr)
        for finding in findings:
            print(f"  {finding}", file=sys.stderr)
        return 1
    print("every string-named signal connection and emission goes through ui/signal_connect.rs")
    return 0


if __name__ == "__main__":
    sys.exit(main())
