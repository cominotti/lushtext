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


def is_showing(node) -> bool:
    try:
        state = node.getState()
        return bool(state.contains(pyatspi.STATE_SHOWING) or state.contains(pyatspi.STATE_VISIBLE))
    except Exception:
        return False


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


def find_accessible(
    *,
    app_pattern: re.Pattern[str],
    name_pattern: re.Pattern[str],
    role_pattern: re.Pattern[str],
    max_depth: int,
    max_nodes: int,
):
    desktop = pyatspi.Registry.getDesktop(0)

    for node in iter_accessibles(desktop, max_depth=max_depth, max_nodes=max_nodes):
        app = application_name(node)
        if not app_pattern.search(app):
            continue
        name, role = node_text(node)
        if not name_pattern.search(name):
            continue
        if not role_pattern.search(role):
            continue
        if not is_showing(node):
            continue
        return node, app, name, role, action_names(node)

    return None


def wait_for_accessible(args):
    app_pattern = re.compile(args.application_regex, re.IGNORECASE)
    name_pattern = re.compile(args.name_regex, re.IGNORECASE)
    role_pattern = re.compile(args.role_regex, re.IGNORECASE)

    deadline = time.monotonic() + args.timeout
    while time.monotonic() < deadline:
        match = find_accessible(
            app_pattern=app_pattern,
            name_pattern=name_pattern,
            role_pattern=role_pattern,
            max_depth=args.max_depth,
            max_nodes=args.max_nodes,
        )
        if match is not None:
            return match
        time.sleep(args.interval)

    return None


def focus_accessible(node) -> bool:
    try:
        component = node.queryComponent()
        return bool(component.grabFocus())
    except Exception as exc:
        print(f"could not focus accessible: {exc}", file=sys.stderr)
        return False


def activate_accessible(node) -> bool:
    try:
        action = node.queryAction()
    except Exception as exc:
        print(f"accessible has no action interface: {exc}", file=sys.stderr)
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


def context_click_accessible(node) -> bool:
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
        pyatspi.Registry.generateMouseEvent(center_x, center_y, pyatspi.MOUSE_B3P)
        time.sleep(0.05)
        pyatspi.Registry.generateMouseEvent(center_x, center_y, pyatspi.MOUSE_B3R)
    except Exception as exc:
        print(f"AT-SPI context click failed: {exc}", file=sys.stderr)
        return False

    print(
        "context-clicked with AT-SPI mouse move/press/release "
        f"at x={center_x} y={center_y} from extents={(x, y, width, height)}"
    )
    return True


def synthesize_key(key: str) -> None:
    if key == "Shift+F10":
        pyatspi.Registry.generateKeyboardEvent(0, "Shift_L", pyatspi.KEY_PRESS)
        time.sleep(0.05)
        pyatspi.Registry.generateKeyboardEvent(0, "F10", pyatspi.KEY_PRESSRELEASE)
        time.sleep(0.05)
        pyatspi.Registry.generateKeyboardEvent(0, "Shift_L", pyatspi.KEY_RELEASE)
        return

    pyatspi.Registry.generateKeyboardEvent(0, key, pyatspi.KEY_PRESSRELEASE)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Focus, activate, or key-drive visible accessibles through AT-SPI."
    )
    parser.add_argument(
        "--command",
        choices=("focus", "activate", "context-click", "key"),
        required=True,
    )
    parser.add_argument("--application-regex", default="^lushtext$")
    parser.add_argument("--name-regex")
    parser.add_argument("--role-regex", default=".*")
    parser.add_argument("--key")
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--interval", type=float, default=0.25)
    parser.add_argument("--max-depth", type=int, default=30)
    parser.add_argument("--max-nodes", type=int, default=20000)
    args = parser.parse_args()

    if args.command == "key":
        if not args.key:
            parser.error("--key is required for --command key")
        synthesize_key(args.key)
        print(f"sent key={args.key!r}")
        return 0

    if not args.name_regex:
        parser.error("--name-regex is required for focus, activate, and context-click commands")

    match = wait_for_accessible(args)
    if match is None:
        print(
            f"no accessible matched app={args.application_regex!r} "
            f"role={args.role_regex!r} name={args.name_regex!r}",
            file=sys.stderr,
        )
        return 1

    node, app, name, role, actions = match
    if args.command == "focus":
        ok = focus_accessible(node)
    elif args.command == "activate":
        ok = activate_accessible(node)
    else:
        ok = context_click_accessible(node)

    print(
        f"{args.command} app={app!r} name={name!r} role={role!r} "
        f"actions={actions!r} ok={ok}"
    )
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
