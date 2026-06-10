#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate automation documentation against the Rust exposure contracts."""

from __future__ import annotations

import argparse
import ast
import re
import sys
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
ACTION_CATALOG = REPO_ROOT / "crates/lushtext-core/src/services/action_catalog/mod.rs"
AUTOMATION_UI = REPO_ROOT / "crates/lushtext-core/src/ui/automation.rs"
AUTOMATION_MODEL = REPO_ROOT / "crates/lushtext-core/src/model/automation.rs"
AUTOMATION_SMOKE_DRIVER = REPO_ROOT / "scripts/automation-smoke-driver.py"
AUTOMATION_CLIENT = REPO_ROOT / "scripts/lushtext-automation.py"
ACCESSIBILITY_SMOKE = REPO_ROOT / "scripts/run-accessibility-smoke.sh"
GUIDE_DOC = REPO_ROOT / "docs/automation.md"
REFERENCE_DOC = REPO_ROOT / "docs/automation-reference.md"
EXPECTED_HELPER_FLAG_MARKER = (
    "<!-- automation-helper-flags: run-automation-smoke --artifact-dir --binary "
    "run-crash-recovery-smoke --artifact-dir --binary "
    "run-accessibility-smoke --artifact-dir --binary "
    "run-visual-smoke --artifact-dir --binary "
    "visual-geometry-smoke --artifact-dir --binary --scenario-dir --case-filter "
    "capture-lushtext-mutter "
    "--file --output --search --expected-search-matches --enable-minimap "
    "--enable-atspi --window-action --window-string-action --window-bool-action --wait-predicate --wait-window-action --wait-atspi-text "
    "--color-scheme --capture-artifact-dir --atspi-tree-output --atspi-focus-output --binary --width --height --keep-artifacts "
    "run-portal-sandbox-smoke --artifact-dir check-flatpak-permissions --manifest --self-test "
        "lushtext-automation introspect catalog snapshot predicates events wait action artifact-summary visual-geometry-capture self-test "
        "--bus-name --object-path --interface --window-path --timeout-ms --json --field --string --bool --uint32 --variant-json "
        "--scenario-id --size-id --direction --color-scheme --word-wrap --fixture-kind --viewport-position -->"
)
REQUIRED_HELPER_TABLE_ROWS = (
    "| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | `--window-bool-action ACTION=true\\|false` | Activates a window-scoped `org.gtk.Actions` action with one boolean parameter before capture; may be repeated. |",
)


@dataclass(frozen=True)
class CheckResult:
    label: str
    missing: list[str]


def camel_to_kebab(value: str) -> str:
    value = re.sub(r"(.)([A-Z][a-z]+)", r"\1-\2", value)
    value = re.sub(r"([a-z0-9])([A-Z])", r"\1-\2", value)
    return value.replace("_", "-").lower()


def anchor_slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")


def split_top_level_args(source: str) -> list[str]:
    args: list[str] = []
    current: list[str] = []
    depth = 0
    in_string = False
    escaped = False

    for char in source:
        if in_string:
            current.append(char)
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
            continue

        if char == '"':
            in_string = True
            current.append(char)
        elif char in "([":
            depth += 1
            current.append(char)
        elif char in ")]":
            depth -= 1
            current.append(char)
        elif char == "," and depth == 0:
            args.append("".join(current).strip())
            current = []
        else:
            current.append(char)

    tail = "".join(current).strip()
    if tail:
        args.append(tail)
    return args


def rust_string_literal(argument: str) -> str:
    match = re.fullmatch(r'"((?:[^"\\]|\\.)*)"', argument.strip(), re.DOTALL)
    if not match:
        raise ValueError(f"expected Rust string literal, got {argument!r}")
    return bytes(match.group(1), "utf-8").decode("unicode_escape")


def enum_value(argument: str) -> str:
    variant = argument.rsplit("::", 1)[-1].strip()
    return camel_to_kebab(variant)


def array_enum_values(argument: str) -> list[str]:
    return [enum_value(match) for match in re.findall(r"[A-Za-z]+::([A-Za-z0-9_]+)", argument)]


def action_scope_prefix(argument: str) -> str:
    return {
        "app": "app",
        "window": "win",
        "search-options": "search-options",
        "sidebar-section": "section",
        "workspace-header": "ws-header",
    }[enum_value(argument)]


def action_catalog_rows(source: str) -> list[dict[str, object]]:
    rows: list[dict[str, object]] = []
    index = source.index("const ACTION_CATALOG")

    while True:
        marker = source.find("ActionCatalogEntry::new(", index)
        if marker == -1:
            break
        body_start = marker + len("ActionCatalogEntry::new(")
        depth = 1
        in_string = False
        escaped = False
        body_end = body_start

        while body_end < len(source):
            char = source[body_end]
            if in_string:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    in_string = False
            else:
                if char == '"':
                    in_string = True
                elif char in "([":
                    depth += 1
                elif char in ")]":
                    depth -= 1
                    if depth == 0:
                        break
            body_end += 1

        args = split_top_level_args(source[body_start:body_end])
        if len(args) != 12:
            raise ValueError("ActionCatalogEntry::new no longer has 12 arguments")
        scope_prefix = action_scope_prefix(args[0])
        name = rust_string_literal(args[1])
        rows.append(
            {
                "action_id": f"{scope_prefix}.{name}",
                "label": rust_string_literal(args[2]),
                "parameter": enum_value(args[3]),
                "state": enum_value(args[4]),
                "enablement": rust_string_literal(args[5]),
                "owner": rust_string_literal(args[6]),
                "surfaces": array_enum_values(args[7]),
                "safety": enum_value(args[8]),
                "exposure": enum_value(args[9]),
                "anchor": rust_string_literal(args[10]),
                "coverage": array_enum_values(args[11]),
            }
        )
        index = body_end + 1

    return rows


def action_catalog_anchors(source: str) -> list[str]:
    return [str(row["anchor"]) for row in action_catalog_rows(source)]


def missing_action_row_fields(reference_text: str, rows: list[dict[str, object]]) -> list[str]:
    missing: list[str] = []
    lines = reference_text.splitlines()
    for row in rows:
        anchor = str(row["anchor"])
        line = next((line for line in lines if f'id="{anchor}"' in line), "")
        expected_terms = [
            str(row["action_id"]),
            str(row["label"]),
            str(row["parameter"]),
            str(row["state"]),
            str(row["exposure"]),
            str(row["safety"]),
            str(row["owner"]),
            str(row["enablement"]),
            ", ".join(row["surfaces"]),  # type: ignore[arg-type]
            ", ".join(row["coverage"]),  # type: ignore[arg-type]
        ]
        for term in expected_terms:
            if term and term not in line:
                missing.append(f"{anchor}: {term}")
    return sorted(missing)


def introspection_block(source: str) -> str:
    match = re.search(
        r'const INTROSPECTION_XML: &str = r#"(.*?)"#;',
        source,
        flags=re.DOTALL,
    )
    if not match:
        raise ValueError("automation introspection XML was not found")
    return match.group(1)


def dbus_anchors(source: str) -> list[str]:
    xml = introspection_block(source)
    anchors = [
        f"dbus-property-{camel_to_kebab(name)}"
        for name in re.findall(r"<property name='([^']+)'", xml)
    ]
    anchors.extend(
        f"dbus-method-{camel_to_kebab(name)}"
        for name in re.findall(r"<method name='([^']+)'", xml)
    )
    return anchors


def dbus_error_names(source: str) -> list[str]:
    return sorted(set(re.findall(r'const ERROR_[A-Z_]+: &str = "([^"]+)"', source)))


def snapshot_field_anchors(source: str) -> list[str]:
    source = source[source.index("pub struct AutomationSnapshot") :]
    return [
        f"snapshot-field-{camel_to_kebab(field)}"
        for field in re.findall(r"^\s+pub ([a-zA-Z0-9_]+):", source, flags=re.MULTILINE)
    ]


def workflow_event_field_anchors(source: str) -> list[str]:
    anchors: list[str] = []
    for struct_name in ("AutomationWorkflowEvent", "AutomationWorkflowEventsSnapshot"):
        match = re.search(
            rf"pub struct {struct_name} \{{(.*?)\n\}}",
            source,
            flags=re.DOTALL,
        )
        if not match:
            raise ValueError(f"{struct_name} struct was not found")
        anchors.extend(
            f"workflow-event-field-{camel_to_kebab(field)}"
            for field in re.findall(r"^\s+pub ([a-zA-Z0-9_]+):", match.group(1), flags=re.MULTILINE)
        )
    return sorted(set(anchors))


def readiness_blocker_anchors(source: str) -> list[str]:
    blockers = sorted(
        set(re.findall(r'pub const READINESS_BLOCKER_[A-Z_]+: &str = "([^"]+)";', source))
    )
    return [f"readiness-{blocker}" for blocker in blockers]


def readiness_predicate_anchors(source: str) -> list[str]:
    match = re.search(
        r"pub enum AutomationReadinessPredicate \{(.*?)\n\}",
        source,
        flags=re.DOTALL,
    )
    if not match:
        raise ValueError("AutomationReadinessPredicate enum was not found")
    predicates = re.findall(r"^\s+([A-Z][A-Za-z0-9]+),", match.group(1), flags=re.MULTILINE)
    return [f"readiness-predicate-{camel_to_kebab(predicate)}" for predicate in predicates]


def scenario_manifest_field_anchors(source: str) -> list[str]:
    match = re.search(
        r"SCENARIO_MANIFEST_FIELDS: tuple\[str, \.\.\.\] = \((.*?)\n\)",
        source,
        flags=re.DOTALL,
    )
    if not match:
        raise ValueError("SCENARIO_MANIFEST_FIELDS tuple was not found")
    fields = re.findall(r'"([^"]+)"', match.group(1))
    return [f"scenario-manifest-field-{field.replace('_', '-')}" for field in fields]


def atspi_anchor_anchors(source: str) -> list[str]:
    anchors = [
        (
            f"atspi-anchor-{anchor_slug(surface)}-"
            f"{anchor_slug(role)}-{anchor_slug(name)}"
        )
        for _capture, surface, role, name in re.findall(
            r'assert_anchor\s+"([^"]+)"\s+"([^"]+)"\s+"([^"]+)"\s+"([^"]+)"',
            source,
        )
    ]
    anchors.extend(
        f"atspi-focus-{anchor_slug(capture)}-{anchor_slug(name)}"
        for capture, name in re.findall(
            r'record_focus_anchor\s+"([^"]+)"\s+"([^"]+)"',
            source,
        )
    )
    return anchors


def python_tuple_string_values(source: str, const_name: str) -> list[str]:
    tree = ast.parse(source)
    for node in tree.body:
        if not isinstance(node, ast.Assign):
            continue
        if not any(isinstance(target, ast.Name) and target.id == const_name for target in node.targets):
            continue
        value = ast.literal_eval(node.value)
        if not isinstance(value, tuple) or not all(isinstance(item, str) for item in value):
            raise ValueError(f"{const_name} is not a string tuple")
        return list(value)
    raise ValueError(f"{const_name} tuple was not found")


def client_command_anchors(source: str) -> list[str]:
    return [
        f"automation-client-command-{command}"
        for command in python_tuple_string_values(source, "CLIENT_COMMANDS")
    ]


def client_status_anchors(source: str) -> list[str]:
    return [
        f"automation-client-status-{status}"
        for status in python_tuple_string_values(source, "CLIENT_STATUSES")
    ]


def client_result_field_anchors(source: str) -> list[str]:
    return [
        f"automation-client-result-field-{field.replace('_', '-')}"
        for field in python_tuple_string_values(source, "RESULT_FIELDS")
    ]


def client_exit_code_anchors(source: str) -> list[str]:
    tree = ast.parse(source)
    exit_codes = None
    for node in tree.body:
        if not isinstance(node, ast.Assign):
            continue
        if any(isinstance(target, ast.Name) and target.id == "EXIT_CODES" for target in node.targets):
            exit_codes = ast.literal_eval(node.value)
            break
    if not isinstance(exit_codes, dict):
        raise ValueError("EXIT_CODES dictionary was not found")
    return [
        f"automation-client-exit-{status}"
        for status in exit_codes.keys()
    ]


def client_artifact_field_anchors(source: str) -> list[str]:
    return [
        f"automation-client-artifact-field-{field.replace('_', '-')}"
        for field in python_tuple_string_values(source, "ARTIFACT_SUMMARY_FIELDS")
    ]


def client_flags(source: str) -> list[str]:
    return python_tuple_string_values(source, "CLIENT_FLAGS")


def missing_anchors(reference_text: str, anchors: list[str]) -> list[str]:
    return sorted(anchor for anchor in anchors if f'id="{anchor}"' not in reference_text)


def run_checks(
    *,
    guide_text: str | None = None,
    reference_text: str | None = None,
) -> list[CheckResult]:
    action_source = ACTION_CATALOG.read_text(encoding="utf-8")
    automation_source = AUTOMATION_UI.read_text(encoding="utf-8")
    model_source = AUTOMATION_MODEL.read_text(encoding="utf-8")
    smoke_driver_source = AUTOMATION_SMOKE_DRIVER.read_text(encoding="utf-8")
    client_source = AUTOMATION_CLIENT.read_text(encoding="utf-8")
    accessibility_source = ACCESSIBILITY_SMOKE.read_text(encoding="utf-8")
    guide = GUIDE_DOC.read_text(encoding="utf-8") if guide_text is None else guide_text
    reference = (
        REFERENCE_DOC.read_text(encoding="utf-8")
        if reference_text is None
        else reference_text
    )

    checks = [
        CheckResult(
            "action catalog anchors",
            missing_anchors(reference, action_catalog_anchors(action_source)),
        ),
        CheckResult(
            "action catalog row fields",
            missing_action_row_fields(reference, action_catalog_rows(action_source)),
        ),
        CheckResult("D-Bus anchors", missing_anchors(reference, dbus_anchors(automation_source))),
        CheckResult(
            "D-Bus errors",
            sorted(error for error in dbus_error_names(automation_source) if error not in reference),
        ),
        CheckResult(
            "snapshot field anchors",
            missing_anchors(reference, snapshot_field_anchors(model_source)),
        ),
        CheckResult(
            "workflow event field anchors",
            missing_anchors(reference, workflow_event_field_anchors(model_source)),
        ),
        CheckResult(
            "readiness blocker anchors",
            missing_anchors(reference, readiness_blocker_anchors(model_source)),
        ),
        CheckResult(
            "readiness predicate anchors",
            missing_anchors(reference, readiness_predicate_anchors(model_source)),
        ),
        CheckResult(
            "scenario manifest field anchors",
            missing_anchors(reference, scenario_manifest_field_anchors(smoke_driver_source)),
        ),
        CheckResult(
            "AT-SPI anchor anchors",
            missing_anchors(reference, atspi_anchor_anchors(accessibility_source)),
        ),
        CheckResult(
            "automation client command anchors",
            missing_anchors(reference, client_command_anchors(client_source)),
        ),
        CheckResult(
            "automation client status anchors",
            missing_anchors(reference, client_status_anchors(client_source)),
        ),
        CheckResult(
            "automation client result field anchors",
            missing_anchors(reference, client_result_field_anchors(client_source)),
        ),
        CheckResult(
            "automation client exit anchors",
            missing_anchors(reference, client_exit_code_anchors(client_source)),
        ),
        CheckResult(
            "automation client artifact field anchors",
            missing_anchors(reference, client_artifact_field_anchors(client_source)),
        ),
    ]

    guide_terms = [
        "/dev/cominotti/lushtext/Automation",
        "dev.cominotti.lushtext.Automation1",
        "GetActionCatalog",
        "GetSnapshot",
        "GetReadinessPredicates",
        "GetWorkflowEvents",
        "WaitForReady",
        "WaitForIdle",
        "predicate-timeout",
        "workflow-failure",
        "automation-unavailable",
        "unsupported-host-tooling",
        "full filesystem",
        "permission-posture.txt",
        "scenario-manifest.json",
        "workflow-events.json",
        "runtime-warning-scan.txt",
        "portals_only_migration=false",
        "DBusActivatable=true",
        "Exec=lushtext %U",
        "ActivateAction",
        "scripts/lushtext-automation.py",
        "artifact-summary",
        "D-Bus activation metadata remains intentionally disabled",
    ]
    checks.append(
        CheckResult(
            "user guide required terms",
            sorted(term for term in guide_terms if term not in guide),
        )
    )

    checks.append(
        CheckResult(
            "helper flag marker",
            [] if EXPECTED_HELPER_FLAG_MARKER in reference else [EXPECTED_HELPER_FLAG_MARKER],
        )
    )
    helper_terms = [
        "scripts/run-automation-smoke.sh",
        "scripts/run-crash-recovery-smoke.sh",
        "scripts/run-accessibility-smoke.sh",
        "scripts/visual-geometry-smoke.py",
        "scripts/run-portal-sandbox-smoke.sh",
        "scripts/lushtext-automation.py",
        "scripts/check-flatpak-permissions.py",
        "--artifact-dir DIR",
        "--binary PATH",
        "--scenario-dir DIR",
        "--case-filter TEXT",
        "--manifest PATH",
        "--self-test",
        *client_flags(client_source),
        "scenario-manifest.json",
        "schema_version",
        "state_assertions",
        "dbus_summaries",
        "bounded_artifact_policy",
    ]
    checks.append(
        CheckResult(
            "helper flag terms",
            sorted(term for term in helper_terms if term not in reference),
        )
    )
    checks.append(
        CheckResult(
            "helper table rows",
            sorted(row for row in REQUIRED_HELPER_TABLE_ROWS if row not in reference),
        )
    )
    return checks


def assert_self_test_detects_missing_anchor(label: str, anchor: str) -> None:
    reference = REFERENCE_DOC.read_text(encoding="utf-8")
    mutated = reference.replace(f'id="{anchor}"', f'id="removed-{anchor}"', 1)
    failures = {result.label: result.missing for result in run_checks(reference_text=mutated)}
    if not any(anchor in missing for missing in failures.values()):
        raise AssertionError(f"self-test did not detect missing {label} anchor {anchor}")


def run_self_tests() -> None:
    action_anchor = action_catalog_anchors(ACTION_CATALOG.read_text(encoding="utf-8"))[0]
    dbus_anchor = dbus_anchors(AUTOMATION_UI.read_text(encoding="utf-8"))[-1]
    snapshot_anchor = snapshot_field_anchors(AUTOMATION_MODEL.read_text(encoding="utf-8"))[0]
    workflow_event_anchor = workflow_event_field_anchors(
        AUTOMATION_MODEL.read_text(encoding="utf-8")
    )[0]
    readiness_blocker_anchor = readiness_blocker_anchors(
        AUTOMATION_MODEL.read_text(encoding="utf-8")
    )[0]
    readiness_predicate_anchor = readiness_predicate_anchors(
        AUTOMATION_MODEL.read_text(encoding="utf-8")
    )[0]
    scenario_manifest_anchor = scenario_manifest_field_anchors(
        AUTOMATION_SMOKE_DRIVER.read_text(encoding="utf-8")
    )[0]
    atspi_anchor = atspi_anchor_anchors(ACCESSIBILITY_SMOKE.read_text(encoding="utf-8"))[0]
    client_command_anchor = client_command_anchors(
        AUTOMATION_CLIENT.read_text(encoding="utf-8")
    )[0]
    client_status_anchor = client_status_anchors(
        AUTOMATION_CLIENT.read_text(encoding="utf-8")
    )[0]
    client_field_anchor = client_result_field_anchors(
        AUTOMATION_CLIENT.read_text(encoding="utf-8")
    )[0]

    assert_self_test_detects_missing_anchor("action", action_anchor)
    assert_self_test_detects_missing_anchor("D-Bus", dbus_anchor)
    assert_self_test_detects_missing_anchor("snapshot", snapshot_anchor)
    assert_self_test_detects_missing_anchor("workflow event", workflow_event_anchor)
    assert_self_test_detects_missing_anchor("readiness blocker", readiness_blocker_anchor)
    assert_self_test_detects_missing_anchor("readiness predicate", readiness_predicate_anchor)
    assert_self_test_detects_missing_anchor("scenario manifest field", scenario_manifest_anchor)
    assert_self_test_detects_missing_anchor("AT-SPI", atspi_anchor)
    assert_self_test_detects_missing_anchor("automation client command", client_command_anchor)
    assert_self_test_detects_missing_anchor("automation client status", client_status_anchor)
    assert_self_test_detects_missing_anchor("automation client result field", client_field_anchor)

    reference = REFERENCE_DOC.read_text(encoding="utf-8").replace(
        EXPECTED_HELPER_FLAG_MARKER,
        "<!-- automation-helper-flags: removed -->",
        1,
    )
    failures = {result.label: result.missing for result in run_checks(reference_text=reference)}
    if not failures.get("helper flag marker"):
        raise AssertionError("self-test did not detect missing helper flag marker")

    reference = REFERENCE_DOC.read_text(encoding="utf-8").replace(
        REQUIRED_HELPER_TABLE_ROWS[0],
        "",
        1,
    )
    failures = {result.label: result.missing for result in run_checks(reference_text=reference)}
    if not failures.get("helper table rows"):
        raise AssertionError("self-test did not detect missing helper table row")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="also prove representative missing documentation anchors fail the check",
    )
    args = parser.parse_args()

    failures = [result for result in run_checks() if result.missing]
    if failures:
        for result in failures:
            print(f"{result.label}: missing {len(result.missing)} item(s)")
            for item in result.missing:
                print(f"  - {item}")
        return 1

    if args.self_test:
        run_self_tests()
        print("automation docs are current; self-test caught representative drift")
    else:
        print("automation docs are current")
    return 0


if __name__ == "__main__":
    sys.exit(main())
