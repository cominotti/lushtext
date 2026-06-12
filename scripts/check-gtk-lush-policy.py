#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Validate the GTK Lush family rails before any extracted API can drift toward
# framework shape or accidental publication.

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
FAMILY_ROOT = REPO_ROOT / "crates" / "gtk-lush"
CARGO_PROOF_TOOL_MEMBER = "crates/cargo-gtk-proof"
EXPECTED_MEMBERS = {
    "proof-harness": "gtk-lush-proof-harness",
    "proof-spine": "gtk-lush-proof-spine",
    "signals": "gtk-lush-signals",
    "settle": "gtk-lush-settle",
    "tasks": "gtk-lush-tasks",
    "viewport": "gtk-lush-viewport",
    "widgets": "gtk-lush-widgets",
}
LUSHTEXT_CRATES = {
    "lushtext",
    "lushtext-build-support",
    "lushtext-core",
}
REQUIRED_FILES = (
    "Cargo.toml",
    "README.md",
    "CHANGELOG.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
    "src/lib.rs",
)
SPDX_HEADER = "// SPDX-License-Identifier: MIT OR Apache-2.0"
PUBLIC_API_PATTERN = re.compile(
    r"(?m)^\s*pub\s+(?:use|extern\s+crate|mod|struct|enum|trait|fn|type|const|static|macro)\b"
)
MACRO_EXPORT_PATTERN = re.compile(r"(?m)^\s*#\s*\[\s*macro_export\s*\]")


def main() -> int:
    errors: list[str] = []
    root_manifest = load_toml(REPO_ROOT / "Cargo.toml", errors)
    if not isinstance(root_manifest, dict):
        return report(errors)

    workspace = root_manifest.get("workspace", {})
    workspace_members = set(workspace.get("members", []))
    workspace_deps = root_manifest.get("workspace", {}).get("dependencies", {})
    check_workspace_tools(workspace_members, errors)

    for member, package_name in family_members(errors).items():
        crate_root = FAMILY_ROOT / member
        relative_member = f"crates/gtk-lush/{member}"
        if not crate_root.is_dir():
            errors.append(f"missing GTK Lush crate directory: {relative_member}")
            continue

        if relative_member not in workspace_members:
            errors.append(f"{relative_member} is missing from [workspace].members")

        workspace_dep = workspace_deps.get(package_name)
        if not isinstance(workspace_dep, dict) or workspace_dep.get("path") != relative_member:
            errors.append(
                f"{package_name} must be listed in [workspace.dependencies] with path {relative_member}"
            )

        check_crate(crate_root, member, package_name, errors)

    return report(errors)


def check_workspace_tools(workspace_members: set[str], errors: list[str]) -> None:
    if CARGO_PROOF_TOOL_MEMBER not in workspace_members:
        errors.append(f"{CARGO_PROOF_TOOL_MEMBER} is missing from [workspace].members")

    misplaced_tool = FAMILY_ROOT / "cargo-gtk-proof"
    if misplaced_tool.exists():
        errors.append(
            "cargo-gtk-proof is a workspace tool and must not live under crates/gtk-lush/"
        )

    manifest = load_toml(REPO_ROOT / CARGO_PROOF_TOOL_MEMBER / "Cargo.toml", errors)
    if not isinstance(manifest, dict):
        return
    package = manifest.get("package", {})
    if package.get("name") != "cargo-gtk-proof":
        errors.append(f"{CARGO_PROOF_TOOL_MEMBER}/Cargo.toml package.name must be 'cargo-gtk-proof'")


def family_members(errors: list[str]) -> dict[str, str]:
    discovered: dict[str, str] = {}
    if FAMILY_ROOT.is_dir():
        for child in sorted(FAMILY_ROOT.iterdir()):
            if child.is_dir() and (child / "Cargo.toml").is_file():
                discovered[child.name] = f"gtk-lush-{child.name}"

    for member, package_name in EXPECTED_MEMBERS.items():
        if member not in discovered:
            errors.append(f"missing GTK Lush crate directory: crates/gtk-lush/{member}")
            discovered[member] = package_name

    return discovered


def check_crate(crate_root: Path, member: str, package_name: str, errors: list[str]) -> None:
    for required in REQUIRED_FILES:
        if not (crate_root / required).is_file():
            errors.append(f"{crate_root.relative_to(REPO_ROOT)} missing {required}")

    manifest = load_toml(crate_root / "Cargo.toml", errors)
    if not isinstance(manifest, dict):
        return

    package = manifest.get("package", {})
    if package.get("name") != package_name:
        errors.append(f"{crate_root / 'Cargo.toml'} package.name must be {package_name!r}")
    if package.get("version") != "0.0.0":
        errors.append(f"{package_name} must remain version 0.0.0 before Phase 5b publication")
    if package.get("license") != "MIT OR Apache-2.0":
        errors.append(f"{package_name} license must be exactly 'MIT OR Apache-2.0'")
    rust_version = package.get("rust-version")
    if not (isinstance(rust_version, dict) and rust_version.get("workspace") is True):
        errors.append(f"{package_name} must inherit rust-version from the workspace")
    lints = manifest.get("lints")
    if not (isinstance(lints, dict) and lints.get("workspace") is True):
        errors.append(f"{package_name} must opt into the workspace lint table")

    check_dependency_direction(crate_root, package_name, manifest, errors)
    check_spdx_headers(crate_root, errors)
    readme_text = check_readme(crate_root, member, errors)
    check_lib_contract(crate_root, package_name, readme_text, errors)
    check_examples(crate_root, errors)


def check_dependency_direction(
    crate_root: Path,
    package_name: str,
    manifest: dict[str, Any],
    errors: list[str],
) -> None:
    dependency_sections = (
        "dependencies",
        "build-dependencies",
        "dev-dependencies",
        "target",
    )
    for section in dependency_sections:
        values = manifest.get(section, {})
        if section == "target":
            for target_values in values.values():
                if isinstance(target_values, dict):
                    for target_section in ("dependencies", "build-dependencies", "dev-dependencies"):
                        target_deps = target_values.get(target_section, {})
                        if isinstance(target_deps, dict):
                            check_dependency_map(crate_root, package_name, target_deps, errors)
            continue
        if isinstance(values, dict):
            check_dependency_map(crate_root, package_name, values, errors)


def check_dependency_map(
    crate_root: Path,
    package_name: str,
    values: dict[str, Any],
    errors: list[str],
) -> None:
    for dependency, spec in values.items():
        check_dependency_name(crate_root, package_name, dependency, errors)
        if isinstance(spec, dict):
            package_alias = spec.get("package")
            if isinstance(package_alias, str):
                check_dependency_name(crate_root, package_name, package_alias, errors)

            dependency_path = spec.get("path")
            if isinstance(dependency_path, str):
                check_dependency_path(crate_root, package_name, dependency_path, errors)


def check_dependency_name(
    crate_root: Path,
    package_name: str,
    dependency: str,
    errors: list[str],
) -> None:
    if dependency in LUSHTEXT_CRATES:
        errors.append(
            f"{package_name} must not depend on LushText crate {dependency!r}: "
            f"{crate_root.relative_to(REPO_ROOT)}/Cargo.toml"
        )
    if dependency.startswith("gtk-lush-"):
        errors.append(f"{package_name} must remain a leaf crate and not depend on {dependency!r}")


def check_dependency_path(
    crate_root: Path,
    package_name: str,
    dependency_path: str,
    errors: list[str],
) -> None:
    resolved_path = (crate_root / dependency_path).resolve(strict=False)
    crate_path = crate_root.resolve(strict=False)
    family_path = FAMILY_ROOT.resolve(strict=False)

    if path_is_relative_to(resolved_path, family_path) and resolved_path != crate_path:
        errors.append(
            f"{package_name} must remain a leaf crate and not path-depend on "
            f"{resolved_path.relative_to(REPO_ROOT)}"
        )

    for lushtext_crate in LUSHTEXT_CRATES:
        lushtext_path = (REPO_ROOT / "crates" / lushtext_crate).resolve(strict=False)
        if path_is_relative_to(resolved_path, lushtext_path):
            errors.append(
                f"{package_name} must not path-depend on LushText crate "
                f"{resolved_path.relative_to(REPO_ROOT)}"
            )


def path_is_relative_to(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
    except ValueError:
        return False
    return True


def check_spdx_headers(crate_root: Path, errors: list[str]) -> None:
    for path in sorted((crate_root / "src").glob("**/*.rs")) + sorted(
        (crate_root / "examples").glob("**/*.rs")
    ):
        first_line = path.read_text(encoding="utf-8").splitlines()[0]
        if first_line != SPDX_HEADER:
            errors.append(f"{path.relative_to(REPO_ROOT)} missing SPDX header {SPDX_HEADER!r}")


def check_lib_contract(
    crate_root: Path,
    package_name: str,
    readme_text: str,
    errors: list[str],
) -> None:
    lib_path = crate_root / "src" / "lib.rs"
    if not lib_path.is_file():
        return

    text = lib_path.read_text(encoding="utf-8")
    if "//! " not in text:
        errors.append(f"{package_name} src/lib.rs must have crate-level docs")
    if "#![forbid(unsafe_code)]" not in text:
        errors.append(f"{package_name} src/lib.rs must forbid unsafe code")
    if "#![deny(missing_docs)]" not in text:
        errors.append(f"{package_name} src/lib.rs must deny missing docs")
    exposes_public_api = bool(PUBLIC_API_PATTERN.search(text) or MACRO_EXPORT_PATTERN.search(text))
    if exposes_public_api and not is_functional_in_tree_readme(readme_text):
        errors.append(f"{package_name} 0.0.0 placeholder must not expose public API items")


def check_readme(crate_root: Path, member: str, errors: list[str]) -> str:
    readme_path = crate_root / "README.md"
    if not readme_path.is_file():
        return ""

    text = readme_path.read_text(encoding="utf-8")
    expected_name = f"gtk-lush-{member}"
    base_required_phrases = (
        expected_name,
        "0.0.0",
        "docs/next/gtk-lush.md",
    )
    for phrase in base_required_phrases:
        if phrase not in text:
            errors.append(f"{readme_path.relative_to(REPO_ROOT)} must mention {phrase!r}")
    if is_functional_in_tree_readme(text):
        for phrase in ("Pre-Publication Status", "not a Phase 5b publication-ready"):
            if phrase not in text:
                errors.append(f"{readme_path.relative_to(REPO_ROOT)} must mention {phrase!r}")
    else:
        for phrase in ("no public API", "Placeholder"):
            if phrase not in text:
                errors.append(f"{readme_path.relative_to(REPO_ROOT)} must mention {phrase!r}")

    return text


def is_functional_in_tree_readme(text: str) -> bool:
    return "Pre-Publication Status" in text and "first functional in-tree" in text


def check_examples(crate_root: Path, errors: list[str]) -> None:
    examples_dir = crate_root / "examples"
    if not examples_dir.is_dir():
        errors.append(f"{crate_root.relative_to(REPO_ROOT)} missing examples directory")
        return

    if not any(examples_dir.glob("*.rs")):
        errors.append(f"{examples_dir.relative_to(REPO_ROOT)} must contain at least one Rust example")


def load_toml(path: Path, errors: list[str]) -> dict[str, Any] | None:
    try:
        with path.open("rb") as file:
            return tomllib.load(file)
    except FileNotFoundError:
        errors.append(f"missing TOML file: {path.relative_to(REPO_ROOT)}")
    except tomllib.TOMLDecodeError as exc:
        errors.append(f"invalid TOML in {path.relative_to(REPO_ROOT)}: {exc}")
    return None


def report(errors: list[str]) -> int:
    if errors:
        print("GTK Lush policy check failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1

    print("GTK Lush policy check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
