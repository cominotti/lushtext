#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Enforce the explicit-SIMD policy (`.agents/rules/rust.md`, "Explicit SIMD").

A hand-written SIMD kernel is admitted only by measured need, runtime dispatch,
and an allowlist row. This check fails when:

1. a Rust file under `crates/` mentions `target_feature` (attribute,
   `cfg_attr`, or `cfg`), `std::arch` / `core::arch` (grouped imports
   included), `is_x86_feature_detected` or `is_aarch64_feature_detected`
   outside `//` comments and string literals, and the allowlist names no kernel in that file;
2. the Cargo, toolchain, Meson, Flatpak or Snap build configuration sets a
   `target-cpu` or `target-feature` flag (the shipped binary must run on the
   target's baseline);
3. `Cargo.lock` resolves a crate that exists to dispatch SIMD (`multiversion`,
   `pulp`, `fearless_simd`) that no allowlist row names;
4. `memchr` is resolved for the shipped `lushtext` binary (normal and build
   edges, `cargo tree`) without its `std` feature, which carries its runtime
   AVX2 detection: without it every `memchr` search falls back to SSE2
   silently, voiding the measurements the accelerated scans were admitted on.

The allowlist is the table under "Explicit-SIMD kernel allowlist" in
`.agents/rules/rust.md`: one row per kernel, giving its file, function,
measurement record, NEON decision and dispatch crate (or `none`). A row
exempts its whole file; that the SIMD in it is the named function is for
review to hold.

Usage:
  scripts/check-explicit-simd.py              # check the tree
  scripts/check-explicit-simd.py --self-test  # plant one violation of each kind, then check the tree
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
RULES_FILE = Path(".agents/rules/rust.md")
ALLOWLIST_HEADING = "### Explicit-SIMD kernel allowlist"

SIMD_SOURCE_RE = re.compile(
    # `#[target_feature]`, `#[cfg_attr(..., target_feature(...))]` and
    # `cfg(target_feature = ...)` alike: any mention in code.
    r"\btarget_feature\b"
    r"|\b(?:std|core)::arch\b"
    # Grouped imports: `use std::{arch::x86_64::*, mem};`.
    r"|\b(?:std|core)::\{[^}]*\barch\b"
    r"|\bis_x86_feature_detected\b"
    r"|\bis_aarch64_feature_detected\b"
)
CPU_FLAG_RE = re.compile(r"target[-_]cpu|target[-_]feature")
DISPATCH_CRATES = ("multiversion", "pulp", "fearless_simd", "fearless-simd")
LOCK_PACKAGE_RE = re.compile(r'^name = "([^"]+)"$', re.M)
MEMCHR_FEATURE_RE = re.compile(r'^memchr feature "([^"]+)"', re.M)

# Build configuration that ships a binary (condition 5 of the policy).
BUILD_CONFIG_FILES = (
    ".cargo/config.toml",
    ".cargo/config",
    "rust-toolchain.toml",
    "rust-toolchain",
    "meson.build",
    "snap/snapcraft.yaml",
)
BUILD_CONFIG_DIRS = ("build-aux",)
# Vendored crate checksums and URLs, not build flags.
BUILD_CONFIG_SKIP = ("cargo-sources.json",)


@dataclass(frozen=True)
class AllowlistRow:
    file: str
    function: str
    dispatch_crate: str


def parse_allowlist(rules_text: str) -> list[AllowlistRow]:
    """Rows of the allowlist table under its heading (header and rule skipped)."""
    rows: list[AllowlistRow] = []
    in_section = False
    for line in rules_text.splitlines():
        if line.startswith("#"):
            in_section = line.strip() == ALLOWLIST_HEADING
            continue
        if not in_section or not line.startswith("|"):
            continue
        cells = [cell.strip().strip("`") for cell in line.strip().strip("|").split("|")]
        if len(cells) < 5 or cells[0] in ("File", "") or set(cells[0]) <= {"-", ":"}:
            continue
        rows.append(AllowlistRow(cells[0], cells[1], cells[4]))
    return rows


def code_of(line: str) -> str:
    """The code of one line: string-literal contents and a trailing `//`
    comment (doc comments included) removed, so prose and strings can name the
    APIs while code after `"http://..."` is still scanned. Block comments and
    raw strings are scanned as code: a false positive at worst, never a miss."""
    code = []
    in_string = False
    escaped = False
    for index, char in enumerate(line):
        if in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
                code.append(char)
        elif char == '"':
            in_string = True
            code.append(char)
        elif line.startswith("//", index):
            break
        else:
            code.append(char)
    return "".join(code)


def check_sources(root: Path, allowed_files: set[str]) -> list[str]:
    findings = []
    crates = root / "crates"
    if not crates.is_dir():
        return findings
    for path in sorted(crates.rglob("*.rs")):
        parts = path.relative_to(crates).parts
        if len(parts) > 1 and parts[1] == "target":
            # A crate-local build directory, not source.
            continue
        relative = path.relative_to(root).as_posix()
        if relative in allowed_files:
            continue
        source = path.read_text(encoding="utf-8", errors="replace")
        for number, line in enumerate(source.splitlines(), start=1):
            if SIMD_SOURCE_RE.search(code_of(line)):
                findings.append(
                    f"{relative}:{number}: explicit SIMD outside the allowlist: {line.strip()}"
                )
    return findings


def build_config_files(root: Path) -> list[Path]:
    files = [root / name for name in BUILD_CONFIG_FILES if (root / name).is_file()]
    for directory in BUILD_CONFIG_DIRS:
        base = root / directory
        if base.is_dir():
            files.extend(
                path
                for path in sorted(base.rglob("*"))
                if path.is_file() and path.name not in BUILD_CONFIG_SKIP
            )
    return files


def check_build_flags(root: Path) -> list[str]:
    findings = []
    for path in build_config_files(root):
        text = path.read_text(encoding="utf-8", errors="replace")
        for number, line in enumerate(text.splitlines(), start=1):
            if CPU_FLAG_RE.search(line):
                findings.append(
                    f"{path.relative_to(root).as_posix()}:{number}: build-time CPU flag "
                    f"(the shipped binary must run on the target baseline): {line.strip()}"
                )
    return findings


def check_dispatch_crates(root: Path, allowed_crates: set[str]) -> list[str]:
    lock = root / "Cargo.lock"
    if not lock.is_file():
        return []
    names = set(LOCK_PACKAGE_RE.findall(lock.read_text(encoding="utf-8")))
    return [
        f"Cargo.lock: SIMD-dispatch crate `{name}` without an allowlist row"
        for name in DISPATCH_CRATES
        if name in names and name not in allowed_crates
    ]


def check_memchr_std(memchr_features: set[str]) -> list[str]:
    """`memchr` as the shipped `lushtext` binary resolves it must carry `std`."""
    if not memchr_features:
        return ["cargo tree: the lushtext binary does not resolve memchr"]
    if "std" not in memchr_features:
        return [
            "cargo tree: memchr is resolved for the lushtext binary without its `std` "
            "feature (runtime AVX2 detection lost; every memchr search would run SSE2)"
        ]
    return []


def shipped_memchr_features(root: Path) -> set[str]:
    """memchr's features in the `lushtext` binary's own graph: normal and build
    edges only, so a dev-dependency (Criterion, proptest) that enables `std`
    cannot hide a shipped build without it, as a unified `cargo metadata`
    resolve would."""
    output = subprocess.run(
        [
            "cargo", "tree", "--locked", "-p", "lushtext",
            "-e", "normal,build,features", "-i", "memchr", "--prefix", "none",
        ],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return set(MEMCHR_FEATURE_RE.findall(output))


def check(root: Path, memchr_features: set[str]) -> list[str]:
    rules = root / RULES_FILE
    rules_text = rules.read_text(encoding="utf-8") if rules.is_file() else ""
    if ALLOWLIST_HEADING not in rules_text:
        return [f"{RULES_FILE}: missing the '{ALLOWLIST_HEADING}' table"]
    rows = parse_allowlist(rules_text)
    findings = check_sources(root, {row.file for row in rows})
    findings += check_build_flags(root)
    findings += check_dispatch_crates(root, {row.dispatch_crate for row in rows})
    findings += check_memchr_std(memchr_features)
    return findings


EMPTY_RULES = f"""# Rust Conventions

{ALLOWLIST_HEADING}

| File | Function | Measurement | NEON | Dispatch crate |
| --- | --- | --- | --- | --- |

## Next
"""


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="check-explicit-simd-") as scratch:
        root = Path(scratch)
        (root / ".agents/rules").mkdir(parents=True)
        (root / RULES_FILE).write_text(EMPTY_RULES, encoding="utf-8")
        source = root / "crates/demo/src"
        source.mkdir(parents=True)
        clean = {"alloc", "default", "std"}

        (source / "lib.rs").write_text(
            "// std::arch and #[target_feature] named in prose are fine.\n"
            'pub const URL: &str = "https://example.org/std::arch"; // core::arch too\n'
            "pub fn scalar() {}\n",
            encoding="utf-8",
        )
        (root / "Cargo.lock").write_text('[[package]]\nname = "memchr"\n', encoding="utf-8")
        assert check(root, clean) == [], check(root, clean)

        planted = {
            "attribute.rs": '#[target_feature(enable = "avx2")]\nunsafe fn kernel() {}\n',
            "cfg_attr.rs": '#[cfg_attr(target_arch = "x86_64", target_feature(enable = "avx2"))]\nfn k() {}\n',
            "std_arch.rs": "use std::arch::x86_64::_mm256_loadu_si256;\n",
            "core_arch.rs": "use core::arch::aarch64::vld1q_u8;\n",
            "grouped_import.rs": "use std::{arch::x86_64::_mm_loadu_si128, mem};\n",
            "after_url.rs": 'const U: &str = "http://x"; use core::arch::x86_64::_mm_pause;\n',
            "x86_detect.rs": 'fn f() -> bool { is_x86_feature_detected!("avx2") }\n',
            "arm_detect.rs": 'fn f() -> bool { std::arch::is_aarch64_feature_detected!("neon") }\n',
        }
        for name, text in planted.items():
            (source / name).write_text(text, encoding="utf-8")
            findings = check(root, clean)
            assert len(findings) == 1 and f"crates/demo/src/{name}:" in findings[0], (name, findings)
            (source / name).unlink()

        allowed_rules = EMPTY_RULES.replace(
            "| --- | --- | --- | --- | --- |\n",
            "| --- | --- | --- | --- | --- |\n"
            "| `crates/demo/src/kernel.rs` | `kernel` | docs/benchmarks/x.md | unmeasured | none |\n",
        )
        (root / RULES_FILE).write_text(allowed_rules, encoding="utf-8")
        (source / "kernel.rs").write_text(planted["attribute.rs"], encoding="utf-8")
        assert check(root, clean) == [], "an allowlisted kernel file passes"
        (source / "kernel.rs").unlink()
        (root / RULES_FILE).write_text(EMPTY_RULES, encoding="utf-8")

        for config, line in (
            (".cargo/config.toml", '[build]\nrustflags = ["-C", "target-cpu=native"]\n'),
            ("rust-toolchain.toml", '[toolchain]\nchannel = "1"\n# -C target-feature=+avx2\n'),
            ("meson.build", "add_project_arguments('-C', 'target-cpu=x86-64-v3')\n"),
            ("build-aux/app.Flatpak.json", '{"env": {"RUSTFLAGS": "-C target-feature=+avx2"}}\n'),
            ("snap/snapcraft.yaml", "environment:\n  RUSTFLAGS: -Ctarget-cpu=haswell\n"),
        ):
            path = root / config
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(line, encoding="utf-8")
            findings = check(root, clean)
            assert len(findings) == 1 and findings[0].startswith(config + ":"), (config, findings)
            path.unlink()

        (root / "Cargo.lock").write_text(
            '[[package]]\nname = "memchr"\n\n[[package]]\nname = "multiversion"\n',
            encoding="utf-8",
        )
        findings = check(root, clean)
        assert len(findings) == 1 and "multiversion" in findings[0], findings
        (root / "Cargo.lock").write_text('[[package]]\nname = "memchr"\n', encoding="utf-8")

        findings = check(root, {"alloc"})
        assert len(findings) == 1 and "`std`" in findings[0], findings
        findings = check(root, set())
        assert len(findings) == 1 and "does not resolve memchr" in findings[0], findings

    print("check-explicit-simd self-test passed")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--self-test", action="store_true")
    arguments = parser.parse_args()
    if arguments.self_test:
        self_test()
    findings = check(REPO_ROOT, shipped_memchr_features(REPO_ROOT))
    for finding in findings:
        print(f"check-explicit-simd: {finding}", file=sys.stderr)
    if findings:
        return 1
    print(
        "check-explicit-simd passed: no explicit SIMD outside the allowlist, "
        "no build-time CPU flags, no unlisted dispatch crate, memchr keeps `std`"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
