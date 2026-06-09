#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Real-process D-Bus automation smoke driver for LushText."""

from __future__ import annotations

import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve()
REPO_ROOT = SCRIPT_PATH.parents[1]
SYSTEM_PYTHON = Path("/usr/bin/python3")
APP_ID = "dev.cominotti.lushtext"
APP_OBJECT_PATH = "/dev/cominotti/lushtext"
WINDOW_OBJECT_PATH = f"{APP_OBJECT_PATH}/window/1"
AUTOMATION_OBJECT_PATH = f"{APP_OBJECT_PATH}/Automation"
AUTOMATION_INTERFACE = "dev.cominotti.lushtext.Automation1"
SCENARIO_ID = "automation-dbus-smoke"
SCENARIO_DESCRIPTION = (
    "Real-process D-Bus automation smoke covering introspection, action catalog, "
    "readiness waits, snapshots, stateful action sync, and parameterized search."
)
SCENARIO_MANIFEST_NAME = "scenario-manifest.json"
SCENARIO_MANIFEST_FIELDS: tuple[str, ...] = (
    "schema_version",
    "scenario_id",
    "description",
    "status",
    "started_at",
    "updated_at",
    "finished_at",
    "failure_reason",
    "skip_reason",
    "launch_mode",
    "helper_arguments",
    "fixture_setup",
    "actions",
    "waits",
    "state_assertions",
    "screenshots",
    "at_spi_assertions",
    "dbus_summaries",
    "warnings",
    "environment",
    "bounded_artifact_policy",
    "steps",
)
MANIFEST_TEXT_LIMIT = 4096
WIDTH = 1280
HEIGHT = 860
WARNING_RE = re.compile(
    r"WARNING|CRITICAL|Gtk-ERROR|Gdk-Message|GDBus\.Error|GLib-GIO-ERROR|"
    r"assertion|panic|Segmentation fault|Trace/breakpoint trap"
)
BENIGN_WARNING_RE = re.compile(
    r"xdg-desktop-portal-WARNING \*\*: .*Choosing gtk\.portal .* deprecated UseIn key|"
    r"xdg-desktop-portal-WARNING \*\*: .*preferred method .*portals\.conf|"
    r"xdg-desktop-portal-WARNING \*\*: .*Failed to connect to PipeWire|"
    r"dbind-WARNING \*\*: .*Couldn't connect to accessibility bus|"
    r"Gtk-CRITICAL \*\*: .*Unable to connect to the accessibility bus|"
    r"Gdk-Message: .*Error reading events from display: Broken pipe"
)


def log_step(message: str) -> None:
    print(f"[automation-smoke] {message}", flush=True)


def now_utc() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def bounded_text(value: object | None) -> str | None:
    if value is None:
        return None
    text = str(value)
    if len(text) <= MANIFEST_TEXT_LIMIT:
        return text
    return f"{text[:MANIFEST_TEXT_LIMIT]} [truncated]"


class ScenarioManifest:
    """Bounded, append-only-enough manifest for reviewing one smoke scenario."""

    def __init__(self, artifact_dir: Path) -> None:
        self.artifact_dir = artifact_dir.resolve()
        self.path = self.artifact_dir / SCENARIO_MANIFEST_NAME
        if self.path.exists():
            self.data = json.loads(self.path.read_text(encoding="utf-8"))
        else:
            self.data = self._blank()

    def _blank(self) -> dict[str, object]:
        started_at = now_utc()
        return {
            "schema_version": 1,
            "scenario_id": SCENARIO_ID,
            "description": SCENARIO_DESCRIPTION,
            "status": "running",
            "started_at": started_at,
            "updated_at": started_at,
            "finished_at": None,
            "failure_reason": None,
            "skip_reason": None,
            "launch_mode": None,
            "helper_arguments": {},
            "fixture_setup": [],
            "actions": [],
            "waits": [],
            "state_assertions": [],
            "screenshots": [],
            "at_spi_assertions": [
                {
                    "name": "at-spi-visible-ui-assertions",
                    "status": "not-run",
                    "reason": "This D-Bus smoke lane disables AT-SPI with NO_AT_BRIDGE=1.",
                }
            ],
            "dbus_summaries": [],
            "warnings": {
                "status": "not-run",
                "artifact": "assertions/runtime-warning-scan.txt",
                "unexpected_count": None,
                "detail": None,
            },
            "environment": {},
            "bounded_artifact_policy": {
                "embedded_text_limit": MANIFEST_TEXT_LIMIT,
                "large_payload_strategy": "manifest stores relative artifact paths and bounded summaries",
            },
            "steps": [],
        }

    def reload(self) -> None:
        if self.path.exists():
            self.data = json.loads(self.path.read_text(encoding="utf-8"))

    def reset(self, args: argparse.Namespace, env: dict[str, str], *, launch_mode: str) -> None:
        self.data = self._blank()
        self.data["launch_mode"] = launch_mode
        self.data["helper_arguments"] = {
            "artifact_dir": str(args.artifact_dir),
            "binary": str(args.binary),
        }
        self.data["environment"] = {
            "app_id": APP_ID,
            "automation_object_path": AUTOMATION_OBJECT_PATH,
            "automation_interface": AUTOMATION_INTERFACE,
            "repo_root": str(REPO_ROOT),
            "binary": str(args.binary),
            "artifact_dir": str(args.artifact_dir),
            "virtual_monitor": f"{WIDTH}x{HEIGHT}",
            "gsettings_backend": env.get("GSETTINGS_BACKEND"),
            "gsettings_schema_dir": env.get("GSETTINGS_SCHEMA_DIR"),
            "gdk_backend": env.get("GDK_BACKEND"),
            "gtk_use_portal": env.get("GTK_USE_PORTAL"),
            "gsk_renderer": env.get("GSK_RENDERER"),
            "no_at_bridge": env.get("NO_AT_BRIDGE"),
            "xdg_cache_home": env.get("XDG_CACHE_HOME"),
            "xdg_config_home": env.get("XDG_CONFIG_HOME"),
            "xdg_data_home": env.get("XDG_DATA_HOME"),
            "xdg_runtime_dir": env.get("XDG_RUNTIME_DIR"),
        }
        self.save()

    def artifact(self, path: Path | str) -> str:
        path = Path(path)
        if not path.is_absolute():
            return path.as_posix()
        try:
            return path.resolve().relative_to(self.artifact_dir).as_posix()
        except ValueError:
            return str(path)

    def save(self) -> None:
        self.data["updated_at"] = now_utc()
        for field in SCENARIO_MANIFEST_FIELDS:
            self.data.setdefault(field, None)
        self.path.write_text(json.dumps(self.data, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    def record_fixture(self, *, name: str, path: Path, kind: str, detail: str) -> None:
        self.reload()
        fixtures = self.data.setdefault("fixture_setup", [])
        assert isinstance(fixtures, list)
        fixtures.append(
            {
                "name": name,
                "kind": kind,
                "artifact": self.artifact(path),
                "detail": bounded_text(detail),
            }
        )
        self.save()

    def update_environment(self, values: dict[str, str | None]) -> None:
        self.reload()
        environment = self.data.setdefault("environment", {})
        assert isinstance(environment, dict)
        environment.update(values)
        self.save()

    def begin_step(self, *, name: str, kind: str, detail: object | None = None, artifacts=None) -> int:
        self.reload()
        steps = self.data.setdefault("steps", [])
        assert isinstance(steps, list)
        index = len(steps) + 1
        steps.append(
            {
                "index": index,
                "name": name,
                "kind": kind,
                "status": "running",
                "started_at": now_utc(),
                "finished_at": None,
                "duration_ms": None,
                "detail": bounded_text(detail),
                "artifacts": [self.artifact(path) for path in (artifacts or [])],
            }
        )
        self.save()
        return index

    def finish_step(
        self,
        index: int,
        *,
        status: str,
        started_monotonic: float,
        detail: object | None = None,
        artifacts=None,
    ) -> None:
        self.reload()
        steps = self.data.setdefault("steps", [])
        assert isinstance(steps, list)
        row = next(step for step in steps if step["index"] == index)
        row["status"] = status
        row["finished_at"] = now_utc()
        row["duration_ms"] = int((time.monotonic() - started_monotonic) * 1000)
        if detail is not None:
            row["detail"] = bounded_text(detail)
        if artifacts:
            existing = list(row.get("artifacts") or [])
            existing.extend(self.artifact(path) for path in artifacts)
            row["artifacts"] = sorted(dict.fromkeys(existing))
        self.save()

    @contextmanager
    def step(self, name: str, kind: str, *, detail: object | None = None, artifacts=None):
        started_monotonic = time.monotonic()
        index = self.begin_step(name=name, kind=kind, detail=detail, artifacts=artifacts)
        try:
            yield
        except Exception as exc:
            self.finish_step(
                index,
                status="failed",
                started_monotonic=started_monotonic,
                detail=exc,
            )
            raise
        else:
            self.finish_step(index, status="passed", started_monotonic=started_monotonic)

    def record_action(
        self,
        *,
        action: str,
        object_path: str,
        parameters: object | None,
        status: str,
        detail: object | None = None,
        artifact: Path | str | None = None,
    ) -> None:
        self.reload()
        actions = self.data.setdefault("actions", [])
        assert isinstance(actions, list)
        actions.append(
            {
                "action": action,
                "object_path": object_path,
                "parameters": parameters,
                "status": status,
                "detail": bounded_text(detail),
                "artifact": self.artifact(artifact) if artifact else None,
            }
        )
        self.save()

    def record_wait(
        self,
        *,
        predicate: str,
        timeout_msec: int,
        ok: bool,
        status: str,
        detail: str,
        artifact: Path | str | None = None,
    ) -> None:
        self.reload()
        waits = self.data.setdefault("waits", [])
        assert isinstance(waits, list)
        waits.append(
            {
                "predicate": predicate,
                "timeout_msec": timeout_msec,
                "ok": ok,
                "status": status,
                "detail": bounded_text(detail),
                "artifact": self.artifact(artifact) if artifact else None,
            }
        )
        self.save()

    def record_state_assertion(
        self,
        *,
        name: str,
        status: str,
        detail: object | None = None,
        artifact: Path | str | None = None,
    ) -> None:
        self.reload()
        assertions = self.data.setdefault("state_assertions", [])
        assert isinstance(assertions, list)
        assertions.append(
            {
                "name": name,
                "status": status,
                "detail": bounded_text(detail),
                "artifact": self.artifact(artifact) if artifact else None,
            }
        )
        self.save()

    def record_dbus_summary(
        self,
        *,
        member: str,
        kind: str,
        status: str,
        detail: object | None = None,
        artifact: Path | str | None = None,
    ) -> None:
        self.reload()
        summaries = self.data.setdefault("dbus_summaries", [])
        assert isinstance(summaries, list)
        summaries.append(
            {
                "member": member,
                "kind": kind,
                "status": status,
                "detail": bounded_text(detail),
                "artifact": self.artifact(artifact) if artifact else None,
            }
        )
        self.save()

    def record_warning_scan(
        self,
        *,
        status: str,
        unexpected_count: int | None,
        detail: object | None = None,
        artifact: Path | str | None = None,
    ) -> None:
        self.reload()
        self.data["warnings"] = {
            "status": status,
            "artifact": self.artifact(artifact or "assertions/runtime-warning-scan.txt"),
            "unexpected_count": unexpected_count,
            "detail": bounded_text(detail),
        }
        self.save()

    def complete(self, status: str, *, reason: object | None = None) -> None:
        self.reload()
        self.data["status"] = status
        self.data["finished_at"] = now_utc()
        if status == "failed":
            self.data["failure_reason"] = bounded_text(reason)
        elif status == "skipped":
            self.data["skip_reason"] = bounded_text(reason)
        self.save()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-dir", required=True, type=Path)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--internal-run", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--mutter-child", action="store_true", help=argparse.SUPPRESS)
    return parser.parse_args()


def require_command(command: str) -> None:
    if shutil.which(command) is None:
        raise RuntimeError(f"unsupported-host-tooling: missing required command: {command}")


def validate_outer_args(args: argparse.Namespace) -> None:
    args.artifact_dir = args.artifact_dir.resolve()
    args.binary = args.binary.resolve()
    if not args.binary.is_file() or not os.access(args.binary, os.X_OK):
        raise RuntimeError(f"LushText binary is not executable: {args.binary}")
    if not SYSTEM_PYTHON.is_file():
        raise RuntimeError("Missing /usr/bin/python3")


def ensure_outer_tools() -> None:
    for command in ("dbus-run-session", "gdbus", "gsettings", "mutter"):
        require_command(command)
    result = subprocess.run(
        [str(SYSTEM_PYTHON), "-c", "import gi"],
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        raise RuntimeError("unsupported-host-tooling: /usr/bin/python3 cannot import gi")


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
    for name in ("assertions", "fixtures", "logs", "state/cache", "state/config", "state/data", "state/runtime"):
        (args.artifact_dir / name).mkdir(parents=True, exist_ok=True)
    os.chmod(args.artifact_dir / "state/runtime", 0o700)

    fixture = args.artifact_dir / "fixtures/automation-smoke.txt"
    fixture.write_text(
        "LushText automation smoke fixture\n\nneedle one\nneedle two\n",
        encoding="utf-8",
    )
    (args.artifact_dir / "fixtures/opened-file.txt").write_text(str(fixture) + "\n", encoding="utf-8")

    env = os.environ.copy()
    env.update(
        {
            "GSETTINGS_BACKEND": "keyfile",
            "GSETTINGS_SCHEMA_DIR": str(REPO_ROOT / "data"),
            "LUSHTEXT_AUTOMATION_SMOKE_ARTIFACT_DIR": str(args.artifact_dir),
            "LUSHTEXT_AUTOMATION_SMOKE_FILE": str(fixture),
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
    manifest = ScenarioManifest(args.artifact_dir)
    manifest.reset(args, env, launch_mode="dbus-run-session+headless-mutter")
    fixture = Path(env["LUSHTEXT_AUTOMATION_SMOKE_FILE"])
    manifest.record_fixture(
        name="file-backed search fixture",
        path=fixture,
        kind="text-file",
        detail="Tiny isolated file containing two occurrences of the query 'needle'.",
    )

    log_path = args.artifact_dir / "logs/dbus-session.log"
    command = [
        "dbus-run-session",
        "--",
        str(SYSTEM_PYTHON),
        str(SCRIPT_PATH),
        *child_cli_args(args, "internal-run"),
    ]
    with manifest.step("launch private D-Bus session", "launch", artifacts=[log_path]):
        with log_path.open("w", encoding="utf-8") as log:
            result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)

    if result.returncode != 0:
        tail_log(log_path, 160, sys.stderr)
        manifest.complete("failed", reason=f"child process exited with status {result.returncode}")
        return result.returncode

    warning_report = args.artifact_dir / "assertions/runtime-warning-scan.txt"
    try:
        with manifest.step("scan runtime warnings", "warning-scan", artifacts=[warning_report]):
            scan_runtime_warnings(args.artifact_dir)
    except Exception as exc:
        manifest.record_warning_scan(
            status="failed",
            unexpected_count=None,
            detail=exc,
            artifact=warning_report,
        )
        manifest.complete("failed", reason=exc)
        raise
    manifest.record_warning_scan(
        status="passed",
        unexpected_count=0,
        detail="No unexpected GTK/GDK/Libadwaita/GIO/D-Bus/portal/AT-SPI/filesystem warnings.",
        artifact=warning_report,
    )
    manifest.complete("passed")
    print(f"PASS: automation D-Bus smoke completed. Artifacts: {args.artifact_dir}")
    return 0


def tail_log(log_path: Path, line_count: int, stream) -> None:
    lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
    for line in lines[-line_count:]:
        print(line, file=stream)


def scan_runtime_warnings(artifact_dir: Path) -> None:
    findings: list[str] = []
    for log_path in sorted((artifact_dir / "logs").glob("*.log")):
        for line_no, line in enumerate(
            log_path.read_text(encoding="utf-8", errors="replace").splitlines(),
            start=1,
        ):
            if WARNING_RE.search(line) and not BENIGN_WARNING_RE.search(line):
                findings.append(f"{log_path.relative_to(artifact_dir)}:{line_no}: {line}")

    report = artifact_dir / "assertions/runtime-warning-scan.txt"
    if findings:
        report.write_text("\n".join(findings) + "\n", encoding="utf-8")
        raise RuntimeError(f"Unexpected runtime warnings found. See {report}")
    report.write_text(
        "PASS: no unexpected GTK/GDK/Libadwaita/GIO/D-Bus/portal/AT-SPI/filesystem warnings\n",
        encoding="utf-8",
    )


def internal_run(args: argparse.Namespace) -> int:
    artifact_dir = args.artifact_dir.resolve()
    manifest = ScenarioManifest(artifact_dir)
    env = os.environ.copy()
    env["GDK_BACKEND"] = "wayland"
    env["NO_AT_BRIDGE"] = "1"
    env["GSK_RENDERER"] = env.get("GSK_RENDERER", "cairo")
    env["GTK_USE_PORTAL"] = "0"
    manifest.update_environment(
        {
            "gdk_backend": env.get("GDK_BACKEND"),
            "gtk_use_portal": env.get("GTK_USE_PORTAL"),
            "gsk_renderer": env.get("GSK_RENDERER"),
            "no_at_bridge": env.get("NO_AT_BRIDGE"),
        }
    )

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
    log_path = artifact_dir / "logs/mutter-child.log"
    with manifest.step("launch headless Mutter compositor", "launch", artifacts=[log_path]):
        with log_path.open("w", encoding="utf-8") as log:
            result = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
    print(log_path.read_text(encoding="utf-8", errors="replace"))
    if result.returncode != 0:
        manifest.complete("failed", reason=f"mutter child exited with status {result.returncode}")
    return result.returncode


def start_logged(command: list[str], log_path: Path, env: dict[str, str]) -> subprocess.Popen:
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


def wait_for_automation_object(bus) -> None:
    deadline = time.monotonic() + 15
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            bus_call(
                bus,
                APP_ID,
                AUTOMATION_OBJECT_PATH,
                "org.freedesktop.DBus.Introspectable",
                "Introspect",
                reply="(s)",
            )
            return
        except Exception as exc:
            last_error = exc
            time.sleep(0.1)
    raise RuntimeError(f"automation-unavailable: LushText did not export the automation object: {last_error}")


def wait_for_window_actions(bus) -> None:
    deadline = time.monotonic() + 15
    last_error: Exception | None = None
    while time.monotonic() < deadline:
        try:
            bus_call(
                bus,
                APP_ID,
                WINDOW_OBJECT_PATH,
                "org.gtk.Actions",
                "List",
                reply="(as)",
            )
            return
        except Exception as exc:
            last_error = exc
            time.sleep(0.1)
    raise RuntimeError(f"automation-unavailable: LushText did not export window actions: {last_error}")


def automation_call(bus, method: str, params=None, reply: str = "(s)"):
    return bus_call(bus, APP_ID, AUTOMATION_OBJECT_PATH, AUTOMATION_INTERFACE, method, params, reply)


def wait_for_idle(bus, timeout_msec: int) -> tuple[bool, str]:
    from gi.repository import GLib

    ok, detail = automation_call(
        bus,
        "WaitForIdle",
        GLib.Variant("(u)", (timeout_msec,)),
        "(bs)",
    ).unpack()
    return bool(ok), str(detail)


def wait_for_ready(bus, predicate: str, timeout_msec: int) -> tuple[bool, str, str]:
    from gi.repository import GLib

    ok, status, detail = automation_call(
        bus,
        "WaitForReady",
        GLib.Variant("(su)", (predicate, timeout_msec)),
        "(bss)",
    ).unpack()
    return bool(ok), str(status), str(detail)


def write_wait_result(path: Path, *, ok: bool, status: str, detail: str) -> None:
    path.write_text(
        f"ok={ok}\nstatus={status}\ndetail={detail}\n",
        encoding="utf-8",
    )


def assert_ready_wait(
    bus,
    manifest: ScenarioManifest,
    *,
    predicate: str,
    timeout_msec: int,
    artifact: Path,
    failure_label: str,
) -> tuple[bool, str, str]:
    ok, status, detail = wait_for_ready(bus, predicate, timeout_msec)
    write_wait_result(artifact, ok=ok, status=status, detail=detail)
    manifest.record_wait(
        predicate=predicate,
        timeout_msec=timeout_msec,
        ok=ok,
        status=status,
        detail=detail,
        artifact=artifact,
    )
    if not ok:
        raise RuntimeError(f"{failure_label}: status={status} detail={detail}")
    return ok, status, detail


def gdbus_call(args: list[str], *, timeout: int = 10) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["gdbus", "call", "--session", *args],
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )


def checked_gdbus_call(args: list[str], label: str) -> subprocess.CompletedProcess[str]:
    result = gdbus_call(args)
    if result.returncode != 0:
        raise RuntimeError(
            f"{label} failed with status {result.returncode}: "
            f"{result.stderr.strip() or result.stdout.strip()}"
    )
    return result


def run_automation_client(
    artifact_dir: Path,
    name: str,
    args: list[str],
    *,
    expected_statuses: set[str] | None = None,
) -> dict[str, object]:
    expected_statuses = expected_statuses or {"ok"}
    result = subprocess.run(
        [str(SYSTEM_PYTHON), str(REPO_ROOT / "scripts/lushtext-automation.py"), *args, "--json"],
        text=True,
        capture_output=True,
        timeout=15,
        check=False,
    )
    artifact = artifact_dir / f"assertions/client-{name}.json"
    artifact.write_text(result.stdout or result.stderr, encoding="utf-8")
    if result.returncode != 0:
        raise RuntimeError(
            f"automation client {name} failed with status {result.returncode}: "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    payload = json.loads(result.stdout)
    status = payload.get("status")
    if status not in expected_statuses:
        raise RuntimeError(
            f"automation client {name} returned status {status!r}, "
            f"expected one of {sorted(expected_statuses)}"
        )
    return payload


def activate_window_action(action_name: str, parameter_array: str = "[]") -> str:
    result = checked_gdbus_call(
        [
            "--dest",
            APP_ID,
            "--object-path",
            WINDOW_OBJECT_PATH,
            "--method",
            "org.gtk.Actions.Activate",
            action_name,
            parameter_array,
            "{}",
        ],
        f"activate {action_name}",
    )
    return result.stdout


def window_action_metadata(action_name: str) -> dict[str, object]:
    list_result = dbus_action_list(WINDOW_OBJECT_PATH)
    describe_result = checked_gdbus_call(
        [
            "--dest",
            APP_ID,
            "--object-path",
            WINDOW_OBJECT_PATH,
            "--method",
            "org.gtk.Actions.Describe",
            action_name,
        ],
        f"describe {action_name}",
    )
    return {
        "action": action_name,
        "list_stdout": list_result.strip(),
        "describe_stdout": describe_result.stdout.strip(),
        "exported": action_name in list_result,
    }


def dbus_action_list(object_path: str) -> str:
    return checked_gdbus_call(
        [
            "--dest",
            APP_ID,
            "--object-path",
            object_path,
            "--method",
            "org.gtk.Actions.List",
        ],
        f"list actions on {object_path}",
    ).stdout


def assert_catalog_exports_match_dbus(catalog: list[dict], artifact_dir: Path) -> None:
    app_list = dbus_action_list(APP_OBJECT_PATH)
    window_list = dbus_action_list(WINDOW_OBJECT_PATH)
    (artifact_dir / "assertions/dbus-app-actions.txt").write_text(app_list, encoding="utf-8")
    (artifact_dir / "assertions/dbus-window-actions.txt").write_text(window_list, encoding="utf-8")

    missing: list[str] = []
    for entry in catalog:
        if entry.get("exposure") != "exported":
            continue
        action_id = str(entry.get("action_id", ""))
        name = str(entry.get("name", ""))
        if action_id.startswith("app."):
            haystack = app_list
        elif action_id.startswith("win."):
            haystack = window_list
        else:
            continue
        if f"'{name}'" not in haystack:
            missing.append(action_id)

    if missing:
        raise RuntimeError(f"D-Bus action list is missing exported catalog actions: {missing}")


def dbus_describe_action(object_path: str, action_name: str) -> str:
    return checked_gdbus_call(
        [
            "--dest",
            APP_ID,
            "--object-path",
            object_path,
            "--method",
            "org.gtk.Actions.Describe",
            action_name,
        ],
        f"describe {action_name} on {object_path}",
    ).stdout


def dbus_action_state(bus, object_path: str, action_name: str):
    from gi.repository import GLib

    description = bus_call(
        bus,
        APP_ID,
        object_path,
        "org.gtk.Actions",
        "Describe",
        GLib.Variant("(s)", (action_name,)),
    ).unpack()[0]
    _enabled, _parameter_type, state_values = description
    if not state_values:
        return None
    state = state_values[0]
    return state.unpack() if hasattr(state, "unpack") else state


def nested_snapshot_value(snapshot: dict, path: tuple[str, ...]):
    value = snapshot
    for key in path:
        value = value[key]
    return value


def assert_action_state_matches_snapshot(
    bus,
    artifact_dir: Path,
    *,
    state_action: str,
    target_action: str,
    snapshot_path: tuple[str, ...],
    manifest: ScenarioManifest | None = None,
) -> list[dict[str, object]]:
    results: list[dict[str, object]] = []
    for desired in (True, False):
        log_step(f"activate {target_action}={desired}")
        activate_window_action(target_action, f"[<{str(desired).lower()}>]")
        if manifest is not None:
            manifest.record_action(
                action=target_action,
                object_path=WINDOW_OBJECT_PATH,
                parameters={"desired": desired},
                status="passed",
            )
        ok, status, detail = wait_for_ready(bus, "idle", 5000)
        if manifest is not None:
            manifest.record_wait(
                predicate="idle",
                timeout_msec=5000,
                ok=ok,
                status=status,
                detail=detail,
            )
        if not ok:
            raise RuntimeError(
                f"WaitForReady(idle) after {target_action}={desired} "
                f"did not settle: status={status} detail={detail}"
            )

        snapshot = wait_for_snapshot_predicate(
            bus,
            f"{target_action}={desired}",
            lambda current: nested_snapshot_value(current, snapshot_path) is desired,
        )
        action_state = dbus_action_state(bus, WINDOW_OBJECT_PATH, state_action)
        snapshot_state = nested_snapshot_value(snapshot, snapshot_path)
        if action_state is not desired or snapshot_state is not desired:
            raise RuntimeError(
                f"{state_action} state mismatch after {target_action}={desired}: "
                f"action_state={action_state!r}, snapshot_state={snapshot_state!r}"
            )
        results.append(
            {
                "state_action": state_action,
                "target_action": target_action,
                "desired": desired,
                "action_state": action_state,
                "snapshot_path": ".".join(snapshot_path),
                "snapshot_state": snapshot_state,
            }
        )

    (artifact_dir / f"assertions/action-state-{state_action}.json").write_text(
        json.dumps(results, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return results


def assert_stateful_actions_match_snapshot(
    bus, artifact_dir: Path, manifest: ScenarioManifest | None = None
) -> list[dict[str, object]]:
    checks = [
        (
            "toggle-sidebar",
            "set-sidebar-visible",
            ("window", "surfaces", "workspace_sidebar_visible"),
        ),
        (
            "toggle-properties",
            "set-properties-visible",
            ("window", "surfaces", "document_properties_visible"),
        ),
        (
            "toggle-minimap",
            "set-minimap-visible",
            ("window", "surfaces", "minimap_requested"),
        ),
        (
            "toggle-focus-mode",
            "set-focus-mode",
            ("window", "surfaces", "focus_mode"),
        ),
        (
            "toggle-preview-pane",
            "set-preview-pane-visible",
            ("window", "surfaces", "preview_pane_visible"),
        ),
        (
            "toggle-preview-mode",
            "set-preview-mode",
            ("window", "surfaces", "preview_mode"),
        ),
    ]

    results: list[dict[str, object]] = []
    for state_action, target_action, snapshot_path in checks:
        results.extend(
            assert_action_state_matches_snapshot(
                bus,
                artifact_dir,
                state_action=state_action,
                target_action=target_action,
                snapshot_path=snapshot_path,
                manifest=manifest,
            )
        )
    artifact = artifact_dir / "assertions/action-state-snapshot-sync.json"
    write_json(artifact, results)
    if manifest is not None:
        manifest.record_state_assertion(
            name="stateful action state matches automation snapshot",
            status="passed",
            detail=f"{len(results)} state/action comparisons passed.",
            artifact=artifact,
        )
    return results


def write_json(path: Path, data) -> None:
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def assert_snapshot_baseline(snapshot: dict, fixture: Path) -> None:
    assert snapshot["interface_version"] == 1, snapshot["interface_version"]
    assert snapshot["enabled"] is True, snapshot["enabled"]
    assert snapshot["app_id"] == APP_ID, snapshot["app_id"]
    assert snapshot["idle"] is True, snapshot["idle_blocker"]
    window = snapshot["window"]
    assert window["tab_count"] >= 1, window["tab_count"]
    assert window["active_tab_index"] == 0, window["active_tab_index"]
    first_tab = window["tabs"][0]
    assert first_tab["document_kind"] == "file", first_tab
    assert first_tab["path"] == str(fixture), first_tab
    assert first_tab["load_state"] == "loaded", first_tab
    assert first_tab["modified"] is False, first_tab


def assert_search_snapshot(snapshot: dict) -> None:
    window = snapshot["window"]
    assert window["surfaces"]["active_transient_surface"] == "editor-search", window["surfaces"]
    assert window["search"]["editor_search_visible"] is True, window["search"]
    assert window["search"]["editor_query"] == "needle", window["search"]
    assert window["search"]["editor_match_count"] == 2, window["search"]


def assert_workflow_events_snapshot(snapshot: dict) -> None:
    assert isinstance(snapshot["last_sequence"], int), snapshot
    assert isinstance(snapshot["capped"], bool), snapshot
    assert isinstance(snapshot["events"], list), snapshot
    for event in snapshot["events"]:
        assert isinstance(event["sequence"], int), event
        assert isinstance(event["workflow_id"], str), event
        assert event["phase"] in {"started", "finished"}, event
        assert event["status"] in {"running", "settled"}, event
        assert isinstance(event["summary"], str), event
        assert event["blocker"] is None or isinstance(event["blocker"], str), event


def snapshot_json(bus) -> dict:
    return json.loads(automation_call(bus, "GetSnapshot").unpack()[0])


def wait_for_snapshot_predicate(bus, description: str, predicate) -> dict:
    deadline = time.monotonic() + 5
    last_snapshot: dict | None = None
    while time.monotonic() < deadline:
        last_snapshot = snapshot_json(bus)
        if predicate(last_snapshot):
            return last_snapshot
        time.sleep(0.1)
    raise RuntimeError(
        f"Timed out waiting for snapshot predicate {description}: "
        f"{json.dumps(last_snapshot, sort_keys=True)[:1000]}"
    )


def mutter_child(args: argparse.Namespace) -> int:
    import gi

    gi.require_version("Gio", "2.0")
    gi.require_version("GLib", "2.0")
    from gi.repository import Gio

    artifact_dir = args.artifact_dir.resolve()
    manifest = ScenarioManifest(artifact_dir)
    fixture = Path(os.environ["LUSHTEXT_AUTOMATION_SMOKE_FILE"]).resolve()
    app_env = os.environ.copy()
    app_env.update(
        {
            "GDK_BACKEND": "wayland",
            "GSETTINGS_BACKEND": "keyfile",
            "GSETTINGS_SCHEMA_DIR": str(REPO_ROOT / "data"),
            "GSK_RENDERER": app_env.get("GSK_RENDERER", "cairo"),
            "GTK_USE_PORTAL": "0",
            "NO_AT_BRIDGE": "1",
        }
    )
    app_env.pop("AT_SPI_BUS_ADDRESS", None)
    manifest.update_environment(
        {
            "gdk_backend": app_env.get("GDK_BACKEND"),
            "gtk_use_portal": app_env.get("GTK_USE_PORTAL"),
            "gsk_renderer": app_env.get("GSK_RENDERER"),
            "no_at_bridge": app_env.get("NO_AT_BRIDGE"),
        }
    )

    app = None
    try:
        app_log = artifact_dir / "logs/lushtext.log"
        pid_artifact = artifact_dir / "app.pid"
        with manifest.step("launch LushText with fixture", "launch", artifacts=[app_log, pid_artifact]):
            app = start_logged(
                [str(args.binary), str(fixture)],
                app_log,
                app_env,
            )
            pid_artifact.write_text(str(app.pid) + "\n", encoding="utf-8")
            print(f"Launched PID: {app.pid}", flush=True)

        bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        with manifest.step("wait for automation object", "wait"):
            log_step("waiting for automation object")
            wait_for_automation_object(bus)
        with manifest.step("wait for window actions", "wait"):
            log_step("waiting for window actions")
            wait_for_window_actions(bus)

        log_step("read automation introspection")
        introspection_artifact = artifact_dir / "assertions/introspection.xml"
        with manifest.step(
            "read automation introspection",
            "dbus",
            artifacts=[introspection_artifact],
        ):
            introspection = bus_call(
                bus,
                APP_ID,
                AUTOMATION_OBJECT_PATH,
                "org.freedesktop.DBus.Introspectable",
                "Introspect",
                reply="(s)",
            ).unpack()[0]
            introspection_artifact.write_text(introspection, encoding="utf-8")
            for member in (
                "InterfaceVersion",
                "Enabled",
                "BuildProfile",
                "GetActionCatalog",
            "GetSnapshot",
            "GetReadinessPredicates",
            "GetWorkflowEvents",
            "WaitForReady",
            "WaitForIdle",
        ):
                if member not in introspection:
                    raise RuntimeError(f"automation introspection is missing {member}")
        manifest.record_dbus_summary(
            member="org.freedesktop.DBus.Introspectable.Introspect",
            kind="method",
            status="passed",
            detail="Automation1 introspection contains required properties and methods.",
            artifact=introspection_artifact,
        )

        log_step("read action catalog")
        catalog_artifact = artifact_dir / "assertions/action-catalog.json"
        with manifest.step("read action catalog", "dbus", artifacts=[catalog_artifact]):
            catalog_json = automation_call(bus, "GetActionCatalog").unpack()[0]
            catalog = json.loads(catalog_json)
            write_json(catalog_artifact, catalog)
            if not any(entry.get("action_id") == "win.set-search-query" for entry in catalog):
                raise RuntimeError("action catalog did not include win.set-search-query")
            assert_catalog_exports_match_dbus(catalog, artifact_dir)
        manifest.record_dbus_summary(
            member="GetActionCatalog",
            kind="method",
            status="passed",
            detail=f"{len(catalog)} catalog entries read and checked against org.gtk.Actions.",
            artifact=catalog_artifact,
        )
        manifest.record_dbus_summary(
            member="org.gtk.Actions.List app",
            kind="method",
            status="passed",
            artifact=artifact_dir / "assertions/dbus-app-actions.txt",
        )
        manifest.record_dbus_summary(
            member="org.gtk.Actions.List window",
            kind="method",
            status="passed",
            artifact=artifact_dir / "assertions/dbus-window-actions.txt",
        )
        manifest.record_state_assertion(
            name="catalog exported actions match D-Bus action lists",
            status="passed",
            artifact=catalog_artifact,
        )

        log_step("read readiness predicates")
        predicates_artifact = artifact_dir / "assertions/readiness-predicates.json"
        with manifest.step("read readiness predicates", "dbus", artifacts=[predicates_artifact]):
            predicates_json = automation_call(bus, "GetReadinessPredicates").unpack()[0]
            readiness_predicates = json.loads(predicates_json)
            write_json(predicates_artifact, readiness_predicates)
            predicate_names = {entry.get("predicate") for entry in readiness_predicates}
            for predicate in (
                "app-startup",
                "window-actions-exported",
                "file-open-complete",
                "search-complete",
                "save-complete",
                "workspace-refresh-complete",
                "session-restore-complete",
                "recovery-restore-complete",
                "idle",
            ):
                if predicate not in predicate_names:
                    raise RuntimeError(f"readiness predicate list is missing {predicate}")
        manifest.record_dbus_summary(
            member="GetReadinessPredicates",
            kind="method",
            status="passed",
            detail=f"{len(readiness_predicates)} readiness predicates read.",
            artifact=predicates_artifact,
        )

        with manifest.step("describe representative GTK actions", "dbus"):
            app_describe = artifact_dir / "assertions/dbus-describe-app-quit.txt"
            win_describe = artifact_dir / "assertions/dbus-describe-win-set-search-query.txt"
            action_metadata = artifact_dir / "assertions/window-action-set-search-query.json"
            app_describe.write_text(
                dbus_describe_action(APP_OBJECT_PATH, "quit"),
                encoding="utf-8",
            )
            win_describe.write_text(
                dbus_describe_action(WINDOW_OBJECT_PATH, "set-search-query"),
                encoding="utf-8",
            )
            write_json(action_metadata, window_action_metadata("set-search-query"))
        manifest.record_dbus_summary(
            member="org.gtk.Actions.Describe app.quit",
            kind="method",
            status="passed",
            artifact=app_describe,
        )
        manifest.record_dbus_summary(
            member="org.gtk.Actions.Describe win.set-search-query",
            kind="method",
            status="passed",
            artifact=win_describe,
        )

        log_step("wait for app startup readiness")
        with manifest.step("wait for app startup readiness", "wait"):
            assert_ready_wait(
                bus,
                manifest,
                predicate="app-startup",
                timeout_msec=5000,
                artifact=artifact_dir / "assertions/wait-app-startup.txt",
                failure_label="WaitForReady(app-startup) did not settle",
            )

        log_step("wait for window action export readiness")
        with manifest.step("wait for window action export readiness", "wait"):
            assert_ready_wait(
                bus,
                manifest,
                predicate="window-actions-exported",
                timeout_msec=5000,
                artifact=artifact_dir / "assertions/wait-window-actions-exported.txt",
                failure_label="WaitForReady(window-actions-exported) did not settle",
            )

        log_step("wait for opened file readiness")
        with manifest.step("wait for opened file readiness", "wait"):
            assert_ready_wait(
                bus,
                manifest,
                predicate="file-open-complete",
                timeout_msec=5000,
                artifact=artifact_dir / "assertions/wait-file-open-complete.txt",
                failure_label="WaitForReady(file-open-complete) did not settle",
            )

        log_step("wait for initial idle")
        with manifest.step("wait for initial idle", "wait"):
            assert_ready_wait(
                bus,
                manifest,
                predicate="idle",
                timeout_msec=5000,
                artifact=artifact_dir / "assertions/wait-initial.txt",
                failure_label="WaitForReady(idle) did not settle",
            )
        with manifest.step("wait for legacy idle compatibility", "wait"):
            legacy_artifact = artifact_dir / "assertions/wait-legacy-idle.txt"
            legacy_ok, legacy_detail = wait_for_idle(bus, 5000)
            legacy_artifact.write_text(
                f"ok={legacy_ok}\ndetail={legacy_detail}\n",
                encoding="utf-8",
            )
            manifest.record_wait(
                predicate="legacy-idle",
                timeout_msec=5000,
                ok=legacy_ok,
                status="ready" if legacy_ok else "predicate-timeout",
                detail=legacy_detail,
                artifact=legacy_artifact,
            )
            if not legacy_ok:
                raise RuntimeError(f"legacy WaitForIdle did not settle: {legacy_detail}")

        log_step("read initial snapshot")
        initial_snapshot_artifact = artifact_dir / "assertions/snapshot-initial.json"
        with manifest.step("read and assert initial snapshot", "state-assertion"):
            initial_snapshot = snapshot_json(bus)
            assert_snapshot_baseline(initial_snapshot, fixture)
            write_json(initial_snapshot_artifact, initial_snapshot)
        manifest.record_dbus_summary(
            member="GetSnapshot initial",
            kind="method",
            status="passed",
            artifact=initial_snapshot_artifact,
        )
        manifest.record_state_assertion(
            name="initial file-backed snapshot baseline",
            status="passed",
            detail=f"tab_count={initial_snapshot['window']['tab_count']}",
            artifact=initial_snapshot_artifact,
        )

        log_step("check stateful action states")
        with manifest.step("check stateful action states", "state-assertion"):
            action_state_results = assert_stateful_actions_match_snapshot(bus, artifact_dir, manifest)

        log_step("run automation client sanity checks")
        with manifest.step("run automation client sanity checks", "dbus"):
            client_results = {
                "catalog": run_automation_client(artifact_dir, "catalog", ["catalog"]),
                "snapshot": run_automation_client(artifact_dir, "snapshot", ["snapshot"]),
                "snapshot-tab-count": run_automation_client(
                    artifact_dir,
                    "snapshot-tab-count",
                    ["snapshot", "--field", "window.tab_count"],
                ),
                "predicates": run_automation_client(artifact_dir, "predicates", ["predicates"]),
                "wait-idle": run_automation_client(
                    artifact_dir,
                    "wait-idle",
                    ["wait", "idle", "--timeout-ms", "5000"],
                    expected_statuses={"ready"},
                ),
                "events": run_automation_client(artifact_dir, "events", ["events"]),
                "action-set-search-query": run_automation_client(
                    artifact_dir,
                    "action-set-search-query",
                    ["action", "win.set-search-query", "--string", "needle"],
                ),
            }
            wait_after_client = run_automation_client(
                artifact_dir,
                "wait-search-complete-client",
                ["wait", "search-complete", "--timeout-ms", "5000"],
                expected_statuses={"ready"},
            )
            client_results["wait-search-complete"] = wait_after_client
            write_json(artifact_dir / "assertions/client-sanity-summary.json", client_results)
        manifest.record_dbus_summary(
            member="scripts/lushtext-automation.py",
            kind="client",
            status="passed",
            detail="Client catalog, snapshot, field extraction, predicates, wait, events, and action commands passed against the live app.",
            artifact=artifact_dir / "assertions/client-sanity-summary.json",
        )
        manifest.record_action(
            action="win.set-search-query",
            object_path=WINDOW_OBJECT_PATH,
            parameters={"query": "needle", "client": "scripts/lushtext-automation.py"},
            status="passed",
            artifact=artifact_dir / "assertions/client-action-set-search-query.json",
        )

        log_step("activate set-search-query")
        activate_artifact = artifact_dir / "assertions/activate-set-search-query.txt"
        with manifest.step("activate set-search-query action", "action", artifacts=[activate_artifact]):
            activate_stdout = activate_window_action("set-search-query", "[<'needle'>]")
            activate_artifact.write_text(
                activate_stdout,
                encoding="utf-8",
            )
        manifest.record_action(
            action="set-search-query",
            object_path=WINDOW_OBJECT_PATH,
            parameters={"query": "needle"},
            status="passed",
            artifact=activate_artifact,
        )
        log_step("wait after set-search-query")
        with manifest.step("wait after set-search-query", "wait"):
            assert_ready_wait(
                bus,
                manifest,
                predicate="search-complete",
                timeout_msec=5000,
                artifact=artifact_dir / "assertions/wait-after-search.txt",
                failure_label="WaitForReady(search-complete) after set-search-query did not settle",
            )

        search_snapshot_artifact = artifact_dir / "assertions/snapshot-after-search.json"
        with manifest.step("assert search snapshot", "state-assertion"):
            search_snapshot = wait_for_snapshot_predicate(
                bus,
                "editor search query and match count",
                lambda snapshot: snapshot["window"]["search"]["editor_search_visible"]
                and snapshot["window"]["search"]["editor_query"] == "needle"
                and snapshot["window"]["search"]["editor_match_count"] == 2,
            )
            write_json(search_snapshot_artifact, search_snapshot)
            assert_search_snapshot(search_snapshot)
        manifest.record_dbus_summary(
            member="GetSnapshot after search",
            kind="method",
            status="passed",
            artifact=search_snapshot_artifact,
        )
        manifest.record_state_assertion(
            name="search query and match count",
            status="passed",
            detail="query=needle match_count=2",
            artifact=search_snapshot_artifact,
        )

        workflow_events_artifact = artifact_dir / "assertions/workflow-events.json"
        with manifest.step("read workflow events", "dbus", artifacts=[workflow_events_artifact]):
            workflow_events_json = automation_call(bus, "GetWorkflowEvents").unpack()[0]
            workflow_events = json.loads(workflow_events_json)
            assert_workflow_events_snapshot(workflow_events)
            write_json(workflow_events_artifact, workflow_events)
        manifest.record_dbus_summary(
            member="GetWorkflowEvents",
            kind="method",
            status="passed",
            detail=f"{len(workflow_events.get('events', []))} workflow events retained.",
            artifact=workflow_events_artifact,
        )
        manifest.record_state_assertion(
            name="workflow event snapshot schema",
            status="passed",
            detail="last_sequence, capped, and event row fields matched Automation1 contract",
            artifact=workflow_events_artifact,
        )

        summary = {
            "status": "passed",
            "app_id": APP_ID,
            "object_path": AUTOMATION_OBJECT_PATH,
            "interface": AUTOMATION_INTERFACE,
            "scenario_manifest": SCENARIO_MANIFEST_NAME,
            "fixture": str(fixture),
            "catalog_entries": len(catalog),
            "action_state_sync_checks": len(action_state_results),
            "initial_tab_count": initial_snapshot["window"]["tab_count"],
            "search_match_count": search_snapshot["window"]["search"]["editor_match_count"],
            "workflow_event_count": len(workflow_events.get("events", [])),
        }
        summary_artifact = artifact_dir / "summary.json"
        write_json(summary_artifact, summary)
        manifest.record_state_assertion(
            name="automation smoke summary",
            status="passed",
            detail="automation object, catalog, snapshot, reusable client, wait, and parameterized action succeeded",
            artifact=summary_artifact,
        )
        manifest.complete("passed")
        print("PASS: automation object, catalog, snapshot, wait, and parameterized action succeeded", flush=True)
        return 0
    except Exception as exc:
        manifest.complete("failed", reason=exc)
        raise
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
    except AssertionError as exc:
        print(f"Assertion failed: {exc}", file=sys.stderr)
        return 1
    except subprocess.CalledProcessError as exc:
        print(f"Command failed: {exc}", file=sys.stderr)
        return exc.returncode or 1
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
