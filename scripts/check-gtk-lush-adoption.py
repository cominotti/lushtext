#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Validate the bounded GTK Lush adoption evidence maintained for the internal
# platform and any future reopened publication track.

from __future__ import annotations

import sys
import tomllib
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
FAMILY_ROOT = REPO_ROOT / "crates" / "gtk-lush"
LAB_MEMBER = "crates/gtk-lush-adoption-lab"
MATRIX_PATH = REPO_ROOT / "docs" / "gtk-lush-adoption" / "matrix.toml"
EXPECTED_PACKAGES = {
    "gtk-lush-signals",
    "gtk-lush-settle",
    "gtk-lush-tasks",
    "gtk-lush-viewport",
    "gtk-lush-widgets",
    "gtk-lush-proof-harness",
    "gtk-lush-proof-spine",
}
LUSHTEXT_PACKAGES = {
    "lushtext",
    "lushtext-build-support",
    "lushtext-core",
}
REQUIRED_MATRIX_FIELDS = (
    "package",
    "lab_workflow",
    "standalone_example",
    "stock_fixture_status",
    "tests_proof_evidence",
    "friction_status",
    "api_decision",
    "follow_up",
)
REQUIRED_EVIDENCE_FILES = (
    "docs/gtk-lush-adoption/README.md",
    "docs/gtk-lush-adoption/matrix.toml",
    "docs/gtk-lush-adoption/timed-stock-settle.md",
    "docs/gtk-lush-adoption/external-project-spike.md",
    "docs/gtk-lush-adoption/api-review.md",
    "docs/gtk-lush-adoption/review-notes.md",
    "docs/gtk-lush-adoption/archive-handoff.md",
)
REQUIRED_GITIGNORE_PATTERNS = (
    "/build/gtk-lush-adoption/",
    "/fixtures/gtk-lush-adoption/*/target/",
    "/.claude/worktrees/",
)
DISALLOWED_RESIDUE_PATHS = (
    "docs/gtk-lush-adoption/external-checkouts",
)


def main() -> int:
    errors: list[str] = []
    root_manifest = load_toml(REPO_ROOT / "Cargo.toml", errors)
    matrix = load_toml(MATRIX_PATH, errors)
    if isinstance(root_manifest, dict):
        check_workspace(root_manifest, errors)
    if isinstance(matrix, dict):
        check_matrix(matrix, errors)
    check_evidence_files(errors)
    check_stock_fixtures(matrix if isinstance(matrix, dict) else {}, errors)
    check_gitignore(errors)
    check_disallowed_residue(errors)
    return report(errors)


def check_workspace(root_manifest: dict[str, Any], errors: list[str]) -> None:
    workspace = root_manifest.get("workspace", {})
    members = set(workspace.get("members", []))
    if LAB_MEMBER not in members:
        errors.append(f"{LAB_MEMBER} must be listed in [workspace].members")

    lab_path = REPO_ROOT / LAB_MEMBER
    if path_is_relative_to(lab_path.resolve(strict=False), FAMILY_ROOT.resolve(strict=False)):
        errors.append("adoption lab must not live under crates/gtk-lush/")

    lab_manifest = load_toml(lab_path / "Cargo.toml", errors)
    if not isinstance(lab_manifest, dict):
        return
    package = lab_manifest.get("package", {})
    if package.get("name") != "gtk-lush-adoption-lab":
        errors.append("adoption lab package name must be gtk-lush-adoption-lab")
    if package.get("publish") is not False:
        errors.append("adoption lab must set publish = false")
    check_no_lushtext_dependencies(lab_path, lab_manifest, errors)


def check_matrix(matrix: dict[str, Any], errors: list[str]) -> None:
    rows = matrix.get("crates", [])
    if not isinstance(rows, list):
        errors.append("matrix.toml must contain [[crates]] rows")
        return

    by_package: dict[str, dict[str, Any]] = {}
    for index, row in enumerate(rows, start=1):
        if not isinstance(row, dict):
            errors.append(f"matrix row {index} is not a TOML table")
            continue
        package = row.get("package")
        if not isinstance(package, str):
            errors.append(f"matrix row {index} missing package")
            continue
        if package in by_package:
            errors.append(f"duplicate matrix row for {package}")
        by_package[package] = row
        for field in REQUIRED_MATRIX_FIELDS:
            value = row.get(field)
            if not isinstance(value, str) or not value.strip():
                errors.append(f"matrix row for {package} missing non-empty {field}")

    missing = EXPECTED_PACKAGES - set(by_package)
    extra = set(by_package) - EXPECTED_PACKAGES
    for package in sorted(missing):
        errors.append(f"matrix missing functional crate {package}")
    for package in sorted(extra):
        errors.append(f"matrix contains unexpected GTK Lush package {package}")


def check_evidence_files(errors: list[str]) -> None:
    for relative_path in REQUIRED_EVIDENCE_FILES:
        path = REPO_ROOT / relative_path
        if not path.is_file():
            errors.append(f"missing adoption evidence file: {relative_path}")
        elif not path.read_text(encoding="utf-8").strip():
            errors.append(f"adoption evidence file is empty: {relative_path}")


def check_stock_fixtures(matrix: dict[str, Any], errors: list[str]) -> None:
    fixture_paths = matrix.get("stock_fixtures", [])
    if not isinstance(fixture_paths, list) or not fixture_paths:
        errors.append("matrix.toml must list stock_fixtures")
        return

    for relative_path in fixture_paths:
        if not isinstance(relative_path, str):
            errors.append("stock fixture entries must be strings")
            continue
        fixture_root = REPO_ROOT / relative_path
        manifest_path = fixture_root / "Cargo.toml"
        manifest = load_toml(manifest_path, errors)
        if not isinstance(manifest, dict):
            continue
        check_one_stock_fixture(fixture_root, manifest, errors)


def check_one_stock_fixture(
    fixture_root: Path,
    manifest: dict[str, Any],
    errors: list[str],
) -> None:
    family_deps: list[tuple[str, str]] = []
    forbidden_deps: list[str] = []
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        dependencies = manifest.get(section, {})
        if not isinstance(dependencies, dict):
            continue
        for dependency, spec in dependencies.items():
            package_name = dependency
            dependency_path = ""
            if isinstance(spec, dict):
                package_name = spec.get("package", dependency)
                dependency_path = spec.get("path", "")
            if package_name in LUSHTEXT_PACKAGES:
                forbidden_deps.append(package_name)
            if package_name.startswith("gtk-lush-"):
                family_deps.append((package_name, dependency_path))

    relative_fixture = fixture_root.relative_to(REPO_ROOT)
    if forbidden_deps:
        errors.append(
            f"{relative_fixture} must not depend on LushText crates: {sorted(forbidden_deps)}"
        )
    if len(family_deps) != 1:
        errors.append(
            f"{relative_fixture} must declare exactly one gtk-lush-* dependency; found {family_deps}"
        )
        return

    package_name, dependency_path = family_deps[0]
    if not dependency_path:
        errors.append(f"{relative_fixture} {package_name} dependency must use a path")
        return
    resolved = (fixture_root / dependency_path).resolve(strict=False)
    family_root = FAMILY_ROOT.resolve(strict=False)
    if not path_is_relative_to(resolved, family_root):
        errors.append(
            f"{relative_fixture} {package_name} dependency must point inside crates/gtk-lush/"
        )


def check_no_lushtext_dependencies(
    crate_root: Path,
    manifest: dict[str, Any],
    errors: list[str],
) -> None:
    forbidden: list[str] = []
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        dependencies = manifest.get(section, {})
        if not isinstance(dependencies, dict):
            continue
        for dependency, spec in dependencies.items():
            package_name = dependency
            if isinstance(spec, dict):
                package_name = spec.get("package", dependency)
            if package_name in LUSHTEXT_PACKAGES:
                forbidden.append(package_name)
    if forbidden:
        relative = crate_root.relative_to(REPO_ROOT)
        errors.append(f"{relative} must not depend on LushText crates: {sorted(forbidden)}")


def check_gitignore(errors: list[str]) -> None:
    gitignore = (REPO_ROOT / ".gitignore").read_text(encoding="utf-8")
    for pattern in REQUIRED_GITIGNORE_PATTERNS:
        if pattern not in gitignore:
            errors.append(f".gitignore missing adoption artifact pattern: {pattern}")


def check_disallowed_residue(errors: list[str]) -> None:
    for relative_path in DISALLOWED_RESIDUE_PATHS:
        if (REPO_ROOT / relative_path).exists():
            errors.append(
                f"temporary adoption residue must stay out of committed docs: {relative_path}"
            )


def load_toml(path: Path, errors: list[str]) -> dict[str, Any] | None:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        errors.append(f"missing TOML file: {path.relative_to(REPO_ROOT)}")
    except tomllib.TOMLDecodeError as error:
        errors.append(f"invalid TOML {path.relative_to(REPO_ROOT)}: {error}")
    return None


def path_is_relative_to(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
    except ValueError:
        return False
    return True


def report(errors: list[str]) -> int:
    if errors:
        print("GTK Lush adoption policy failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("GTK Lush adoption policy passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
