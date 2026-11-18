#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""The smoke drivers' one way into a private headless session.

Every Python lane starts its session through ``scripts/headless-session`` (the
``gtk-lush-session`` binary of ``gtk-lush-proof-harness``), never by naming
the session bus launcher, the compositor or the media daemons itself
(``make check-session-hygiene`` enforces it). The outer stage is
``session --profile <name>``: the session bus, a private monitored system
bus, quiet media for the screencast and accessibility profiles, and the
runtime guard, which exits ``SESSION_HYGIENE_EXIT_CODE`` with
``SESSION-HYGIENE:`` lines when the session touched the host. Inside it, the
compositor stage is ``compositor --monitor WxH``.

Profiles: ``widget`` (nothing but the bus and the compositor), ``plain``
(portals allowed), ``screencast`` (quiet PipeWire and WirePlumber for mutter's
ScreenCast), ``accessibility`` (AT-SPI plus screencast).
"""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
HEADLESS_SESSION = REPO_ROOT / "scripts" / "headless-session"
# gtk_lush_proof_harness::session::SESSION_HYGIENE_EXIT_CODE
SESSION_HYGIENE_EXIT_CODE = 93
# The helper's status for a host that cannot run the session.
UNSUPPORTED_HOST_EXIT_CODE = 77
# The manifests' failure status for a session that touched the host.
SESSION_HYGIENE_FAILURE_STATUS = "session-hygiene"


def session_command(
    profile: str,
    child: list[str],
    *,
    log_dir: Path | None = None,
    options: tuple[str, ...] = (),
) -> list[str]:
    """The outer stage around ``child``, in the caller's private
    ``XDG_RUNTIME_DIR`` (the helper refuses the user's real one)."""
    command = [str(HEADLESS_SESSION), "session", "--profile", profile, *options, "--inherit-runtime-dir"]
    if log_dir is not None:
        command += ["--log-dir", str(log_dir)]
    return [*command, "--", *child]


def compositor_command(width: int, height: int, child: list[str]) -> list[str]:
    """The headless compositor stage, valid only inside a helper session."""
    return [str(HEADLESS_SESSION), "compositor", "--monitor", f"{width}x{height}", "--", *child]


def check_tools(profile: str, *options: str) -> str | None:
    """None when the host can run ``profile``; otherwise why not."""
    result = subprocess.run(
        [str(HEADLESS_SESSION), "check-tools", "--profile", profile, *options],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=False,
    )
    if result.returncode == 0:
        return None
    tail = result.stdout.strip().splitlines()[-1:] or [""]
    return f"the private headless session ({profile}) is unavailable (exit {result.returncode}): {tail[0]}"


def _tools_report(profile: str, *options: str) -> dict:
    result = subprocess.run(
        [str(HEADLESS_SESSION), "check-tools", "--profile", profile, *options, "--json"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        check=False,
    )
    try:
        report = json.loads(result.stdout)
    except json.JSONDecodeError:
        return {}
    return report if isinstance(report, dict) else {}


def compositor_version(profile: str, *options: str) -> tuple[str, str | None]:
    """The compositor's command name and ``--version`` line, by role, from
    ``check-tools --json`` (name ``compositor`` and no version when the
    helper cannot report it). Manifests key the version by that name."""
    report = _tools_report(profile, *options)
    name = str(report.get("roles", {}).get("compositor") or "compositor")
    tool = report.get("commands", {}).get(name)
    version = tool.get("version") if isinstance(tool, dict) else None
    return name, (str(version) if version else None)


def tool_versions(profile: str, *options: str) -> dict[str, str]:
    """Each session command's ``--version`` line, from ``check-tools --json``
    (empty when the host cannot run ``profile``)."""
    commands = _tools_report(profile, *options).get("commands", {})
    versions: dict[str, str] = {}
    for name, tool in commands.items():
        if isinstance(tool, dict):
            versions[str(name)] = str(tool.get("version") or "")
    return versions
