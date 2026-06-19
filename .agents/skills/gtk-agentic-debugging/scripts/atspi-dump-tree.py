#!/usr/bin/python3
from __future__ import annotations

import argparse
import re
import sys
import time
from collections import deque
from pathlib import Path

import pyatspi


STATE_FLAGS = (
    ("focused", pyatspi.STATE_FOCUSED),
    ("enabled", pyatspi.STATE_ENABLED),
    ("sensitive", pyatspi.STATE_SENSITIVE),
    ("showing", pyatspi.STATE_SHOWING),
    ("visible", pyatspi.STATE_VISIBLE),
)
TEXT_SAMPLE_LIMIT = 120


def safe_text(value) -> str:
    try:
        text = value() if callable(value) else value
    except Exception:
        return ""
    return str(text or "").replace("\n", "\\n")


def node_name(node) -> str:
    return safe_text(lambda: node.name)


def node_role(node) -> str:
    return safe_text(lambda: node.getRoleName())


def node_state(node) -> str:
    try:
        state_set = node.getState()
    except Exception:
        return "unavailable"

    states = [name for name, flag in STATE_FLAGS if state_set.contains(flag)]
    return ",".join(states) if states else "none"


def child_at(node, index: int):
    try:
        return node.getChildAtIndex(index)
    except Exception:
        return None


def child_count(node) -> int:
    try:
        return int(node.childCount)
    except Exception:
        return 0


def application_name(node) -> str:
    try:
        app = node.getApplication()
        return app.name or ""
    except Exception:
        return ""


def node_text_summary(node) -> str:
    try:
        text = node.queryText()
    except Exception:
        return ""

    try:
        character_count = int(text.characterCount)
    except Exception:
        character_count = -1

    try:
        caret_offset = int(text.caretOffset)
    except Exception:
        caret_offset = -1

    try:
        selection_count = int(text.getNSelections())
    except Exception:
        selection_count = -1

    sample = ""
    if character_count > 0:
        try:
            sample = text.getText(0, min(character_count, TEXT_SAMPLE_LIMIT))
        except Exception:
            sample = ""

    return (
        f" text_chars={character_count} caret={caret_offset} "
        f"selections={selection_count} text_sample={sample!r}"
    )


def iter_accessibles(root, *, max_depth: int, max_nodes: int):
    queue = deque([(root, 0, "0")])
    seen = 0

    while queue and seen < max_nodes:
        node, depth, path = queue.popleft()
        seen += 1
        yield node, depth, path

        if depth >= max_depth:
            continue

        for index in range(child_count(node)):
            child = child_at(node, index)
            if child is not None:
                queue.append((child, depth + 1, f"{path}/{index}"))


def find_application_root(pattern: re.Pattern[str], *, timeout: float, interval: float):
    desktop = pyatspi.Registry.getDesktop(0)
    deadline = time.monotonic() + timeout
    last_seen: list[str] = []

    while time.monotonic() < deadline:
        last_seen.clear()
        for index in range(child_count(desktop)):
            child = child_at(desktop, index)
            if child is None:
                continue
            app_name = application_name(child) or node_name(child)
            if app_name:
                last_seen.append(app_name)
            if pattern.search(app_name):
                return child
        time.sleep(interval)

    seen = ", ".join(sorted(set(last_seen))) or "<none>"
    raise RuntimeError(f"no accessible application matched {pattern.pattern!r}; seen: {seen}")


def dump_tree(root, *, max_depth: int, max_nodes: int) -> tuple[list[str], list[str]]:
    tree_lines: list[str] = []
    focus_lines: list[str] = []

    for node, depth, path in iter_accessibles(root, max_depth=max_depth, max_nodes=max_nodes):
        name = node_name(node)
        role = node_role(node)
        states = node_state(node)
        app = application_name(node)
        line = (
            f"path={path} depth={depth} role={role!r} name={name!r} "
            f"app={app!r} states={states}{node_text_summary(node)}"
        )
        tree_lines.append(line)
        if "focused" in states.split(","):
            focus_lines.append(line)

    if not focus_lines:
        focus_lines.append("<no focused accessible node reported>")

    return tree_lines, focus_lines


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Dump a bounded AT-SPI tree subset and focused-node path."
    )
    parser.add_argument("--application-regex", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--focus-output", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--interval", type=float, default=0.25)
    parser.add_argument("--max-depth", type=int, default=30)
    parser.add_argument("--max-nodes", type=int, default=20000)
    args = parser.parse_args()

    app_pattern = re.compile(args.application_regex, re.IGNORECASE)
    root = find_application_root(
        app_pattern,
        timeout=args.timeout,
        interval=args.interval,
    )
    tree_lines, focus_lines = dump_tree(
        root,
        max_depth=args.max_depth,
        max_nodes=args.max_nodes,
    )

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.focus_output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text("\n".join(tree_lines) + "\n", encoding="utf-8")
    args.focus_output.write_text("\n".join(focus_lines) + "\n", encoding="utf-8")
    print(f"dumped_nodes={len(tree_lines)}")
    print(f"focused_nodes={0 if focus_lines[0].startswith('<') else len(focus_lines)}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        raise SystemExit(1)
