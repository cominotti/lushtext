#!/usr/bin/env python3
"""Adversarial tests for the dependency-free agent-skill validator."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("validate-agent-skills.py")
SPEC = importlib.util.spec_from_file_location("validate_agent_skills", SCRIPT)
if SPEC is None or SPEC.loader is None:  # pragma: no cover - import machinery failure
    raise RuntimeError(f"Could not load {SCRIPT}")
VALIDATOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VALIDATOR)


class ValidatorFixture:
    """Build one isolated skills tree without touching the repository."""

    def __init__(self, root: Path) -> None:
        self.root = root

    def add_skill(
        self,
        name: str,
        *,
        frontmatter: str | None = None,
        body: str = "# Instructions\n\nDo the bounded task.\n",
        openai: str | None = None,
    ) -> Path:
        skill = self.root / name
        (skill / "agents").mkdir(parents=True)
        metadata = frontmatter or f'name: {name}\ndescription: "Use for deterministic {name} work."'
        (skill / "SKILL.md").write_text(
            f"---\n{metadata}\n---\n\n{body}", encoding="utf-8"
        )
        interface = openai or (
            "interface:\n"
            f'  display_name: "{name.title()}"\n'
            '  short_description: "Run one deterministic repository workflow"\n'
            f'  default_prompt: "Use ${name} to complete this bounded workflow."\n'
        )
        (skill / "agents" / "openai.yaml").write_text(interface, encoding="utf-8")
        return skill

    def write_policy(
        self,
        *,
        implicit: dict[str, bool],
        umbrella: str,
        leaves: tuple[str, ...] = (),
    ) -> None:
        implicit_rows = "\n".join(
            f'{name} = {str(value).lower()}' for name, value in implicit.items()
        )
        leaf_rows = ", ".join(f'"{name}"' for name in leaves)
        (self.root / "skill-policy.toml").write_text(
            "schema_version = 1\n"
            'excluded_prefixes = ["openspec-", "speckit-"]\n\n'
            "[implicit_invocation]\n"
            f"{implicit_rows}\n\n"
            "[performance]\n"
            f'umbrella = "{umbrella}"\n'
            f"leaves = [{leaf_rows}]\n"
            'package_metadata_namespace = "lushtext-agent"\n'
            'package_metadata_key = "performance-roots"\n'
            'fallback_suffixes = ["src/ui", "src/services", "src/model", "benches"]\n\n'
            "[filesystem_contract]\n"
            "paths = []\n\n"
            "[release]\n"
            'workflow_role_marker = "agent-release-role"\n'
            'required_workflow_roles = ["publication", "benchmark-report"]\n',
            encoding="utf-8",
        )


class AgentSkillValidatorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name) / "skills"
        self.root.mkdir()
        self.fixture = ValidatorFixture(self.root)
        self.fixture.add_skill(
            "routing-root",
            openai=(
                "interface:\n"
                '  display_name: "Routing Root"\n'
                '  short_description: "Provide the fixture routing umbrella policy"\n'
                '  default_prompt: "Use $routing-root for fixture routing."\n'
                "policy:\n"
                "  allow_implicit_invocation: true\n"
            ),
        )
        self.fixture.write_policy(implicit={"routing-root": True}, umbrella="routing-root")

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def errors(self) -> list[str]:
        return VALIDATOR.validate_skills(self.root)

    def test_valid_tree_accepts_images_anchors_spaces_parentheses_and_exclusions(self) -> None:
        skill = self.fixture.add_skill(
            "valid-skill",
            body=(
                "# Instructions\n\n"
                "See [the guide](<references/Space (Guide).md#section-one> \"Guide title\"), "
                "[the local section](#local-section), and ![diagram](assets/diagram.png).\n\n"
                "Inline code is not a link: `[ignored](references/missing.md)`.\n\n"
                "Reference forms also work: [guide][guide-ref] and ![diagram][diagram-ref].\n\n"
                "[guide-ref]: <references/Space (Guide).md#section-one> \"Reference title\"\n"
                "[diagram-ref]: assets/diagram.png \"Diagram title\"\n\n"
                "## Local section\n\nUse the evidence.\n"
            ),
        )
        (skill / "references").mkdir()
        (skill / "references" / "Space (Guide).md").write_text(
            "# Guide\n\n## Section one\n\nCurrent guidance.\n", encoding="utf-8"
        )
        (skill / "assets").mkdir()
        (skill / "assets" / "diagram.png").write_bytes(b"fixture")

        # These families are generated or externally maintained and deliberately excluded.
        (self.root / "openspec-invalid").mkdir()
        (self.root / "speckit-invalid").mkdir()

        self.assertEqual(self.errors(), [])

    def test_rejects_duplicate_and_malformed_yaml_deterministically(self) -> None:
        self.fixture.add_skill(
            "duplicate-yaml",
            frontmatter=(
                "name: duplicate-yaml\n"
                "name: duplicate-yaml\n"
                'description: "Duplicate metadata."'
            ),
        )
        self.fixture.add_skill(
            "malformed-yaml",
            frontmatter=(
                "name: malformed-yaml\n"
                ' description: "Odd indentation is invalid."'
            ),
        )

        first = self.errors()
        second = self.errors()
        self.assertEqual(first, second)
        self.assertEqual(first, sorted(set(first)))
        self.assertTrue(any("duplicate key 'name'" in error for error in first))
        self.assertTrue(any("indentation must use two-space steps" in error for error in first))

    def test_rejects_missing_body_and_missing_interface_metadata(self) -> None:
        self.fixture.add_skill("empty-body", body="   \n")
        self.fixture.add_skill(
            "missing-interface",
            openai=(
                "interface:\n"
                '  display_name: "Missing Interface"\n'
                '  short_description: "Missing its required default prompt"\n'
            ),
        )
        missing_file = self.fixture.add_skill("missing-metadata-file")
        (missing_file / "agents" / "openai.yaml").unlink()

        errors = self.errors()
        self.assertTrue(any("instruction body must not be empty" in error for error in errors))
        self.assertTrue(any("missing interface keys: default_prompt" in error for error in errors))
        self.assertTrue(
            any("missing-metadata-file/agents/openai.yaml: required file is missing" in error for error in errors)
        )

    def test_rejects_icons_outside_assets_or_outside_skill(self) -> None:
        outside = self.root / "outside.svg"
        outside.write_text("<svg/>", encoding="utf-8")
        skill = self.fixture.add_skill(
            "escaped-icon",
            openai=(
                "interface:\n"
                '  display_name: "Escaped Icon"\n'
                '  short_description: "Reject escaped icon metadata paths"\n'
                '  default_prompt: "Use $escaped-icon to validate icon containment."\n'
                '  icon_small: "../outside.svg"\n'
                '  icon_large: "icon.svg"\n'
            ),
        )
        local = skill / "icon.svg"
        local.write_text("<svg/>", encoding="utf-8")

        errors = self.errors()
        containment_errors = [error for error in errors if "contained in assets/" in error]
        self.assertEqual(len(containment_errors), 2)
        self.assertTrue(any("../outside.svg" in error for error in containment_errors))
        self.assertTrue(any("icon.svg" in error for error in containment_errors))

    def test_rejects_broken_file_image_and_heading_links(self) -> None:
        skill = self.fixture.add_skill(
            "broken-links",
            body=(
                "# Instructions\n\n"
                "[missing](references/missing.md)\n\n"
                "![missing image](assets/missing.png)\n\n"
                "[bad anchor](references/guide.md#absent-heading)\n"
                "[missing reference][missing-ref]\n\n"
                "![missing reference image][missing-image-ref]\n\n"
                "[missing-ref]: references/reference-missing.md \"Missing\"\n"
                "[missing-image-ref]: assets/reference-missing.png\n"
            ),
        )
        (skill / "references").mkdir()
        (skill / "references" / "guide.md").write_text(
            "# Guide\n\n## Present heading\n", encoding="utf-8"
        )

        errors = self.errors()
        self.assertTrue(any("references/missing.md" in error for error in errors))
        self.assertTrue(any("assets/missing.png" in error for error in errors))
        self.assertTrue(any("references/reference-missing.md" in error for error in errors))
        self.assertTrue(any("assets/reference-missing.png" in error for error in errors))
        self.assertTrue(any("broken Markdown anchor" in error for error in errors))

    def test_rejects_oversized_skill_and_long_reference_without_contents(self) -> None:
        body = "# Instructions\n" + "\nReasoned rule." * 500 + "\n"
        skill = self.fixture.add_skill("oversized-skill", body=body)
        (skill / "references").mkdir()
        (skill / "references" / "long.md").write_text(
            "# Long reference\n" + "\nEvidence." * 101 + "\n", encoding="utf-8"
        )

        errors = self.errors()
        self.assertTrue(any("500-line ceiling" in error for error in errors))
        self.assertTrue(any("need a Contents section" in error for error in errors))

    def test_rejects_broken_automatic_invocation_topology(self) -> None:
        self.fixture.add_skill(
            "learn",
            openai=(
                "interface:\n"
                '  display_name: "Learn"\n'
                '  short_description: "Review durable repository learning candidates"\n'
                '  default_prompt: "Use $learn to review completed work."\n'
                "policy:\n"
                "  allow_implicit_invocation: false\n"
            ),
        )
        self.fixture.add_skill(
            "gtk-perf-review",
            openai=(
                "interface:\n"
                '  display_name: "GTK Performance Review"\n'
                '  short_description: "Review GTK Rust performance across domains"\n'
                '  default_prompt: "Use $gtk-perf-review to review this change."\n'
                "policy:\n"
                "  allow_implicit_invocation: false\n"
            ),
        )
        for name in ("gtk-perf-rust-optimize", "gtk-perf-scale", "gtk-responsiveness"):
            self.fixture.add_skill(name)

        self.fixture.write_policy(
            implicit={
                "learn": True,
                "gtk-perf-review": True,
                "gtk-perf-rust-optimize": False,
                "gtk-perf-scale": False,
                "gtk-responsiveness": False,
            },
            umbrella="gtk-perf-review",
            leaves=("gtk-perf-rust-optimize", "gtk-perf-scale", "gtk-responsiveness"),
        )

        errors = self.errors()
        self.assertTrue(any("learn must set allow_implicit_invocation: true" in error for error in errors))
        self.assertTrue(
            any("gtk-perf-review must set allow_implicit_invocation: true" in error for error in errors)
        )
        for name in ("gtk-perf-rust-optimize", "gtk-perf-scale", "gtk-responsiveness"):
            self.assertTrue(
                any(f"{name} must set allow_implicit_invocation: false" in error for error in errors)
            )

    def test_registry_makes_a_skill_rename_fail_loudly(self) -> None:
        self.fixture.write_policy(implicit={"renamed-away": True}, umbrella="renamed-away")
        errors = self.errors()
        self.assertTrue(
            any("registered skill is missing: renamed-away" in error for error in errors)
        )


if __name__ == "__main__":
    unittest.main()
