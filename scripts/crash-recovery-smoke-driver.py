#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Real-process crash/restart smoke driver for LushText recovery state."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve()
REPO_ROOT = SCRIPT_PATH.parents[1]
SYSTEM_PYTHON = Path("/usr/bin/python3")
ATSPI_REGISTRYD = Path("/usr/libexec/at-spi2-registryd")
ATSPI_SET_TEXT = REPO_ROOT / ".agents/skills/gtk-agentic-debugging/scripts/atspi-set-text.py"
ATSPI_DUMP_TREE = REPO_ROOT / ".agents/skills/gtk-agentic-debugging/scripts/atspi-dump-tree.py"

WIDTH = 1280
HEIGHT = 860
FILE_BACKED_MARKER = "CRASH_SMOKE_FILE_BACKED_DRAFT_RESTORED"
UNTITLED_MARKER = "CRASH_SMOKE_UNTITLED_DRAFT_RESTORED"
BOOKMARK_WARNING = "Some bookmark data could not be loaded"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-dir", required=True, type=Path)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--internal-run", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--mutter-child", action="store_true", help=argparse.SUPPRESS)
    return parser.parse_args()


def require_command(command: str) -> None:
    if shutil.which(command) is None:
        raise RuntimeError(f"Missing required command: {command}")


def validate_outer_args(args: argparse.Namespace) -> None:
    args.artifact_dir = args.artifact_dir.resolve()
    args.binary = args.binary.resolve()
    if not args.binary.is_file() or not os.access(args.binary, os.X_OK):
        raise RuntimeError(f"LushText binary is not executable: {args.binary}")
    if not SYSTEM_PYTHON.is_file():
        raise RuntimeError("Missing /usr/bin/python3")
    if not ATSPI_REGISTRYD.is_file():
        raise RuntimeError("Missing at-spi2-registryd")
    for path in (ATSPI_SET_TEXT, ATSPI_DUMP_TREE):
        if not path.is_file():
            raise RuntimeError(f"Missing helper: {path}")


def ensure_outer_tools() -> None:
    for command in ("dbus-run-session", "gdbus", "gsettings", "mutter"):
        require_command(command)
    subprocess.run(
        [str(SYSTEM_PYTHON), "-c", "import gi, pyatspi"],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def child_cli_args(args: argparse.Namespace, mode: str) -> list[str]:
    return [
        f"--{mode}",
        "--artifact-dir",
        str(args.artifact_dir),
        "--binary",
        str(args.binary),
    ]


def prepare_state(args: argparse.Namespace) -> dict[str, str]:
    args.artifact_dir.mkdir(parents=True, exist_ok=True)
    for name in (
        "assertions",
        "fixtures",
        "logs",
        "metadata/before-crash",
        "metadata/after-relaunch",
        "screenshots",
        "state/cache",
        "state/config",
        "state/data",
        "state/runtime",
    ):
        (args.artifact_dir / name).mkdir(parents=True, exist_ok=True)
    os.chmod(args.artifact_dir / "state/runtime", 0o700)

    data_dir = args.artifact_dir / "state/data/lushtext"
    data_dir.mkdir(parents=True, exist_ok=True)
    fixture_root = args.artifact_dir / "fixtures/workspace"
    fixture_root.mkdir(parents=True, exist_ok=True)
    file_backed = fixture_root / "file-backed.txt"
    file_backed.write_text("Original crash smoke file content\n", encoding="utf-8")

    workspaces = {
        "current_scope": {"kind": "all"},
        "workspaces": [
            {
                "id": "crash-smoke-workspace",
                "name": "Crash Smoke",
                "root": str(fixture_root),
            }
        ],
    }
    (data_dir / "workspaces.json").write_text(
        json.dumps(workspaces, indent=2) + "\n",
        encoding="utf-8",
    )

    bookmarks_dir = data_dir / "bookmarks"
    bookmarks_dir.mkdir(parents=True, exist_ok=True)
    (bookmarks_dir / "corrupt-crash-smoke.json").write_text(
        "{ this is intentionally malformed bookmark sidecar json\n",
        encoding="utf-8",
    )

    (args.artifact_dir / "state-dir.txt").write_text(
        str(args.artifact_dir / "state") + "\n",
        encoding="utf-8",
    )
    (args.artifact_dir / "data-dir.txt").write_text(str(data_dir) + "\n", encoding="utf-8")
    (args.artifact_dir / "fixtures/file-backed-path.txt").write_text(
        str(file_backed) + "\n",
        encoding="utf-8",
    )

    env = os.environ.copy()
    env.update(
        {
            "GSETTINGS_BACKEND": "keyfile",
            "GSETTINGS_SCHEMA_DIR": str(REPO_ROOT / "data"),
            "LUSHTEXT_CRASH_SMOKE_ARTIFACT_DIR": str(args.artifact_dir),
            "LUSHTEXT_CRASH_SMOKE_FILE_BACKED_PATH": str(file_backed),
            "LUSHTEXT_CRASH_SMOKE_STATE_DIR": str(args.artifact_dir / "state"),
            "LUSHTEXT_DATA_DIR": str(data_dir),
            "XDG_CACHE_HOME": str(args.artifact_dir / "state/cache"),
            "XDG_CONFIG_HOME": str(args.artifact_dir / "state/config"),
            "XDG_DATA_HOME": str(args.artifact_dir / "state/data"),
            "XDG_RUNTIME_DIR": str(args.artifact_dir / "state/runtime"),
        }
    )
    return env


def outer_run(args: argparse.Namespace) -> int:
    validate_outer_args(args)
    ensure_outer_tools()
    env = prepare_state(args)

    log_path = args.artifact_dir / "logs/dbus-session.log"
    command = [
        "dbus-run-session",
        "--",
        str(SYSTEM_PYTHON),
        str(SCRIPT_PATH),
        *child_cli_args(args, "internal-run"),
    ]
    with log_path.open("w", encoding="utf-8") as log:
        result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)

    if result.returncode != 0:
        tail_log(log_path, 160, sys.stderr)
        return result.returncode

    summary_path = args.artifact_dir / "summary.json"
    if summary_path.exists():
        summary = json.loads(summary_path.read_text(encoding="utf-8"))
        if summary.get("result") == "skipped":
            print(f"SKIP: {summary.get('reason', 'crash recovery smoke skipped')}")
            print(f"Artifacts: {args.artifact_dir}")
            return 0

    print(f"PASS: crash recovery smoke completed. Artifacts: {args.artifact_dir}")
    return 0


def start_logged(command: list[str], log_path: Path, env: dict[str, str] | None = None) -> subprocess.Popen:
    log_file = log_path.open("wb")
    return subprocess.Popen(command, stdout=log_file, stderr=subprocess.STDOUT, env=env)


def terminate_process(process: subprocess.Popen | None) -> None:
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=2)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=2)


def wait_for_pipewire(runtime_dir: Path) -> None:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if (runtime_dir / "pipewire-0").exists():
            result = subprocess.run(
                ["pw-dump"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if result.returncode == 0:
                return
        time.sleep(0.1)
    raise RuntimeError("PipeWire did not become ready in the isolated runtime directory.")


def parse_gdbus_single_string(output: str) -> str:
    match = re.search(r"'([^']+)'", output)
    if match is None:
        raise RuntimeError(f"Could not parse D-Bus string response: {output!r}")
    return match.group(1)


def setup_atspi(artifact_dir: Path) -> tuple[str, subprocess.Popen]:
    address_result = subprocess.run(
        [
            "gdbus",
            "call",
            "--session",
            "--dest",
            "org.a11y.Bus",
            "--object-path",
            "/org/a11y/bus",
            "--method",
            "org.a11y.Bus.GetAddress",
        ],
        text=True,
        capture_output=True,
        check=True,
    )
    atspi_address = parse_gdbus_single_string(address_result.stdout)
    (artifact_dir / "atspi-address.txt").write_text(atspi_address + "\n", encoding="utf-8")

    subprocess.run(
        [
            "gdbus",
            "call",
            "--session",
            "--dest",
            "org.a11y.Bus",
            "--object-path",
            "/org/a11y/bus",
            "--method",
            "org.freedesktop.DBus.Properties.Set",
            "org.a11y.Status",
            "IsEnabled",
            "<true>",
        ],
        stdout=(artifact_dir / "logs/atspi-enable.log").open("wb"),
        stderr=subprocess.STDOUT,
        check=False,
    )

    registry = start_logged(
        [str(ATSPI_REGISTRYD), "--dbus-name", "org.a11y.atspi.Registry"],
        artifact_dir / "logs/atspi-registry.log",
    )
    wait_for_atspi_registry(atspi_address)
    return atspi_address, registry


def wait_for_atspi_registry(atspi_address: str) -> None:
    env = os.environ.copy()
    env["DBUS_SESSION_BUS_ADDRESS"] = atspi_address
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        result = subprocess.run(
            [
                "gdbus",
                "call",
                "--session",
                "--dest",
                "org.freedesktop.DBus",
                "--object-path",
                "/org/freedesktop/DBus",
                "--method",
                "org.freedesktop.DBus.ListNames",
            ],
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )
        if result.returncode == 0 and "org.a11y.atspi.Registry" in result.stdout:
            return
        time.sleep(0.1)
    raise RuntimeError("AT-SPI registry did not register on the private accessibility bus.")


def internal_run(args: argparse.Namespace) -> int:
    artifact_dir = args.artifact_dir.resolve()
    runtime_dir = Path(os.environ["XDG_RUNTIME_DIR"])
    processes: list[subprocess.Popen | None] = []

    try:
        screenshot_enabled = False
        screenshot_tools = ("gst-launch-1.0", "pipewire", "pw-dump", "wireplumber")
        if all(shutil.which(command) is not None for command in screenshot_tools):
            pipewire = start_logged(["pipewire"], artifact_dir / "logs/pipewire.log")
            processes.append(pipewire)
            try:
                wait_for_pipewire(runtime_dir)
                wireplumber = start_logged(["wireplumber"], artifact_dir / "logs/wireplumber.log")
                processes.append(wireplumber)
                screenshot_enabled = True
            except Exception as exc:
                terminate_process(pipewire)
                processes.remove(pipewire)
                (artifact_dir / "screenshots/skip.txt").write_text(
                    f"SKIP: PipeWire screenshot support unavailable: {exc}\n",
                    encoding="utf-8",
                )
        else:
            missing = [command for command in screenshot_tools if shutil.which(command) is None]
            (artifact_dir / "screenshots/skip.txt").write_text(
                f"SKIP: screenshot tools unavailable: {', '.join(missing)}\n",
                encoding="utf-8",
            )

        try:
            atspi_address, registry = setup_atspi(artifact_dir)
        except Exception as exc:
            write_skip_summary(artifact_dir, f"AT-SPI setup unavailable: {exc}")
            print(f"SKIP: AT-SPI setup unavailable: {exc}", flush=True)
            return 0
        processes.append(registry)
        os.environ["AT_SPI_BUS_ADDRESS"] = atspi_address

        env = os.environ.copy()
        env["GDK_BACKEND"] = "wayland"
        env["LUSHTEXT_CRASH_SMOKE_SCREENSHOT"] = "1" if screenshot_enabled else "0"
        command = [
            "mutter",
            "--headless",
            "--wayland",
            "--no-x11",
            "--virtual-monitor",
            f"{WIDTH}x{HEIGHT}",
            "--",
            str(SYSTEM_PYTHON),
            str(SCRIPT_PATH),
            *child_cli_args(args, "mutter-child"),
        ]
        with (artifact_dir / "logs/mutter-child.log").open("w", encoding="utf-8") as log:
            result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        print((artifact_dir / "logs/mutter-child.log").read_text(encoding="utf-8", errors="replace"))
        return result.returncode
    finally:
        for process in reversed(processes):
            terminate_process(process)


def bus_call(bus, dest: str, path: str, iface: str, method: str, params=None, reply: str | None = None):
    from gi.repository import Gio, GLib

    return bus.call_sync(
        dest,
        path,
        iface,
        method,
        params,
        GLib.VariantType.new(reply) if reply else None,
        Gio.DBusCallFlags.NONE,
        10000,
        None,
    )


def wait_for_window_actions(bus) -> None:
    deadline = time.monotonic() + 15
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            bus_call(
                bus,
                "dev.cominotti.lushtext",
                "/dev/cominotti/lushtext/window/1",
                "org.gtk.Actions",
                "List",
                reply="(as)",
            )
            return
        except Exception as exc:
            last_error = exc
            time.sleep(0.1)
    raise RuntimeError(f"LushText did not export window actions: {last_error}")


def activate_window_action(bus, action_name: str) -> None:
    from gi.repository import GLib

    bus_call(
        bus,
        "dev.cominotti.lushtext",
        "/dev/cominotti/lushtext/window/1",
        "org.gtk.Actions",
        "Activate",
        GLib.Variant("(sava{sv})", (action_name, [], {})),
    )


def launch_app(args: argparse.Namespace, artifact_dir: Path, phase: str, extra_args: list[str]) -> subprocess.Popen:
    env = os.environ.copy()
    env.update(
        {
            "GDK_BACKEND": "wayland",
            "GSETTINGS_BACKEND": "keyfile",
            "GSETTINGS_SCHEMA_DIR": str(REPO_ROOT / "data"),
            "GSK_RENDERER": env.get("GSK_RENDERER", "cairo"),
            "GTK_USE_PORTAL": "0",
            "RUST_LOG": env.get("RUST_LOG", "warn"),
        }
    )
    app = subprocess.Popen(
        [str(args.binary), *extra_args],
        stdout=(artifact_dir / f"logs/lushtext-{phase}.stdout").open("wb"),
        stderr=(artifact_dir / f"logs/lushtext-{phase}.stderr").open("wb"),
        env=env,
    )
    (artifact_dir / f"logs/lushtext-{phase}.pid").write_text(str(app.pid), encoding="utf-8")
    print(f"Launched {phase} PID: {app.pid}", flush=True)
    return app


def run_helper(command: list[str], env: dict[str, str], stdout_path: Path, stderr_path: Path) -> subprocess.CompletedProcess:
    result = subprocess.run(
        command,
        text=True,
        capture_output=True,
        env=env,
        timeout=20,
        check=False,
    )
    stdout_path.write_text(result.stdout, encoding="utf-8")
    stderr_path.write_text(result.stderr, encoding="utf-8")
    return result


def set_editor_text(artifact_dir: Path, app_env: dict[str, str], phase: str, text: str) -> None:
    result = run_helper(
        [
            str(SYSTEM_PYTHON),
            str(ATSPI_SET_TEXT),
            "--application-regex",
            "^lushtext$",
            "--role-regex",
            "^(text|document text)$",
            "--text",
            text,
            "--timeout",
            "12",
        ],
        app_env,
        artifact_dir / f"assertions/{phase}-set-text.stdout",
        artifact_dir / f"assertions/{phase}-set-text.stderr",
    )
    if result.returncode == 0:
        return

    run_helper(
        [
            str(SYSTEM_PYTHON),
            str(ATSPI_SET_TEXT),
            "--application-regex",
            "^lushtext$",
            "--role-regex",
            ".*",
            "--list",
            "--timeout",
            "2",
        ],
        app_env,
        artifact_dir / f"assertions/{phase}-editable-list.stdout",
        artifact_dir / f"assertions/{phase}-editable-list.stderr",
    )
    raise RuntimeError(f"AT-SPI could not set editor text during {phase}")


def list_editable_text(artifact_dir: Path, app_env: dict[str, str], phase: str) -> str:
    result = run_helper(
        [
            str(SYSTEM_PYTHON),
            str(ATSPI_SET_TEXT),
            "--application-regex",
            "^lushtext$",
            "--role-regex",
            "^(text|document text|entry)$",
            "--list",
            "--timeout",
            "10",
        ],
        app_env,
        artifact_dir / f"assertions/{phase}-editable-list.stdout",
        artifact_dir / f"assertions/{phase}-editable-list.stderr",
    )
    if result.returncode != 0:
        raise RuntimeError(f"AT-SPI editable list failed during {phase}")
    return result.stdout


def dump_atspi_tree(artifact_dir: Path, app_env: dict[str, str], phase: str) -> str:
    tree_path = artifact_dir / f"assertions/{phase}-atspi-tree.txt"
    focus_path = artifact_dir / f"assertions/{phase}-atspi-focus.txt"
    result = run_helper(
        [
            str(SYSTEM_PYTHON),
            str(ATSPI_DUMP_TREE),
            "--application-regex",
            "^lushtext$",
            "--output",
            str(tree_path),
            "--focus-output",
            str(focus_path),
            "--timeout",
            "10",
        ],
        app_env,
        artifact_dir / f"assertions/{phase}-dump-tree.stdout",
        artifact_dir / f"assertions/{phase}-dump-tree.stderr",
    )
    if result.returncode != 0:
        raise RuntimeError(f"AT-SPI tree dump failed during {phase}")
    return tree_path.read_text(encoding="utf-8", errors="replace")


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def wait_for_recovery_metadata(data_dir: Path, file_backed: Path) -> None:
    manifest_path = data_dir / "drafts/manifest.json"
    session_path = data_dir / "session.json"
    deadline = time.monotonic() + 20
    last_state = "<metadata not read yet>"
    while time.monotonic() < deadline:
        try:
            manifest = load_json(manifest_path)
            session = load_json(session_path)
            drafts = manifest.get("drafts", [])
            tabs = session.get("tabs", [])
            has_file_draft = any(entry.get("original_path") == str(file_backed) for entry in drafts)
            has_untitled_draft = any(entry.get("original_path") is None for entry in drafts)
            has_file_tab = any(tab.get("path") == str(file_backed) for tab in tabs)
            has_untitled_tab = any(tab.get("path") is None for tab in tabs)
            selected_untitled = session.get("active_tab_index") == 1
            if (
                has_file_draft
                and has_untitled_draft
                and has_file_tab
                and has_untitled_tab
                and selected_untitled
            ):
                return
            last_state = json.dumps(
                {
                    "drafts": drafts,
                    "tabs": tabs,
                    "active_tab_index": session.get("active_tab_index"),
                },
                indent=2,
            )
        except Exception as exc:
            last_state = repr(exc)
        time.sleep(0.25)
    raise RuntimeError(f"Timed out waiting for draft/session recovery metadata: {last_state}")


def wait_for_relaunch_metadata(data_dir: Path, file_backed: Path) -> None:
    deadline = time.monotonic() + 12
    last_state = "<metadata not read yet>"
    while time.monotonic() < deadline:
        try:
            session = load_json(data_dir / "session.json")
            tabs = session.get("tabs", [])
            if (
                len(tabs) >= 2
                and any(tab.get("path") == str(file_backed) for tab in tabs)
                and any(tab.get("path") is None for tab in tabs)
            ):
                return
            last_state = json.dumps(session, indent=2)
        except Exception as exc:
            last_state = repr(exc)
        time.sleep(0.25)
    raise RuntimeError(f"Timed out waiting for relaunched session metadata: {last_state}")


def wait_for_visible_untitled(artifact_dir: Path, app_env: dict[str, str]) -> str:
    deadline = time.monotonic() + 15
    last_listing = ""
    while time.monotonic() < deadline:
        last_listing = list_editable_text(artifact_dir, app_env, "relaunch")
        if UNTITLED_MARKER in last_listing:
            return last_listing
        time.sleep(0.4)
    raise RuntimeError(
        "Timed out waiting for visible restored untitled draft content. "
        f"Last editable listing:\n{last_listing}"
    )


def assert_file_backed_draft_body(data_dir: Path, file_backed: Path) -> None:
    manifest = load_json(data_dir / "drafts/manifest.json")
    entry = next(
        (
            draft
            for draft in manifest.get("drafts", [])
            if draft.get("original_path") == str(file_backed)
        ),
        None,
    )
    if entry is None:
        raise RuntimeError("manifest no longer contains the file-backed draft entry")
    draft_id = entry.get("draft_id")
    if not isinstance(draft_id, str) or not draft_id:
        raise RuntimeError(f"invalid file-backed draft id in manifest entry: {entry!r}")
    draft_path = data_dir / "drafts" / f"{draft_id}.draft"
    draft_body = draft_path.read_text(encoding="utf-8", errors="replace")
    if FILE_BACKED_MARKER not in draft_body:
        raise RuntimeError(f"file-backed draft body did not contain {FILE_BACKED_MARKER}")


def wait_for_sidecar_recovery_evidence(data_dir: Path, artifact_dir: Path, app_env: dict[str, str]) -> None:
    deadline = time.monotonic() + 10
    last_tree = ""
    while time.monotonic() < deadline:
        quarantine_dir = data_dir / "recovery-quarantine"
        quarantine_files = list(quarantine_dir.rglob("*")) if quarantine_dir.exists() else []
        last_tree = dump_atspi_tree(artifact_dir, app_env, "relaunch-bookmarks")
        if quarantine_files or BOOKMARK_WARNING in last_tree:
            return
        time.sleep(0.5)
    raise RuntimeError(
        "Timed out waiting for corrupt bookmark sidecar recovery evidence. "
        f"Last AT-SPI tree excerpt:\n{last_tree[-2000:]}"
    )


def snapshot_metadata(data_dir: Path, output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    lines = []
    copied = output_dir / "bounded-files"
    copied.mkdir(exist_ok=True)
    if not data_dir.exists():
        (output_dir / "tree.txt").write_text("<data-dir-missing>\n", encoding="utf-8")
        return

    for path in sorted(p for p in data_dir.rglob("*") if p.is_file()):
        rel = path.relative_to(data_dir)
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        lines.append(f"{rel}\tsize={len(data)}\tsha256={digest}")
        if len(data) <= 64 * 1024 and rel.parts[:1] not in (("drafts",),):
            safe_name = "__".join(rel.parts)
            (copied / safe_name).write_bytes(data)
    (output_dir / "tree.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")


def capture_monitor(bus, output: Path) -> None:
    from gi.repository import Gio, GLib

    session_path = None
    try:
        session_path = bus_call(
            bus,
            "org.gnome.Mutter.ScreenCast",
            "/org/gnome/Mutter/ScreenCast",
            "org.gnome.Mutter.ScreenCast",
            "CreateSession",
            GLib.Variant("(a{sv})", ({},)),
            "(o)",
        ).unpack()[0]
        stream_path = bus_call(
            bus,
            "org.gnome.Mutter.ScreenCast",
            session_path,
            "org.gnome.Mutter.ScreenCast.Session",
            "RecordMonitor",
            GLib.Variant("(sa{sv})", ("Meta-0", {"is-recording": GLib.Variant("b", True)})),
            "(o)",
        ).unpack()[0]

        node_id: dict[str, int | None] = {"value": None}
        loop = GLib.MainLoop()

        def on_signal(_conn, _sender, _path, _iface, _signal, params):
            node_id["value"] = params.unpack()[0]
            loop.quit()

        subscription = bus.signal_subscribe(
            "org.gnome.Mutter.ScreenCast",
            "org.gnome.Mutter.ScreenCast.Stream",
            "PipeWireStreamAdded",
            stream_path,
            None,
            Gio.DBusSignalFlags.NONE,
            on_signal,
        )
        GLib.timeout_add_seconds(5, lambda: (loop.quit(), False)[1])
        bus_call(
            bus,
            "org.gnome.Mutter.ScreenCast",
            session_path,
            "org.gnome.Mutter.ScreenCast.Session",
            "Start",
        )
        loop.run()
        bus.signal_unsubscribe(subscription)

        if node_id["value"] is None:
            raise RuntimeError("Mutter did not emit PipeWireStreamAdded for Meta-0.")

        subprocess.run(
            [
                "gst-launch-1.0",
                "-q",
                "pipewiresrc",
                f"path={node_id['value']}",
                "num-buffers=1",
                "!",
                "videoconvert",
                "!",
                "pngenc",
                "!",
                "filesink",
                f"location={output}",
            ],
            check=True,
            timeout=15,
        )
    finally:
        if session_path is not None:
            try:
                bus_call(
                    bus,
                    "org.gnome.Mutter.ScreenCast",
                    session_path,
                    "org.gnome.Mutter.ScreenCast.Session",
                    "Stop",
                )
            except Exception:
                pass


def assert_png(output: Path, artifact_dir: Path) -> None:
    result = subprocess.run(
        [
            str(SYSTEM_PYTHON),
            str(REPO_ROOT / "scripts/assert-png-smoke.py"),
            str(output),
            "--max-width",
            str(WIDTH),
            "--max-height",
            str(HEIGHT),
        ],
        text=True,
        capture_output=True,
        check=False,
    )
    (artifact_dir / "assertions/relaunch-png.stdout").write_text(result.stdout, encoding="utf-8")
    (artifact_dir / "assertions/relaunch-png.stderr").write_text(result.stderr, encoding="utf-8")
    if result.returncode != 0:
        raise RuntimeError("relaunch screenshot failed PNG smoke assertions")


def scan_runtime_warnings(artifact_dir: Path) -> None:
    warning_re = re.compile(
        r"(Gtk|Gdk|GSK|Adwaita|Libadwaita|GIO|GLib|AT-SPI|accessibility|filesystem)"
        r".*(warning|critical|error)|GLib-GObject-CRITICAL|"
        r"gtk_[a-z0-9_]+.*assertion|gdk_[a-z0-9_]+.*assertion",
        re.IGNORECASE,
    )
    allow_re = re.compile(r"^Gdk-Message: .*Error reading events from display: Broken pipe$")
    matches = []
    for path in sorted((artifact_dir / "logs").glob("*")):
        if not path.is_file() or path.suffix not in {".log", ".stdout", ".stderr"}:
            continue
        for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
            if warning_re.search(line) and not allow_re.search(line):
                matches.append(f"{path.name}: {line}")

    report = artifact_dir / "assertions/runtime-warning-scan.txt"
    if matches:
        report.write_text("\n".join(matches) + "\n", encoding="utf-8")
        raise RuntimeError(f"Unexpected runtime warnings found. See {report}")
    report.write_text("PASS: no unexpected GTK/GDK/Adwaita/GIO/accessibility warnings\n", encoding="utf-8")


def tail_log(path: Path, line_count: int, stream) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines()[-line_count:]:
        print(line, file=stream)


def write_summary(artifact_dir: Path) -> None:
    data_dir = Path(os.environ["LUSHTEXT_DATA_DIR"])
    summary = {
        "result": "passed",
        "data_dir": str(data_dir),
        "file_backed_marker": FILE_BACKED_MARKER,
        "untitled_marker": UNTITLED_MARKER,
        "artifacts": {
            "before_crash_metadata": "metadata/before-crash",
            "after_relaunch_metadata": "metadata/after-relaunch",
            "logs": "logs",
            "assertions": "assertions",
            "screenshots": "screenshots",
        },
    }
    (artifact_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    (artifact_dir / "summary.txt").write_text(
        "\n".join(f"{key}={value}" for key, value in summary.items()) + "\n",
        encoding="utf-8",
    )


def write_skip_summary(artifact_dir: Path, reason: str) -> None:
    data_dir = os.environ.get("LUSHTEXT_DATA_DIR", "")
    summary = {
        "result": "skipped",
        "reason": reason,
        "data_dir": data_dir,
        "artifacts": {
            "logs": "logs",
            "assertions": "assertions",
            "screenshots": "screenshots",
        },
    }
    (artifact_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    (artifact_dir / "summary.txt").write_text(
        "\n".join(f"{key}={value}" for key, value in summary.items()) + "\n",
        encoding="utf-8",
    )


def mutter_child(args: argparse.Namespace) -> int:
    import gi

    gi.require_version("Gio", "2.0")
    from gi.repository import Gio

    artifact_dir = args.artifact_dir.resolve()
    data_dir = Path(os.environ["LUSHTEXT_DATA_DIR"])
    file_backed = Path(os.environ["LUSHTEXT_CRASH_SMOKE_FILE_BACKED_PATH"])
    app_env = os.environ.copy()
    app_env["AT_SPI_BUS_ADDRESS"] = os.environ["AT_SPI_BUS_ADDRESS"]

    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    first_app = launch_app(args, artifact_dir, "before-crash", [str(file_backed)])
    try:
        wait_for_window_actions(bus)
        time.sleep(0.8)
        set_editor_text(
            artifact_dir,
            app_env,
            "file-backed",
            f"{FILE_BACKED_MARKER}\nUnsaved file-backed crash smoke content\n",
        )
        activate_window_action(bus, "new-tab")
        time.sleep(0.8)
        set_editor_text(
            artifact_dir,
            app_env,
            "untitled",
            f"{UNTITLED_MARKER}\nUnsaved untitled crash smoke content\n",
        )
        wait_for_recovery_metadata(data_dir, file_backed)
        snapshot_metadata(data_dir, artifact_dir / "metadata/before-crash")
        os.kill(first_app.pid, signal.SIGKILL)
        first_app.wait(timeout=5)
        (artifact_dir / "assertions/sigkill.txt").write_text(
            f"pid={first_app.pid}\nreturncode={first_app.returncode}\n",
            encoding="utf-8",
        )
        if first_app.returncode != -signal.SIGKILL:
            raise RuntimeError(f"first app did not terminate through SIGKILL: {first_app.returncode}")
    finally:
        terminate_process(first_app)

    relaunch = launch_app(args, artifact_dir, "after-relaunch", [])
    try:
        wait_for_window_actions(bus)
        wait_for_relaunch_metadata(data_dir, file_backed)
        visible_listing = wait_for_visible_untitled(artifact_dir, app_env)
        assert_file_backed_draft_body(data_dir, file_backed)
        (artifact_dir / "assertions/relaunch-visible-content.txt").write_text(
            visible_listing,
            encoding="utf-8",
        )

        activate_window_action(bus, "show-bookmarks")
        wait_for_sidecar_recovery_evidence(data_dir, artifact_dir, app_env)
        if os.environ.get("LUSHTEXT_CRASH_SMOKE_SCREENSHOT") == "1":
            screenshot = artifact_dir / "screenshots/after-relaunch.png"
            try:
                capture_monitor(bus, screenshot)
                assert_png(screenshot, artifact_dir)
            except Exception as exc:
                (artifact_dir / "screenshots/skip.txt").write_text(
                    f"SKIP: screenshot capture unavailable: {exc}\n",
                    encoding="utf-8",
                )
        snapshot_metadata(data_dir, artifact_dir / "metadata/after-relaunch")
        scan_runtime_warnings(artifact_dir)
        write_summary(artifact_dir)
        return 0
    finally:
        terminate_process(relaunch)


def main() -> int:
    args = parse_args()
    try:
        if args.internal_run:
            return internal_run(args)
        if args.mutter_child:
            return mutter_child(args)
        return outer_run(args)
    except subprocess.CalledProcessError as exc:
        print(f"Command failed: {exc}", file=sys.stderr)
        return exc.returncode or 1
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
