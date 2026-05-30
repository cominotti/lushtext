#!/usr/bin/python3
from __future__ import annotations

import argparse
import re
import sys
import time
from collections import deque

import pyatspi


DEFAULT_BUTTON_PATTERN = r"^(Screenshot|Share|Allow|OK|Open)$"


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


def accessible_text(node) -> tuple[str, str]:
    try:
        name = node.name or ""
    except Exception:
        name = ""

    try:
        role = node.getRoleName() or ""
    except Exception:
        role = ""

    return name, role


def application_name(node) -> str:
    try:
        app = node.getApplication()
        return app.name or ""
    except Exception:
        return ""


def action_names(node) -> list[str]:
    try:
        action = node.queryAction()
    except Exception:
        return []

    names = []
    for index in range(action.nActions):
        try:
            names.append(action.getName(index))
        except Exception:
            names.append("")
    return names


def click_accessible(node) -> bool:
    try:
        action = node.queryAction()
    except Exception:
        return False

    preferred = {"click", "press", "activate"}
    fallback = None

    for index in range(action.nActions):
        try:
            name = action.getName(index)
        except Exception:
            name = ""
        if fallback is None:
            fallback = index
        if name in preferred:
            return bool(action.doAction(index))

    if fallback is not None:
        return bool(action.doAction(fallback))

    return False


def mouse_click_accessible(node) -> bool:
    try:
        component = node.queryComponent()
        x, y, width, height = component.getExtents(pyatspi.DESKTOP_COORDS)
    except Exception as exc:
        print(f"could not resolve accessible extents: {exc}", file=sys.stderr)
        return False

    if width <= 0 or height <= 0:
        print(f"accessible has empty extents: {(x, y, width, height)}", file=sys.stderr)
        return False

    center_x = x + width // 2
    center_y = y + height // 2

    try:
        pyatspi.Registry.generateMouseEvent(center_x, center_y, pyatspi.MOUSE_ABS)
        time.sleep(0.05)
        pyatspi.Registry.generateMouseEvent(center_x, center_y, pyatspi.MOUSE_B1P)
        time.sleep(0.05)
        pyatspi.Registry.generateMouseEvent(center_x, center_y, pyatspi.MOUSE_B1R)
    except Exception as exc:
        print(f"AT-SPI mouse click failed: {exc}", file=sys.stderr)
        return False

    print(
        "clicked with AT-SPI mouse move/press/release "
        f"at x={center_x} y={center_y} from extents={(x, y, width, height)}"
    )
    return True


def describe_accessibles(
    pattern: re.Pattern[str],
    *,
    app_pattern: re.Pattern[str] | None,
    max_depth: int,
    max_nodes: int,
) -> int:
    desktop = pyatspi.Registry.getDesktop(0)
    count = 0

    for node in iter_accessibles(desktop, max_depth=max_depth, max_nodes=max_nodes):
        name, role = accessible_text(node)
        app = application_name(node)
        actions = action_names(node)
        if app_pattern is not None and not app_pattern.search(app):
            continue
        if not name and not actions:
            continue
        if pattern.search(name) or actions:
            print(f"app={app!r} name={name!r} role={role!r} actions={actions!r}")
            count += 1

    return count


def find_button(
    pattern: re.Pattern[str],
    *,
    app_pattern: re.Pattern[str] | None,
    max_depth: int,
    max_nodes: int,
):
    desktop = pyatspi.Registry.getDesktop(0)

    for node in iter_accessibles(desktop, max_depth=max_depth, max_nodes=max_nodes):
        name, role = accessible_text(node)
        app = application_name(node)
        if app_pattern is not None and not app_pattern.search(app):
            continue
        if pattern.search(name) and "button" in role.lower():
            actions = action_names(node)
            return node, app, name, role, actions

    return None


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Click a visible button through AT-SPI accessibility."
    )
    parser.add_argument(
        "--name-regex",
        default=DEFAULT_BUTTON_PATTERN,
        help=f"Button accessible-name regex (default: {DEFAULT_BUTTON_PATTERN})",
    )
    parser.add_argument(
        "--application-regex",
        help="Restrict matches to accessibles owned by an application name regex",
    )
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--interval", type=float, default=0.25)
    parser.add_argument("--max-depth", type=int, default=12)
    parser.add_argument("--max-nodes", type=int, default=5000)
    parser.add_argument(
        "--fallback-mouse",
        action="store_true",
        help=(
            "Deprecated and disabled: coordinate fallback is unsafe under GNOME Shell"
        ),
    )
    parser.add_argument(
        "--list",
        action="store_true",
        help="List matching/named actionable accessibles instead of clicking",
    )
    args = parser.parse_args()

    pattern = re.compile(args.name_regex, re.IGNORECASE)
    app_pattern = (
        re.compile(args.application_regex, re.IGNORECASE)
        if args.application_regex
        else None
    )

    if args.fallback_mouse and app_pattern is None:
        parser.error("--fallback-mouse requires --application-regex to avoid blind shell clicks")

    if args.list:
        count = describe_accessibles(
            pattern,
            app_pattern=app_pattern,
            max_depth=args.max_depth,
            max_nodes=args.max_nodes,
        )
        print(f"listed={count}")
        return 0

    deadline = time.monotonic() + args.timeout
    last_error = "no matching accessible button found"

    while time.monotonic() < deadline:
        match = find_button(
            pattern,
            app_pattern=app_pattern,
            max_depth=args.max_depth,
            max_nodes=args.max_nodes,
        )
        if match is not None:
            node, app, name, role, actions = match
            if click_accessible(node):
                print(f"clicked app={app!r} name={name!r} role={role!r} actions={actions!r}")
                return 0
            if args.fallback_mouse:
                print(
                    f"matched app={app!r} name={name!r} role={role!r} "
                    "but coordinate fallback is disabled",
                    file=sys.stderr,
                )
                return 1
            last_error = f"matched {name!r} in app {app!r} but action invocation failed"
        time.sleep(args.interval)

    print(last_error, file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
