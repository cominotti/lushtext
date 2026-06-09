#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Check that LushText's Flatpak manifest keeps intentional filesystem access."""

from __future__ import annotations

import argparse
import json
import sys
import tempfile
from pathlib import Path


REQUIRED_FILESYSTEM_PERMISSION = "--filesystem=host"


def check_manifest(path: Path) -> list[str]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return [f"{path}: manifest not found"]
    except json.JSONDecodeError as error:
        return [f"{path}: invalid JSON: {error}"]

    finish_args = data.get("finish-args")
    if not isinstance(finish_args, list) or not all(isinstance(arg, str) for arg in finish_args):
        return [f"{path}: finish-args must be a JSON string array"]

    if REQUIRED_FILESYSTEM_PERMISSION not in finish_args:
        return [
            f"{path}: missing {REQUIRED_FILESYSTEM_PERMISSION}; "
            "LushText intentionally keeps full filesystem permission for workspace editing, "
            "search, replace, monitoring, notes, history, and recovery workflows"
        ]

    return []


def run_self_test() -> list[str]:
    with tempfile.TemporaryDirectory(prefix="lushtext-flatpak-permission-check-") as temp:
        temp_path = Path(temp)
        valid = temp_path / "valid.json"
        invalid = temp_path / "invalid.json"
        valid.write_text(
            json.dumps({"finish-args": ["--socket=wayland", REQUIRED_FILESYSTEM_PERMISSION]}),
            encoding="utf-8",
        )
        invalid.write_text(
            json.dumps({"finish-args": ["--socket=wayland", "--filesystem=home"]}),
            encoding="utf-8",
        )

        failures = []
        if check_manifest(valid):
            failures.append("self-test valid manifest unexpectedly failed")
        if not check_manifest(invalid):
            failures.append("self-test invalid manifest unexpectedly passed")
        return failures


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("build-aux/dev.cominotti.lushtext.Flatpak.json"),
        help="Flatpak manifest to inspect",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="also prove the checker fails when the full filesystem permission is removed",
    )
    args = parser.parse_args(argv)

    failures: list[str] = []
    if args.self_test:
        failures.extend(run_self_test())
    failures.extend(check_manifest(args.manifest))

    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1

    print(
        f"Flatpak permission guard passed: {args.manifest} keeps "
        f"{REQUIRED_FILESYSTEM_PERMISSION}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
