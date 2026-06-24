#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Run and classify LushText GtkBuilder diagnostics artifacts."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = REPO_ROOT / "scripts" / "builder-diagnostics-coverage.json"
MONITOR = "2560x1600"

BENIGN_PATTERNS = [
    re.compile(r"^libmutter-Message:"),
    re.compile(r"^\*\* Message: .*Obtained a high priority EGL context"),
    re.compile(r"^\(mutter:[0-9]+\): mutter-WARNING \*\*: .*org\.freedesktop\.locale1"),
    re.compile(r"^\(mutter:[0-9]+\): libmutter-WARNING \*\*: .*colord daemon"),
    re.compile(r"^dbus-daemon\[[0-9]+\]: .*org\.freedesktop\."),
    re.compile(r"^dbus-daemon\[[0-9]+\]: .*org\.a11y\.Bus"),
    re.compile(r"^dbus-daemon\[[0-9]+\]: .*org\.freedesktop\.systemd1"),
    re.compile(r"^\(/usr/libexec/xdg-desktop-portal:[0-9]+\): xdg-desktop-portal-WARNING \*\*: .*deprecated UseIn key"),
    re.compile(r"^\(/usr/libexec/xdg-desktop-portal:[0-9]+\): xdg-desktop-portal-WARNING \*\*: .*portals\.conf"),
    re.compile(r"^\(/usr/libexec/xdg-desktop-portal:[0-9]+\): xdg-desktop-portal-WARNING \*\*: .*PipeWire"),
    re.compile(r"^\(/usr/libexec/xdg-desktop-portal:[0-9]+\): xdg-desktop-portal-WARNING \*\*: .*Document portal fuse mount point unknown"),
    re.compile(r"^\(/usr/libexec/xdg-desktop-portal:[0-9]+\): xdg-desktop-portal-WARNING \*\*: .*RealtimeKit proxy"),
    re.compile(r"^\(/usr/libexec/xdg-desktop-portal:[0-9]+\): xdg-desktop-portal-WARNING \*\*: .*portal requires an access impl"),
    re.compile(r"^\(xdg-desktop-portal-gtk:[0-9]+\): Gtk-WARNING \*\*: .*GTK_DEBUG set but ignored"),
    re.compile(r"^\(process:[0-9]+\): Gtk-CRITICAL \*\*: .*org\.a11y\.atspi\.Registry"),
    re.compile(r"^error: fuse init failed: Can't mount path .*/doc$"),
    re.compile(r"^Gdk-Message: .*Broken pipe$"),
    re.compile(r"^Finished `test` profile"),
    re.compile(r"^Running tests/widget\.rs"),
    re.compile(r"^running [0-9]+ tests?$"),
    re.compile(r"^test .* \.\.\. ok$"),
    re.compile(r"^test result: ok\."),
]

ACTIONABLE_PATTERNS = [
    re.compile(r"Gtk-(?:WARNING|CRITICAL|ERROR).*Builder", re.IGNORECASE),
    re.compile(r"Gtk-(?:WARNING|CRITICAL|ERROR).*buildable", re.IGNORECASE),
    re.compile(r"Gtk-(?:WARNING|CRITICAL|ERROR).*template", re.IGNORECASE),
    re.compile(r"Failed to precompile template", re.IGNORECASE),
    re.compile(r"Unknown (?:property|signal|object|type|child)", re.IGNORECASE),
    re.compile(r"Invalid (?:property|object|type|child)", re.IGNORECASE),
    re.compile(r"Could not (?:set|parse|load).*GtkBuilder", re.IGNORECASE),
]

KNOWN_STANDALONE_LIMIT_PATTERNS = [
    re.compile(r"Invalid object type 'Adw", re.IGNORECASE),
    re.compile(r"Invalid object type 'Lushtext", re.IGNORECASE),
    re.compile(r"Failed to lookup template parent type Adw", re.IGNORECASE),
    re.compile(r"Failed to lookup template parent type Lushtext", re.IGNORECASE),
    re.compile(r"template class .* not found", re.IGNORECASE),
    re.compile(r"Could not find object type", re.IGNORECASE),
    re.compile(r"Could not initialize windowing system", re.IGNORECASE),
]

SUSPICIOUS_PATTERNS = [
    re.compile(r"WARNING|CRITICAL|ERROR|GtkBuilder|builder|buildable|template", re.IGNORECASE),
]

ADW_SHORTCUTS_DIALOG_HOST_PATTERN = re.compile(
    r"^\(process:[0-9]+\): Gtk-WARNING \*\*: .*"
    r"Allocating size to AdwDialogHost 0x[0-9a-f]+ "
    r"without calling gtk_widget_measure\(\)\. "
    r"How does the code know the size to allocate\?$"
)


@dataclass
class CommandResult:
    status: int
    stdout: Path
    stderr: Path
    command: list[str]


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def slug(value: str) -> str:
    return re.sub(r"[^a-zA-Z0-9_.-]+", "-", value).strip("-")


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace") if path.exists() else ""


def run_command(
    command: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None,
    stdout: Path,
    stderr: Path,
    timeout: int = 180,
) -> CommandResult:
    stdout.parent.mkdir(parents=True, exist_ok=True)
    stderr.parent.mkdir(parents=True, exist_ok=True)
    with stdout.open("w", encoding="utf-8") as out, stderr.open("w", encoding="utf-8") as err:
        try:
            completed = subprocess.run(
                command,
                cwd=cwd,
                env=env,
                stdout=out,
                stderr=err,
                timeout=timeout,
                check=False,
            )
            status = completed.returncode
        except subprocess.TimeoutExpired:
            err.write(f"\nTIMEOUT: command exceeded {timeout}s\n")
            status = 124
    return CommandResult(status=status, stdout=stdout, stderr=stderr, command=command)


def command_available(name: str) -> bool:
    return shutil.which(name) is not None


def package_versions() -> dict[str, str]:
    versions: dict[str, str] = {}
    if not command_available("pkg-config"):
        return versions
    for package in ("gtk4", "libadwaita-1", "gtksourceview-5"):
        probe = subprocess.run(
            ["pkg-config", "--modversion", package],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            check=False,
        )
        versions[package] = probe.stdout.strip() if probe.returncode == 0 else "missing"
    return versions


def runtime_metadata(provider: str, image: str | None) -> dict[str, Any]:
    metadata: dict[str, Any] = {
        "timestamp": now_iso(),
        "provider": provider,
        "image": image or "",
        "repo": str(REPO_ROOT),
        "versions": package_versions(),
        "environment": {
            "LUSHTEXT_BUILDER_DIAGNOSTICS_IN_CONTAINER": os.environ.get(
                "LUSHTEXT_BUILDER_DIAGNOSTICS_IN_CONTAINER", ""
            ),
            "LUSHTEXT_GTK_DEBUG_PREFIX": os.environ.get("LUSHTEXT_GTK_DEBUG_PREFIX", ""),
            "PKG_CONFIG_PATH": os.environ.get("PKG_CONFIG_PATH", ""),
            "LD_LIBRARY_PATH": os.environ.get("LD_LIBRARY_PATH", ""),
            "GI_TYPELIB_PATH": os.environ.get("GI_TYPELIB_PATH", ""),
            "XDG_DATA_DIRS": os.environ.get("XDG_DATA_DIRS", ""),
        },
    }
    if command_available("blueprint-compiler"):
        probe = subprocess.run(
            ["blueprint-compiler", "--version"],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
        )
        metadata["blueprint_compiler"] = probe.stdout.strip()
    return metadata


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def debug_capability(artifact_dir: Path) -> dict[str, Any]:
    probe_dir = artifact_dir / "capability"
    probe_dir.mkdir(parents=True, exist_ok=True)
    probe_ui = probe_dir / "debug-probe.ui"
    probe_ui.write_text(
        """<?xml version="1.0" encoding="UTF-8"?>
<interface>
  <object class="GtkBox" id="root"/>
</interface>
""",
        encoding="utf-8",
    )

    if not command_available("gtk4-builder-tool"):
        return {
            "status": "unsupported_runtime",
            "reason": "gtk4-builder-tool is not installed",
            "supported": False,
        }

    env = os.environ.copy()
    env["GTK_DEBUG"] = "help"
    result = run_command(
        ["gtk4-builder-tool", "validate", str(probe_ui)],
        cwd=REPO_ROOT,
        env=env,
        stdout=probe_dir / "gtk-debug-help.stdout",
        stderr=probe_dir / "gtk-debug-help.stderr",
        timeout=30,
    )
    combined = read_text(result.stdout) + read_text(result.stderr)
    ignored = "GTK_DEBUG set but ignored because gtk isn't built with G_ENABLE_DEBUG" in combined
    lists_builder = bool(re.search(r"(^|\s)builder(-objects)?(\s|$)", combined))
    supported = lists_builder and not ignored
    status = "supported" if supported else "unsupported_runtime"
    reason = "GTK_DEBUG help lists builder diagnostics" if supported else "GTK debug channels unavailable"
    return {
        "status": status,
        "reason": reason,
        "supported": supported,
        "command": result.command,
        "status_code": result.status,
        "stdout": str(result.stdout),
        "stderr": str(result.stderr),
    }


def classify_standalone_output(status: int, combined: str) -> str:
    if status == 0:
        return "standalone_validated"
    if any(pattern.search(combined) for pattern in KNOWN_STANDALONE_LIMIT_PATTERNS):
        return "known_standalone_tool_limit"
    return "future_gate_candidate"


def standalone_validation(manifest: dict[str, Any], artifact_dir: Path) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    output_dir = artifact_dir / "standalone"
    for template in manifest["templates"]:
        path = REPO_ROOT / template["path"]
        name = slug(template["path"])
        if not path.exists():
            results.append(
                {
                    "template": template["path"],
                    "status": "missing",
                    "category": "actionable",
                    "reason": "template file is missing",
                }
            )
            continue
        result = run_command(
            ["gtk4-builder-tool", "validate", str(path)],
            cwd=REPO_ROOT,
            env=os.environ.copy(),
            stdout=output_dir / f"{name}.stdout",
            stderr=output_dir / f"{name}.stderr",
            timeout=60,
        )
        combined = read_text(result.stdout) + read_text(result.stderr)
        category = classify_standalone_output(result.status, combined)
        results.append(
            {
                "template": template["path"],
                "command": result.command,
                "status_code": result.status,
                "category": category,
                "stdout": str(result.stdout),
                "stderr": str(result.stderr),
            }
        )
    return results


def runtime_probe_env(runtime_dir: Path) -> dict[str, str]:
    env = os.environ.copy()
    env.update(
        {
            "GTK_DEBUG": "builder,builder-objects",
            "NO_AT_BRIDGE": "1",
            "GDK_DEBUG": "no-portals",
            "GTK_USE_PORTAL": "0",
            "GSK_RENDERER": env.get("GSK_RENDERER", "cairo"),
            "XDG_RUNTIME_DIR": str(runtime_dir),
            "GDK_BACKEND": "wayland",
            "LUSHTEXT_WIDGET_HEADLESS_RUNNER": "1",
            "LUSHTEXT_WIDGET_HEADLESS_MONITOR": MONITOR,
        }
    )
    env.pop("DISPLAY", None)
    env.pop("WAYLAND_DISPLAY", None)
    return env


def runtime_probes(manifest: dict[str, Any], artifact_dir: Path) -> list[dict[str, Any]]:
    missing = [name for name in ("dbus-run-session", "mutter", "cargo") if not command_available(name)]
    if missing:
        return [
            {
                "id": "runtime-preflight",
                "status": "unsupported_runtime",
                "status_code": 77,
                "reason": f"missing commands: {', '.join(missing)}",
                "templates": [],
            }
        ]

    output_dir = artifact_dir / "runtime"
    results: list[dict[str, Any]] = []
    for probe in manifest["runtime_probes"]:
        name = slug(probe["id"])
        with tempfile.TemporaryDirectory(
            prefix="lushtext-builder-diagnostics-",
            ignore_cleanup_errors=True,
        ) as runtime_dir:
            env = runtime_probe_env(Path(runtime_dir))
            command = [
                "dbus-run-session",
                "--",
                "mutter",
                "--headless",
                "--wayland",
                "--no-x11",
                "--virtual-monitor",
                MONITOR,
                "--",
                "cargo",
                "test",
                "-p",
                "lushtext",
                "--test",
                "widget",
                "--",
                probe["test"],
                "--exact",
                "--nocapture",
            ]
            result = run_command(
                command,
                cwd=REPO_ROOT,
                env=env,
                stdout=output_dir / f"{name}.stdout",
                stderr=output_dir / f"{name}.stderr",
                timeout=240,
            )
        results.append(
            {
                "id": probe["id"],
                "test": probe["test"],
                "state": probe["state"],
                "templates": probe["templates"],
                "command": result.command,
                "status_code": result.status,
                "status": "passed" if result.status == 0 else "failed",
                "stdout": str(result.stdout),
                "stderr": str(result.stderr),
            }
        )
    return results


def classify_line(line: str, *, source: str) -> str | None:
    stripped = line.strip()
    if not stripped:
        return None
    if any(pattern.search(stripped) for pattern in BENIGN_PATTERNS):
        return "benign_noise"
    if (
        source.endswith("runtime/shortcuts-no-context.stderr")
        and ADW_SHORTCUTS_DIALOG_HOST_PATTERN.search(stripped)
    ):
        return "benign_noise"
    if "GTK_DEBUG set but ignored because gtk isn't built with G_ENABLE_DEBUG" in stripped:
        return "unsupported_runtime"
    if "standalone" in source and any(
        pattern.search(stripped) for pattern in KNOWN_STANDALONE_LIMIT_PATTERNS
    ):
        return "known_standalone_tool_limit"
    if any(pattern.search(stripped) for pattern in ACTIONABLE_PATTERNS):
        return "actionable"
    if any(pattern.search(stripped) for pattern in SUSPICIOUS_PATTERNS):
        return "future_gate_candidate"
    return None


def classify_logs(
    standalone: list[dict[str, Any]], runtime: list[dict[str, Any]]
) -> dict[str, list[dict[str, str]]]:
    findings: dict[str, list[dict[str, str]]] = {
        "actionable": [],
        "known_standalone_tool_limit": [],
        "benign_noise": [],
        "unsupported_runtime": [],
        "future_gate_candidate": [],
        "unclassified": [],
    }

    for entry in [*standalone, *runtime]:
        for field in ("stdout", "stderr"):
            path_text = entry.get(field)
            if not path_text:
                continue
            path = Path(path_text)
            if not path.exists():
                continue
            for lineno, line in enumerate(read_text(path).splitlines(), start=1):
                category = classify_line(line, source=path_text)
                if category is None:
                    continue
                finding = {
                    "source": path_text,
                    "line": str(lineno),
                    "text": line[:500],
                }
                findings[category].append(finding)

    for entry in standalone:
        if entry.get("category") == "known_standalone_tool_limit":
            findings["known_standalone_tool_limit"].append(
                {
                    "source": entry.get("stderr", ""),
                    "line": "0",
                    "text": f"{entry['template']} requires initialized runtime context",
                }
            )
        if entry.get("category") == "future_gate_candidate":
            findings["future_gate_candidate"].append(
                {
                    "source": entry.get("stderr", ""),
                    "line": "0",
                    "text": f"{entry['template']} did not validate standalone",
                }
            )

    for entry in runtime:
        if entry.get("status") == "unsupported_runtime":
            findings["unsupported_runtime"].append(
                {"source": "runtime-preflight", "line": "0", "text": entry.get("reason", "")}
            )
        elif entry.get("status_code") != 0:
            findings["actionable"].append(
                {
                    "source": entry.get("stderr", ""),
                    "line": "0",
                    "text": f"runtime probe {entry.get('id')} exited {entry.get('status_code')}",
                }
            )

    return findings


def coverage_report(
    manifest: dict[str, Any],
    standalone: list[dict[str, Any]],
    runtime: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    standalone_by_template = {entry["template"]: entry for entry in standalone}
    covered_by_template: dict[str, list[dict[str, Any]]] = {}
    for probe in runtime:
        if probe.get("status") != "passed":
            continue
        for template in probe.get("templates", []):
            covered_by_template.setdefault(template, []).append(
                {
                    "probe": probe["id"],
                    "test": probe["test"],
                    "state": probe["state"],
                }
            )

    rows: list[dict[str, Any]] = []
    for template in manifest["templates"]:
        path = template["path"]
        probes = covered_by_template.get(path, [])
        standalone_result = standalone_by_template.get(path)
        if probes:
            status = "runtime_instantiated"
        elif standalone_result and standalone_result.get("category") == "standalone_validated":
            status = "standalone_validated"
        elif standalone_result and standalone_result.get("category") == "known_standalone_tool_limit":
            status = "known_standalone_tool_limit"
        else:
            status = "uncovered"
        rows.append(
            {
                "template": path,
                "source": template.get("source", ""),
                "coverage_status": status,
                "runtime_probes": probes,
                "standalone_category": standalone_result.get("category") if standalone_result else "",
                "standalone_status_code": standalone_result.get("status_code")
                if standalone_result
                else None,
            }
        )
    return rows


def write_markdown_summary(
    path: Path,
    *,
    capability: dict[str, Any],
    coverage: list[dict[str, Any]],
    findings: dict[str, list[dict[str, str]]],
    runtime: list[dict[str, Any]],
    exit_status: int,
) -> None:
    counts = {category: len(rows) for category, rows in findings.items()}
    lines = [
        "# Builder Diagnostics Summary",
        "",
        f"- Generated: {now_iso()}",
        f"- Exit status: {exit_status}",
        f"- Runtime capability: {capability['status']} ({capability['reason']})",
        f"- Runtime probes: {sum(1 for item in runtime if item.get('status') == 'passed')}/{len(runtime)} passed",
        "",
        "## Finding Counts",
        "",
    ]
    for category in (
        "actionable",
        "unsupported_runtime",
        "future_gate_candidate",
        "known_standalone_tool_limit",
        "benign_noise",
    ):
        lines.append(f"- {category}: {counts.get(category, 0)}")
    lines.extend(["", "## Coverage", ""])
    for row in coverage:
        probes = ", ".join(probe["probe"] for probe in row["runtime_probes"]) or "none"
        lines.append(f"- `{row['template']}`: {row['coverage_status']} (runtime probes: {probes})")
    lines.extend(["", "## Actionable Findings", ""])
    if findings["actionable"]:
        for finding in findings["actionable"][:50]:
            lines.append(f"- `{finding['source']}:{finding['line']}` {finding['text']}")
    else:
        lines.append("No actionable builder diagnostics were classified for the covered surfaces.")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifact-dir", default="build/smoke/builder-diagnostics")
    parser.add_argument("--manifest", default=str(DEFAULT_MANIFEST))
    parser.add_argument("--provider", default="host")
    parser.add_argument("--image", default="")
    parser.add_argument("--required-runtime", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    artifact_dir = Path(args.artifact_dir).resolve()
    artifact_dir.mkdir(parents=True, exist_ok=True)
    for child in ("capability", "standalone", "runtime"):
        path = artifact_dir / child
        if path.exists():
            shutil.rmtree(path)
    manifest = json.loads(Path(args.manifest).read_text(encoding="utf-8"))

    metadata = runtime_metadata(args.provider, args.image)
    write_json(artifact_dir / "runtime-metadata.json", metadata)

    capability = debug_capability(artifact_dir)
    write_json(artifact_dir / "capability.json", capability)
    if not capability["supported"]:
        coverage = [
            {
                "template": template["path"],
                "source": template.get("source", ""),
                "coverage_status": "unsupported_runtime",
                "runtime_probes": [],
                "standalone_category": "",
                "standalone_status_code": None,
            }
            for template in manifest["templates"]
        ]
        findings = {
            "actionable": [],
            "known_standalone_tool_limit": [],
            "benign_noise": [],
            "unsupported_runtime": [
                {"source": "capability", "line": "0", "text": capability["reason"]}
            ],
            "future_gate_candidate": [],
            "unclassified": [],
        }
        exit_status = 1 if args.required_runtime else 0
        write_json(artifact_dir / "coverage.json", coverage)
        write_json(artifact_dir / "findings.json", findings)
        write_json(
            artifact_dir / "summary.json",
            {
                "status": "unsupported_runtime",
                "runtime_capability": capability,
                "finding_counts": {key: len(value) for key, value in findings.items()},
                "coverage": coverage,
                "exit_status": exit_status,
            },
        )
        write_markdown_summary(
            artifact_dir / "summary.md",
            capability=capability,
            coverage=coverage,
            findings=findings,
            runtime=[],
            exit_status=exit_status,
        )
        print(f"Builder diagnostics unsupported runtime. Artifacts: {artifact_dir}")
        return exit_status

    standalone = standalone_validation(manifest, artifact_dir)
    runtime = runtime_probes(manifest, artifact_dir)
    findings = classify_logs(standalone, runtime)
    coverage = coverage_report(manifest, standalone, runtime)

    write_json(artifact_dir / "standalone-results.json", standalone)
    write_json(artifact_dir / "runtime-results.json", runtime)
    write_json(artifact_dir / "findings.json", findings)
    write_json(artifact_dir / "coverage.json", coverage)

    actionable_count = len(findings["actionable"])
    unsupported_count = len(findings["unsupported_runtime"])
    uncovered_count = sum(1 for row in coverage if row["coverage_status"] == "uncovered")
    exit_status = 1 if actionable_count or unsupported_count else 0
    summary_status = "failed" if exit_status else "passed"
    write_json(
        artifact_dir / "summary.json",
        {
            "status": summary_status,
            "runtime_capability": capability,
            "finding_counts": {key: len(value) for key, value in findings.items()},
            "coverage_counts": {
                "total": len(coverage),
                "runtime_instantiated": sum(
                    1 for row in coverage if row["coverage_status"] == "runtime_instantiated"
                ),
                "standalone_validated": sum(
                    1 for row in coverage if row["coverage_status"] == "standalone_validated"
                ),
                "known_standalone_tool_limit": sum(
                    1 for row in coverage if row["coverage_status"] == "known_standalone_tool_limit"
                ),
                "uncovered": uncovered_count,
            },
            "coverage": coverage,
            "exit_status": exit_status,
        },
    )
    write_markdown_summary(
        artifact_dir / "summary.md",
        capability=capability,
        coverage=coverage,
        findings=findings,
        runtime=runtime,
        exit_status=exit_status,
    )
    print(f"Builder diagnostics {summary_status}. Artifacts: {artifact_dir}")
    return exit_status


if __name__ == "__main__":
    sys.exit(main())
