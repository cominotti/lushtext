#!/usr/bin/env python3
"""Validate maintained repository skills without third-party dependencies."""

from __future__ import annotations

import argparse
import json
import re
import sys
import tomllib
from pathlib import Path
from urllib.parse import unquote


REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SKILLS_ROOT = REPO_ROOT / ".agents" / "skills"
REGISTRY_FILENAME = "skill-policy.toml"
FRONTMATTER_KEYS = {"name", "description"}
INTERFACE_KEYS = {
    "display_name",
    "short_description",
    "default_prompt",
    "icon_small",
    "icon_large",
    "brand_color",
}
REQUIRED_INTERFACE_KEYS = {"display_name", "short_description", "default_prompt"}
TOP_LEVEL_OPENAI_KEYS = {"interface", "dependencies", "policy"}
FENCE_RE = re.compile(r"```.*?```|~~~.*?~~~", re.DOTALL)
FRONTMATTER_RE = re.compile(r"\A---\n(.*?)\n---(?:\n|\Z)", re.DOTALL)
TOC_RE = re.compile(r"^## (?:Table of Contents|Contents)\s*$", re.MULTILINE)
KEY_VALUE_RE = re.compile(r"^([A-Za-z_][A-Za-z0-9_-]*):(.*)$")
HEADING_RE = re.compile(r"^#{1,6}[ \t]+(.+?)[ \t]*#*[ \t]*$", re.MULTILINE)
EXPLICIT_ANCHOR_RE = re.compile(r"<(?:a\s+(?:id|name)|span\s+id)=[\"']([^\"']+)[\"']", re.I)
EXTERNAL_SCHEME_RE = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")


class SubsetYamlError(ValueError):
    """Raised when metadata leaves the deterministic supported YAML subset."""


def load_policy_registry(skills_root: Path, errors: list[str]) -> dict[str, object] | None:
    """Load the single source of truth for maintained-skill routing policy."""

    path = skills_root / REGISTRY_FILENAME
    if not path.is_file():
        errors.append(f"{display_path(path)}: required policy registry is missing")
        return None
    try:
        registry = tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        errors.append(f"{display_path(path)}: invalid TOML: {error}")
        return None
    if set(registry) != {
        "schema_version",
        "excluded_prefixes",
        "implicit_invocation",
        "performance",
        "filesystem_contract",
        "release",
    }:
        errors.append(f"{display_path(path)}: unsupported or missing top-level registry keys")
        return None
    if registry["schema_version"] != 1:
        errors.append(f"{display_path(path)}: schema_version must be 1")
    prefixes = registry["excluded_prefixes"]
    if (
        not isinstance(prefixes, list)
        or not prefixes
        or any(not isinstance(prefix, str) or not prefix for prefix in prefixes)
    ):
        errors.append(f"{display_path(path)}: excluded_prefixes must be non-empty strings")
    implicit = registry["implicit_invocation"]
    if not isinstance(implicit, dict) or any(
        not isinstance(name, str) or not isinstance(value, bool)
        for name, value in implicit.items()
    ):
        errors.append(f"{display_path(path)}: implicit_invocation must map skill names to booleans")
    performance = registry["performance"]
    expected_performance_keys = {
        "umbrella",
        "leaves",
        "package_metadata_namespace",
        "package_metadata_key",
        "fallback_suffixes",
    }
    if not isinstance(performance, dict) or set(performance) != expected_performance_keys:
        errors.append(f"{display_path(path)}: performance policy has unsupported shape")
    else:
        if not isinstance(performance["umbrella"], str) or not performance["umbrella"]:
            errors.append(f"{display_path(path)}: performance.umbrella must be a skill name")
        for key in ("leaves", "fallback_suffixes"):
            values = performance[key]
            if not isinstance(values, list) or any(
                not isinstance(value, str) or not value for value in values
            ):
                errors.append(f"{display_path(path)}: performance.{key} must contain strings")
        for key in ("package_metadata_namespace", "package_metadata_key"):
            if not isinstance(performance[key], str) or not performance[key]:
                errors.append(f"{display_path(path)}: performance.{key} must be a string")
    filesystem = registry["filesystem_contract"]
    if not isinstance(filesystem, dict) or set(filesystem) != {"paths"}:
        errors.append(f"{display_path(path)}: filesystem_contract must contain only paths")
    else:
        paths = filesystem["paths"]
        if not isinstance(paths, list) or any(
            not isinstance(value, str)
            or not value
            or Path(value).is_absolute()
            or ".." in Path(value).parts
            for value in paths
        ):
            errors.append(
                f"{display_path(path)}: filesystem_contract.paths must be safe relative paths"
            )
    release = registry["release"]
    if not isinstance(release, dict) or set(release) != {
        "workflow_role_marker",
        "required_workflow_roles",
    }:
        errors.append(f"{display_path(path)}: release policy has unsupported shape")
    else:
        marker = release["workflow_role_marker"]
        roles = release["required_workflow_roles"]
        if not isinstance(marker, str) or not marker:
            errors.append(f"{display_path(path)}: release.workflow_role_marker must be a string")
        if (
            not isinstance(roles, list)
            or not roles
            or any(not isinstance(role, str) or not role for role in roles)
            or len(roles) != len(set(roles))
        ):
            errors.append(
                f"{display_path(path)}: release.required_workflow_roles must be unique strings"
            )
    return registry


def display_path(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def parse_scalar(value: str, line_number: int) -> object:
    value = value.strip()
    if not value:
        raise SubsetYamlError(f"line {line_number}: missing scalar value")
    if value.startswith('"'):
        try:
            parsed = json.loads(value)
        except json.JSONDecodeError as error:
            raise SubsetYamlError(f"line {line_number}: malformed double-quoted scalar: {error.msg}") from error
        if not isinstance(parsed, str):
            raise SubsetYamlError(f"line {line_number}: quoted metadata values must be strings")
        return parsed
    if value.startswith("'"):
        if len(value) < 2 or not value.endswith("'"):
            raise SubsetYamlError(f"line {line_number}: malformed single-quoted scalar")
        return value[1:-1].replace("''", "'")
    if value in {"true", "false"}:
        return value == "true"
    if value in {"null", "~"}:
        return None
    if value[0] in "[{&*!|>@`" or value.endswith(":"):
        raise SubsetYamlError(f"line {line_number}: unsupported YAML syntax")
    return value


def yaml_logical_lines(text: str) -> list[tuple[int, int, str]]:
    physical = text.splitlines()
    logical: list[tuple[int, int, str]] = []
    index = 0
    while index < len(physical):
        raw = physical[index]
        line_number = index + 1
        index += 1
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        if "\t" in raw[: len(raw) - len(raw.lstrip())]:
            raise SubsetYamlError(f"line {line_number}: tabs are not allowed for indentation")
        indent = len(raw) - len(raw.lstrip(" "))
        if indent % 2:
            raise SubsetYamlError(f"line {line_number}: indentation must use two-space steps")
        content = raw[indent:].rstrip()

        match = KEY_VALUE_RE.match(content.removeprefix("- "))
        if match and match.group(2).lstrip().startswith('"'):
            prefix = content[: content.find(":") + 1]
            scalar = match.group(2).strip()
            while True:
                try:
                    json.loads(scalar)
                    break
                except json.JSONDecodeError as error:
                    if "Unterminated string" not in error.msg or index >= len(physical):
                        raise SubsetYamlError(
                            f"line {line_number}: malformed double-quoted scalar: {error.msg}"
                        ) from error
                    scalar += " " + physical[index].strip()
                    index += 1
            content = prefix + " " + scalar
        logical.append((line_number, indent, content))
    return logical


def parse_yaml_subset(text: str) -> object:
    """Parse mappings/lists/scalars used by SKILL.md and agents/openai.yaml."""

    lines = yaml_logical_lines(text)
    if not lines:
        raise SubsetYamlError("document is empty")

    def parse_block(index: int, indent: int) -> tuple[object, int]:
        if index >= len(lines) or lines[index][1] != indent:
            raise SubsetYamlError("invalid nested indentation")
        if lines[index][2].startswith("- "):
            return parse_list(index, indent)
        return parse_mapping(index, indent)

    def parse_mapping(index: int, indent: int) -> tuple[dict[str, object], int]:
        result: dict[str, object] = {}
        while index < len(lines):
            line_number, current_indent, content = lines[index]
            if current_indent < indent:
                break
            if current_indent > indent:
                raise SubsetYamlError(f"line {line_number}: unexpected indentation")
            if content.startswith("- "):
                break
            match = KEY_VALUE_RE.match(content)
            if not match:
                raise SubsetYamlError(f"line {line_number}: expected key: value")
            key, raw_value = match.groups()
            if key in result:
                raise SubsetYamlError(f"line {line_number}: duplicate key {key!r}")
            index += 1
            if raw_value.strip():
                result[key] = parse_scalar(raw_value, line_number)
            else:
                if index >= len(lines) or lines[index][1] != indent + 2:
                    raise SubsetYamlError(f"line {line_number}: key {key!r} needs a nested value")
                result[key], index = parse_block(index, indent + 2)
        return result, index

    def parse_list(index: int, indent: int) -> tuple[list[object], int]:
        result: list[object] = []
        while index < len(lines):
            line_number, current_indent, content = lines[index]
            if current_indent < indent:
                break
            if current_indent != indent or not content.startswith("- "):
                raise SubsetYamlError(f"line {line_number}: malformed list indentation")
            item = content[2:].strip()
            if not item:
                raise SubsetYamlError(f"line {line_number}: empty list items are unsupported")
            match = KEY_VALUE_RE.match(item)
            index += 1
            if not match:
                result.append(parse_scalar(item, line_number))
                continue
            key, raw_value = match.groups()
            entry: dict[str, object] = {}
            if raw_value.strip():
                entry[key] = parse_scalar(raw_value, line_number)
            else:
                if index >= len(lines) or lines[index][1] != indent + 2:
                    raise SubsetYamlError(f"line {line_number}: key {key!r} needs a nested value")
                entry[key], index = parse_block(index, indent + 2)
            if index < len(lines) and lines[index][1] == indent + 2 and not lines[index][2].startswith("- "):
                continuation, index = parse_mapping(index, indent + 2)
                duplicate = set(entry) & set(continuation)
                if duplicate:
                    raise SubsetYamlError(
                        f"line {line_number}: duplicate key {sorted(duplicate)[0]!r}"
                    )
                entry.update(continuation)
            result.append(entry)
        return result, index

    parsed, final_index = parse_block(0, lines[0][1])
    if lines[0][1] != 0 or final_index != len(lines):
        line_number = lines[final_index][0] if final_index < len(lines) else lines[0][0]
        raise SubsetYamlError(f"line {line_number}: document must have one root mapping")
    return parsed


def load_yaml(path: Path, text: str, errors: list[str]) -> object | None:
    try:
        return parse_yaml_subset(text)
    except SubsetYamlError as error:
        errors.append(f"{display_path(path)}: invalid YAML: {error}")
        return None


def validate_frontmatter(skill_dir: Path, errors: list[str]) -> str | None:
    path = skill_dir / "SKILL.md"
    if not path.is_file():
        errors.append(f"{display_path(path)}: required file is missing")
        return None
    text = path.read_text(encoding="utf-8")
    if len(text.splitlines()) > 500:
        errors.append(f"{display_path(path)}: SKILL.md exceeds the 500-line ceiling")
    match = FRONTMATTER_RE.match(text)
    if not match:
        errors.append(f"{display_path(path)}: missing or malformed YAML frontmatter")
        return None
    if not text[match.end() :].strip():
        errors.append(f"{display_path(path)}: instruction body must not be empty")
    data = load_yaml(path, match.group(1), errors)
    if not isinstance(data, dict):
        errors.append(f"{display_path(path)}: frontmatter must be a mapping")
        return None
    unexpected = sorted(set(data) - FRONTMATTER_KEYS)
    if unexpected:
        errors.append(f"{display_path(path)}: unsupported frontmatter keys: {', '.join(unexpected)}")
    name = data.get("name")
    description = data.get("description")
    if not isinstance(name, str) or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name):
        errors.append(f"{display_path(path)}: name must be non-empty hyphen-case")
        return None
    if len(name) > 64:
        errors.append(f"{display_path(path)}: name exceeds 64 characters")
    if name != skill_dir.name:
        errors.append(f"{display_path(path)}: frontmatter name {name!r} does not match directory")
    if not isinstance(description, str) or not description.strip():
        errors.append(f"{display_path(path)}: description must be a non-empty string")
    elif len(description) > 1024 or "<" in description or ">" in description:
        errors.append(
            f"{display_path(path)}: description must be at most 1024 characters and contain no angle brackets"
        )
    return name


def require_string(mapping: dict[object, object], key: str, path: Path, errors: list[str]) -> str | None:
    value = mapping.get(key)
    if not isinstance(value, str) or not value.strip():
        errors.append(f"{display_path(path)}: {key} must be a non-empty string")
        return None
    return value


def validate_openai_yaml(
    skill_dir: Path,
    skill_name: str | None,
    expected_implicit: bool | None,
    errors: list[str],
) -> None:
    path = skill_dir / "agents" / "openai.yaml"
    if not path.is_file():
        errors.append(f"{display_path(path)}: required file is missing")
        return
    data = load_yaml(path, path.read_text(encoding="utf-8"), errors)
    if not isinstance(data, dict):
        errors.append(f"{display_path(path)}: document must be a mapping")
        return
    unexpected_top = sorted(set(data) - TOP_LEVEL_OPENAI_KEYS)
    if unexpected_top:
        errors.append(f"{display_path(path)}: unsupported top-level keys: {', '.join(unexpected_top)}")
    interface = data.get("interface")
    if not isinstance(interface, dict):
        errors.append(f"{display_path(path)}: interface must be a mapping")
        return
    missing = sorted(REQUIRED_INTERFACE_KEYS - set(interface))
    unexpected = sorted(set(interface) - INTERFACE_KEYS)
    if missing:
        errors.append(f"{display_path(path)}: missing interface keys: {', '.join(missing)}")
    if unexpected:
        errors.append(f"{display_path(path)}: unsupported interface keys: {', '.join(unexpected)}")
    for key in sorted(set(interface) & INTERFACE_KEYS):
        require_string(interface, key, path, errors)
    short = interface.get("short_description")
    if isinstance(short, str) and not 25 <= len(short) <= 64:
        errors.append(f"{display_path(path)}: short_description must be 25-64 characters")
    prompt = interface.get("default_prompt")
    if skill_name and isinstance(prompt, str) and f"${skill_name}" not in prompt:
        errors.append(f"{display_path(path)}: default_prompt must mention ${skill_name}")
    color = interface.get("brand_color")
    if color is not None and (not isinstance(color, str) or not re.fullmatch(r"#[0-9A-Fa-f]{6}", color)):
        errors.append(f"{display_path(path)}: brand_color must be a six-digit hex color")
    assets_root = (skill_dir / "assets").resolve()
    for key in ("icon_small", "icon_large"):
        icon = interface.get(key)
        if not isinstance(icon, str):
            continue
        icon_path = Path(icon)
        resolved = (skill_dir / icon_path).resolve()
        if icon_path.is_absolute() or not resolved.is_relative_to(assets_root) or not resolved.is_file():
            errors.append(
                f"{display_path(path)}: {key} must be an existing relative file contained in assets/: {icon}"
            )
    policy = data.get("policy")
    if policy is not None:
        if not isinstance(policy, dict) or set(policy) - {"allow_implicit_invocation"}:
            errors.append(f"{display_path(path)}: policy has unsupported shape")
        elif "allow_implicit_invocation" in policy and not isinstance(policy["allow_implicit_invocation"], bool):
            errors.append(f"{display_path(path)}: allow_implicit_invocation must be boolean")
    implicit = policy.get("allow_implicit_invocation") if isinstance(policy, dict) else None
    if expected_implicit is not None and implicit is not expected_implicit:
        expected = str(expected_implicit).lower()
        errors.append(
            f"{display_path(path)}: {skill_name} must set allow_implicit_invocation: {expected}"
        )
    dependencies = data.get("dependencies")
    if dependencies is not None:
        if not isinstance(dependencies, dict) or set(dependencies) != {"tools"}:
            errors.append(f"{display_path(path)}: dependencies must contain only tools")
        elif not isinstance(dependencies["tools"], list):
            errors.append(f"{display_path(path)}: dependencies.tools must be a list")
        else:
            allowed = {"type", "value", "description", "transport", "url"}
            for index, tool in enumerate(dependencies["tools"]):
                if not isinstance(tool, dict) or set(tool) - allowed:
                    errors.append(f"{display_path(path)}: dependencies.tools[{index}] has unsupported shape")
                    continue
                for key in ("type", "value", "description"):
                    require_string(tool, key, path, errors)
                if tool.get("type") != "mcp":
                    errors.append(f"{display_path(path)}: dependencies.tools[{index}].type must be mcp")


REFERENCE_DEFINITION_RE = re.compile(
    r"^[ ]{0,3}\[([^\]]+)\]:[ \t]*(.+?)\s*$", re.MULTILINE
)
REFERENCE_USE_RE = re.compile(r"!?\[([^\]]*)\]\[([^\]]*)\]")


def strip_inline_code(text: str) -> str:
    """Blank CommonMark-style code spans so examples are not treated as links."""

    output = list(text)
    index = 0
    while index < len(text):
        if text[index] != "`":
            index += 1
            continue
        run_end = index
        while run_end < len(text) and text[run_end] == "`":
            run_end += 1
        marker = text[index:run_end]
        close = text.find(marker, run_end)
        if close < 0:
            index = run_end
            continue
        for position in range(index, close + len(marker)):
            output[position] = " "
        index = close + len(marker)
    return "".join(output)


def markdown_destination(raw: str) -> str:
    """Return a destination while discarding an optional CommonMark link title."""

    raw = raw.strip()
    if not raw:
        return ""
    if raw.startswith("<"):
        escaped = False
        for index, char in enumerate(raw[1:], start=1):
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == ">":
                return raw[1:index]
        return raw

    depth = 0
    escaped = False
    for index, char in enumerate(raw):
        if escaped:
            escaped = False
            continue
        if char == "\\":
            escaped = True
        elif char == "(":
            depth += 1
        elif char == ")" and depth:
            depth -= 1
        elif char.isspace() and depth == 0:
            return raw[:index]
    return raw


def markdown_links(text: str) -> list[str]:
    """Extract inline and reference link/image destinations deterministically."""

    links: list[str] = []
    index = 0
    while True:
        marker = text.find("](", index)
        if marker < 0:
            break
        cursor = marker + 2
        depth = 1
        end = cursor
        while end < len(text) and depth:
            char = text[end]
            if char == "\\":
                end += 2
                continue
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
            end += 1
        if depth == 0:
            destination = markdown_destination(text[cursor : end - 1])
            if destination:
                links.append(destination)
            index = end
        else:
            index = marker + 2

    definitions: dict[str, str] = {}
    for match in REFERENCE_DEFINITION_RE.finditer(text):
        label = " ".join(match.group(1).lower().split())
        destination = markdown_destination(match.group(2))
        if destination:
            definitions[label] = destination
            links.append(destination)
    for match in REFERENCE_USE_RE.finditer(text):
        label_text = match.group(2) or match.group(1)
        label = " ".join(label_text.lower().split())
        destination = definitions.get(label)
        if destination:
            links.append(destination)
    return links


def markdown_anchors(text: str) -> set[str]:
    anchors = set(EXPLICIT_ANCHOR_RE.findall(text))
    counts: dict[str, int] = {}
    for heading in HEADING_RE.findall(FENCE_RE.sub("", text)):
        heading = re.sub(r"<[^>]+>", "", heading.lower())
        heading = heading.replace("`", "").replace("*", "").replace("~", "")
        slug = "".join(char for char in heading if char.isalnum() or char in "-_" or char.isspace())
        slug = re.sub(r"\s", "-", slug)
        duplicate = counts.get(slug, 0)
        counts[slug] = duplicate + 1
        anchors.add(slug if duplicate == 0 else f"{slug}-{duplicate}")
    return anchors


def validate_markdown(skill_dir: Path, allowed_root: Path, errors: list[str]) -> None:
    repo_root = allowed_root.resolve()
    for path in sorted(skill_dir.rglob("*.md")):
        text = path.read_text(encoding="utf-8")
        if "references" in path.relative_to(skill_dir).parts and len(text.splitlines()) > 100:
            first_lines = "\n".join(text.splitlines()[:40])
            if not TOC_RE.search(first_lines):
                errors.append(
                    f"{display_path(path)}: references over 100 lines need a Contents section in the first 40 lines"
                )
        searchable = strip_inline_code(FENCE_RE.sub("", text))
        for raw_target in markdown_links(searchable):
            target = raw_target.strip()
            if not target or EXTERNAL_SCHEME_RE.match(target):
                continue
            file_part, separator, fragment = target.partition("#")
            file_part = unquote(file_part.split("?", 1)[0])
            target_path = path if not file_part else path.parent / file_part
            resolved = target_path.resolve()
            if Path(file_part).is_absolute() or not resolved.is_relative_to(repo_root):
                errors.append(f"{display_path(path)}: local link escapes the repository: {target!r}")
                continue
            if not resolved.is_file():
                errors.append(f"{display_path(path)}: broken local file/image link {target!r}")
                continue
            if separator and resolved.suffix.lower() == ".md":
                anchors = markdown_anchors(resolved.read_text(encoding="utf-8"))
                decoded_fragment = unquote(fragment)
                if decoded_fragment not in anchors:
                    errors.append(f"{display_path(path)}: broken Markdown anchor {target!r}")


def validate_skills(skills_root: Path) -> list[str]:
    errors: list[str] = []
    if not skills_root.is_dir():
        return [f"{display_path(skills_root)}: skills root is missing"]
    registry = load_policy_registry(skills_root, errors)
    if registry is None:
        return sorted(set(errors))
    prefixes = tuple(registry["excluded_prefixes"])
    skill_dirs = sorted(
        (path for path in skills_root.iterdir() if path.is_dir() and not path.name.startswith(prefixes)),
        key=lambda path: path.name,
    )
    if not skill_dirs:
        return [f"{display_path(skills_root)}: no maintained agent skills found"]
    skill_names = {path.name for path in skill_dirs}
    implicit = registry["implicit_invocation"]
    performance = registry["performance"]
    registered_names = {
        *implicit,
        performance["umbrella"],
        *performance["leaves"],
    }
    for missing in sorted(registered_names - skill_names):
        errors.append(
            f"{display_path(skills_root / REGISTRY_FILENAME)}: registered skill is missing: {missing}"
        )
    umbrella = performance["umbrella"]
    if implicit.get(umbrella) is not True:
        errors.append(
            f"{display_path(skills_root / REGISTRY_FILENAME)}: performance umbrella must allow implicit invocation"
        )
    for leaf in performance["leaves"]:
        if implicit.get(leaf) is not False:
            errors.append(
                f"{display_path(skills_root / REGISTRY_FILENAME)}: performance leaf must disable implicit invocation: {leaf}"
            )
    allowed_root = REPO_ROOT if skills_root.resolve() == DEFAULT_SKILLS_ROOT.resolve() else skills_root
    for skill_dir in skill_dirs:
        skill_name = validate_frontmatter(skill_dir, errors)
        validate_openai_yaml(skill_dir, skill_name, implicit.get(skill_name), errors)
        validate_markdown(skill_dir, allowed_root, errors)
    if skills_root.resolve() == DEFAULT_SKILLS_ROOT.resolve():
        for relative in registry["filesystem_contract"]["paths"]:
            if not (REPO_ROOT / relative).is_file():
                errors.append(
                    f"{display_path(skills_root / REGISTRY_FILENAME)}: filesystem contract path is missing: {relative}"
                )
    return sorted(set(errors))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--skills-root", type=Path, default=DEFAULT_SKILLS_ROOT)
    parser.add_argument("--print-filesystem-contract-paths", action="store_true")
    args = parser.parse_args(argv)
    if args.print_filesystem_contract_paths:
        errors: list[str] = []
        registry = load_policy_registry(args.skills_root.resolve(), errors)
        if registry is None or errors:
            for error in errors:
                print(error, file=sys.stderr)
            return 1
        print("\n".join(registry["filesystem_contract"]["paths"]))
        return 0
    errors = validate_skills(args.skills_root.resolve())
    if errors:
        print("Agent skill validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    registry_errors: list[str] = []
    registry = load_policy_registry(args.skills_root.resolve(), registry_errors)
    if registry is None or registry_errors:
        for error in registry_errors:
            print(error, file=sys.stderr)
        return 1
    prefixes = tuple(registry["excluded_prefixes"])
    count = sum(
        1
        for path in args.skills_root.iterdir()
        if path.is_dir()
        and not path.name.startswith(prefixes)
    )
    print(f"Validated {count} maintained agent skills.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
