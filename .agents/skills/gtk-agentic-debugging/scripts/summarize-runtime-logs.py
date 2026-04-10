#!/usr/bin/env python3
from __future__ import annotations

import collections
import pathlib
import re
import sys

ANSI_RE = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
TIME_RE = re.compile(r"\b\d{2}:\d{2}:\d{2}(?:\.\d+)?\b")

WARNING_PATTERNS = [
    ("gtk-warning", re.compile(r"Gtk-WARNING")),
    ("gtk-critical", re.compile(r"Gtk-CRITICAL")),
    ("glib-critical", re.compile(r"GLib(?:-[A-Za-z]+)?-CRITICAL")),
    ("gdk-critical", re.compile(r"Gdk-CRITICAL")),
    ("adwaita-warning", re.compile(r"(?:Adwaita|libadwaita).*(?:WARNING|CRITICAL|ERROR)", re.I)),
    ("rust-panic", re.compile(r"thread '.*' panicked|panicked at")),
    ("rust-error", re.compile(r"\berror\b", re.I)),
]

DBUS_RE = re.compile(
    r"interface=(?P<interface>[^;]+); member=(?P<member>[A-Za-z0-9_]+)"
)


def read_lines(path: pathlib.Path) -> list[str]:
    if not path.exists():
        return []
    return path.read_text(errors="replace").splitlines()


def normalize_warning(line: str) -> str:
    line = ANSI_RE.sub("", line).strip()
    line = TIME_RE.sub("<time>", line)
    line = re.sub(r"0x[0-9a-fA-F]+", "0xADDR", line)
    line = re.sub(r"\s+", " ", line)
    return line


def extract_warnings(lines: list[str]) -> list[tuple[str, str]]:
    hits: list[tuple[str, str]] = []
    for line in lines:
        if line.startswith("Script started on ") or line.startswith("Script done on "):
            continue
        normalized = normalize_warning(line)
        for label, pattern in WARNING_PATTERNS:
            if pattern.search(normalized):
                hits.append((label, normalized))
                break
    return hits


def summarize_warnings(lines: list[str]) -> str:
    hits = extract_warnings(lines)
    if not hits:
        return "No GTK, GLib, Adwaita, or Rust warning signatures found."

    counter: collections.Counter[tuple[str, str]] = collections.Counter(hits)
    out = []
    for (label, message), count in counter.most_common(10):
        out.append(f"- `{label}` x{count}: {message}")
    return "\n".join(out)


def summarize_dbus(lines: list[str]) -> str:
    counter: collections.Counter[str] = collections.Counter()
    for line in lines:
        match = DBUS_RE.search(line)
        if match:
            key = f"{match.group('interface')}.{match.group('member')}"
            counter[key] += 1
    if not counter:
        return "No D-Bus interface/member pairs were parsed."
    return "\n".join(
        f"- `{name}` x{count}" for name, count in counter.most_common(10)
    )


def summarize_status(path: pathlib.Path) -> str:
    if not path.exists():
        return "No status log found."
    lines = [line.strip() for line in read_lines(path) if line.strip()]
    if not lines:
        return "Status log is empty."
    return "\n".join(f"- {line}" for line in lines[-10:])


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: summarize-runtime-logs.py <artifact-dir>", file=sys.stderr)
        return 1

    root = pathlib.Path(sys.argv[1])
    app_lines = read_lines(root / "app.typescript")
    dbus_lines = read_lines(root / "dbus.log")
    journal_lines = read_lines(root / "journal.log")

    combined_lines = app_lines + journal_lines

    print("# GTK Debug Summary\n")
    print(f"- Artifact directory: `{root}`")
    print(f"- App log present: `{(root / 'app.typescript').exists()}`")
    print(f"- D-Bus log present: `{(root / 'dbus.log').exists()}`")
    print(f"- Journal log present: `{(root / 'journal.log').exists()}`\n")

    print("## Launcher Status\n")
    print(summarize_status(root / "status.txt"))
    print("\n## Top Warnings\n")
    print(summarize_warnings(combined_lines))
    print("\n## D-Bus Activity\n")
    print(summarize_dbus(dbus_lines))

    geometry_lines = [
        normalize_warning(line)
        for line in combined_lines
        if "Trying to measure GtkBox" in line
    ]
    print("\n## Geometry Warnings\n")
    if geometry_lines:
        geometry_counter = collections.Counter(geometry_lines)
        for line, count in geometry_counter.most_common(10):
            print(f"- x{count}: {line}")
    else:
        print("No GtkBox measurement warnings were detected.")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
