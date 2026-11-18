#!/usr/bin/env python3
"""Adversarial tests for layout-independent agent topology discovery."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("agent-topology.py")
SPEC = importlib.util.spec_from_file_location("agent_topology", SCRIPT)
if SPEC is None or SPEC.loader is None:  # pragma: no cover
    raise RuntimeError(f"Could not load {SCRIPT}")
TOPOLOGY = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = TOPOLOGY
SPEC.loader.exec_module(TOPOLOGY)


class AgentTopologyTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.policy = {
            "performance": {
                "package_metadata_namespace": "lushtext-agent",
                "package_metadata_key": "performance-roots",
                "fallback_suffixes": ["src/ui", "src/services", "src/model", "benches"],
            },
            "release": {
                "workflow_role_marker": "agent-release-role",
                "required_workflow_roles": ["publication", "benchmark-report"],
            },
        }
        engine = self.root / "components" / "renamed-editor"
        platform = self.root / "libraries" / "renamed-platform"
        for directory in (engine, platform):
            directory.mkdir(parents=True)
            (directory / "Cargo.toml").write_text("[package]\nname='fixture'\n", encoding="utf-8")
        self.metadata = {
            "workspace_root": str(self.root),
            "workspace_members": ["engine-id", "platform-id"],
            "packages": [
                {
                    "id": "engine-id",
                    "name": "text-engine-after-rename",
                    "manifest_path": str(engine / "Cargo.toml"),
                    "metadata": {
                        "lushtext-agent": {
                            "performance-roots": ["source/presentation", "source/io"],
                            "release-ui-roots": ["source/presentation"],
                            "release-service-roots": ["source/io"],
                            "release-model-roots": ["source/domain"],
                        }
                    },
                    "targets": [
                        {
                            "name": "renamed-widget-suite",
                            "kind": ["test"],
                            "src_path": str(engine / "verification" / "widgets.rs"),
                            "required-features": ["gtk-tests"],
                            "test": True,
                        }
                    ],
                },
                {
                    "id": "platform-id",
                    "name": "platform-after-rename",
                    "manifest_path": str(platform / "Cargo.toml"),
                    "metadata": {"lushtext-agent": {"performance-roots": ["code"]}},
                    "targets": [],
                },
            ],
        }
        self.topology = TOPOLOGY.WorkspaceTopology(self.metadata, self.policy, self.root)

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def test_performance_scope_survives_package_and_directory_renames(self) -> None:
        self.assertTrue(
            self.topology.performance_path(
                "components/renamed-editor/source/presentation/window.rs"
            )
        )
        self.assertTrue(
            self.topology.performance_path("libraries/renamed-platform/code/lib.rs")
        )
        self.assertTrue(
            self.topology.performance_path("components/renamed-editor/Cargo.toml")
        )
        self.assertTrue(self.topology.performance_path("Cargo.lock"))
        self.assertFalse(
            self.topology.performance_path("components/renamed-editor/docs/design.md")
        )

    def test_release_categories_use_roles_and_file_types_not_fixed_names(self) -> None:
        self.assertEqual(
            self.topology.release_categories(
                "components/renamed-editor/source/presentation/window.rs"
            ),
            {"ui"},
        )
        self.assertEqual(
            self.topology.release_categories("packaging/org.example.Editor.metainfo.xml.in"),
            {"packaging"},
        )
        self.assertEqual(
            self.topology.release_categories(".github/workflows/publish-renamed.yaml"),
            {"release-automation"},
        )

    def test_test_target_discovery_uses_cargo_target_metadata(self) -> None:
        self.assertEqual(
            self.topology.testing_surfaces(),
            [
                {
                    "package": "text-engine-after-rename",
                    "manifest": "components/renamed-editor/Cargo.toml",
                    "target": "renamed-widget-suite",
                    "kind": ["test"],
                    "src_path": "components/renamed-editor/verification/widgets.rs",
                    "required_features": ["gtk-tests"],
                }
            ],
        )

    def test_metainfo_discovery_accepts_an_alternate_name_and_location(self) -> None:
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        metainfo = self.root / "packaging" / "org.example.Editor.metainfo.xml.in"
        metainfo.parent.mkdir()
        metainfo.write_text("<component/>\n", encoding="utf-8")
        subprocess.run(["git", "add", str(metainfo.relative_to(self.root))], cwd=self.root, check=True)
        self.assertEqual(
            TOPOLOGY.discover_metainfo(self.root),
            ["packaging/org.example.Editor.metainfo.xml.in"],
        )

    def test_metadata_fixture_round_trips_through_loader(self) -> None:
        metadata_path = self.root / "metadata.json"
        metadata_path.write_text(json.dumps(self.metadata), encoding="utf-8")
        self.assertEqual(
            TOPOLOGY.load_cargo_metadata(self.root, metadata_path)["workspace_members"],
            ["engine-id", "platform-id"],
        )

    def test_release_workflow_roles_survive_file_and_display_name_renames(self) -> None:
        workflows = self.root / ".github" / "workflows"
        workflows.mkdir(parents=True)
        (workflows / "publish-any-name.yaml").write_text(
            "# agent-release-role: publication\n"
            "name: Ship Renamed Product\n"
            "on:\n  push:\n    tags: ['v*']\n",
            encoding="utf-8",
        )
        (workflows / "measure-any-name.yml").write_text(
            "# agent-release-role: benchmark-report\n"
            'name: "Measure Renamed Product"\n'
            "on:\n  push:\n    tags:\n      - 'v*'\n",
            encoding="utf-8",
        )
        discovered = TOPOLOGY.discover_release_workflows(self.root, self.policy)
        self.assertEqual(
            [(item["role"], item["name"]) for item in discovered],
            [
                ("benchmark-report", "Measure Renamed Product"),
                ("publication", "Ship Renamed Product"),
            ],
        )

    def test_stale_registered_root_fails_instead_of_hiding_scope(self) -> None:
        errors = self.topology.validation_errors()
        self.assertIn(
            "text-engine-after-rename: performance-roots root does not exist: source/presentation",
            errors,
        )


class ReviewLensTests(unittest.TestCase):
    """The review fan-out flags of `review-lenses`, from the skills' declared triggers."""

    POLICY = {
        "performance": {
            "package_metadata_namespace": "lushtext-agent",
            "package_metadata_key": "performance-roots",
            "fallback_suffixes": ["src/ui", "src/services", "src/model", "benches"],
        },
        "data_safety": {
            "skill": "data-safety",
            "path_suffixes": ["src/ui", "src/services", "src/model"],
            "content_patterns": [
                {"keyword": "spawn_blocking_then", "pattern": "spawn_blocking_then"},
                {"keyword": "persistence", "pattern": "json_store|durable_write"},
            ],
        },
    }

    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        packages = []
        for name, root, roots in (
            ("lushtext-core", "crates/lushtext-core", ["src/ui", "src/services", "src/model"]),
            ("gtk-lush-settle", "crates/gtk-lush/settle", ["src"]),
            ("gtk-lush-tasks", "crates/gtk-lush/tasks", []),
        ):
            directory = self.root / root
            directory.mkdir(parents=True)
            (directory / "Cargo.toml").write_text("[package]\nname='fixture'\n", encoding="utf-8")
            packages.append(
                {
                    "id": name,
                    "name": name,
                    "manifest_path": str(directory / "Cargo.toml"),
                    "metadata": {"lushtext-agent": {"performance-roots": roots}},
                    "targets": [],
                }
            )
        metadata = {
            "workspace_root": str(self.root),
            "workspace_members": [package["id"] for package in packages],
            "packages": packages,
        }
        self.topology = TOPOLOGY.WorkspaceTopology(metadata, self.POLICY, self.root)

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def lenses(self, *changes: tuple[str, str], texts: dict[str, str] | None = None) -> dict[str, object]:
        return TOPOLOGY.review_lenses(
            self.topology,
            self.POLICY,
            [TOPOLOGY.ChangedPath(status, path) for status, path in changes],
            texts or {},
        )

    def flags(self, result: dict[str, object]) -> tuple[bool, bool, bool]:
        return result["data"], result["performance"], result["modules_moved"]

    def test_draft_service_edit_is_data_and_performance(self) -> None:
        result = self.lenses(("M", "crates/lushtext-core/src/services/draft_service.rs"))
        self.assertEqual(self.flags(result), (True, True, False))

    def test_docs_edit_needs_no_domain_lens(self) -> None:
        result = self.lenses(("M", "docs/memorials/x.md"), ("A", "openspec/changes/x/design.md"))
        self.assertEqual(self.flags(result), (False, False, False))

    def test_gtk_lush_settle_edit_is_performance_only(self) -> None:
        result = self.lenses(
            ("M", "crates/gtk-lush/settle/src/lib.rs"),
            texts={"crates/gtk-lush/settle/src/lib.rs": "fn debounce() {}"},
        )
        self.assertEqual(self.flags(result), (False, True, False))

    def test_renamed_module_is_a_module_move(self) -> None:
        result = self.lenses(
            ("R", "crates/lushtext-core/src/model/old_name.rs"),
            ("R", "crates/lushtext-core/src/model/new_name.rs"),
        )
        self.assertEqual(self.flags(result), (True, True, True))
        result = self.lenses(("A", "crates/gtk-lush/settle/tests/new_test.rs"))
        self.assertFalse(result["modules_moved"], "a test file outside src/ is not a module move")

    def test_content_pattern_outside_the_suffixes_is_data(self) -> None:
        path = "crates/gtk-lush/tasks/src/lib.rs"
        result = self.lenses(("M", path), texts={path: "pub fn spawn_blocking_then<S>() {}"})
        self.assertEqual(self.flags(result), (True, False, False))
        self.assertEqual(result["data_content_hits"], {path: ["spawn_blocking_then"]})
        result = self.lenses(("M", path), texts={path: "pub fn unrelated() {}"})
        self.assertEqual(self.flags(result), (False, False, False))

    def test_collect_changes_reads_renames_uncommitted_and_untracked(self) -> None:
        env = dict(TOPOLOGY.scrubbed_git_env(), GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1")

        def git(*args: str) -> None:
            subprocess.run(
                ["git", "-C", str(self.root), "-c", "core.hooksPath=/dev/null", "-c", "user.name=t",
                 "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false", *args],
                check=True, capture_output=True, env=env,
            )

        subprocess.run(["git", "init", "-q", "-b", "main", str(self.root)], check=True, env=env)
        source = self.root / "crates" / "gtk-lush" / "tasks" / "src"
        source.mkdir(parents=True)
        (source / "lib.rs").write_text("pub fn a() {}\npub fn b() {}\npub fn c() {}\n", encoding="utf-8")
        git("add", "-A")
        git("commit", "-q", "-m", "base")
        git("checkout", "-q", "-b", "change")
        git("mv", "crates/gtk-lush/tasks/src/lib.rs", "crates/gtk-lush/tasks/src/core.rs")
        git("commit", "-q", "-m", "rename")
        (source / "core.rs").write_text(
            "pub fn a() {}\npub fn b() {}\npub fn c() {}\npub fn spawn_blocking_then() {}\n", encoding="utf-8"
        )
        (source / "fresh.rs").write_text("pub fn json_store() {}\n", encoding="utf-8")
        changes, texts = TOPOLOGY.collect_changes(self.root, "main")
        by_path = {change.path: change.status for change in changes}
        self.assertEqual(by_path.get("crates/gtk-lush/tasks/src/lib.rs"), "R")
        self.assertEqual(by_path.get("crates/gtk-lush/tasks/src/core.rs"), "R")
        self.assertEqual(by_path.get("crates/gtk-lush/tasks/src/fresh.rs"), "A")
        self.assertIn("spawn_blocking_then", texts["crates/gtk-lush/tasks/src/core.rs"])
        self.assertIn("json_store", texts["crates/gtk-lush/tasks/src/fresh.rs"])
        result = TOPOLOGY.review_lenses(self.topology, self.POLICY, changes, texts)
        self.assertEqual(self.flags(result), (True, False, True))


if __name__ == "__main__":
    unittest.main()
