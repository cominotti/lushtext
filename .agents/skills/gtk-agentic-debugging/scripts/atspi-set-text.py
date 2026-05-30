#!/usr/bin/python3
from __future__ import annotations

import argparse
import re
import sys
import time
from collections import deque

import pyatspi


def iter_accessibles(root, *, max_depth: int, max_nodes: int):
    queue = deque([(root, 0)])
    seen = 0

    while queue and seen < max_nodes:
        node, depth = queue.popleft()
        seen += 1
        yield node

        if depth >= max_depth:
            continue

        try:
            child_count = node.childCount
        except Exception:
            continue

        for index in range(child_count):
            try:
                child = node.getChildAtIndex(index)
            except Exception:
                continue
            if child is not None:
                queue.append((child, depth + 1))


def application_name(node) -> str:
    try:
        app = node.getApplication()
        return app.name or ""
    except Exception:
        return ""


def node_text(node) -> tuple[str, str]:
    try:
        name = node.name or ""
    except Exception:
        name = ""

    try:
        role = node.getRoleName() or ""
    except Exception:
        role = ""

    return name, role


def is_focused(node) -> bool:
    try:
        return bool(node.getState().contains(pyatspi.STATE_FOCUSED))
    except Exception:
        return False


def editable_text(node):
    try:
        return node.queryEditableText()
    except Exception:
        return None


def current_text(node) -> str:
    try:
        text = node.queryText()
        return text.getText(0, -1)
    except Exception:
        return ""


def matching_candidates(
    *,
    app_pattern: re.Pattern[str],
    name_pattern: re.Pattern[str] | None,
    role_pattern: re.Pattern[str] | None,
    max_depth: int,
    max_nodes: int,
):
    desktop = pyatspi.Registry.getDesktop(0)
    candidates = []

    for node in iter_accessibles(desktop, max_depth=max_depth, max_nodes=max_nodes):
        app = application_name(node)
        if not app_pattern.search(app):
            continue

        name, role = node_text(node)
        if name_pattern is not None and not name_pattern.search(name):
            continue
        if role_pattern is not None and not role_pattern.search(role):
            continue

        editable = editable_text(node)
        if editable is None:
            continue

        candidates.append((node, app, name, role, is_focused(node), current_text(node)))

    candidates.sort(key=lambda item: (not item[4], item[3] != "entry"))
    return candidates


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Set text in an editable widget through AT-SPI accessibility."
    )
    parser.add_argument("--application-regex", required=True)
    parser.add_argument("--name-regex")
    parser.add_argument("--role-regex", default="^entry$")
    parser.add_argument("--text", help="Text to set; not required with --list")
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--interval", type=float, default=0.25)
    parser.add_argument("--max-depth", type=int, default=30)
    parser.add_argument("--max-nodes", type=int, default=20000)
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()

    if not args.list and args.text is None:
        parser.error("--text is required unless --list is used")

    app_pattern = re.compile(args.application_regex, re.IGNORECASE)
    name_pattern = re.compile(args.name_regex, re.IGNORECASE) if args.name_regex else None
    role_pattern = re.compile(args.role_regex, re.IGNORECASE) if args.role_regex else None

    deadline = time.monotonic() + args.timeout
    candidates = []

    while time.monotonic() < deadline:
        candidates = matching_candidates(
            app_pattern=app_pattern,
            name_pattern=name_pattern,
            role_pattern=role_pattern,
            max_depth=args.max_depth,
            max_nodes=args.max_nodes,
        )
        if candidates:
            break
        time.sleep(args.interval)

    if args.list:
        for _, app, name, role, focused, text in candidates:
            print(
                f"app={app!r} name={name!r} role={role!r} "
                f"focused={focused} text={text!r}"
            )
        print(f"listed={len(candidates)}")
        return 0

    if not candidates:
        print("no matching editable accessible found", file=sys.stderr)
        return 1

    node, app, name, role, focused, _ = candidates[0]
    editable = editable_text(node)
    if editable is None:
        print("selected accessible is no longer editable", file=sys.stderr)
        return 1

    editable.setTextContents(args.text)
    print(
        f"set text app={app!r} name={name!r} role={role!r} "
        f"focused={focused} text={args.text!r}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
