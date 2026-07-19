#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Developer and agent CLI for LushText's D-Bus automation surface."""

from __future__ import annotations

import argparse
import ast
import importlib.util
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
    "visual-geometry-capture",
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
    "visual-comparison-failed",
    "pixel-anchor-failed",
    "state-mismatch",
    "warning-scan-failed",
    "missing-field",
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
    "visual-comparison-failed": 1,
    "pixel-anchor-failed": 1,
    "state-mismatch": 1,
    "warning-scan-failed": 1,
    "workflow-failure": 1,
    "dbus-error": 1,
    "artifact-error": 1,
    "usage-error": 2,
    "unknown-predicate": 2,
    "unknown-action": 2,
    "unsupported-action": 2,
    "parameter-mismatch": 2,
    "missing-field": 2,
    "automation-unavailable": 3,
    "unsupported-host-tooling": 4,
}
ARTIFACT_SUMMARY_FIELDS = (
    "artifact_dir",
    "status",
    "schema_version",
    "engine",
    "scenario_source",
    "parity",
    "parity_report",
    "environment_report",
    "missing_capabilities",
    "case_filters",
    "scenario_id",
    "scenario_type",
    "failure_status",
    "failure_reason",
    "skip_reason",
    "invariant_id",
    "manifest",
    "source_manifest",
    "summary",
    "runtime_warning_scan",
    "warnings",
    "workflow_events",
    "snapshots",
    "geometry_snapshots",
    "screenshots",
    "protected_regions",
    "pixel_anchors",
    "relative_pixel_anchors",
    "pixel_anchor_assertion_count",
    "pixel_anchor_evidence",
    "final_geometry",
    "app_vs_rendered_disagreements",
    "rendered_anchor_stability",
    "animation_sampling",
    "animation_frame_evidence",
    "animation_frame_sample_count",
    "allowed_changing_regions",
    "comparison_report",
    "visual_geometry_cases",
    "verified_invariant_ids",
    "pixel_verified_invariant_ids",
    "animation_verified_invariant_ids",
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
    "--scenario-id",
    "--size-id",
    "--direction",
    "--color-scheme",
    "--word-wrap",
    "--fixture-kind",
    "--viewport-position",
)

REPO_ROOT = Path(__file__).resolve().parents[1]
VISUAL_GEOMETRY_PYTHON_VALIDATOR = REPO_ROOT / "scripts/visual-geometry-smoke.py"
VISUAL_GEOMETRY_REPLAY_PREFIX = ["cargo", "run", "-q", "-p", "cargo-gtk-proof", "--", "run"]
NATIVE_MINIMAP_HIGHLIGHT_INVARIANT = "native-minimap-highlight-anchors"
NATIVE_MINIMAP_ANIMATION_INVARIANT = "native-minimap-animation-highlight-anchors"


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


def artifact_json(path: Path | None) -> Any | None:
    if path is None or not path.is_file():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def summarize_artifacts(artifact_dir: Path) -> ClientResult:
    manifest_path = artifact_dir / "scenario-manifest.json"
    if not manifest_path.is_file():
        return summarize_generic_artifacts(artifact_dir)
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        return failure("artifact-summary", "artifact-error", str(exc), {"manifest": str(manifest_path)})
    if schema_error := unsupported_schema_version(manifest, manifest_path):
        return schema_error

    status = str(manifest.get("status") or "unknown")
    summary_path = artifact_dir / "summary.json"
    summary_payload = artifact_json(summary_path)
    warning_path = artifact_dir / "assertions/runtime-warning-scan.txt"
    visual_warning_path = artifact_dir / "warning-scan.json"
    visual_comparison_path = (
        artifact_dir / str(manifest["comparison_report"])
        if manifest.get("comparison_report")
        else None
    )
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
        "schema_version": manifest.get("schema_version"),
        "engine": summary_payload.get("engine")
        if isinstance(summary_payload, dict)
        else manifest.get("engine"),
        "scenario_source": summary_payload.get("scenario_source")
        if isinstance(summary_payload, dict)
        else manifest.get("scenario_source"),
        "parity": summary_payload.get("parity")
        if isinstance(summary_payload, dict)
        else manifest.get("parity"),
        "parity_report": artifact_json(artifact_dir / "parity-report.json"),
        "environment_report": summary_payload.get("environment_report")
        if isinstance(summary_payload, dict)
        else artifact_json(artifact_dir / "environment-report.json"),
        "missing_capabilities": summary_payload.get("missing_capabilities", [])
        if isinstance(summary_payload, dict)
        else manifest.get("missing_capabilities", []),
        "scenario_id": manifest.get("scenario_id"),
        "scenario_type": manifest.get("scenario_type"),
        "failure_status": manifest.get("failure_status"),
        "failure_reason": manifest.get("failure_reason"),
        "skip_reason": manifest.get("skip_reason"),
        "invariant_id": manifest.get("invariant_id"),
        "manifest": str(manifest_path),
        "source_manifest": manifest.get("source_manifest"),
        "summary": summary_payload,
        "runtime_warning_scan": artifact_text(warning_path),
        "warnings": manifest.get("warnings") or artifact_json(visual_warning_path),
        "workflow_events": workflow_events,
        "snapshots": snapshots,
        "geometry_snapshots": manifest.get("geometry_snapshots", []),
        "screenshots": manifest.get("screenshots", []),
        "protected_regions": manifest.get("protected_regions", []),
        "pixel_anchors": manifest.get("pixel_anchors", []),
        "relative_pixel_anchors": manifest.get("relative_pixel_anchors", []),
        "pixel_anchor_assertion_count": manifest.get("pixel_anchor_assertion_count", 0),
        "pixel_anchor_evidence": manifest.get("pixel_anchor_evidence", []),
        "final_geometry": manifest.get("final_geometry"),
        "app_vs_rendered_disagreements": manifest.get("app_vs_rendered_disagreements", []),
        "rendered_anchor_stability": manifest.get("rendered_anchor_stability", []),
        "animation_sampling": manifest.get("animation_sampling"),
        "verified_invariant_ids": manifest.get("verified_invariant_ids", []),
        "pixel_verified_invariant_ids": manifest.get("pixel_verified_invariant_ids", []),
        "animation_verified_invariant_ids": manifest.get("animation_verified_invariant_ids", []),
        "animation_frame_evidence": manifest.get("animation_frame_evidence"),
        "animation_frame_sample_count": manifest.get("animation_frame_sample_count", 0),
        "allowed_changing_regions": manifest.get("allowed_changing_regions", []),
        "comparison_report": artifact_json(visual_comparison_path),
        "visual_geometry_cases": [],
        "dbus_artifacts": dbus_artifacts,
        "state_assertions": manifest.get("state_assertions", []),
        "waits": manifest.get("waits", []),
        "actions": manifest.get("actions", []),
    }
    if status == "failed":
        failure_status = str(manifest.get("failure_status") or "artifact-error")
        detail = str(manifest.get("failure_reason") or "scenario manifest reports failure")
        if failure_status not in CLIENT_STATUSES:
            failure_status = "artifact-error"
        return failure("artifact-summary", failure_status, detail, data)
    if status == "passed":
        if animation_failure := animation_artifact_failure(data):
            return failure("artifact-summary", "artifact-error", animation_failure, data)
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


def command_visual_geometry_capture(args: argparse.Namespace) -> ClientResult:
    snapshot = command_snapshot(args)
    if not snapshot.ok:
        return snapshot
    return write_live_visual_geometry_capture(args, snapshot.data)


def write_live_visual_geometry_capture(args: argparse.Namespace, snapshot: dict[str, Any]) -> ClientResult:
    artifact_dir = args.artifact_dir.resolve()
    artifact_dir.mkdir(parents=True, exist_ok=True)
    snapshot_path = artifact_dir / "live-snapshot.json"
    snapshot_path.write_text(json.dumps(snapshot, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    missing, live_state = live_visual_geometry_state(snapshot, args)
    scenario_path: Path | None = None
    replay_command: list[str] = []
    scenario: dict[str, Any] | None = None
    if not missing:
        scenario = generated_visual_geometry_scenario(live_state, args)
        scenario_dir = artifact_dir / "generated-scenarios"
        scenario_dir.mkdir(exist_ok=True)
        scenario_path = scenario_dir / f"{scenario['scenario_id']}.json"
        scenario_path.write_text(json.dumps(scenario, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        validation_error = validate_generated_visual_geometry_scenario(scenario_dir)
        if validation_error:
            missing.append({"field": "generated_scenario", "reason": validation_error})
            scenario_path = None
            scenario = None
        else:
            replay_command = [
                *VISUAL_GEOMETRY_REPLAY_PREFIX,
                "--artifact-dir",
                str((artifact_dir / "replay").relative_to(REPO_ROOT))
                if (artifact_dir / "replay").is_relative_to(REPO_ROOT)
                else str(artifact_dir / "replay"),
                "--scenario-dir",
                str(scenario_dir.relative_to(REPO_ROOT))
                if scenario_dir.is_relative_to(REPO_ROOT)
                else str(scenario_dir),
                "--case-filter",
                str(scenario["scenario_id"]),
            ]

    status = "failed" if missing else "passed"
    manifest = {
        "schema_version": 1,
        "scenario_id": args.scenario_id,
        "capture_kind": "live-visual-geometry",
        "status": status,
        "failure_status": "missing-field" if missing else None,
        "failure_reason": "required live visual geometry fields are missing" if missing else None,
        "live_snapshot": snapshot_path.name,
        "generated_scenario": f"generated-scenarios/{scenario_path.name}" if scenario_path else None,
        "replay_command": " ".join(replay_command) if replay_command else None,
        "replay_command_argv": replay_command,
        "live_state": live_state,
        "missing_fields": missing,
        "context_screenshot": {
            "status": "not-run",
            "proof_role": "context-only",
            "detail": "portal screenshots are optional context and never count as visual invariant proof",
        },
        "proof_source": {
            "live_capture": "Automation1 bounded snapshot",
            "invariant_replay": "generated headless visual-geometry scenario",
        },
    }
    manifest_path = artifact_dir / "capture-manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    data = {
        "artifact_dir": str(artifact_dir),
        "manifest": str(manifest_path),
        "live_snapshot": str(snapshot_path),
        "generated_scenario": str(scenario_path) if scenario_path else None,
        "replay_command": manifest["replay_command"],
        "live_state": live_state,
        "missing_fields": missing,
    }
    if missing:
        return failure(
            "visual-geometry-capture",
            "missing-field",
            "required live visual geometry fields need explicit overrides",
            data,
        )
    return success("visual-geometry-capture", "live visual geometry scenario captured", data)


def live_visual_geometry_state(snapshot: dict[str, Any], args: argparse.Namespace) -> tuple[list[dict[str, str]], dict[str, Any]]:
    missing: list[dict[str, str]] = []
    window = snapshot.get("window")
    if not isinstance(window, dict):
        return ([{"field": "window", "reason": "Automation1 snapshot has no active window"}], {})
    geometry = window.get("visual_geometry")
    if not isinstance(geometry, dict):
        return ([{"field": "visual_geometry", "reason": "Automation1 snapshot has no visual_geometry"}], {})

    surfaces = window.get("surfaces") if isinstance(window.get("surfaces"), dict) else {}
    active_tab = active_tab_snapshot(window)
    size = infer_live_window_size(geometry)
    if size is None:
        missing.append({"field": "window_size", "reason": "could not infer live window size from visible surfaces"})

    minimap_visible = visual_surface_visible(geometry, "minimap-shell") and visual_surface_visible(
        geometry,
        "minimap-source-map",
    )
    if not minimap_visible:
        missing.append({"field": "minimap", "reason": "minimap-shell and minimap-source-map must be visible"})

    direction = args.direction or infer_sidebar_direction(surfaces)
    if direction is None:
        missing.append({"field": "direction", "reason": "sidebar state is unavailable; pass --direction"})

    color_scheme = args.color_scheme or infer_text_token(active_tab, ("force-light", "force-dark", "default"))
    if color_scheme is None:
        missing.append({"field": "color_scheme", "reason": "theme is not exposed by snapshot; pass --color-scheme"})

    word_wrap = args.word_wrap
    if word_wrap is None:
        word_wrap = infer_word_wrap(active_tab)
    if word_wrap is None:
        missing.append({"field": "word_wrap", "reason": "word wrap is not exposed by snapshot; pass --word-wrap"})

    fixture_kind = args.fixture_kind or infer_fixture_kind(active_tab)
    if fixture_kind is None:
        missing.append({"field": "fixture_kind", "reason": "active fixture kind is ambiguous; pass --fixture-kind"})

    viewport_position = args.viewport_position or infer_viewport_position(geometry)
    if viewport_position is None:
        missing.append(
            {"field": "viewport_position", "reason": "source-view scroll anchor is unavailable; pass --viewport-position"}
        )

    size_id = args.size_id
    if size and size_id is None:
        size_id = f"live-{size['width']}x{size['height']}"

    return missing, {
        "window_size": size,
        "size_id": size_id,
        "scale_factor": geometry.get("scale_factor"),
        "coordinate_space": geometry.get("coordinate_space"),
        "workspace_sidebar_visible": surfaces.get("workspace_sidebar_visible"),
        "workspace_sidebar_requested": surfaces.get("workspace_sidebar_requested"),
        "minimap_requested": surfaces.get("minimap_requested"),
        "minimap_visible": minimap_visible,
        "direction": direction,
        "color_scheme": color_scheme,
        "word_wrap": word_wrap,
        "fixture_kind": fixture_kind,
        "viewport_position": viewport_position,
        "active_tab": bounded_active_tab(active_tab),
        "overrides": {
            "direction": args.direction is not None,
            "color_scheme": args.color_scheme is not None,
            "word_wrap": args.word_wrap is not None,
            "fixture_kind": args.fixture_kind is not None,
            "viewport_position": args.viewport_position is not None,
        },
    }


def generated_visual_geometry_scenario(live_state: dict[str, Any], args: argparse.Namespace) -> dict[str, Any]:
    size = live_state["window_size"]
    return {
        "schema_version": 1,
        "scenario_id": args.scenario_id,
        "scenario_type": "minimap-sidebar",
        "invariant_id": NATIVE_MINIMAP_HIGHLIGHT_INVARIANT,
        "animation_sampling": default_minimap_animation_sampling(),
        "description": "Generated from a live Automation1 visual-geometry capture.",
        "matrix": {
            "sizes": [
                {
                    "id": live_state["size_id"],
                    "width": int(size["width"]),
                    "height": int(size["height"]),
                }
            ],
            "color_schemes": [live_state["color_scheme"]],
            "word_wrap": [bool(live_state["word_wrap"])],
            "directions": [live_state["direction"]],
            "viewport_positions": [live_state["viewport_position"]],
            "fixture_kinds": [live_state["fixture_kind"]],
        },
        "protected_regions": default_minimap_protected_regions(),
        "pixel_anchors": default_minimap_pixel_anchors(),
        "relative_pixel_anchors": default_minimap_relative_pixel_anchors(),
        "allowed_changing_regions": default_minimap_allowed_changing_regions(),
        "readiness_predicates": [
            "file-open-complete",
            "visual-geometry-settled",
            "final-sidebar-geometry",
        ],
    }


def default_minimap_protected_regions() -> list[dict[str, Any]]:
    return [
        {
            "name": "header-bar",
            "surface": "header-bar",
            "comparison": "exact-equality",
            "require_same_rect": True,
            "mask_rects": [],
        },
        {
            "name": "status-bar-stable-chrome",
            "surface": "status-bar",
            "comparison": "exact-equality",
            "require_same_rect": True,
            "mask_rects": [{"x": 0, "y": 0, "width": 80, "height": 80}],
        },
    ]


def default_minimap_pixel_anchors() -> list[dict[str, Any]]:
    return [
        {
            "name": "minimap-native-viewport-top-edge",
            "crop_surface": "minimap-shell",
            "detector": "native-minimap-viewport-top-edge-row",
            "min_pixels": 12,
            "max_screen_y_delta": 0,
        },
    ]


def default_minimap_relative_pixel_anchors() -> list[dict[str, Any]]:
    return []


def default_minimap_animation_sampling() -> dict[str, Any]:
    return {
        "enabled": True,
        "capture_mode": "stream",
        "invariant_id": NATIVE_MINIMAP_ANIMATION_INVARIANT,
        "stream_frame_count": 48,
        "stream_timeout_ms": 1400,
        "sample_interval_ms": 16,
        "max_sample_skew_ms": 80,
        "max_screen_y_delta": 0,
        "require_intermediate_geometry": True,
        "required_anchors": ["minimap-native-viewport-top-edge"],
    }


def default_minimap_allowed_changing_regions() -> list[dict[str, str]]:
    return [
        {
            "surface": "workspace-sidebar",
            "relationship": "visibility toggles according to direction",
        },
        {
            "surface": "editor-viewport",
            "relationship": "width changes while top-left scroll anchor remains true",
        },
        {
            "surface": "minimap-shell",
            "relationship": "position may move with the editor body but remains visible and allocated",
        },
    ]


def validate_generated_visual_geometry_scenario(scenario_dir: Path) -> str | None:
    try:
        spec = importlib.util.spec_from_file_location(
            "visual_geometry_smoke",
            VISUAL_GEOMETRY_PYTHON_VALIDATOR,
        )
        if spec is None or spec.loader is None:
            return f"could not load {VISUAL_GEOMETRY_PYTHON_VALIDATOR}"
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module.load_manifests(scenario_dir)
    except Exception as exc:  # pragma: no cover - surfaced through command status.
        return bounded_text(exc)
    return None


def active_tab_snapshot(window: dict[str, Any]) -> dict[str, Any] | None:
    tabs = window.get("tabs")
    if not isinstance(tabs, list):
        return None
    active_index = window.get("active_tab_index")
    for tab in tabs:
        if isinstance(tab, dict) and tab.get("active") is True:
            return tab
    for tab in tabs:
        if isinstance(tab, dict) and tab.get("index") == active_index:
            return tab
    return None


def bounded_active_tab(tab: dict[str, Any] | None) -> dict[str, Any] | None:
    if not tab:
        return None
    return {
        "index": tab.get("index"),
        "title": tab.get("title"),
        "document_kind": tab.get("document_kind"),
        "path": tab.get("path"),
        "load_state": tab.get("load_state"),
    }


def infer_live_window_size(geometry: dict[str, Any]) -> dict[str, int] | None:
    max_x = 0
    max_y = 0
    found = False
    for row in geometry.get("surfaces", []):
        if not isinstance(row, dict) or not row.get("visible") or not isinstance(row.get("rect"), dict):
            continue
        rect = row["rect"]
        max_x = max(max_x, int(rect.get("x", 0)) + int(rect.get("width", 0)))
        max_y = max(max_y, int(rect.get("y", 0)) + int(rect.get("height", 0)))
        found = True
    if not found or max_x <= 0 or max_y <= 0:
        return None
    return {"width": max_x, "height": max_y}


def visual_surface_visible(geometry: dict[str, Any], name: str) -> bool:
    for row in geometry.get("surfaces", []):
        if isinstance(row, dict) and row.get("name") == name:
            return row.get("visible") is True and isinstance(row.get("rect"), dict)
    return False


def infer_sidebar_direction(surfaces: dict[str, Any]) -> str | None:
    if "workspace_sidebar_visible" not in surfaces:
        return None
    return "hide" if surfaces.get("workspace_sidebar_visible") is True else "show"


def infer_text_token(tab: dict[str, Any] | None, tokens: tuple[str, ...]) -> str | None:
    text = " ".join(
        str(value)
        for value in ((tab or {}).get("title"), (tab or {}).get("path"))
        if value is not None
    )
    for token in tokens:
        if token in text:
            return token
    return None


def infer_word_wrap(tab: dict[str, Any] | None) -> bool | None:
    token = infer_text_token(tab, ("wrap-true", "wrap-false"))
    if token == "wrap-true":
        return True
    if token == "wrap-false":
        return False
    return None


def infer_fixture_kind(tab: dict[str, Any] | None) -> str | None:
    text = " ".join(
        str(value)
        for value in ((tab or {}).get("title"), (tab or {}).get("path"))
        if value is not None
    )
    if "markdown-dense" in text or text.endswith(".md"):
        return "markdown-dense"
    if "plain-lines" in text or text.endswith(".txt"):
        return "plain-lines"
    return None


def infer_viewport_position(geometry: dict[str, Any]) -> str | None:
    for row in geometry.get("scroll_anchors", []):
        if isinstance(row, dict) and row.get("name") == "source-view":
            if row.get("at_top") is True:
                return "top"
            if row.get("at_top") is False:
                return "mid"
    return None


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
        if schema_error := unsupported_schema_version(summary_payload, summary_path):
            return schema_error

    visual_cases = visual_geometry_case_rows(artifact_dir, summary_payload)
    data = {
        "artifact_dir": str(artifact_dir),
        "status": summary_payload.get("status") if isinstance(summary_payload, dict) else "generic",
        "schema_version": summary_payload.get("schema_version")
        if isinstance(summary_payload, dict)
        else None,
        "engine": summary_payload.get("engine") if isinstance(summary_payload, dict) else None,
        "scenario_source": summary_payload.get("scenario_source")
        if isinstance(summary_payload, dict)
        else None,
        "parity": summary_payload.get("parity") if isinstance(summary_payload, dict) else None,
        "parity_report": summary_payload.get("parity_report")
        or artifact_json(artifact_dir / "parity-report.json")
        if isinstance(summary_payload, dict)
        else artifact_json(artifact_dir / "parity-report.json"),
        "environment_report": artifact_json(artifact_dir / "environment-report.json"),
        "missing_capabilities": summary_payload.get("missing_capabilities", [])
        if isinstance(summary_payload, dict)
        else [],
        "case_filters": summary_payload.get("case_filters", [])
        if isinstance(summary_payload, dict)
        else [],
        "scenario_id": None,
        "scenario_type": None,
        "failure_status": None,
        "failure_reason": None,
        "skip_reason": summary_payload.get("skip_reason") if isinstance(summary_payload, dict) else None,
        "invariant_id": None,
        "manifest": [row["artifact"] for row in manifest_rows],
        "source_manifest": None,
        "summary": summary_payload,
        "runtime_warning_scan": artifact_text(artifact_dir / "assertions/runtime-warning-scan.txt"),
        "warnings": summary_payload.get("warnings") if isinstance(summary_payload, dict) else None,
        "workflow_events": workflow_event_summary(artifact_dir),
        "snapshots": sorted(
            path.relative_to(artifact_dir).as_posix()
            for path in (artifact_dir / "assertions").glob("*snapshot*.json")
        ),
        "geometry_snapshots": [],
        "screenshots": summary_payload.get("screenshots", [])
        if isinstance(summary_payload, dict)
        else [],
        "protected_regions": [],
        "pixel_anchors": [],
        "relative_pixel_anchors": [],
        "pixel_anchor_assertion_count": summary_payload.get("pixel_anchor_assertion_count", 0)
        if isinstance(summary_payload, dict)
        else 0,
        "pixel_anchor_evidence": [],
        "final_geometry": None,
        "app_vs_rendered_disagreements": [],
        "rendered_anchor_stability": [],
        "animation_sampling": None,
        "animation_frame_evidence": None,
        "animation_frame_sample_count": summary_payload.get("animation_frame_sample_count", 0)
        if isinstance(summary_payload, dict)
        else 0,
        "allowed_changing_regions": [],
        "comparison_report": None,
        "visual_geometry_cases": visual_cases,
        "verified_invariant_ids": summary_payload.get("verified_invariant_ids", [])
        if isinstance(summary_payload, dict)
        else [],
        "pixel_verified_invariant_ids": summary_payload.get("pixel_verified_invariant_ids", [])
        if isinstance(summary_payload, dict)
        else [],
        "animation_verified_invariant_ids": summary_payload.get(
            "animation_verified_invariant_ids", []
        )
        if isinstance(summary_payload, dict)
        else [],
        "dbus_artifacts": sorted(
            path.relative_to(artifact_dir).as_posix()
            for path in (artifact_dir / "assertions").glob("*")
            if any(token in path.name for token in ("dbus", "catalog", "snapshot", "workflow", "introspection"))
        ),
        "state_assertions": manifest_rows,
        "waits": summary_payload.get("waits", []) if isinstance(summary_payload, dict) else [],
        "actions": summary_payload.get("actions", []) if isinstance(summary_payload, dict) else [],
    }

    if isinstance(summary_payload, dict) and summary_payload.get("status") in {
        "skipped",
        "unsupported-host",
    }:
        return success("artifact-summary", "smoke lane was skipped", data, status="artifact-skipped")
    if animation_failure := animation_artifact_failure(data):
        return failure("artifact-summary", "artifact-error", animation_failure, data)
    failed_visual = next((row for row in visual_cases if row.get("status") == "failed"), None)
    if failed_visual is not None:
        failure_status = str(failed_visual.get("failure_status") or "artifact-error")
        if failure_status not in CLIENT_STATUSES:
            failure_status = "artifact-error"
        return failure("artifact-summary", failure_status, "visual geometry summary includes a failed case", data)
    if any(row.get("status") in {"failed", "parse-error"} for row in manifest_rows):
        return failure("artifact-summary", "artifact-error", "generic smoke artifact includes a failed manifest", data)
    if any(row.get("status") == "skipped" for row in manifest_rows):
        return success("artifact-summary", "generic smoke artifact includes a skipped manifest", data, status="artifact-skipped")
    return success("artifact-summary", "generic smoke artifact summary read", data)


def unsupported_schema_version(payload: Any, path: Path) -> ClientResult | None:
    if not isinstance(payload, dict) or "schema_version" not in payload:
        return None
    if payload.get("schema_version") == 1:
        return None
    return failure(
        "artifact-summary",
        "artifact-error",
        f"unsupported schema_version in {path}: {payload.get('schema_version')}",
        {"artifact": str(path), "schema_version": payload.get("schema_version")},
    )


def animation_artifact_failure(data: dict[str, Any]) -> str | None:
    claimed = string_list(data.get("animation_verified_invariant_ids"))
    if not claimed:
        return None

    visual_cases = data.get("visual_geometry_cases")
    if isinstance(visual_cases, list) and visual_cases:
        for invariant_id in claimed:
            matching_cases = [
                row
                for row in visual_cases
                if isinstance(row, dict)
                and row.get("status") == "passed"
                and invariant_id in string_list(row.get("animation_verified_invariant_ids"))
            ]
            if not matching_cases:
                return (
                    "animation invariant "
                    f"{invariant_id} has no passing visual geometry case row"
                )
            if not any(
                has_actionable_animation_frame_evidence(row.get("animation_frame_evidence"))
                for row in matching_cases
            ):
                return (
                    "animation invariant "
                    f"{invariant_id} lacks stream intermediate frame evidence"
                )
        return None

    if not has_actionable_animation_frame_evidence(data.get("animation_frame_evidence")):
        return "scenario claims animation invariant without stream intermediate frame evidence"
    return None


def has_actionable_animation_frame_evidence(evidence: Any) -> bool:
    if not isinstance(evidence, dict):
        return False
    if (
        evidence.get("status") != "passed"
        or evidence.get("capture_mode") != "stream"
        or not positive_number(evidence.get("sampled_frame_count"))
        or not positive_number(evidence.get("mapped_intermediate_frame_count"))
    ):
        return False
    max_skew = evidence.get("max_sample_skew_ms")
    observed_skew = evidence.get("max_sample_skew_observed_ms")
    if not real_number(max_skew) or not real_number(observed_skew) or observed_skew > max_skew:
        return False
    frames = evidence.get("frames")
    if not isinstance(frames, list) or not frames:
        return False

    has_mapped_intermediate_anchor = False
    for frame in frames:
        if not isinstance(frame, dict):
            return False
        if frame.get("status") != "passed" or frame.get("mapped_sample_elapsed_ms") is None:
            return False
        sample_skew = frame.get("sample_skew_ms")
        if not real_number(sample_skew) or sample_skew > max_skew:
            return False
        anchors = frame.get("anchors")
        has_passed_anchor = isinstance(anchors, list) and any(
            anchor_passed_with_rows(anchor) for anchor in anchors
        )
        if frame.get("sidebar_phase") == "intermediate" and has_passed_anchor:
            has_mapped_intermediate_anchor = True
    return has_mapped_intermediate_anchor


def anchor_passed_with_rows(anchor: Any) -> bool:
    return (
        isinstance(anchor, dict)
        and anchor.get("status") == "passed"
        and anchor.get("baseline_row_y") is not None
        and anchor.get("frame_row_y") is not None
    )


def string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, str)]


def positive_number(value: Any) -> bool:
    return real_number(value) and value > 0


def real_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def visual_geometry_case_rows(artifact_dir: Path, summary_payload: Any) -> list[dict[str, Any]]:
    if not isinstance(summary_payload, dict):
        return []
    cases = summary_payload.get("cases")
    if not isinstance(cases, list):
        return []
    rows = []
    for case in cases:
        if not isinstance(case, dict):
            continue
        case_dir_name = case.get("artifact_dir")
        manifest_name = case.get("manifest")
        manifest_path = None
        if isinstance(case_dir_name, str) and isinstance(manifest_name, str):
            manifest_path = artifact_dir / case_dir_name / manifest_name
        row = {
            "case_id": case.get("case_id"),
            "status": case.get("status"),
            "failure_status": case.get("failure_status"),
            "artifact_dir": case_dir_name,
            "manifest": str(manifest_path) if manifest_path and manifest_path.is_file() else None,
        }
        if manifest_path and manifest_path.is_file():
            try:
                manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
                row.update(
                    {
                        "scenario_type": manifest.get("scenario_type"),
                        "invariant_id": manifest.get("invariant_id"),
                        "protected_region_count": len(manifest.get("protected_regions", [])),
                        "pixel_anchor_assertion_count": manifest.get(
                            "pixel_anchor_assertion_count", 0
                        ),
                        "pixel_verified_invariant_ids": manifest.get(
                            "pixel_verified_invariant_ids", []
                        ),
                        "pixel_anchor_evidence": manifest.get("pixel_anchor_evidence", []),
                        "final_geometry": manifest.get("final_geometry"),
                        "app_vs_rendered_disagreements": manifest.get(
                            "app_vs_rendered_disagreements", []
                        ),
                        "rendered_anchor_stability": manifest.get(
                            "rendered_anchor_stability", []
                        ),
                        "animation_sampling": manifest.get("animation_sampling"),
                        "animation_verified_invariant_ids": manifest.get(
                            "animation_verified_invariant_ids", []
                        ),
                        "animation_frame_evidence": manifest.get("animation_frame_evidence"),
                        "animation_frame_sample_count": manifest.get(
                            "animation_frame_sample_count", 0
                        ),
                        "allowed_changing_region_count": len(
                            manifest.get("allowed_changing_regions", [])
                        ),
                        "comparison_report": manifest.get("comparison_report"),
                        "warnings": manifest.get("warnings"),
                    }
                )
            except json.JSONDecodeError as exc:
                row.update({"status": "parse-error", "detail": bounded_text(exc)})
        rows.append(row)
    return rows


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
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            case_dir = root / "visual-case"
            (case_dir / "comparisons").mkdir(parents=True)
            (case_dir / "before-geometry-snapshot.json").write_text(
                '{"document_text":"SECRET"}\n',
                encoding="utf-8",
            )
            (case_dir / "comparisons/comparison-report.json").write_text(
                json.dumps(
                    {
                        "status": "passed",
                        "invariant_id": "native-minimap-highlight-anchors",
                        "regions": [{"name": "header", "status": "passed"}],
                        "pixel_anchors": {
                            "status": "passed",
                            "anchors": [
                                {"name": "minimap-native-viewport-top-edge"},
                            ],
                            "relationships": [],
                        },
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            (case_dir / "scenario-manifest.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "scenario_id": "visual-self-test",
                        "scenario_type": "minimap-sidebar",
                        "invariant_id": "native-minimap-highlight-anchors",
                        "status": "passed",
                        "failure_status": None,
                        "failure_reason": None,
                        "skip_reason": None,
                        "source_manifest": "scripts/visual-geometry-scenarios/minimap-sidebar-top.json",
                        "same_session": True,
                        "screenshots": [{"name": "before", "artifact": "before.png"}],
                        "geometry_snapshots": [
                            {"name": "before", "artifact": "before-geometry-snapshot.json"}
                        ],
                        "protected_regions": [{"name": "header", "surface": "header-bar"}],
                        "pixel_anchors": [
                            {"name": "minimap-native-viewport-top-edge"},
                        ],
                        "relative_pixel_anchors": [],
                        "pixel_anchor_assertion_count": 1,
                        "rendered_anchor_stability": [
                            {
                                "name": "before",
                                "artifact": "before-rendered-anchor-stability.json",
                                "status": "passed",
                            }
                        ],
                        "animation_sampling": {
                            "enabled": True,
                            "invariant_id": "native-minimap-animation-highlight-anchors",
                        },
                        "animation_verified_invariant_ids": [
                            "native-minimap-animation-highlight-anchors"
                        ],
                        "animation_frame_evidence": {
                            "status": "passed",
                            "capture_mode": "stream",
                            "invariant_id": "native-minimap-animation-highlight-anchors",
                            "sampled_frame_count": 2,
                            "geometry_sample_count": 2,
                            "intermediate_geometry_sample_count": 1,
                            "mapped_intermediate_frame_count": 1,
                            "max_sample_skew_ms": 80,
                            "max_sample_skew_observed_ms": 8,
                            "phase_sequence": ["shown", "intermediate", "shown"],
                            "max_row_drift": 0,
                            "frames": [
                                {
                                    "frame_index": 0,
                                    "status": "passed",
                                    "mapped_sample_elapsed_ms": 48,
                                    "sample_skew_ms": 8,
                                    "sidebar_phase": "intermediate",
                                    "anchors": [
                                        {
                                            "name": "minimap-native-viewport-top-edge",
                                            "status": "passed",
                                            "baseline_row_y": 10,
                                            "frame_row_y": 10,
                                        }
                                    ],
                                }
                            ],
                        },
                        "animation_frame_sample_count": 2,
                        "allowed_changing_regions": [
                            {"surface": "editor-viewport", "relationship": "width changes"}
                        ],
                        "comparison_report": "comparisons/comparison-report.json",
                        "warnings": {"status": "passed"},
                    }
                ),
                encoding="utf-8",
            )
            (case_dir / "summary.json").write_text('{"status":"passed"}\n', encoding="utf-8")
            visual_summary = summarize_artifacts(case_dir)
            assert visual_summary.ok
            assert visual_summary.data["scenario_type"] == "minimap-sidebar"
            assert visual_summary.data["invariant_id"] == "native-minimap-highlight-anchors"
            assert visual_summary.data["pixel_anchor_assertion_count"] == 1
            assert visual_summary.data["comparison_report"]["status"] == "passed"
            assert visual_summary.data["comparison_report"]["pixel_anchors"]["status"] == "passed"
            assert "SECRET" not in json.dumps(visual_summary.data)

            (root / "summary.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "status": "passed",
                        "engine": {
                            "name": "cargo-gtk-proof",
                            "mode": "rust-staged-runner",
                            "tool_version": "0.0.0",
                        },
                        "scenario_source": {
                            "scenario_dir": "scripts/visual-geometry-scenarios",
                            "manifest_count": 1,
                            "expanded_case_count": 1,
                        },
                        "parity": {
                            "status": "passed",
                            "compared": 1,
                            "failed": 0,
                        },
                        "parity_report": {
                            "status": "passed",
                            "compared": 1,
                            "failed": 0,
                        },
                        "verified_invariant_ids": ["native-minimap-highlight-anchors"],
                        "pixel_verified_invariant_ids": ["native-minimap-highlight-anchors"],
                        "animation_verified_invariant_ids": [
                            "native-minimap-animation-highlight-anchors"
                        ],
                        "animation_frame_sample_count": 2,
                        "pixel_anchor_assertion_count": 1,
                        "case_count": 1,
                        "cases": [
                            {
                                "case_id": "visual-self-test",
                                "status": "passed",
                                "invariant_id": "native-minimap-highlight-anchors",
                                "pixel_anchor_assertion_count": 1,
                                "artifact_dir": "visual-case",
                                "manifest": "scenario-manifest.json",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            (root / "environment-report.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "status": "ready",
                        "runtime": {
                            "session_bus": "dbus-run-session",
                            "environment": [{"key": "XDG_RUNTIME_DIR", "value": "runtime"}],
                        },
                    }
                ),
                encoding="utf-8",
            )
            root_summary = summarize_artifacts(root)
            assert root_summary.ok
            assert root_summary.exit_code == 0
            assert root_summary.data["schema_version"] == 1
            assert root_summary.data["engine"]["name"] == "cargo-gtk-proof"
            assert root_summary.data["scenario_source"]["manifest_count"] == 1
            assert root_summary.data["parity"]["failed"] == 0
            assert root_summary.data["parity_report"]["status"] == "passed"
            assert root_summary.data["environment_report"]["status"] == "ready"
            assert root_summary.data["verified_invariant_ids"] == [
                "native-minimap-highlight-anchors"
            ]
            assert root_summary.data["pixel_verified_invariant_ids"] == [
                "native-minimap-highlight-anchors"
            ]
            assert root_summary.data["animation_verified_invariant_ids"] == [
                "native-minimap-animation-highlight-anchors"
            ]
            assert root_summary.data["visual_geometry_cases"][0]["protected_region_count"] == 1
            assert root_summary.data["visual_geometry_cases"][0]["pixel_anchor_assertion_count"] == 1
            assert (
                root_summary.data["visual_geometry_cases"][0]["animation_frame_sample_count"]
                == 2
            )

            def write_animation_summary_fixture(
                fixture_root: Path,
                evidence: dict[str, Any] | None,
            ) -> ClientResult:
                fixture_root.mkdir()
                fixture_case_dir = fixture_root / "animation-case"
                fixture_case_dir.mkdir()
                fixture_manifest = {
                    "schema_version": 1,
                    "scenario_id": "animation-policy-self-test",
                    "scenario_type": "minimap-sidebar",
                    "invariant_id": "native-minimap-highlight-anchors",
                    "status": "passed",
                    "animation_verified_invariant_ids": [
                        "native-minimap-animation-highlight-anchors"
                    ],
                    "animation_frame_sample_count": 2,
                }
                if evidence is not None:
                    fixture_manifest["animation_frame_evidence"] = evidence
                (fixture_case_dir / "scenario-manifest.json").write_text(
                    json.dumps(fixture_manifest),
                    encoding="utf-8",
                )
                (fixture_root / "summary.json").write_text(
                    json.dumps(
                        {
                            "schema_version": 1,
                            "status": "passed",
                            "engine": {
                                "name": "cargo-gtk-proof",
                                "mode": "rust-staged-runner",
                            },
                            "animation_verified_invariant_ids": [
                                "native-minimap-animation-highlight-anchors"
                            ],
                            "case_count": 1,
                            "cases": [
                                {
                                    "case_id": "animation-policy-self-test",
                                    "status": "passed",
                                    "artifact_dir": "animation-case",
                                    "manifest": "scenario-manifest.json",
                                }
                            ],
                        }
                    ),
                    encoding="utf-8",
                )
                return summarize_artifacts(fixture_root)

            base_animation_evidence = {
                "status": "passed",
                "capture_mode": "stream",
                "sampled_frame_count": 2,
                "mapped_intermediate_frame_count": 1,
                "max_sample_skew_ms": 80,
                "max_sample_skew_observed_ms": 8,
                "frames": [
                    {
                        "status": "passed",
                        "mapped_sample_elapsed_ms": 48,
                        "sample_skew_ms": 8,
                        "sidebar_phase": "intermediate",
                        "anchors": [
                            {
                                "status": "passed",
                                "baseline_row_y": 10,
                                "frame_row_y": 10,
                            }
                        ],
                    }
                ],
            }
            missing_animation_summary = write_animation_summary_fixture(
                root / "missing-animation-evidence-root",
                None,
            )
            assert not missing_animation_summary.ok
            assert missing_animation_summary.status == "artifact-error"
            assert "lacks stream intermediate frame evidence" in missing_animation_summary.detail

            final_settle_evidence = json.loads(json.dumps(base_animation_evidence))
            final_settle_evidence["capture_mode"] = "screenshot"
            final_settle_evidence["mapped_intermediate_frame_count"] = 0
            final_settle_summary = write_animation_summary_fixture(
                root / "final-settle-animation-root",
                final_settle_evidence,
            )
            assert not final_settle_summary.ok
            assert final_settle_summary.status == "artifact-error"
            assert "lacks stream intermediate frame evidence" in final_settle_summary.detail

            stale_frame_evidence = json.loads(json.dumps(base_animation_evidence))
            stale_frame_evidence["max_sample_skew_observed_ms"] = 120
            stale_frame_evidence["frames"][0]["sample_skew_ms"] = 120
            stale_frame_summary = write_animation_summary_fixture(
                root / "stale-frame-animation-root",
                stale_frame_evidence,
            )
            assert not stale_frame_summary.ok
            assert stale_frame_summary.status == "artifact-error"
            assert "lacks stream intermediate frame evidence" in stale_frame_summary.detail

            single_manifest_dir = root / "single-manifest-animation-policy"
            single_manifest_dir.mkdir()
            (single_manifest_dir / "scenario-manifest.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "scenario_id": "single-animation-policy-self-test",
                        "scenario_type": "minimap-sidebar",
                        "status": "passed",
                        "animation_verified_invariant_ids": [
                            "native-minimap-animation-highlight-anchors"
                        ],
                    }
                ),
                encoding="utf-8",
            )
            single_manifest_summary = summarize_artifacts(single_manifest_dir)
            assert not single_manifest_summary.ok
            assert single_manifest_summary.status == "artifact-error"
            assert "scenario claims animation invariant" in single_manifest_summary.detail

            failed_dir = root / "failed-case"
            failed_dir.mkdir()
            (failed_dir / "scenario-manifest.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "scenario_id": "visual-failed",
                        "scenario_type": "minimap-sidebar",
                        "status": "failed",
                        "failure_status": "visual-comparison-failed",
                        "failure_reason": "pixel delta",
                    }
                ),
                encoding="utf-8",
            )
            failed_summary = summarize_artifacts(failed_dir)
            assert not failed_summary.ok
            assert failed_summary.status == "visual-comparison-failed"

            (root / "summary.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "status": "failed",
                        "engine": {"name": "cargo-gtk-proof", "mode": "rust-staged-runner"},
                        "case_count": 1,
                        "passed": 0,
                        "failed": 1,
                        "skipped": 0,
                        "cases": [
                            {
                                "case_id": "visual-failed",
                                "status": "failed",
                                "failure_status": "visual-comparison-failed",
                                "artifact_dir": "failed-case",
                                "manifest": "scenario-manifest.json",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            failed_root_summary = summarize_artifacts(root)
            assert not failed_root_summary.ok
            assert failed_root_summary.status == "visual-comparison-failed"
            assert failed_root_summary.exit_code == 1

            pixel_failed_dir = root / "pixel-failed-case"
            pixel_failed_dir.mkdir()
            (pixel_failed_dir / "scenario-manifest.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "scenario_id": "visual-pixel-failed",
                        "scenario_type": "minimap-sidebar",
                        "status": "failed",
                        "failure_status": "pixel-anchor-failed",
                        "failure_reason": "pixel anchor assertion failed",
                    }
                ),
                encoding="utf-8",
            )
            pixel_failed_summary = summarize_artifacts(pixel_failed_dir)
            assert not pixel_failed_summary.ok
            assert pixel_failed_summary.status == "pixel-anchor-failed"

            skipped_dir = root / "skipped-case"
            skipped_dir.mkdir()
            (skipped_dir / "scenario-manifest.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "scenario_id": "visual-skipped",
                        "scenario_type": "minimap-sidebar",
                        "status": "skipped",
                        "skip_reason": "missing compositor",
                    }
                ),
                encoding="utf-8",
            )
            skipped_summary = summarize_artifacts(skipped_dir)
            assert skipped_summary.ok
            assert skipped_summary.status == "artifact-skipped"
            assert skipped_summary.exit_code == 0

            malformed_dir = root / "malformed-case"
            malformed_dir.mkdir()
            (malformed_dir / "scenario-manifest.json").write_text("{not json", encoding="utf-8")
            malformed_summary = summarize_artifacts(malformed_dir)
            assert malformed_summary.status == "artifact-error"
            assert malformed_summary.exit_code == 1

            unsupported_schema_dir = root / "unsupported-schema-case"
            unsupported_schema_dir.mkdir()
            (unsupported_schema_dir / "scenario-manifest.json").write_text(
                json.dumps(
                    {
                        "schema_version": 999,
                        "scenario_id": "visual-future",
                        "scenario_type": "minimap-sidebar",
                        "status": "passed",
                    }
                ),
                encoding="utf-8",
            )
            unsupported_schema_summary = summarize_artifacts(unsupported_schema_dir)
            assert not unsupported_schema_summary.ok
            assert unsupported_schema_summary.status == "artifact-error"
            assert "unsupported schema_version" in unsupported_schema_summary.detail

            unsupported_root = root / "unsupported-root"
            unsupported_root.mkdir()
            (unsupported_root / "summary.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "status": "unsupported-host",
                        "engine": {"name": "cargo-gtk-proof", "mode": "rust-staged-runner"},
                        "skip_reason": "unsupported host tooling: mutter",
                        "missing_capabilities": ["missing required command: mutter"],
                        "case_count": 0,
                        "passed": 0,
                        "failed": 0,
                        "skipped": 0,
                        "cases": [],
                    }
                ),
                encoding="utf-8",
            )
            (unsupported_root / "environment-report.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "status": "unsupported-host",
                        "missing_capabilities": ["missing required command: mutter"],
                    }
                ),
                encoding="utf-8",
            )
            unsupported_summary = summarize_artifacts(unsupported_root)
            assert unsupported_summary.ok
            assert unsupported_summary.status == "artifact-skipped"
            assert unsupported_summary.exit_code == 0
            assert unsupported_summary.data["status"] == "unsupported-host"
            assert unsupported_summary.data["missing_capabilities"] == [
                "missing required command: mutter"
            ]
            assert unsupported_summary.data["environment_report"]["status"] == "unsupported-host"

            oracle_root = root / "python-oracle-root"
            oracle_root.mkdir()
            (oracle_root / "summary.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "status": "passed",
                        "engine": {"name": "visual-geometry-smoke.py", "mode": "oracle"},
                        "case_count": 0,
                        "passed": 0,
                        "failed": 0,
                        "skipped": 0,
                        "cases": [],
                    }
                ),
                encoding="utf-8",
            )
            oracle_summary = summarize_artifacts(oracle_root)
            assert oracle_summary.ok
            assert oracle_summary.data["engine"]["mode"] == "oracle"

            parity_root = root / "parity-root"
            parity_root.mkdir()
            (parity_root / "summary.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "status": "passed",
                        "engine": {"name": "cargo-gtk-proof", "mode": "parity-replay"},
                        "parity": {
                            "status": "passed",
                            "python_oracle": {
                                "name": "visual-geometry-smoke.py",
                                "mode": "oracle",
                            },
                            "compared": 2,
                            "failed": 0,
                        },
                        "case_count": 0,
                        "passed": 0,
                        "failed": 0,
                        "skipped": 0,
                        "cases": [],
                    }
                ),
                encoding="utf-8",
            )
            (parity_root / "parity-report.json").write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "status": "passed",
                        "compared": 2,
                        "failed": 0,
                        "rust_engine": {"name": "cargo-gtk-proof"},
                        "python_oracle": {"name": "visual-geometry-smoke.py"},
                    }
                ),
                encoding="utf-8",
            )
            parity_summary = summarize_artifacts(parity_root)
            assert parity_summary.ok
            assert parity_summary.data["parity"]["compared"] == 2
            assert parity_summary.data["parity_report"]["status"] == "passed"

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            capture_args = argparse.Namespace(
                artifact_dir=root / "capture",
                scenario_id="live-self-test",
                size_id=None,
                direction=None,
                color_scheme="force-light",
                word_wrap=True,
                fixture_kind=None,
                viewport_position=None,
            )
            capture = write_live_visual_geometry_capture(capture_args, self_test_visual_snapshot())
            assert capture.ok
            assert capture.data["generated_scenario"].endswith("live-self-test.json")
            manifest = json.loads((root / "capture/capture-manifest.json").read_text(encoding="utf-8"))
            assert manifest["status"] == "passed"
            assert manifest["context_screenshot"]["proof_role"] == "context-only"
            assert "--scenario-dir" in manifest["replay_command"]
            generated = json.loads(
                (root / "capture/generated-scenarios/live-self-test.json").read_text(encoding="utf-8")
            )
            assert generated["matrix"]["sizes"][0]["width"] == 1822
            assert generated["matrix"]["word_wrap"] == [True]
            assert generated["matrix"]["fixture_kinds"] == ["plain-lines"]
            assert (
                generated["animation_sampling"]["invariant_id"]
                == "native-minimap-animation-highlight-anchors"
            )

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            missing_args = argparse.Namespace(
                artifact_dir=root / "capture-missing",
                scenario_id="live-missing-self-test",
                size_id=None,
                direction=None,
                color_scheme=None,
                word_wrap=None,
                fixture_kind=None,
                viewport_position=None,
            )
            missing = write_live_visual_geometry_capture(
                missing_args,
                self_test_visual_snapshot(path="/tmp/live-unknown"),
            )
            assert not missing.ok
            assert missing.status == "missing-field"
            missing_manifest = json.loads(
                (root / "capture-missing/capture-manifest.json").read_text(encoding="utf-8")
            )
            missing_fields = {row["field"] for row in missing_manifest["missing_fields"]}
            assert {"color_scheme", "word_wrap", "fixture_kind"} <= missing_fields
    # AssertionError remains uncaught: failed self-test assertions are programming
    # defects, while the expected operational failures below still get a compact
    # CLI result instead of a traceback.
    except (
        OSError,
        ValueError,
        TypeError,
        KeyError,
        RuntimeError,
        subprocess.SubprocessError,
    ) as exc:
        return failure("self-test", "workflow-failure", f"self-test failed: {exc}")
    return success("self-test", "lushtext automation client self-test passed")


def self_test_visual_snapshot(path: str = "/tmp/live-wrap-true-plain-lines.txt") -> dict[str, Any]:
    return {
        "window": {
            "active_tab_index": 0,
            "tabs": [
                {
                    "index": 0,
                    "active": True,
                    "title": Path(path).name,
                    "document_kind": "file",
                    "path": path,
                    "load_state": "loaded",
                }
            ],
            "surfaces": {
                "workspace_sidebar_visible": True,
                "workspace_sidebar_requested": True,
                "minimap_requested": True,
            },
            "visual_geometry": {
                "scale_factor": 1,
                "coordinate_space": "window-logical-pixels",
                "surfaces": [
                    {
                        "name": "header-bar",
                        "visible": True,
                        "rect": {"x": 0, "y": 0, "width": 1822, "height": 46},
                    },
                    {
                        "name": "workspace-sidebar",
                        "visible": True,
                        "rect": {"x": 0, "y": 96, "width": 360, "height": 1128},
                    },
                    {
                        "name": "editor-viewport",
                        "visible": True,
                        "rect": {"x": 360, "y": 96, "width": 1341, "height": 1128},
                    },
                    {
                        "name": "minimap-shell",
                        "visible": True,
                        "rect": {"x": 1701, "y": 96, "width": 121, "height": 1128},
                    },
                    {
                        "name": "minimap-source-map",
                        "visible": True,
                        "rect": {"x": 1701, "y": 96, "width": 110, "height": 1128},
                    },
                    {
                        "name": "status-bar",
                        "visible": True,
                        "rect": {"x": 0, "y": 1224, "width": 1822, "height": 48},
                    },
                ],
                "scroll_anchors": [
                    {
                        "name": "source-view",
                        "at_top": True,
                        "at_left": True,
                        "x_value_milli": 0,
                        "y_value_milli": 0,
                    }
                ],
            },
        }
    }


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

    capture_parser = subparsers.add_parser("visual-geometry-capture", parents=[common])
    capture_parser.add_argument("artifact_dir", type=Path)
    capture_parser.add_argument("--scenario-id", default="live-minimap-sidebar")
    capture_parser.add_argument("--size-id")
    capture_parser.add_argument("--direction", choices=("hide", "show"))
    capture_parser.add_argument("--color-scheme", choices=("default", "force-light", "force-dark"))
    capture_parser.add_argument("--word-wrap", type=parse_bool)
    capture_parser.add_argument("--fixture-kind", choices=("plain-lines", "markdown-dense"))
    capture_parser.add_argument("--viewport-position", choices=("top", "mid"))

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
        case "visual-geometry-capture":
            return command_visual_geometry_capture(args)
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
