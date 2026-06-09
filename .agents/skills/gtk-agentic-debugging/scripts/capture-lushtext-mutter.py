#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve()
REPO_ROOT = SCRIPT_PATH.parents[4]
SYSTEM_PYTHON = Path("/usr/bin/python3")
ATSPI_REGISTRYD = Path("/usr/libexec/at-spi2-registryd")


def usage_binary() -> Path:
    return REPO_ROOT / "target/debug/lushtext"


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return parsed


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Launch LushText under an isolated headless Mutter session, drive "
            "optional search text through D-Bus plus AT-SPI, and capture the "
            "Mutter monitor to a PNG."
        )
    )
    parser.add_argument("--file", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--search")
    parser.add_argument("--enable-minimap", action="store_true")
    parser.add_argument(
        "--enable-atspi",
        action="store_true",
        help="Start the private AT-SPI registry even when no search text is set.",
    )
    parser.add_argument(
        "--window-action",
        action="append",
        default=[],
        help="Window action to activate before capture; may be repeated.",
    )
    parser.add_argument(
        "--color-scheme",
        choices=("default", "force-light", "force-dark"),
        default="default",
        help="LushText color-scheme GSettings value to apply before launch.",
    )
    parser.add_argument(
        "--capture-artifact-dir",
        type=Path,
        help="Directory for the helper's internal logs instead of a temporary directory.",
    )
    parser.add_argument(
        "--atspi-tree-output",
        type=Path,
        help="Write a bounded AT-SPI tree subset for the launched app.",
    )
    parser.add_argument(
        "--atspi-focus-output",
        type=Path,
        help="Write the focused accessible node path for the launched app.",
    )
    parser.add_argument("--binary", type=Path, default=usage_binary())
    parser.add_argument("--width", type=positive_int, default=1600)
    parser.add_argument("--height", type=positive_int, default=1000)
    parser.add_argument("--keep-artifacts", action="store_true")
    parser.add_argument("--internal-run", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--mutter-child", action="store_true", help=argparse.SUPPRESS)
    return parser.parse_args()


def require_command(command: str) -> None:
    if shutil.which(command) is None:
        raise RuntimeError(
            f"Missing required command: {command}. Run make dev-tools inside the Toolbx/container."
        )


def validate_args(args: argparse.Namespace) -> None:
    args.file = args.file.resolve()
    args.output = args.output.resolve()
    args.binary = args.binary.resolve()

    if not args.file.is_file():
        raise RuntimeError(f"File to open does not exist: {args.file}")
    if not args.binary.is_file() or not os.access(args.binary, os.X_OK):
        raise RuntimeError(f"LushText binary is not executable: {args.binary}")
    if not SYSTEM_PYTHON.is_file():
        raise RuntimeError("Missing /usr/bin/python3. Run make dev-tools inside the Toolbx/container.")


def cleanup_runtime_root(runtime_root: Path, artifact_dir: Path) -> str:
    errors: list[str] = []
    for attempt in range(1, 6):
        try:
            shutil.rmtree(runtime_root)
        except FileNotFoundError:
            return "removed"
        except Exception as exc:
            errors.append(f"attempt {attempt}: {type(exc).__name__}: {exc}")
        if not runtime_root.exists():
            return "removed"
        time.sleep(0.2)

    remaining: list[str] = []
    for path in runtime_root.rglob("*"):
        try:
            remaining.append(str(path.relative_to(runtime_root)))
        except ValueError:
            remaining.append(str(path))
    (artifact_dir / "runtime-dir-cleanup-errors.txt").write_text(
        "\n".join([*errors, "remaining:", *sorted(remaining)]) + "\n",
        encoding="utf-8",
    )
    return "remove_failed"


def child_cli_args(args: argparse.Namespace, mode: str) -> list[str]:
    cli = [
        f"--{mode}",
        "--file",
        str(args.file),
        "--output",
        str(args.output),
        "--binary",
        str(args.binary),
        "--width",
        str(args.width),
        "--height",
        str(args.height),
    ]
    if args.search is not None:
        cli.extend(["--search", args.search])
    if args.enable_minimap:
        cli.append("--enable-minimap")
    if args.enable_atspi:
        cli.append("--enable-atspi")
    if args.keep_artifacts:
        cli.append("--keep-artifacts")
    if args.color_scheme != "default":
        cli.extend(["--color-scheme", args.color_scheme])
    for action in args.window_action:
        cli.extend(["--window-action", action])
    if args.capture_artifact_dir is not None:
        cli.extend(["--capture-artifact-dir", str(args.capture_artifact_dir)])
    if args.atspi_tree_output is not None:
        cli.extend(["--atspi-tree-output", str(args.atspi_tree_output)])
    if args.atspi_focus_output is not None:
        cli.extend(["--atspi-focus-output", str(args.atspi_focus_output)])
    return cli


def outer_run(args: argparse.Namespace) -> int:
    validate_args(args)
    for command in (
        "dbus-run-session",
        "gdbus",
        "gsettings",
        "gst-launch-1.0",
        "mutter",
        "pipewire",
        "pw-dump",
        "wireplumber",
    ):
        require_command(command)

    if args.search is not None or args.enable_atspi or args.atspi_tree_output is not None:
        if not ATSPI_REGISTRYD.is_file():
            raise RuntimeError("Missing at-spi2-registryd. Run make dev-tools inside the Toolbx/container.")
        subprocess.run(
            [str(SYSTEM_PYTHON), "-c", "import gi, pyatspi"],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

    if args.capture_artifact_dir is None:
        artifact_dir = Path(tempfile.mkdtemp(prefix="lushtext-mutter-debug."))
        remove_artifacts_after_run = not args.keep_artifacts
    else:
        artifact_dir = args.capture_artifact_dir.resolve()
        artifact_dir.mkdir(parents=True, exist_ok=True)
        remove_artifacts_after_run = False
    for name in ("data", "config", "cache"):
        (artifact_dir / name).mkdir(exist_ok=True)
    # PipeWire uses Unix sockets, so keep XDG_RUNTIME_DIR short even when
    # callers preserve artifacts under a deeply nested checkout path.
    runtime_root = Path(tempfile.mkdtemp(prefix="lt-rt-"))
    runtime_dir = runtime_root / "runtime"
    runtime_dir.mkdir()
    os.chmod(runtime_dir, 0o700)
    (artifact_dir / "runtime-dir.txt").write_text(str(runtime_dir) + "\n", encoding="utf-8")
    (artifact_dir / "runtime-dir-status.txt").write_text(
        f"path={runtime_dir}\nstatus=active\ncleanup=pending\n",
        encoding="utf-8",
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env.update(
        {
            "GSETTINGS_BACKEND": "keyfile",
            "GSETTINGS_SCHEMA_DIR": str(REPO_ROOT / "data"),
            "LUSHTEXT_MUTTER_ARTIFACT_DIR": str(artifact_dir),
            "XDG_CACHE_HOME": str(artifact_dir / "cache"),
            "XDG_CONFIG_HOME": str(artifact_dir / "config"),
            "XDG_DATA_HOME": str(artifact_dir / "data"),
            "XDG_RUNTIME_DIR": str(runtime_dir),
        }
    )

    log_path = artifact_dir / "session.log"
    command = [
        "dbus-run-session",
        "--",
        str(SYSTEM_PYTHON),
        str(SCRIPT_PATH),
        *child_cli_args(args, "internal-run"),
    ]
    with log_path.open("w", encoding="utf-8") as log:
        result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)

    if result.returncode == 0:
        cleanup_status = cleanup_runtime_root(runtime_root, artifact_dir)
        (artifact_dir / "runtime-dir-status.txt").write_text(
            f"path={runtime_dir}\nstatus=success\ncleanup={cleanup_status}\n",
            encoding="utf-8",
        )
        print_interesting_log_lines(log_path)
        if shutil.which("file") is not None:
            subprocess.run(["file", str(args.output)], check=False)
        print(f"Screenshot saved to {args.output}")
        if not remove_artifacts_after_run:
            print(f"Artifacts kept in {artifact_dir}")
        else:
            shutil.rmtree(artifact_dir, ignore_errors=True)
        return 0

    (artifact_dir / "runtime-dir-status.txt").write_text(
        f"path={runtime_dir}\nstatus=failed\ncleanup=retained\n",
        encoding="utf-8",
    )
    print(f"Headless Mutter capture failed. Artifacts kept in {artifact_dir}", file=sys.stderr)
    print(f"Runtime diagnostics kept in {runtime_dir}", file=sys.stderr)
    print("Last session log lines:", file=sys.stderr)
    tail_log(log_path, line_count=100, stream=sys.stderr)
    return result.returncode


def print_interesting_log_lines(log_path: Path) -> None:
    interesting = (
        "Launched PID:",
        "AT-SPI",
        "PipeWire node:",
        "Mutter monitor capture complete",
    )
    for line in log_path.read_text(encoding="utf-8", errors="replace").splitlines():
        if any(marker in line for marker in interesting):
            print(line)


def tail_log(log_path: Path, *, line_count: int, stream) -> None:
    lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
    for line in lines[-line_count:]:
        print(line, file=stream)


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
        stdout=(artifact_dir / "atspi-enable.log").open("wb"),
        stderr=subprocess.STDOUT,
        check=False,
    )

    registry = start_logged(
        [str(ATSPI_REGISTRYD), "--dbus-name", "org.a11y.atspi.Registry"],
        artifact_dir / "atspi-registry.log",
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
    artifact_dir = Path(os.environ["LUSHTEXT_MUTTER_ARTIFACT_DIR"])
    runtime_dir = Path(os.environ["XDG_RUNTIME_DIR"])
    processes: list[subprocess.Popen | None] = []
    atspi_address: str | None = None

    try:
        pipewire = start_logged(["pipewire"], artifact_dir / "pipewire.log")
        processes.append(pipewire)
        wait_for_pipewire(runtime_dir)

        wireplumber = start_logged(["wireplumber"], artifact_dir / "wireplumber.log")
        processes.append(wireplumber)

        if args.search is not None or args.enable_atspi or args.atspi_tree_output is not None:
            atspi_address, registry = setup_atspi(artifact_dir)
            processes.append(registry)
            os.environ["AT_SPI_BUS_ADDRESS"] = atspi_address

        if args.enable_minimap:
            subprocess.run(
                ["gsettings", "set", "dev.cominotti.lushtext", "show-minimap", "true"],
                check=True,
            )
        if args.color_scheme != "default":
            subprocess.run(
                [
                    "gsettings",
                    "set",
                    "dev.cominotti.lushtext",
                    "color-scheme",
                    args.color_scheme,
                ],
                check=True,
            )

        env = os.environ.copy()
        if atspi_address is None:
            env["NO_AT_BRIDGE"] = "1"
            env.pop("AT_SPI_BUS_ADDRESS", None)

        command = [
            "mutter",
            "--headless",
            "--wayland",
            "--no-x11",
            "--virtual-monitor",
            f"{args.width}x{args.height}",
            "--",
            str(SYSTEM_PYTHON),
            str(SCRIPT_PATH),
            *child_cli_args(args, "mutter-child"),
        ]
        with (artifact_dir / "mutter-child.log").open("w", encoding="utf-8") as log:
            result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        print((artifact_dir / "mutter-child.log").read_text(encoding="utf-8", errors="replace"))
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


def set_search_text(args: argparse.Namespace, artifact_dir: Path, env: dict[str, str]) -> None:
    result = subprocess.run(
        [
            str(SYSTEM_PYTHON),
            str(REPO_ROOT / ".agents/skills/gtk-agentic-debugging/scripts/atspi-set-text.py"),
            "--application-regex",
            "^lushtext$",
            "--role-regex",
            "^entry$",
            "--text",
            args.search or "",
            "--timeout",
            "10",
        ],
        text=True,
        capture_output=True,
        env=env,
        timeout=15,
    )
    (artifact_dir / "atspi-set-text.stdout").write_text(result.stdout, encoding="utf-8")
    (artifact_dir / "atspi-set-text.stderr").write_text(result.stderr, encoding="utf-8")
    print(f"AT-SPI set-text status: {result.returncode}")
    if result.stdout.strip():
        print(f"AT-SPI set-text stdout: {result.stdout.strip()}")
    if result.stderr.strip():
        print(f"AT-SPI set-text stderr: {result.stderr.strip()}")
    if result.returncode != 0:
        raise RuntimeError("AT-SPI could not set the LushText search entry text.")


def dump_atspi_tree(args: argparse.Namespace, artifact_dir: Path, env: dict[str, str]) -> None:
    if args.atspi_tree_output is None:
        return

    focus_output = args.atspi_focus_output or (artifact_dir / "atspi-focus.txt")
    args.atspi_tree_output.parent.mkdir(parents=True, exist_ok=True)
    focus_output.parent.mkdir(parents=True, exist_ok=True)
    result = subprocess.run(
        [
            str(SYSTEM_PYTHON),
            str(REPO_ROOT / ".agents/skills/gtk-agentic-debugging/scripts/atspi-dump-tree.py"),
            "--application-regex",
            "^lushtext$",
            "--output",
            str(args.atspi_tree_output),
            "--focus-output",
            str(focus_output),
            "--timeout",
            "10",
        ],
        text=True,
        capture_output=True,
        env=env,
        timeout=15,
    )
    (artifact_dir / "atspi-dump-tree.stdout").write_text(result.stdout, encoding="utf-8")
    (artifact_dir / "atspi-dump-tree.stderr").write_text(result.stderr, encoding="utf-8")
    print(f"AT-SPI dump-tree status: {result.returncode}")
    if result.stdout.strip():
        print(f"AT-SPI dump-tree stdout: {result.stdout.strip()}")
    if result.stderr.strip():
        print(f"AT-SPI dump-tree stderr: {result.stderr.strip()}")
    if result.returncode != 0:
        raise RuntimeError("AT-SPI tree dump failed.")


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
            GLib.Variant(
                "(sa{sv})",
                ("Meta-0", {"is-recording": GLib.Variant("b", True)}),
            ),
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

        print(f"PipeWire node: {node_id['value']}")
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
        print("Mutter monitor capture complete")
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


def mutter_child(args: argparse.Namespace) -> int:
    import gi

    gi.require_version("Gio", "2.0")
    from gi.repository import Gio

    artifact_dir = Path(os.environ["LUSHTEXT_MUTTER_ARTIFACT_DIR"])
    app_env = os.environ.copy()
    if app_env.get("NO_AT_BRIDGE") == "1":
        app_env.pop("AT_SPI_BUS_ADDRESS", None)
    app_env.update(
        {
            "GDK_BACKEND": "wayland",
            "GSETTINGS_BACKEND": "keyfile",
            "GSETTINGS_SCHEMA_DIR": str(REPO_ROOT / "data"),
            "GSK_RENDERER": app_env.get("GSK_RENDERER", "cairo"),
            "GTK_USE_PORTAL": "0",
        }
    )

    app = subprocess.Popen(
        [str(args.binary), str(args.file)],
        stdout=(artifact_dir / "lushtext.stdout").open("wb"),
        stderr=(artifact_dir / "lushtext.stderr").open("wb"),
        env=app_env,
    )
    (artifact_dir / "app.pid").write_text(str(app.pid), encoding="utf-8")
    print(f"Launched PID: {app.pid}", flush=True)

    try:
        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        wait_for_window_actions(bus)
        if args.search is not None:
            activate_window_action(bus, "begin-search")
            time.sleep(0.8)
            set_search_text(args, artifact_dir, app_env)
            time.sleep(0.5)
        for action_name in args.window_action:
            activate_window_action(bus, action_name)
            print(f"Activated window action: {action_name}", flush=True)
            time.sleep(0.8)
        if args.search is None and not args.window_action:
            time.sleep(0.8)
        dump_atspi_tree(args, artifact_dir, app_env)
        capture_monitor(bus, args.output)
        return 0
    finally:
        terminate_process(app)


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
