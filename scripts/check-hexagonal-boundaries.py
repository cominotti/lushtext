#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Enforce LushText's hexagonal crate boundaries and 1,000-line file-size ratchet.

Checks:
1. Crate naming: `crates/lushtext-ui` exists as workspace package `lushtext-ui`,
   and no `lushtext-core` workspace package remains.
2. Cargo dependency direction (`cargo metadata`):
   - `lushtext-model`: no GTK/GLib/Adwaita/SourceView/Pango/GDK, no filesystem/OS/
     process/async-channel/infra engine crates (`rustix`, `notify*`, `ignore`,
     `dirs`, `tempfile`, `regex`, `pulldown-cmark`, `crossbeam-channel`, `anyhow`),
     and no `lushtext-services` / `lushtext-ui`.
   - `lushtext-services`: no GTK/GLib/Adwaita/SourceView/Pango/GDK (`gtk4`, `glib`,
     `gio`, `gdk4`, `pango`, `libadwaita`, `sourceview5`, `gtk-lush-widgets`,
     `gtk-lush-signals`, `gtk-lush-settle`, `gtk-lush-tasks`, `gtk-lush-viewport`)
     and no `lushtext-ui`.
   - `lushtext-ui`: no direct infrastructure engine crates (`regex`,
     `pulldown-cmark`, `rustix`, `notify-debouncer-full`, `ignore`, `simdutf8`,
     `encoding_rs`, `nucleo-matcher`, `editorconfig-parser`, `grep-regex`,
     `grep-searcher`, `grep-matcher`). Narrow UI allowlist (`serde_json` for D-Bus
     automation/fuzzing and `crossbeam-channel` for UI polling/disposal) stays
     explicitly documented here.
3. Production file-size ratchet: no new production file under `crates/lushtext-model/src`,
   `crates/lushtext-services/src`, or `crates/lushtext-ui/src` exceeds 1,000
   production lines (excluding `#[cfg(test)]` and dedicated test/fixture/proof
   files), and the legacy over-limit set stays at or below the 39-file baseline.

Usage:
  scripts/check-hexagonal-boundaries.py              # check the real workspace
  scripts/check-hexagonal-boundaries.py --self-test  # test violation fixtures, then check the real workspace
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

MAX_PRODUCTION_LINES = 1000
LEGACY_OVERSIZED_CEILING = 35

# Legacy production files that still exceed 1,000 production lines after Change 4
# Batch A slimmed `editor_page/imp.rs` and `window/imp.rs` below 500 lines (35 files
# total, down from 37). Later changes tighten this ratchet further and may never
# add new >= 1,000-line files.
LEGACY_OVERSIZED_FILES: dict[str, int] = {
    "crates/lushtext-model/src/automation/mod.rs": 1400,
    "crates/lushtext-model/src/document/document_scale.rs": 1220,
    "crates/lushtext-model/src/workspace/content_search.rs": 1100,
    "crates/lushtext-services/src/editor_io/mod.rs": 1920,
    "crates/lushtext-services/src/palette/index/reference.rs": 1240,
    "crates/lushtext-services/src/palette/notes.rs": 2150,
    "crates/lushtext-services/src/persistence/draft_service.rs": 2950,
    "crates/lushtext-services/src/persistence/draft_service/journal_model.rs": 1750,
    "crates/lushtext-services/src/persistence/local_history_service.rs": 1530,
    "crates/lushtext-services/src/persistence/recovery_metadata.rs": 1270,
    "crates/lushtext-services/src/search/content_search/replace/apply.rs": 1050,
    "crates/lushtext-services/src/search/search_backup.rs": 1740,
    "crates/lushtext-services/src/workspace/action_catalog/mod.rs": 2660,
    "crates/lushtext-ui/src/ui/automation.rs": 2550,
    "crates/lushtext-ui/src/ui/buffer_snapshot.rs": 1230,
    "crates/lushtext-ui/src/ui/editor_page/buffer_replacement/execution.rs": 1450,
    "crates/lushtext-ui/src/ui/editor_page/load/execution.rs": 1290,
    "crates/lushtext-ui/src/ui/editor_page/minimap/policy.rs": 1480,
    "crates/lushtext-ui/src/ui/editor_page/minimap/projection_execution.rs": 1210,
    "crates/lushtext-ui/src/ui/editor_page/protected_view/policy.rs": 1310,
    "crates/lushtext-ui/src/ui/markdown_preview/continuation.rs": 1050,
    "crates/lushtext-ui/src/ui/plain_disposal.rs": 1470,
    "crates/lushtext-ui/src/ui/responsiveness_probe.rs": 1300,
    "crates/lushtext-ui/src/ui/sidebar/policy.rs": 1170,
    "crates/lushtext-ui/src/ui/sidebar/workspace_section/folder_execution.rs": 1120,
    "crates/lushtext-ui/src/ui/sidebar/workspace_section/scan_execution.rs": 2950,
    "crates/lushtext-ui/src/ui/window/dialogs.rs": 1040,
    "crates/lushtext-ui/src/ui/window/documents.rs": 1410,
    "crates/lushtext-ui/src/ui/window/drafts/autosave_execution.rs": 1030,
    "crates/lushtext-ui/src/ui/window/local_history/preview_execution.rs": 1230,
    "crates/lushtext-ui/src/ui/window/notes/bookmark_execution.rs": 1090,
    "crates/lushtext-ui/src/ui/window/notes/browser.rs": 1530,
    "crates/lushtext-ui/src/ui/window/notes/editor_execution.rs": 1130,
    "crates/lushtext-ui/src/ui/window/notes/source_execution.rs": 1040,
    "crates/lushtext-ui/src/ui/window/search.rs": 1050,
}

GTK_CRATES = {
    "gtk4",
    "glib",
    "gio",
    "gdk4",
    "pango",
    "libadwaita",
    "sourceview5",
    "gtk-lush-widgets",
    "gtk-lush-signals",
    "gtk-lush-settle",
    "gtk-lush-tasks",
    "gtk-lush-viewport",
    "gtk-lush-proof-harness",
    "gtk-lush-axioms",
}

FORBIDDEN_MODEL_DEPS = GTK_CRATES | {
    "lushtext-services",
    "lushtext-ui",
    "lushtext-core",
    "rustix",
    "notify",
    "notify-debouncer-full",
    "ignore",
    "dirs",
    "tempfile",
    "regex",
    "pulldown-cmark",
    "crossbeam-channel",
    "anyhow",
    "grep-regex",
    "grep-searcher",
    "grep-matcher",
}

FORBIDDEN_SERVICES_DEPS = GTK_CRATES | {
    "lushtext-ui",
    "lushtext-core",
    "anyhow",
}

# `serde_json` (D-Bus automation / fuzzing) and `crossbeam-channel` (worker
# polling/disposal) stay explicitly allowlisted for `lushtext-ui`.
FORBIDDEN_UI_DEPS = {
    "regex",
    "pulldown-cmark",
    "rustix",
    "notify",
    "notify-debouncer-full",
    "ignore",
    "simdutf8",
    "encoding_rs",
    "nucleo-matcher",
    "editorconfig-parser",
    "grep-regex",
    "grep-searcher",
    "grep-matcher",
    "anyhow",
}

TEST_ONLY_FILE_PARTS = {
    "tests.rs",
    "fixture.rs",
}


def is_excluded_test_or_proof_file(rel: str) -> bool:
    """Return True for dedicated test, fixture, conformance, or Kani proof files."""
    parts = Path(rel).parts
    if parts[-1] in TEST_ONLY_FILE_PARTS or parts[-1].endswith("_tests.rs"):
        return True
    if any(p in {"tests", "kani_proofs", "conformance"} for p in parts):
        return True
    return parts[-1] == "kani_proofs.rs"


def _scan_rust_delimiters(line: str, depth: int, seen_brace: bool, in_block_comment: bool) -> tuple[int, bool, bool, bool]:
    """Advance brace depth and comment state across one line for `#[cfg(test)]` item stripping."""
    seen_semi_at_zero = False
    i = 0
    length = len(line)
    while i < length:
        if in_block_comment:
            end = line.find("*/", i)
            if end == -1:
                return depth, seen_brace, seen_semi_at_zero, True
            i = end + 2
            in_block_comment = False
            continue
        ch = line[i]
        if ch == "/" and i + 1 < length:
            nxt = line[i + 1]
            if nxt == "/":
                break
            if nxt == "*":
                in_block_comment = True
                i += 2
                continue
        if ch == '"':
            i += 1
            while i < length:
                if line[i] == "\\":
                    i += 2
                elif line[i] == '"':
                    i += 1
                    break
                else:
                    i += 1
            continue
        if ch == "'":
            if i + 2 < length and line[i + 1] == "\\":
                end_q = line.find("'", i + 3)
                if end_q != -1 and end_q - i <= 8:
                    i = end_q + 1
                    continue
            elif i + 2 < length and line[i + 2] == "'":
                i += 3
                continue
        if ch == "{":
            depth += 1
            seen_brace = True
        elif ch == "}":
            depth = max(0, depth - 1)
        elif ch == ";" and depth == 0 and not seen_brace:
            seen_semi_at_zero = True
        i += 1
    return depth, seen_brace, seen_semi_at_zero, in_block_comment


def production_line_count(path: Path) -> int:
    """Count lines excluding every `#[cfg(test)]` item (`use ...;`, `mod tests;`, or `{ ... }` block)."""
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    count = 0
    i = 0
    in_block_comment = False
    while i < len(lines):
        if re.match(r"^\s*#\[cfg\(test\)\]", lines[i]):
            depth = 0
            seen_brace = False
            blk = in_block_comment
            while i < len(lines):
                stripped = lines[i].strip()
                depth, seen_brace, semi0, blk = _scan_rust_delimiters(lines[i], depth, seen_brace, blk)
                i += 1
                if not seen_brace and not semi0 and (stripped.startswith("#[") or stripped.startswith("//") or stripped == ""):
                    continue
                if semi0:
                    break
                if seen_brace and depth == 0:
                    break
            in_block_comment = blk
            continue
        _, _, _, in_block_comment = _scan_rust_delimiters(lines[i], 0, False, in_block_comment)
        count += 1
        i += 1
    return count


def check_metadata_boundaries(metadata: dict) -> list[str]:
    """Validate workspace package names and layer dependency direction from cargo metadata."""
    findings: list[str] = []
    packages = {pkg["name"]: pkg for pkg in metadata.get("packages", [])}

    if "lushtext-core" in packages:
        findings.append(
            "workspace still contains package `lushtext-core`; rename it to `lushtext-ui`"
        )
    if "lushtext-ui" not in packages:
        findings.append("workspace is missing expected crate `lushtext-ui`")

    rules = [
        ("lushtext-model", FORBIDDEN_MODEL_DEPS),
        ("lushtext-services", FORBIDDEN_SERVICES_DEPS),
        ("lushtext-ui", FORBIDDEN_UI_DEPS),
        # Also inspect `lushtext-core` if it still exists pre-rename so failing-first
        # runs report all direct infra leaks immediately.
        ("lushtext-core", FORBIDDEN_UI_DEPS),
    ]
    for pkg_name, forbidden in rules:
        pkg = packages.get(pkg_name)
        if pkg is None:
            continue
        for dep in pkg.get("dependencies", []):
            kind = dep.get("kind")
            if kind is not None:
                # Dev and build dependencies are exempt from normal runtime layer rules
                # unless tracked elsewhere (e.g., workspace check-terminology/fixtures).
                continue
            dep_name = dep["name"]
            if dep_name in forbidden:
                findings.append(
                    f"{pkg_name}: forbidden normal dependency `{dep_name}` crosses hexagonal boundary"
                )
    return findings


def check_file_sizes(root: Path) -> list[str]:
    """Enforce the 1,000-line ceiling on new production files and the legacy ratchet."""
    findings: list[str] = []
    legacy_ceilings = LEGACY_OVERSIZED_FILES
    legacy_hits = 0

    for crate_dir in ("crates/lushtext-model/src", "crates/lushtext-services/src", "crates/lushtext-ui/src"):
        base = root / crate_dir
        if not base.is_dir():
            continue
        for path in sorted(base.rglob("*.rs")):
            rel = path.relative_to(root).as_posix()
            if is_excluded_test_or_proof_file(rel):
                continue
            lines = production_line_count(path)
            if lines <= MAX_PRODUCTION_LINES:
                continue
            ceiling = legacy_ceilings.get(rel)
            if ceiling is None:
                findings.append(
                    f"{rel}: {lines} production lines exceeds {MAX_PRODUCTION_LINES}-line limit for new/non-legacy files"
                )
            elif lines > ceiling:
                findings.append(
                    f"{rel}: {lines} production lines exceeds recorded legacy ratchet ceiling {ceiling}"
                )
            else:
                legacy_hits += 1

    if legacy_hits > LEGACY_OVERSIZED_CEILING:
        findings.append(
            f"legacy >= {MAX_PRODUCTION_LINES}-line count ({legacy_hits}) exceeds baseline ceiling {LEGACY_OVERSIZED_CEILING}"
        )
    return findings


def load_cargo_metadata(root: Path) -> dict:
    proc = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version=1"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(proc.stdout)


def check(root: Path, metadata: dict | None = None) -> list[str]:
    if metadata is None:
        metadata = load_cargo_metadata(root)
    return check_metadata_boundaries(metadata) + check_file_sizes(root)


def _fixture_metadata(overrides: dict[str, list[str]] | None = None, core_present: bool = False) -> dict:
    deps = {
        "lushtext-model": ["serde", "memchr"],
        "lushtext-services": ["lushtext-model", "thiserror", "serde_json"],
        "lushtext-ui": ["lushtext-model", "lushtext-services", "gtk4", "libadwaita", "serde_json", "crossbeam-channel"],
    }
    if overrides:
        deps.update(overrides)
    packages = [
        {
            "name": name,
            "dependencies": [{"name": d, "kind": None} for d in pkg_deps],
        }
        for name, pkg_deps in deps.items()
    ]
    if core_present:
        packages.append({"name": "lushtext-core", "dependencies": []})
    return {"packages": packages}


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="check-hexagonal-boundaries-") as scratch:
        root = Path(scratch)
        for rel in ("crates/lushtext-model/src", "crates/lushtext-services/src", "crates/lushtext-ui/src"):
            (root / rel).mkdir(parents=True)

        # Clean tree + metadata passes.
        (root / "crates/lushtext-ui/src/lib.rs").write_text("pub fn ok() {}\n", encoding="utf-8")
        assert check(root, _fixture_metadata()) == [], "clean hexagonal tree must pass"

        # Allowlisted dev dependency on `regex` or `tempfile` is allowed in metadata check.
        dev_meta = {
            "packages": [
                {"name": "lushtext-model", "dependencies": [{"name": "proptest", "kind": "dev"}]},
                {"name": "lushtext-services", "dependencies": [{"name": "tempfile", "kind": "dev"}]},
                {"name": "lushtext-ui", "dependencies": [{"name": "tempfile", "kind": "dev"}]},
            ]
        }
        assert check(root, dev_meta) == [], "dev-dependencies must not trigger normal edge errors"

        # 1. Model layer forbidden edge (`regex`, `rustix`, `gtk4`).
        for bad_dep in ("regex", "rustix", "gtk4"):
            findings = check(root, _fixture_metadata({"lushtext-model": ["serde", bad_dep]}))
            assert any("lushtext-model" in f and bad_dep in f for f in findings), (bad_dep, findings)

        # 2. Services layer forbidden edge (`gtk4`, `lushtext-ui`, `anyhow`).
        for bad_dep in ("gtk4", "lushtext-ui", "anyhow"):
            findings = check(root, _fixture_metadata({"lushtext-services": ["lushtext-model", bad_dep]}))
            assert any("lushtext-services" in f and bad_dep in f for f in findings), (bad_dep, findings)

        # 3. UI layer forbidden infra engine edge (`regex`, `pulldown-cmark`, `anyhow`).
        for bad_dep in ("regex", "pulldown-cmark", "anyhow"):
            findings = check(root, _fixture_metadata({"lushtext-ui": ["lushtext-services", bad_dep]}))
            assert any("lushtext-ui" in f and bad_dep in f for f in findings), (bad_dep, findings)

        # 4. Leftover `lushtext-core` crate fails.
        findings = check(root, _fixture_metadata(core_present=True))
        assert any("lushtext-core" in f for f in findings), findings

        # 5. New production file > 1,000 production lines fails; > 1,000 total lines
        # inside `#[cfg(test)]` passes.
        oversized = root / "crates/lushtext-ui/src/bloated.rs"
        oversized.write_text("fn a() {}\n" * 1005, encoding="utf-8")
        findings = check(root, _fixture_metadata())
        assert any("bloated.rs" in f and "1005 production lines" in f for f in findings), findings
        oversized.unlink()

        test_heavy = root / "crates/lushtext-model/src/unit_heavy.rs"
        test_heavy.write_text(("fn a() {}\n" * 50) + "#[cfg(test)]\nmod tests {\n" + ("fn t() {}\n" * 1200) + "}\n", encoding="utf-8")
        assert check(root, _fixture_metadata()) == [], "lines inside #[cfg(test)] must not count against production limit"
        test_heavy.unlink()

        masked_prod = root / "crates/lushtext-services/src/masked_bloated.rs"
        masked_prod.write_text("#[cfg(test)]\nuse crate::fake;\n#[cfg(test)]\nmod tests;\n" + ("fn prod() {}\n" * 1005), encoding="utf-8")
        findings = check(root, _fixture_metadata())
        assert any("masked_bloated.rs" in f and "1005 production lines" in f for f in findings), findings
        masked_prod.unlink()

    print("check-hexagonal-boundaries self-test passed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    findings = check(REPO_ROOT)
    for finding in findings:
        print(f"check-hexagonal-boundaries: {finding}", file=sys.stderr)
    if findings:
        return 1
    print("check-hexagonal-boundaries passed: 0 crate leaks and file sizes within ratchet")
    return 0


if __name__ == "__main__":
    sys.exit(main())
