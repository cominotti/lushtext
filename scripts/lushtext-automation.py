#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Developer and agent CLI for LushText's D-Bus automation surface."""

from __future__ import annotations

import argparse
import ast
import json
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


APP_ID = "dev.cominotti.lushtext"
APP_OBJECT_PATH = "/dev/cominotti/lushtext"
WINDOW_OBJECT_PATH = f"{APP_OBJECT_PATH}/window/1"
AUTOMATION_OBJECT_PATH = f"{APP_OBJECT_PATH}/Automation"
AUTOMATION_INTERFACE = "dev.cominotti.lushtext.Automation1"
CLIENT_TEXT_LIMIT = 4096

CLIENT_COMMANDS = (
    "introspect",
    "catalog",
    "snapshot",
    "predicates",
    "events",
    "wait",
    "action",
    "artifact-summary",
    "self-test",
)
CLIENT_STATUSES = (
    "ok",
    "ready",
    "usage-error",
    "unsupported-host-tooling",
    "automation-unavailable",
    "dbus-error",
    "unknown-predicate",
    "unknown-action",
    "unsupported-action",
    "parameter-mismatch",
    "predicate-timeout",
    "workflow-failure",
    "artifact-error",
    "artifact-skipped",
)
RESULT_FIELDS = ("ok", "status", "command", "detail", "data")
EXIT_CODES = {
    "ok": 0,
    "ready": 0,
    "artifact-skipped": 0,
    "predicate-timeout": 1,
    "workflow-failure": 1,
    "dbus-error": 1,
    "artifact-error": 1,
    "usage-error": 2,
    "unknown-predicate": 2,
    "unknown-action": 2,
    "unsupported-action": 2,
    "parameter-mismatch": 2,
    "automation-unavailable": 3,
    "unsupported-host-tooling": 4,
}
ARTIFACT_SUMMARY_FIELDS = (
    "artifact_dir",
    "status",
    "scenario_id",
    "failure_reason",
    "skip_reason",
    "manifest",
    "summary",
    "runtime_warning_scan",
    "workflow_events",
    "snapshots",
    "dbus_artifacts",
    "state_assertions",
    "waits",
    "actions",
)
CLIENT_FLAGS = (
    "--bus-name",
    "--object-path",
    "--interface",
    "--window-path",
    "--timeout-ms",
    "--json",
    "--field",
    "--string",
    "--bool",
    "--uint32",
    "--variant-json",
)


@dataclass(frozen=True)
class ClientResult:
    command: str
    status: str
    detail: str
    data: Any = None

    @property
    def ok(self) -> bool:
        return self.status in {"ok", "ready", "artifact-skipped"}

    @property
    def exit_code(self) -> int:
        return EXIT_CODES.get(self.status, 1)

    def envelope(self) -> dict[str, Any]:
        return {
            "ok": self.ok,
            "status": self.status,
            "command": self.command,
            "detail": self.detail,
            "data": self.data,
        }


def success(command: str, detail: str, data: Any = None, *, status: str = "ok") -> ClientResult:
    return ClientResult(command=command, status=status, detail=bounded_text(detail), data=data)


def failure(command: str, status: str, detail: str, data: Any = None) -> ClientResult:
    if status not in CLIENT_STATUSES:
        status = "workflow-failure"
    return ClientResult(command=command, status=status, detail=bounded_text(detail), data=data)


def bounded_text(value: object) -> str:
    text = str(value)
    if len(text) <= CLIENT_TEXT_LIMIT:
        return text
    return f"{text[:CLIENT_TEXT_LIMIT]} [truncated]"


def require_gdbus(command: str) -> ClientResult | None:
    if shutil.which("gdbus") is None:
        return failure(command, "unsupported-host-tooling", "gdbus is not available on PATH")
    return None


def app_object_path_from_automation(object_path: str) -> str:
    return object_path.removesuffix("/Automation") or APP_OBJECT_PATH


def run_gdbus_call(
    args: argparse.Namespace,
    command: str,
    *,
    object_path: str,
    method: str,
    parameters: list[str] | None = None,
    status_on_error: str = "automation-unavailable",
) -> tuple[ClientResult | None, str]:
    if missing := require_gdbus(command):
        return missing, ""

    invocation = [
        "gdbus",
        "call",
        "--session",
        "--dest",
        args.bus_name,
        "--object-path",
        object_path,
        "--method",
        method,
        *(parameters or []),
    ]
    try:
        result = subprocess.run(
            invocation,
            check=False,
            capture_output=True,
            text=True,
            timeout=max(1.0, args.timeout_ms / 1000 + 1),
        )
    except subprocess.TimeoutExpired:
        return failure(command, status_on_error, f"{method} timed out"), ""

    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or f"{method} failed"
        return failure(command, status_on_error, detail), ""
    return None, result.stdout


def parse_single_string_tuple(output: str) -> str:
    value = ast.literal_eval(output.strip())
    if (
        isinstance(value, tuple)
        and len(value) == 1
        and isinstance(value[0], str)
    ):
        return value[0]
    raise ValueError(f"expected single string tuple, got {output.strip()!r}")


WAIT_READY_RE = re.compile(
    r"^\(\s*(true|false),\s*('(?:[^'\\]|\\.)*'),\s*('(?:[^'\\]|\\.)*')\s*\)\s*$",
    re.DOTALL,
)
WAIT_IDLE_RE = re.compile(
    r"^\(\s*(true|false),\s*('(?:[^'\\]|\\.)*')\s*\)\s*$",
    re.DOTALL,
)


def parse_wait_ready_tuple(output: str) -> tuple[bool, str, str]:
    match = WAIT_READY_RE.match(output.strip())
    if not match:
        raise ValueError(f"expected WaitForReady tuple, got {output.strip()!r}")
    return (
        match.group(1) == "true",
        ast.literal_eval(match.group(2)),
        ast.literal_eval(match.group(3)),
    )


def parse_wait_idle_tuple(output: str) -> tuple[bool, str]:
    match = WAIT_IDLE_RE.match(output.strip())
    if not match:
        raise ValueError(f"expected WaitForIdle tuple, got {output.strip()!r}")
    return match.group(1) == "true", ast.literal_eval(match.group(2))


def call_json_method(args: argparse.Namespace, command: str, method: str) -> ClientResult:
    error, output = run_gdbus_call(
        args,
        command,
        object_path=args.object_path,
        method=f"{args.interface}.{method}",
    )
    if error:
        return error
    try:
        return success(
            command,
            f"{method} returned JSON",
            json.loads(parse_single_string_tuple(output)),
        )
    except (ValueError, SyntaxError, json.JSONDecodeError) as exc:
        return failure(command, "dbus-error", str(exc), {"stdout": output})


def command_introspect(args: argparse.Namespace) -> ClientResult:
    error, output = run_gdbus_call(
        args,
        "introspect",
        object_path=args.object_path,
        method="org.freedesktop.DBus.Introspectable.Introspect",
    )
    if error:
        return error
    try:
        return success("introspect", "automation introspection read", parse_single_string_tuple(output))
    except (ValueError, SyntaxError) as exc:
        return failure("introspect", "dbus-error", str(exc), {"stdout": output})


def command_catalog(args: argparse.Namespace) -> ClientResult:
    return call_json_method(args, "catalog", "GetActionCatalog")


def command_snapshot(args: argparse.Namespace) -> ClientResult:
    return call_json_method(args, "snapshot", "GetSnapshot")


def command_predicates(args: argparse.Namespace) -> ClientResult:
    return call_json_method(args, "predicates", "GetReadinessPredicates")


def command_events(args: argparse.Namespace) -> ClientResult:
    return call_json_method(args, "events", "GetWorkflowEvents")


def command_wait(args: argparse.Namespace) -> ClientResult:
    predicate = args.predicate or "idle"
    if predicate == "legacy-idle":
        error, output = run_gdbus_call(
            args,
            "wait",
            object_path=args.object_path,
            method=f"{args.interface}.WaitForIdle",
            parameters=[f"uint32 {args.timeout_ms}"],
        )
        if error:
            return error
        try:
            ok, detail = parse_wait_idle_tuple(output)
        except (ValueError, SyntaxError) as exc:
            return failure("wait", "dbus-error", str(exc), {"stdout": output})
        data = {"predicate": predicate, "ok": ok, "status": "ready" if ok else "predicate-timeout", "detail": detail}
    else:
        error, output = run_gdbus_call(
            args,
            "wait",
            object_path=args.object_path,
            method=f"{args.interface}.WaitForReady",
            parameters=[predicate, f"uint32 {args.timeout_ms}"],
        )
        if error:
            return error
        try:
            ok, status, detail = parse_wait_ready_tuple(output)
        except (ValueError, SyntaxError) as exc:
            return failure("wait", "dbus-error", str(exc), {"stdout": output})
        data = {"predicate": predicate, "ok": ok, "status": status, "detail": detail}

    if data["ok"]:
        return success("wait", f"{predicate} is ready", data, status="ready")
    status = str(data["status"])
    if status in {"unknown-predicate", "workflow-failure", "predicate-timeout"}:
        return failure("wait", status, str(data["detail"]), data)
    return failure("wait", "predicate-timeout", str(data["detail"]), data)


def gvariant_string(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace("'", "\\'")
    return f"'{escaped}'"


def gvariant_for_json_value(value: Any) -> str:
    if isinstance(value, bool):
        return f"<{str(value).lower()}>"
    if isinstance(value, int) and 0 <= value <= 2**32 - 1:
        return f"<uint32 {value}>"
    if isinstance(value, str):
        return f"<{gvariant_string(value)}>"
    raise ValueError(f"unsupported variant-json value {value!r}; use string, bool, or u32")


def parameter_array_for_action(args: argparse.Namespace, row: dict[str, Any]) -> tuple[ClientResult | None, str]:
    parameter_type = row.get("parameter_type")
    supplied = [
        name
        for name, value in (
            ("--string", args.string),
            ("--bool", args.bool_value),
            ("--uint32", args.uint32),
            ("--variant-json", args.variant_json),
        )
        if value is not None
    ]
    if parameter_type == "none":
        if supplied:
            return failure("action", "parameter-mismatch", f"{row['action_id']} takes no parameter"), ""
        return None, "[]"
    if len(supplied) != 1:
        return (
            failure(
                "action",
                "parameter-mismatch",
                f"{row['action_id']} requires exactly one {parameter_type} parameter",
            ),
            "",
        )
    if parameter_type == "string" and args.string is not None:
        return None, f"[<{gvariant_string(args.string)}>]"
    if parameter_type == "bool" and args.bool_value is not None:
        return None, f"[<{str(args.bool_value).lower()}>]"
    if parameter_type == "u32" and args.uint32 is not None:
        if args.uint32 < 0 or args.uint32 > 2**32 - 1:
            return failure("action", "parameter-mismatch", "--uint32 is out of range"), ""
        return None, f"[<uint32 {args.uint32}>]"
    if parameter_type == "variant-map" and args.variant_json is not None:
        try:
            values = json.loads(args.variant_json)
            if not isinstance(values, dict):
                raise ValueError("variant JSON must be an object")
            items = ", ".join(
                f"{gvariant_string(str(key))}: {gvariant_for_json_value(value)}"
                for key, value in sorted(values.items())
            )
        except (json.JSONDecodeError, ValueError) as exc:
            return failure("action", "parameter-mismatch", str(exc)), ""
        return None, f"[<{{{items}}}>]"
    return (
        failure(
            "action",
            "parameter-mismatch",
            f"{supplied[0]} does not match catalog parameter type {parameter_type}",
        ),
        "",
    )


def catalog_row_for_action(args: argparse.Namespace, action: str) -> tuple[ClientResult | None, dict[str, Any] | None]:
    catalog = command_catalog(args)
    if not catalog.ok:
        return catalog, None
    action_id = action if "." in action else f"win.{action}"
    for row in catalog.data:
        if row.get("action_id") == action_id:
            if row.get("exposure") != "exported":
                return (
                    failure("action", "unsupported-action", f"{action_id} is {row.get('exposure')}"),
                    None,
                )
            if row.get("external_activation") == "unsupported-gap":
                return (
                    failure("action", "unsupported-action", f"{action_id} is not externally activatable"),
                    None,
                )
            if not action_id.startswith(("app.", "win.")):
                return (
                    failure("action", "unsupported-action", f"{action_id} is not on an app/window action group"),
                    None,
                )
            return None, row
    return failure("action", "unknown-action", f"{action_id} is not in the action catalog"), None


def command_action(args: argparse.Namespace) -> ClientResult:
    row_error, row = catalog_row_for_action(args, args.action_id)
    if row_error:
        return row_error
    assert row is not None
    parameter_error, parameter_array = parameter_array_for_action(args, row)
    if parameter_error:
        return parameter_error

    action_id = str(row["action_id"])
    object_path = args.window_path if action_id.startswith("win.") else app_object_path_from_automation(args.object_path)
    error, output = run_gdbus_call(
        args,
        "action",
        object_path=object_path,
        method="org.gtk.Actions.Activate",
        parameters=[str(row["name"]), parameter_array, "{}"],
        status_on_error="dbus-error",
    )
    if error:
        return error
    return success(
        "action",
        f"activated {action_id}",
        {
            "action_id": action_id,
            "object_path": object_path,
            "parameter_array": parameter_array,
            "stdout": output.strip(),
        },
    )


def artifact_text(path: Path | None) -> str | None:
    if path is None or not path.is_file():
        return None
    return bounded_text(path.read_text(encoding="utf-8", errors="replace"))


def summarize_artifacts(artifact_dir: Path) -> ClientResult:
    manifest_path = artifact_dir / "scenario-manifest.json"
    if not manifest_path.is_file():
        return summarize_generic_artifacts(artifact_dir)
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return failure("artifact-summary", "artifact-error", str(exc), {"manifest": str(manifest_path)})

    status = str(manifest.get("status") or "unknown")
    summary_path = artifact_dir / "summary.json"
    warning_path = artifact_dir / "assertions/runtime-warning-scan.txt"
    dbus_artifacts = sorted(
        path.relative_to(artifact_dir).as_posix()
        for path in (artifact_dir / "assertions").glob("*")
        if any(token in path.name for token in ("dbus", "catalog", "snapshot", "workflow", "introspection"))
    )
    workflow_events = workflow_event_summary(artifact_dir)
    snapshots = sorted(
        path.relative_to(artifact_dir).as_posix()
        for path in (artifact_dir / "assertions").glob("*snapshot*.json")
    )
    data = {
        "artifact_dir": str(artifact_dir),
        "status": status,
        "scenario_id": manifest.get("scenario_id"),
        "failure_reason": manifest.get("failure_reason"),
        "skip_reason": manifest.get("skip_reason"),
        "manifest": str(manifest_path),
        "summary": json.loads(summary_path.read_text(encoding="utf-8")) if summary_path.is_file() else None,
        "runtime_warning_scan": artifact_text(warning_path),
        "workflow_events": workflow_events,
        "snapshots": snapshots,
        "dbus_artifacts": dbus_artifacts,
        "state_assertions": manifest.get("state_assertions", []),
        "waits": manifest.get("waits", []),
        "actions": manifest.get("actions", []),
    }
    if status == "failed":
        return failure("artifact-summary", "artifact-error", "scenario manifest reports failure", data)
    if status == "passed":
        return success("artifact-summary", "scenario manifest reports success", data)
    if status == "skipped":
        return success(
            "artifact-summary",
            "scenario manifest reports skipped",
            data,
            status="artifact-skipped",
        )
    return failure("artifact-summary", "artifact-error", f"unknown scenario status: {status}", data)


def command_artifact_summary(args: argparse.Namespace) -> ClientResult:
    return summarize_artifacts(args.artifact_dir.resolve())


def summarize_generic_artifacts(artifact_dir: Path) -> ClientResult:
    summary_path = artifact_dir / "summary.json"
    manifests = sorted((artifact_dir / "assertions").glob("*-manifest.json"))
    if not summary_path.is_file() and not manifests:
        return failure(
            "artifact-summary",
            "artifact-error",
            f"{artifact_dir} has no recognized summary or manifest",
        )

    manifest_rows: list[dict[str, Any]] = []
    for manifest_path in manifests:
        try:
            payload = json.loads(manifest_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            manifest_rows.append(
                {
                    "artifact": manifest_path.relative_to(artifact_dir).as_posix(),
                    "status": "parse-error",
                    "detail": bounded_text(exc),
                }
            )
            continue
        manifest_rows.append(
            {
                "artifact": manifest_path.relative_to(artifact_dir).as_posix(),
                "scenario_id": payload.get("scenario_id"),
                "status": payload.get("status"),
                "failure_reason": payload.get("failure_reason"),
                "skip_reason": payload.get("skip_reason"),
            }
        )

    summary_payload = None
    if summary_path.is_file():
        try:
            summary_payload = json.loads(summary_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            return failure(
                "artifact-summary",
                "artifact-error",
                str(exc),
                {"summary": str(summary_path)},
            )

    data = {
        "artifact_dir": str(artifact_dir),
        "status": "generic",
        "scenario_id": None,
        "failure_reason": None,
        "skip_reason": None,
        "manifest": [row["artifact"] for row in manifest_rows],
        "summary": summary_payload,
        "runtime_warning_scan": artifact_text(artifact_dir / "assertions/runtime-warning-scan.txt"),
        "workflow_events": workflow_event_summary(artifact_dir),
        "snapshots": sorted(
            path.relative_to(artifact_dir).as_posix()
            for path in (artifact_dir / "assertions").glob("*snapshot*.json")
        ),
        "dbus_artifacts": sorted(
            path.relative_to(artifact_dir).as_posix()
            for path in (artifact_dir / "assertions").glob("*")
            if any(token in path.name for token in ("dbus", "catalog", "snapshot", "workflow", "introspection"))
        ),
        "state_assertions": manifest_rows,
        "waits": [],
        "actions": [],
    }

    if any(row.get("status") in {"failed", "parse-error"} for row in manifest_rows):
        return failure("artifact-summary", "artifact-error", "generic smoke artifact includes a failed manifest", data)
    if any(row.get("status") == "skipped" for row in manifest_rows):
        return success("artifact-summary", "generic smoke artifact includes a skipped manifest", data, status="artifact-skipped")
    return success("artifact-summary", "generic smoke artifact summary read", data)


def workflow_event_summary(artifact_dir: Path) -> dict[str, Any] | None:
    for path in (
        artifact_dir / "assertions/workflow-events.json",
        artifact_dir / "workflow-events.json",
    ):
        if not path.is_file():
            continue
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            return {"artifact": path.relative_to(artifact_dir).as_posix(), "parse_error": True}
        events = payload.get("events")
        return {
            "artifact": path.relative_to(artifact_dir).as_posix(),
            "last_sequence": payload.get("last_sequence"),
            "capped": payload.get("capped"),
            "event_count": len(events) if isinstance(events, list) else None,
        }
    return None


def select_field(data: Any, field: str) -> Any:
    parts = field.split(".")

    def select(value: Any, remaining: list[str]) -> Any:
        if not remaining:
            return value
        head, *tail = remaining
        if isinstance(value, list):
            return [select(item, remaining) for item in value]
        if isinstance(value, dict) and head in value:
            return select(value[head], tail)
        raise KeyError(field)

    return select(data, parts)


def apply_field(result: ClientResult, field: str | None) -> ClientResult:
    if field is None or not result.ok:
        return result
    try:
        return success(result.command, f"{result.detail}; selected field {field}", select_field(result.data, field))
    except KeyError:
        return failure(result.command, "usage-error", f"field not found: {field}", result.data)


def print_result(args: argparse.Namespace, result: ClientResult) -> None:
    if args.json:
        print(json.dumps(result.envelope(), indent=2, sort_keys=True))
        return
    if result.ok and isinstance(result.data, str):
        print(result.data)
    elif result.data is not None:
        print(json.dumps(result.data, indent=2, sort_keys=True))
    else:
        print(f"{result.status}: {result.detail}")


def parse_bool(value: str) -> bool:
    normalized = value.strip().lower()
    if normalized in {"1", "true", "yes", "on"}:
        return True
    if normalized in {"0", "false", "no", "off"}:
        return False
    raise argparse.ArgumentTypeError("expected true or false")


def command_self_test(_args: argparse.Namespace) -> ClientResult:
    try:
        parsed = parse_args(["action", "win.show-help-overlay"])
        assert parsed.command == "action"
        assert parsed.action_id == "win.show-help-overlay"
        envelope = success("self-test", "ok").envelope()
        assert tuple(envelope.keys()) == RESULT_FIELDS
        assert envelope["status"] == "ok"
        assert failure("self-test", "parameter-mismatch", "bad").exit_code == 2
        assert failure("self-test", "automation-unavailable", "bad").exit_code == 3
        assert failure("self-test", "unsupported-host-tooling", "bad").exit_code == 4
        assert bounded_text("x" * (CLIENT_TEXT_LIMIT + 1)).endswith(" [truncated]")
        assert parse_single_string_tuple("('hello',)\n") == "hello"
        assert parse_wait_ready_tuple("(true, 'ready', 'idle')") == (True, "ready", "idle")
        assert parse_wait_idle_tuple("(false, 'busy')") == (False, "busy")
        fake_args = argparse.Namespace(
            string="needle",
            bool_value=None,
            uint32=None,
            variant_json=None,
        )
        error, array = parameter_array_for_action(fake_args, {"action_id": "win.set-search-query", "parameter_type": "string"})
        assert error is None
        assert array == "[<'needle'>]"
        fake_args = argparse.Namespace(
            string=None,
            bool_value=True,
            uint32=None,
            variant_json=None,
        )
        error, array = parameter_array_for_action(fake_args, {"action_id": "win.set-sidebar-visible", "parameter_type": "bool"})
        assert error is None
        assert array == "[<true>]"
        with tempfile.TemporaryDirectory() as directory:
            artifact_dir = Path(directory)
            (artifact_dir / "assertions").mkdir()
            (artifact_dir / "scenario-manifest.json").write_text(
                json.dumps(
                    {
                        "status": "passed",
                        "scenario_id": "self-test",
                        "state_assertions": [{"name": "snapshot", "status": "passed"}],
                        "waits": [{"predicate": "idle", "ok": True}],
                        "actions": [{"action": "set-search-query", "status": "passed"}],
                    }
                ),
                encoding="utf-8",
            )
            (artifact_dir / "summary.json").write_text('{"status":"passed"}\n', encoding="utf-8")
            (artifact_dir / "assertions/runtime-warning-scan.txt").write_text("PASS\n", encoding="utf-8")
            summary = summarize_artifacts(artifact_dir)
            assert summary.ok
            assert summary.data["status"] == "passed"
            missing = summarize_artifacts(artifact_dir / "missing")
            assert missing.status == "artifact-error"
    except Exception as exc:  # pragma: no cover - deliberately catches assertion details for CLI users.
        return failure("self-test", "workflow-failure", f"self-test failed: {exc}")
    return success("self-test", "lushtext automation client self-test passed")


def add_common_flags(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--bus-name", default=APP_ID)
    parser.add_argument("--object-path", default=AUTOMATION_OBJECT_PATH)
    parser.add_argument("--interface", default=AUTOMATION_INTERFACE)
    parser.add_argument("--window-path", default=WINDOW_OBJECT_PATH)
    parser.add_argument("--timeout-ms", type=int, default=5000)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--field")


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog=(
            "Defaults: --bus-name dev.cominotti.lushtext, --object-path "
            "/dev/cominotti/lushtext/Automation, --interface "
            "dev.cominotti.lushtext.Automation1, --window-path "
            "/dev/cominotti/lushtext/window/1. Common flags are accepted after "
            "each subcommand."
        ),
    )
    common = argparse.ArgumentParser(add_help=False)
    add_common_flags(common)
    subparsers = parser.add_subparsers(dest="command", required=True)

    for command in ("introspect", "catalog", "snapshot", "predicates", "events"):
        subparsers.add_parser(command, parents=[common])

    wait_parser = subparsers.add_parser("wait", parents=[common])
    wait_parser.add_argument("predicate", nargs="?")

    action_parser = subparsers.add_parser("action", parents=[common])
    action_parser.add_argument("action_id")
    action_parser.add_argument("--string")
    action_parser.add_argument("--bool", dest="bool_value", type=parse_bool)
    action_parser.add_argument("--uint32", type=int)
    action_parser.add_argument("--variant-json")

    artifact_parser = subparsers.add_parser("artifact-summary", parents=[common])
    artifact_parser.add_argument("artifact_dir", type=Path)

    subparsers.add_parser("self-test", parents=[common])
    return parser.parse_args(argv)


def dispatch(args: argparse.Namespace) -> ClientResult:
    if args.timeout_ms <= 0:
        return failure(args.command, "usage-error", "--timeout-ms must be positive")
    match args.command:
        case "introspect":
            return command_introspect(args)
        case "catalog":
            return command_catalog(args)
        case "snapshot":
            return command_snapshot(args)
        case "predicates":
            return command_predicates(args)
        case "events":
            return command_events(args)
        case "wait":
            return command_wait(args)
        case "action":
            return command_action(args)
        case "artifact-summary":
            return command_artifact_summary(args)
        case "self-test":
            return command_self_test(args)
        case _:
            return failure(args.command, "usage-error", f"unknown command: {args.command}")


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    result = apply_field(dispatch(args), args.field)
    print_result(args, result)
    return result.exit_code


if __name__ == "__main__":
    sys.exit(main())
