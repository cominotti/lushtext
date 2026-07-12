#!/usr/bin/env python3
"""Discover agent-facing Cargo, release, and test topology without fixed crate paths."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Iterable


SCRIPT_PATH = Path(__file__).resolve()
DEFAULT_REPO_ROOT = SCRIPT_PATH.parent.parent
POLICY_FILENAME = "skill-policy.toml"
DEFAULT_POLICY = DEFAULT_REPO_ROOT / ".agents" / "skills" / POLICY_FILENAME


def normalized(path: str | Path) -> str:
    """Return one repository-style relative path without accepting traversal."""

    value = str(path).replace("\\", "/").removeprefix("./")
    pure = PurePosixPath(value)
    if pure.is_absolute() or ".." in pure.parts:
        raise ValueError(f"unsafe repository-relative path: {path}")
    return pure.as_posix()


def is_under(path: str, root: str) -> bool:
    path_parts = PurePosixPath(path).parts
    root_parts = PurePosixPath(root).parts
    return len(path_parts) >= len(root_parts) and path_parts[: len(root_parts)] == root_parts


def load_policy(path: Path = DEFAULT_POLICY) -> dict[str, object]:
    return tomllib.loads(path.read_text(encoding="utf-8"))


def load_cargo_metadata(repo_root: Path, metadata_json: Path | None = None) -> dict[str, object]:
    if metadata_json is not None:
        return json.loads(metadata_json.read_text(encoding="utf-8"))
    override = os.environ.get("LUSHTEXT_AGENT_METADATA_JSON")
    if override:
        return json.loads(Path(override).read_text(encoding="utf-8"))
    result = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version=1"],
        cwd=repo_root,
        check=True,
        text=True,
        capture_output=True,
    )
    return json.loads(result.stdout)


@dataclass(frozen=True)
class WorkspacePackage:
    name: str
    root: str
    manifest: str
    metadata: dict[str, object]
    targets: tuple[dict[str, object], ...]


class WorkspaceTopology:
    """Semantic workspace view derived from Cargo's versioned metadata JSON."""

    def __init__(
        self,
        metadata: dict[str, object],
        policy: dict[str, object],
        workspace_root: Path,
    ) -> None:
        workspace_root = workspace_root.resolve()
        reported_root = os.path.realpath(str(metadata["workspace_root"]))
        if reported_root != str(workspace_root):
            raise ValueError(
                f"Cargo metadata workspace root {reported_root!r} does not match {workspace_root}"
            )
        member_ids = set(metadata["workspace_members"])
        packages: list[WorkspacePackage] = []
        for package in metadata["packages"]:
            if package["id"] not in member_ids:
                continue
            manifest_real = os.path.realpath(str(package["manifest_path"]))
            if os.path.commonpath((str(workspace_root), manifest_real)) != str(workspace_root):
                raise ValueError(f"Cargo manifest escapes workspace root: {manifest_real}")
            manifest = normalized(os.path.relpath(manifest_real, workspace_root))
            root = PurePosixPath(manifest).parent.as_posix()
            packages.append(
                WorkspacePackage(
                    name=package["name"],
                    root="" if root == "." else root,
                    manifest=manifest,
                    metadata=package.get("metadata") or {},
                    targets=tuple(package.get("targets", [])),
                )
            )
        self.workspace_root = workspace_root
        self.packages = tuple(sorted(packages, key=lambda package: (-len(package.root), package.name)))
        self.performance_policy = policy["performance"]

    def owner(self, path: str) -> WorkspacePackage | None:
        for package in self.packages:
            if not package.root or is_under(path, package.root):
                return package
        return None

    @staticmethod
    def package_relative(path: str, package: WorkspacePackage) -> str:
        if not package.root:
            return path
        return PurePosixPath(path).relative_to(package.root).as_posix()

    def role_roots(self, package: WorkspacePackage, key: str) -> tuple[str, ...]:
        namespace = self.performance_policy["package_metadata_namespace"]
        namespace_data = package.metadata.get(namespace, {})
        if not isinstance(namespace_data, dict):
            return ()
        roots = namespace_data.get(key, [])
        if not isinstance(roots, list) or any(not isinstance(root, str) for root in roots):
            return ()
        return tuple(normalized(root) for root in roots)

    def performance_path(self, path: str) -> bool:
        if path == "Cargo.lock":
            return any(
                self.role_roots(package, self.performance_policy["package_metadata_key"])
                for package in self.packages
            )
        owner = self.owner(path)
        if owner is not None:
            if path == owner.manifest:
                return bool(
                    self.role_roots(owner, self.performance_policy["package_metadata_key"])
                )
            relative = self.package_relative(path, owner)
            roots = self.role_roots(owner, self.performance_policy["package_metadata_key"])
            if any(is_under(relative, root) for root in roots):
                return True
        return any(
            path == suffix or f"/{suffix}/" in f"/{path}/" or path.endswith(f"/{suffix}")
            for suffix in self.performance_policy["fallback_suffixes"]
        )

    def release_categories(self, path: str) -> set[str]:
        categories: set[str] = set()
        owner = self.owner(path)
        if owner is not None:
            relative = self.package_relative(path, owner)
            for key, category in (
                ("release-ui-roots", "ui"),
                ("release-service-roots", "services"),
                ("release-model-roots", "model"),
            ):
                if any(is_under(relative, root) for root in self.role_roots(owner, key)):
                    categories.add(category)
        pure = PurePosixPath(path)
        name = pure.name
        if (
            name.endswith((".metainfo.xml", ".metainfo.xml.in", ".desktop", ".desktop.in"))
            or name.endswith((".Flatpak.json", ".flatpakref", ".flatpakrepo"))
            or name.endswith((".gschema.xml", ".gresource.xml"))
            or name in {"meson.build", "meson_options.txt"}
            or "icons" in pure.parts
            or pure.parts[:1] == ("po",)
        ):
            categories.add("packaging")
        if name.endswith((".blp", ".ui", ".css")):
            categories.add("ui")
        if (
            is_under(path, ".github/workflows")
            or is_under(path, "scripts")
            or name == "Makefile"
        ):
            categories.add("release-automation")
        if name == "Cargo.lock" or name == "Cargo.toml":
            categories.add("dependencies")
        return categories

    def testing_surfaces(self) -> list[dict[str, object]]:
        surfaces: list[dict[str, object]] = []
        for package in sorted(self.packages, key=lambda package: package.name):
            for target in package.targets:
                kinds = target.get("kind", [])
                if not target.get("test", False) and not any(
                    kind in {"test", "bench"} for kind in kinds
                ):
                    continue
                src_real = os.path.realpath(str(target["src_path"]))
                if os.path.commonpath((str(self.workspace_root), src_real)) != str(
                    self.workspace_root
                ):
                    raise ValueError(f"Cargo target source escapes workspace root: {src_real}")
                surfaces.append(
                    {
                        "package": package.name,
                        "manifest": package.manifest,
                        "target": target["name"],
                        "kind": kinds,
                        "src_path": normalized(os.path.relpath(src_real, self.workspace_root)),
                        "required_features": target.get("required-features", []),
                    }
                )
        return surfaces

    def validation_errors(self) -> list[str]:
        """Reject stale package-local role roots before they can hide review scope."""

        errors: list[str] = []
        namespace = self.performance_policy["package_metadata_namespace"]
        registered_performance = 0
        for package in self.packages:
            namespace_data = package.metadata.get(namespace, {})
            if not isinstance(namespace_data, dict):
                errors.append(f"{package.name}: {namespace} metadata must be a table")
                continue
            for key, roots in namespace_data.items():
                if not key.endswith("-roots"):
                    errors.append(f"{package.name}: unsupported {namespace} metadata key: {key}")
                    continue
                if not isinstance(roots, list) or any(not isinstance(root, str) for root in roots):
                    errors.append(f"{package.name}: {key} must contain relative path strings")
                    continue
                if key == self.performance_policy["package_metadata_key"] and roots:
                    registered_performance += 1
                for root in roots:
                    try:
                        relative = normalized(root)
                    except ValueError as error:
                        errors.append(f"{package.name}: {key}: {error}")
                        continue
                    candidate = os.path.realpath(
                        os.path.join(str(self.workspace_root), package.root, relative)
                    )
                    if os.path.commonpath((str(self.workspace_root), candidate)) != str(
                        self.workspace_root
                    ):
                        errors.append(f"{package.name}: {key} root escapes workspace: {relative}")
                    elif not os.path.exists(candidate):
                        errors.append(f"{package.name}: {key} root does not exist: {relative}")
        if registered_performance == 0:
            errors.append("no Cargo package registers performance roots")
        return sorted(errors)


def input_paths(arguments: list[str]) -> list[str]:
    values = arguments if arguments else [line for line in sys.stdin.read().splitlines() if line]
    return sorted({normalized(value) for value in values})


def discover_metainfo(repo_root: Path) -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", "*metainfo*.xml*"],
        cwd=repo_root,
        check=True,
        text=True,
        capture_output=True,
    )
    return sorted(line for line in result.stdout.splitlines() if line)


def discover_release_workflows(repo_root: Path, policy: dict[str, object]) -> list[dict[str, object]]:
    """Resolve required release responsibilities from semantic workflow markers."""

    release_policy = policy["release"]
    marker = re.escape(release_policy["workflow_role_marker"])
    role_pattern = re.compile(rf"^\s*#\s*{marker}:\s*([a-z0-9-]+)\s*$", re.MULTILINE)
    def has_v_tag_trigger(text: str) -> bool:
        lines = text.splitlines()
        for index, line in enumerate(lines):
            stripped = line.lstrip()
            if not stripped.startswith("tags:"):
                continue
            indent = len(line) - len(stripped)
            if "v*" in stripped.partition(":")[2]:
                return True
            for nested in lines[index + 1 :]:
                if not nested.strip():
                    continue
                nested_indent = len(nested) - len(nested.lstrip())
                if nested_indent <= indent:
                    break
                if "v*" in nested:
                    return True
        return False

    def workflow_name(text: str) -> str | None:
        for line in text.splitlines():
            if line.startswith("name:"):
                return line.partition(":")[2].strip().strip("'\"")
        return None
    workflows: list[dict[str, object]] = []
    workflow_root = repo_root / ".github" / "workflows"
    for path in sorted((*workflow_root.glob("*.yml"), *workflow_root.glob("*.yaml"))):
        text = path.read_text(encoding="utf-8")
        role_match = role_pattern.search(text)
        if role_match is None:
            continue
        name = workflow_name(text)
        if not name:
            raise ValueError(f"release workflow has no top-level name: {path}")
        workflows.append(
            {
                "role": role_match.group(1),
                "name": name,
                "path": path.relative_to(repo_root).as_posix(),
                "tag_triggered": has_v_tag_trigger(text),
            }
        )
    by_role: dict[str, list[dict[str, object]]] = {}
    for workflow in workflows:
        by_role.setdefault(workflow["role"], []).append(workflow)
    for role in release_policy["required_workflow_roles"]:
        matches = by_role.get(role, [])
        if len(matches) != 1:
            raise ValueError(
                f"required release workflow role {role!r} must have exactly one owner; found {len(matches)}"
            )
        if not matches[0]["tag_triggered"]:
            raise ValueError(f"required release workflow role {role!r} must trigger on v* tags")
    return sorted(workflows, key=lambda workflow: workflow["role"])


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=DEFAULT_REPO_ROOT)
    parser.add_argument("--metadata-json", type=Path)
    parser.add_argument("--policy", type=Path)
    subparsers = parser.add_subparsers(dest="command", required=True)
    for command in ("performance-scope", "release-hints"):
        child = subparsers.add_parser(command)
        child.add_argument("paths", nargs="*")
    subparsers.add_parser("testing-surfaces")
    subparsers.add_parser("metainfo-files")
    subparsers.add_parser("release-workflows")
    subparsers.add_parser("validate")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    repo_root = args.repo_root.resolve()
    policy = load_policy(
        args.policy
        if args.policy is not None
        else repo_root / ".agents" / "skills" / POLICY_FILENAME
    )
    if args.command == "metainfo-files":
        print("\n".join(discover_metainfo(repo_root)))
        return 0
    if args.command == "release-workflows":
        json.dump(discover_release_workflows(repo_root, policy), sys.stdout, indent=2, sort_keys=True)
        print()
        return 0
    topology = WorkspaceTopology(
        load_cargo_metadata(repo_root, args.metadata_json),
        policy,
        repo_root,
    )
    if args.command == "validate":
        errors = topology.validation_errors()
        try:
            discover_release_workflows(repo_root, policy)
        except ValueError as error:
            errors.append(str(error))
        if errors:
            print("Agent topology validation failed:", file=sys.stderr)
            for error in sorted(errors):
                print(f"- {error}", file=sys.stderr)
            return 1
        print(
            f"Validated {len(topology.packages)} Cargo packages and required release workflow roles."
        )
    elif args.command == "performance-scope":
        print("\n".join(path for path in input_paths(args.paths) if topology.performance_path(path)))
    elif args.command == "release-hints":
        for path in input_paths(args.paths):
            for category in sorted(topology.release_categories(path)):
                print(f"{category}\t{path}")
    elif args.command == "testing-surfaces":
        json.dump(topology.testing_surfaces(), sys.stdout, indent=2, sort_keys=True)
        print()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
