#!/usr/bin/python3
from __future__ import annotations

import argparse
import pathlib
import re
import shutil
import subprocess
import sys
import time
import urllib.parse
import uuid
from collections import deque

try:
    from gi.repository import Gio, GLib
except Exception as exc:  # pragma: no cover - exercised by missing host deps.
    Gio = None
    GLib = None
    GI_IMPORT_ERROR = exc
else:
    GI_IMPORT_ERROR = None

try:
    import pyatspi
except Exception as exc:  # pragma: no cover - exercised by missing host deps.
    pyatspi = None
    PYATSPI_IMPORT_ERROR = exc
else:
    PYATSPI_IMPORT_ERROR = None


PORTAL_BUS = "org.freedesktop.portal.Desktop"
PORTAL_PATH = "/org/freedesktop/portal/desktop"
SCREENSHOT_IFACE = "org.freedesktop.portal.Screenshot"
REQUEST_IFACE = "org.freedesktop.portal.Request"
DEFAULT_APPROVE_PATTERN = r"^(Take Screenshot|Screenshot|Share|Allow|OK)$"
DEFAULT_APPROVE_APP_PATTERN = r"^(gnome-shell|xdg-desktop-portal-gnome|xdg-desktop-portal-gtk)$"


def run_checked(cmd: list[str], *, timeout: int | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        check=False,
        text=True,
        capture_output=True,
        timeout=timeout,
    )


def try_gnome_screenshot(output: pathlib.Path, timeout_seconds: int) -> tuple[bool, str]:
    binary = shutil.which("gnome-screenshot")
    if not binary:
        return False, "gnome-screenshot not installed"
    try:
        result = run_checked([binary, "-f", str(output)], timeout=timeout_seconds)
    except subprocess.TimeoutExpired:
        return False, f"gnome-screenshot timed out after {timeout_seconds}s"
    if result.returncode == 0 and output.exists() and output.stat().st_size > 0:
        return True, f"saved via gnome-screenshot to {output}"
    return False, (result.stderr or result.stdout or "gnome-screenshot failed").strip()


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


def is_showing(node) -> bool:
    try:
        state = node.getState()
        return bool(
            state.contains(pyatspi.STATE_SHOWING)
            or state.contains(pyatspi.STATE_VISIBLE)
        )
    except Exception:
        return False


def click_accessible_action(node) -> bool:
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


def click_accessible_with_atspi_mouse(node) -> tuple[bool, str]:
    try:
        component = node.queryComponent()
        x, y, width, height = component.getExtents(pyatspi.DESKTOP_COORDS)
    except Exception as exc:
        return False, f"could not resolve accessible extents: {exc}"

    if width <= 0 or height <= 0:
        return False, f"accessible has empty extents: {(x, y, width, height)}"

    center_x = x + width // 2
    center_y = y + height // 2
    try:
        pyatspi.Registry.generateMouseEvent(center_x, center_y, pyatspi.MOUSE_ABS)
        time.sleep(0.05)
        pyatspi.Registry.generateMouseEvent(center_x, center_y, pyatspi.MOUSE_B1P)
        time.sleep(0.05)
        pyatspi.Registry.generateMouseEvent(center_x, center_y, pyatspi.MOUSE_B1R)
    except Exception as exc:
        return False, f"AT-SPI mouse click failed: {exc}"

    return True, (
        "AT-SPI mouse move/press/release "
        f"at x={center_x} y={center_y} extents={(x, y, width, height)}"
    )


def find_approval_button(
    *,
    app_pattern: re.Pattern[str],
    name_pattern: re.Pattern[str],
    max_depth: int,
    max_nodes: int,
):
    desktop = pyatspi.Registry.getDesktop(0)

    for node in iter_accessibles(desktop, max_depth=max_depth, max_nodes=max_nodes):
        app = application_name(node)
        if not app_pattern.search(app):
            continue

        name, role = node_text(node)
        if (
            name_pattern.search(name)
            and "button" in role.lower()
            and is_showing(node)
        ):
            return node, app, name, role, action_names(node)

    return None


def try_approve_portal_button(
    *,
    app_pattern: re.Pattern[str],
    name_pattern: re.Pattern[str],
    max_depth: int,
    max_nodes: int,
) -> str | None:
    if pyatspi is None:
        return f"pyatspi unavailable: {PYATSPI_IMPORT_ERROR}"

    match = find_approval_button(
        app_pattern=app_pattern,
        name_pattern=name_pattern,
        max_depth=max_depth,
        max_nodes=max_nodes,
    )
    if match is None:
        return None

    node, app, name, role, actions = match
    if click_accessible_action(node):
        return f"clicked app={app!r} name={name!r} role={role!r} actions={actions!r}"

    return (
        f"matched app={app!r} name={name!r} role={role!r} but it exposes no "
        "invokable AT-SPI action; refusing coordinate fallback"
    )


def unpack_variant_value(value):
    while hasattr(value, "get_type_string") and value.get_type_string() == "v":
        value = value.get_variant()
    if hasattr(value, "unpack"):
        return value.unpack()
    return value


def unpack_results(value) -> dict[str, object]:
    results = {}
    for index in range(value.n_children()):
        entry = value.get_child_value(index)
        key = entry.get_child_value(0).unpack()
        variant = entry.get_child_value(1)
        results[key] = unpack_variant_value(variant)
    return results


def copy_portal_uri(uri: object, output: pathlib.Path) -> tuple[bool, str]:
    if not isinstance(uri, str):
        return False, f"portal response did not include a string URI: {uri!r}"

    parsed = urllib.parse.urlparse(urllib.parse.unquote(uri))
    if parsed.scheme != "file":
        return False, f"portal returned non-file URI: {uri}"

    source = pathlib.Path(parsed.path)
    if not source.exists():
        return False, f"portal URI does not exist on disk: {source}"

    shutil.copy2(source, output)
    return True, f"saved via portal to {output}"


def try_portal(
    output: pathlib.Path,
    timeout_seconds: int,
    *,
    interactive: bool,
    auto_approve: bool,
    approve_name_regex: str,
    approve_app_regex: str,
) -> tuple[bool, str]:
    if Gio is None or GLib is None:
        return False, f"PyGObject Gio unavailable: {GI_IMPORT_ERROR}"

    if auto_approve and pyatspi is None:
        return False, f"pyatspi unavailable for --auto-approve: {PYATSPI_IMPORT_ERROR}"

    approve_name_pattern = re.compile(approve_name_regex, re.IGNORECASE)
    approve_app_pattern = re.compile(approve_app_regex, re.IGNORECASE)
    connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    responses: list[tuple[str, int, dict[str, object], str]] = []

    def on_response(
        _connection,
        _sender_name,
        object_path,
        _interface_name,
        _signal_name,
        parameters,
        _user_data,
    ):
        response = int(parameters.get_child_value(0).unpack())
        results = unpack_results(parameters.get_child_value(1))
        responses.append((object_path, response, results, parameters.print_(True)))

    subscription_id = connection.signal_subscribe(
        PORTAL_BUS,
        REQUEST_IFACE,
        "Response",
        None,
        None,
        Gio.DBusSignalFlags.NONE,
        on_response,
        None,
    )

    token = f"lushtext_{uuid.uuid4().hex}"
    options = {
        "handle_token": GLib.Variant("s", token),
        "interactive": GLib.Variant("b", interactive),
    }
    context = GLib.MainContext.default()
    handle = ""
    click_notes: list[str] = []

    try:
        result = connection.call_sync(
            PORTAL_BUS,
            PORTAL_PATH,
            SCREENSHOT_IFACE,
            "Screenshot",
            GLib.Variant("(sa{sv})", ("", options)),
            GLib.VariantType("(o)"),
            Gio.DBusCallFlags.NONE,
            timeout_seconds * 1000,
            None,
        )
        handle = result.unpack()[0]
    except Exception as exc:
        connection.signal_unsubscribe(subscription_id)
        return False, f"portal call failed: {exc}"

    deadline = time.monotonic() + timeout_seconds
    next_click_attempt = 0.0
    try:
        while time.monotonic() < deadline:
            while context.pending():
                context.iteration(False)

            for object_path, response, results, raw in list(responses):
                if object_path != handle:
                    continue
                if response != 0:
                    return False, f"portal response={response} results={results!r}"
                ok, message = copy_portal_uri(results.get("uri"), output)
                if ok:
                    if click_notes:
                        return True, f"{message}; approvals: {'; '.join(click_notes)}"
                    return True, message
                return False, f"portal responded but no usable file URI was found: {raw}"

            now = time.monotonic()
            if auto_approve and now >= next_click_attempt:
                note = try_approve_portal_button(
                    app_pattern=approve_app_pattern,
                    name_pattern=approve_name_pattern,
                    max_depth=14,
                    max_nodes=8000,
                )
                if note:
                    click_notes.append(note)
                    print(f"portal approval: {note}", file=sys.stderr)
                next_click_attempt = now + 0.35

            time.sleep(0.05)
    finally:
        connection.signal_unsubscribe(subscription_id)

    detail = f"handle={handle}"
    if click_notes:
        detail += f"; approvals attempted: {'; '.join(click_notes)}"
    return False, f"portal timed out or required approval ({detail})"


def main() -> int:
    parser = argparse.ArgumentParser(description="Capture a screenshot for GTK debugging.")
    parser.add_argument("output", type=pathlib.Path, help="Destination file path")
    parser.add_argument("--timeout", type=int, default=10, help="Portal wait timeout in seconds")
    parser.add_argument(
        "--portal-only",
        action="store_true",
        help="Skip direct gnome-screenshot and request capture through the desktop portal",
    )
    parser.add_argument(
        "--non-interactive",
        action="store_true",
        help="Do not ask the portal to show an interactive screenshot picker",
    )
    parser.add_argument(
        "--auto-approve",
        action="store_true",
        help="Use AT-SPI to click a visible desktop screenshot approval button",
    )
    parser.add_argument(
        "--approve-name-regex",
        default=DEFAULT_APPROVE_PATTERN,
        help=f"Accessible button-name regex for --auto-approve (default: {DEFAULT_APPROVE_PATTERN})",
    )
    parser.add_argument(
        "--approve-application-regex",
        default=DEFAULT_APPROVE_APP_PATTERN,
        help=(
            "Accessible application-name regex for --auto-approve "
            f"(default: {DEFAULT_APPROVE_APP_PATTERN})"
        ),
    )
    args = parser.parse_args()

    args.output.parent.mkdir(parents=True, exist_ok=True)
    if not args.output.suffix:
        args.output = args.output.with_suffix(".png")

    if args.portal_only:
        message = "skipped by --portal-only"
    else:
        ok, message = try_gnome_screenshot(args.output, args.timeout)
        if ok:
            print(message)
            return 0

    ok, portal_message = try_portal(
        args.output,
        args.timeout,
        interactive=not args.non_interactive,
        auto_approve=args.auto_approve,
        approve_name_regex=args.approve_name_regex,
        approve_app_regex=args.approve_application_regex,
    )
    if ok:
        print(portal_message)
        return 0

    print("screenshot capture failed", file=sys.stderr)
    print(f"- gnome-screenshot: {message}", file=sys.stderr)
    print(f"- portal: {portal_message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
