#!/usr/bin/env python3
"""Tests for rename-safe GTK debugging helper configuration."""

from __future__ import annotations

import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


MUTTER_SCRIPT = (
    Path(__file__).resolve().parent.parent
    / ".agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py"
)
XVFB_SCRIPT = MUTTER_SCRIPT.with_name("capture-lushtext-xvfb.sh")
SPEC = importlib.util.spec_from_file_location("capture_lushtext_mutter", MUTTER_SCRIPT)
if SPEC is None or SPEC.loader is None:  # pragma: no cover
    raise RuntimeError(f"Could not load {MUTTER_SCRIPT}")
MUTTER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MUTTER
SPEC.loader.exec_module(MUTTER)


class GtkDebugConfigurationTests(unittest.TestCase):
    def test_app_id_and_repo_root_rebase_all_dependent_defaults(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            argv = [
                str(MUTTER_SCRIPT),
                "--file",
                str(root / "fixture.txt"),
                "--output",
                str(root / "capture.png"),
                "--repo-root",
                str(root),
                "--app-id",
                "org.example.OnlyIdChanged",
            ]
            with mock.patch.object(sys, "argv", argv):
                args = MUTTER.parse_args()
            MUTTER.configure_runtime(args)
            self.assertEqual(args.binary, root / "target/debug/lushtext")
            self.assertEqual(args.app_object_path, "/org/example/OnlyIdChanged")
            self.assertEqual(args.automation_interface, "org.example.OnlyIdChanged.Automation1")
            self.assertEqual(args.gsettings_schema, "org.example.OnlyIdChanged")
            self.assertEqual(args.gsettings_schema_dir, root / "data")

    def test_mutter_cli_propagates_renamed_identity_to_child_processes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            schema_dir = root / "schemas"
            schema_dir.mkdir()
            argv = [
                str(MUTTER_SCRIPT),
                "--file",
                str(root / "fixture.txt"),
                "--output",
                str(root / "capture.png"),
                "--binary",
                str(root / "target/debug/renamed-editor"),
                "--repo-root",
                str(root),
                "--app-id",
                "org.example.RenamedEditor",
                "--app-object-path",
                "/org/example/RenamedEditor",
                "--automation-interface",
                "org.example.RenamedEditor.Automation9",
                "--gsettings-schema",
                "org.example.RenamedEditor.Settings",
                "--gsettings-schema-dir",
                str(schema_dir),
            ]
            with mock.patch.object(sys, "argv", argv):
                args = MUTTER.parse_args()
            MUTTER.configure_runtime(args)
            child = MUTTER.child_cli_args(args, "internal-run")

            self.assertEqual(MUTTER.APP_ID, "org.example.RenamedEditor")
            self.assertEqual(MUTTER.WINDOW_OBJECT_PATH, "/org/example/RenamedEditor/window/1")
            self.assertEqual(
                MUTTER.AUTOMATION_INTERFACE,
                "org.example.RenamedEditor.Automation9",
            )
            for flag, value in (
                ("--repo-root", str(root)),
                ("--app-id", "org.example.RenamedEditor"),
                ("--app-object-path", "/org/example/RenamedEditor"),
                ("--automation-interface", "org.example.RenamedEditor.Automation9"),
                ("--gsettings-schema", "org.example.RenamedEditor.Settings"),
                ("--gsettings-schema-dir", str(schema_dir)),
            ):
                self.assertEqual(child[child.index(flag) + 1], value)

    def test_xvfb_help_exposes_the_same_identity_override_surface(self) -> None:
        result = subprocess.run(
            [str(XVFB_SCRIPT), "--help"],
            check=True,
            text=True,
            capture_output=True,
        )
        for flag in (
            "--repo-root",
            "--binary",
            "--app-id",
            "--app-object-path",
            "--gsettings-schema",
            "--gsettings-schema-dir",
        ):
            self.assertIn(flag, result.stdout)


if __name__ == "__main__":
    unittest.main()
