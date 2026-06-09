#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Audit generated GtkBuilder templates against LushText's UI contract."""

from __future__ import annotations

import argparse
import difflib
import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
UI_DIR = ROOT / "resources" / "ui"
GRESOURCE_XML = ROOT / "resources" / "dev.cominotti.lushtext.gresource.xml"
RUST_UI_DIR = ROOT / "crates" / "lushtext-core" / "src" / "ui"
RESOURCE_PREFIX = "/dev/cominotti/lushtext/"

CHECKED_PROPERTIES = {
    "action-name",
    "activatable",
    "can-focus",
    "column-spacing",
    "default-height",
    "default-width",
    "ellipsize",
    "focus-on-click",
    "halign",
    "has-frame",
    "height-request",
    "hexpand",
    "homogeneous",
    "hscrollbar-policy",
    "icon-name",
    "label",
    "margin-bottom",
    "margin-end",
    "margin-start",
    "margin-top",
    "max-width-chars",
    "min-content-height",
    "min-content-width",
    "modal",
    "orientation",
    "placeholder-text",
    "propagate-natural-height",
    "propagate-natural-width",
    "resize-end-child",
    "resize-start-child",
    "reveal-child",
    "row-spacing",
    "selectable",
    "sensitive",
    "shrink-end-child",
    "shrink-start-child",
    "subtitle",
    "title",
    "tooltip-text",
    "transition-duration",
    "transition-type",
    "valign",
    "vexpand",
    "visible",
    "vscrollbar-policy",
    "width-chars",
    "width-request",
    "wrap",
    "wrap-mode",
    "xalign",
    "yalign",
}

ENUM_NORMALIZERS = {
    "ellipsize": {"none": "0", "start": "1", "middle": "2", "end": "3"},
    "halign": {"fill": "0", "start": "1", "end": "2", "center": "3", "baseline": "4"},
    "hscrollbar-policy": {"always": "0", "automatic": "1", "never": "2", "external": "3"},
    "orientation": {"horizontal": "0", "vertical": "1"},
    "transition-type": {
        "none": "0",
        "crossfade": "1",
        "slide-right": "2",
        "slide-left": "3",
        "slide-up": "4",
        "slide-down": "5",
        "slide_right": "2",
        "slide_left": "3",
        "slide_up": "4",
        "slide_down": "5",
    },
    "valign": {"fill": "0", "start": "1", "end": "2", "center": "3", "baseline": "4"},
    "vscrollbar-policy": {"always": "0", "automatic": "1", "never": "2", "external": "3"},
    "wrap-mode": {"none": "0", "char": "1", "word": "2", "word-char": "2", "word_char": "2"},
}


def text_of(element: ET.Element) -> str:
    return "".join(element.itertext()).strip()


def normalize_property(name: str, value: str) -> str:
    value = value.strip()
    if re.fullmatch(r"[+-]?\d+\.0", value):
        value = value[:-2]
    mapping = ENUM_NORMALIZERS.get(name)
    if mapping is not None:
        return mapping.get(value, value)
    if name == "attributes":
        return value.replace("0 4294967295 ", "0 -1 ")
    return value


def normalize_label_attribute(name: str, value: str) -> str:
    if name == "font-features":
        escaped = value.strip().replace('"', '\\"')
        return f'0 -1 font-features "{escaped}"'
    return value.strip()


def element_name(element: ET.Element) -> str:
    if element.tag == "template":
        return f"template:{element.attrib.get('class', '')}"
    if element.tag == "object":
        class_name = element.attrib.get("class", "")
        object_id = element.attrib.get("id")
        return f"object:{class_name}#{object_id}" if object_id else f"object:{class_name}"
    if element.tag in {"menu", "section", "submenu", "item"}:
        object_id = element.attrib.get("id")
        return f"{element.tag}#{object_id}" if object_id else element.tag
    return element.tag


def walk_template(element: ET.Element, path: str, out: dict[str, list[dict[str, Any]]]) -> None:
    if element.tag == "template":
        out["roots"].append(
            {
                "path": path,
                "tag": "template",
                "class": element.attrib.get("class", ""),
                "parent": element.attrib.get("parent", ""),
            }
        )
    elif element.tag == "object":
        out["objects"].append(
            {
                "path": path,
                "class": element.attrib.get("class", ""),
                "id": element.attrib.get("id", ""),
            }
        )
    elif element.tag == "child":
        out["child_roles"].append(
            {
                "path": path,
                "type": element.attrib.get("type", ""),
                "internal-child": element.attrib.get("internal-child", ""),
            }
        )
    elif element.tag == "property":
        name = element.attrib.get("name", "")
        value = normalize_property(name, text_of(element))
        if name in CHECKED_PROPERTIES or name == "attributes":
            out["properties"].append(
                {
                    "path": path,
                    "name": name,
                    "value": value,
                    "translatable": element.attrib.get("translatable", ""),
                    "context": element.attrib.get("context", ""),
                }
            )
        if element.attrib.get("translatable"):
            out["translations"].append(
                {
                    "path": path,
                    "name": name,
                    "value": value,
                    "context": element.attrib.get("context", ""),
                }
            )
    elif element.tag == "class":
        out["style_classes"].append({"path": path, "name": element.attrib.get("name", "")})
    elif element.tag == "attribute" and ":attributes/" not in path:
        name = element.attrib.get("name", "")
        value = element.attrib.get("value", text_of(element))
        out["menu_attributes"].append(
            {
                "path": path,
                "name": name,
                "value": value.strip(),
                "translatable": element.attrib.get("translatable", ""),
                "context": element.attrib.get("context", ""),
            }
        )
    elif element.tag in {"menu", "section", "submenu", "item"}:
        out["menus"].append(
            {
                "path": path,
                "tag": element.tag,
                "id": element.attrib.get("id", ""),
            }
        )

    if element.tag == "layout":
        for prop in element.findall("property"):
            name = prop.attrib.get("name", "")
            out["layout_properties"].append(
                {"path": path, "name": name, "value": normalize_property(name, text_of(prop))}
            )

    if element.tag == "accessibility":
        for prop in element.findall("property"):
            out["accessibility"].append(
                {
                    "path": path,
                    "name": prop.attrib.get("name", ""),
                    "value": text_of(prop),
                    "translatable": prop.attrib.get("translatable", ""),
                    "context": prop.attrib.get("context", ""),
                }
            )

    if element.tag == "attributes":
        for attr in element.findall("attribute"):
            name = attr.attrib.get("name", "")
            value = attr.attrib.get("value", text_of(attr))
            property_path = re.sub(r":attributes$", ":property", path)
            out["properties"].append(
                {
                    "path": property_path,
                    "name": "attributes",
                    "value": normalize_label_attribute(name, value),
                    "translatable": attr.attrib.get("translatable", ""),
                    "context": attr.attrib.get("context", ""),
                }
            )

    for index, child in enumerate(list(element)):
        child_path = f"{path}/{index}:{element_name(child)}"
        walk_template(child, child_path, out)


def fingerprint_ui(path: Path) -> dict[str, Any]:
    tree = ET.parse(path)
    interface = tree.getroot()
    out: dict[str, list[dict[str, Any]]] = {
        "roots": [],
        "objects": [],
        "child_roles": [],
        "properties": [],
        "layout_properties": [],
        "style_classes": [],
        "translations": [],
        "accessibility": [],
        "menus": [],
        "menu_attributes": [],
    }
    root_index = 0
    for child in list(interface):
        if child.tag == "requires":
            continue
        walk_template(child, f"{root_index}:{element_name(child)}", out)
        root_index += 1
    return out


def current_contract() -> dict[str, Any]:
    files = {}
    for ui_file in sorted(UI_DIR.glob("*.ui")):
        files[ui_file.name] = fingerprint_ui(ui_file)
    return {"version": 1, "files": files}


def load_gresource_ui_files() -> set[str]:
    tree = ET.parse(GRESOURCE_XML)
    root = tree.getroot()
    result = set()
    for gresource in root.findall("gresource"):
        if gresource.attrib.get("prefix") != RESOURCE_PREFIX.rstrip("/"):
            continue
        for file_node in gresource.findall("file"):
            name = text_of(file_node)
            if name.startswith("ui/") and name.endswith(".ui"):
                result.add(name)
    return result


def parse_rust_templates() -> list[dict[str, Any]]:
    template_re = re.compile(r'#\[template\(resource = "([^"]+)"\)\]')
    child_re = re.compile(r"pub\s+([A-Za-z0-9_]+)\s*:\s*TemplateChild<([^>]+)>")
    entries = []
    for path in sorted(RUST_UI_DIR.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        template_match = template_re.search(text)
        if not template_match:
            continue
        children = [
            {"field": field, "rust_type": rust_type.strip()}
            for field, rust_type in child_re.findall(text)
        ]
        entries.append(
            {
                "path": path,
                "resource": template_match.group(1),
                "children": children,
            }
        )
    return entries


def rust_type_to_xml_class(rust_type: str) -> str:
    rust_type = rust_type.strip()
    if rust_type.startswith("gtk4::"):
        return "Gtk" + rust_type.rsplit("::", 1)[1]
    if rust_type.startswith("libadwaita::"):
        return "Adw" + rust_type.rsplit("::", 1)[1]
    if rust_type == "sourceview5::View":
        return "GtkSourceView"
    return rust_type.rsplit("::", 1)[-1]


def object_ids(ui_file: Path) -> dict[str, str]:
    tree = ET.parse(ui_file)
    ids = {}
    for obj in tree.getroot().iter("object"):
        object_id = obj.attrib.get("id")
        if object_id:
            ids[object_id] = obj.attrib.get("class", "")
    return ids


def audit_resource_and_bindings() -> list[str]:
    errors = []
    resource_entries = load_gresource_ui_files()
    ui_files = {f"ui/{path.name}" for path in UI_DIR.glob("*.ui")}
    blp_files = {path.with_suffix(".ui").name for path in UI_DIR.glob("*.blp")}

    for missing in sorted(ui_files - resource_entries):
        errors.append(f"{missing} is present on disk but missing from the GResource manifest")
    for stale in sorted(resource_entries - ui_files):
        errors.append(f"{stale} is listed in the GResource manifest but missing on disk")
    for ui_name in sorted(path.name for path in UI_DIR.glob("*.ui")):
        if ui_name not in blp_files:
            errors.append(f"resources/ui/{ui_name} has no matching Blueprint source")

    for entry in parse_rust_templates():
        resource = entry["resource"]
        if not resource.startswith(RESOURCE_PREFIX):
            errors.append(f"{entry['path']}: template resource {resource} uses an unexpected prefix")
            continue
        relative = resource.removeprefix(RESOURCE_PREFIX)
        if relative not in resource_entries:
            errors.append(f"{entry['path']}: template resource {resource} is not in the GResource manifest")
            continue
        ui_file = ROOT / "resources" / relative
        ids = object_ids(ui_file)
        for child in entry["children"]:
            field = child["field"]
            rust_type = child["rust_type"]
            expected = rust_type_to_xml_class(rust_type)
            actual = ids.get(field)
            if actual is None:
                errors.append(f"{entry['path']}: TemplateChild {field} is missing from {relative}")
            elif actual != expected:
                errors.append(
                    f"{entry['path']}: TemplateChild {field} expects {expected}, "
                    f"but {relative} defines {actual}"
                )

    return errors


def write_baseline(path: Path) -> None:
    data = current_contract()
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Wrote {path.relative_to(ROOT)}")


def check_baseline(path: Path) -> int:
    if not path.exists():
        print(f"error: missing template contract baseline: {path}", file=sys.stderr)
        return 1

    expected = json.loads(path.read_text(encoding="utf-8"))
    actual = current_contract()
    errors = audit_resource_and_bindings()

    status = 0
    if expected != actual:
        status = 1
        expected_text = json.dumps(expected, indent=2, sort_keys=True).splitlines()
        actual_text = json.dumps(actual, indent=2, sort_keys=True).splitlines()
        print("error: generated UI template contract drift detected", file=sys.stderr)
        print(
            "\n".join(
                difflib.unified_diff(
                    expected_text,
                    actual_text,
                    fromfile=str(path.relative_to(ROOT)),
                    tofile="current generated UI contract",
                    lineterm="",
                )
            ),
            file=sys.stderr,
        )

    if errors:
        status = 1
        print("error: UI template resource/binding audit failed", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)

    if status == 0:
        print("UI template contract, GResource paths, and TemplateChild bindings are valid.")
    return status


def main() -> int:
    global UI_DIR

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--ui-dir",
        type=Path,
        default=UI_DIR,
        help="Directory containing generated .ui files",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=UI_DIR / "template-contract.json",
        help="Template contract JSON path",
    )
    parser.add_argument(
        "--write-baseline",
        action="store_true",
        help="Write the current generated .ui contract baseline",
    )
    args = parser.parse_args()

    UI_DIR = args.ui_dir if args.ui_dir.is_absolute() else ROOT / args.ui_dir
    baseline = args.baseline if args.baseline.is_absolute() else ROOT / args.baseline
    if args.write_baseline:
        write_baseline(baseline)
        return 0
    return check_baseline(baseline)


if __name__ == "__main__":
    raise SystemExit(main())
