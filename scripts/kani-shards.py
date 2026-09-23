#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the Kani proof harnesses in named shards, and check the shard table.

Every `#[kani::proof]` harness belongs to exactly one shard. `make kani` runs
every shard in turn; the scheduled CI lane runs one shard per matrix job, so
each job stays inside the repository's 30-minute job cap even though the whole
lane takes longer than that. `check` (part of `make check-policy`, no Kani
needed) fails when a harness anywhere under `crates/` matches no shard or more
than one, so a new harness can never silently drop out of CI.

This table is the one source of truth for the shards, and the Makefile's
`KANI_VERSION` for the pinned Kani: `github-outputs` hands both to
`.github/workflows/kani.yml`, which builds its matrix from them.

Usage:
  scripts/kani-shards.py list
  scripts/kani-shards.py check
  scripts/kani-shards.py github-outputs
  scripts/kani-shards.py run all|<shard> [--target-dir DIR] [--self-test]
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CRATES = REPO_ROOT / "crates"
MAKEFILE = REPO_ROOT / "Makefile"
KANI_VERSION_RE = re.compile(r"^KANI_VERSION \?= (\S+)$", re.M)

# The crates that own checked code, and each crate's source root. A harness's
# Kani name is its module path below the crate root plus its function name.
PACKAGES = {
    "gtk-lush-widgets": REPO_ROOT / "crates/gtk-lush/widgets/src",
    "lushtext-core": REPO_ROOT / "crates/lushtext-core/src",
}

# shard name -> (package, harness filters, which Kani matches as substrings
# of the fully qualified harness name, as `check` does). Local measured CBMC times
# (Kani 0.68.0, this toolbox) are in docs/next/formal-verification.md.
SHARDS: dict[str, tuple[str, tuple[str, ...]]] = {
    "widgets-geometry": (
        "gtk-lush-widgets",
        (
            "kani_proofs::no_input_panics",
            "kani_proofs::whole_pixel_band_",
            "kani_proofs::slice_lies_",
            "kani_proofs::slice_covers_",
            "kani_proofs::slice_containment_",
            "kani_proofs::resting_bin_",
            "kani_proofs::requests_are_",
            "kani_proofs::request_lands_",
            "kani_proofs::request_landing_",
        ),
    ),
    "widgets-slice-loop-rest": (
        "gtk-lush-widgets",
        ("kani_proofs::slice_loop_rests_",),
    ),
    "widgets-slice-loop-requests": (
        "gtk-lush-widgets",
        (
            "kani_proofs::slice_loop_honours_",
            "kani_proofs::slice_loop_learning_frame_",
            "kani_proofs::slice_loop_one_reconfiguring_",
            "kani_proofs::slice_loop_two_reconfiguring_",
        ),
    ),
    "core-journal-and-write": (
        "lushtext-core",
        (
            "services::draft_service::kani_proofs::journal_",
            "services::draft_service::kani_proofs::a_dirty_editor_",
            "services::filesystem::write_protocol::kani_proofs::",
        ),
    ),
    "core-second-writer": (
        "lushtext-core",
        ("services::draft_service::kani_proofs::a_second_writer_",),
    ),
}

PROOF_RE = re.compile(r"#\[kani::proof\][^\n]*\n(?:\s*#\[[^\n]*\n)*\s*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)")


def module_path(source_root: Path, path: Path) -> str:
    """The module path of a source file, relative to its crate root."""
    parts = list(path.relative_to(source_root).with_suffix("").parts)
    if parts[-1] in ("mod", "lib"):
        parts.pop()
    return "::".join(parts)


def discover() -> tuple[dict[str, list[str]], list[Path]]:
    """Every harness in the tree, as `package -> [kani names]`, plus the files
    holding harnesses outside every listed package (which no shard can run)."""
    found: dict[str, list[str]] = {package: [] for package in PACKAGES}
    unowned = []
    for path in sorted(CRATES.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        if "kani::proof" not in text or not PROOF_RE.search(text):
            continue
        owner = next(
            ((package, root) for package, root in PACKAGES.items() if path.is_relative_to(root)),
            None,
        )
        if owner is None:
            unowned.append(path)
            continue
        package, root = owner
        prefix = module_path(root, path)
        for match in PROOF_RE.finditer(text):
            found[package].append(f"{prefix}::{match.group(1)}" if prefix else match.group(1))
    return found, unowned


def kani_version() -> str:
    """The pinned Kani version, from the Makefile."""
    match = KANI_VERSION_RE.search(MAKEFILE.read_text(encoding="utf-8"))
    if match is None:
        raise SystemExit("kani-shards: no `KANI_VERSION ?=` line in the Makefile")
    return match.group(1)


def check(found: dict[str, list[str]], unowned: list[Path]) -> list[str]:
    """Every harness matches exactly one shard of its own package."""
    problems = [
        f"{path.relative_to(REPO_ROOT)} holds Kani harnesses outside every package in PACKAGES"
        for path in unowned
    ]
    for package, names in found.items():
        for name in names:
            owners = [
                shard
                for shard, (shard_package, prefixes) in SHARDS.items()
                if shard_package == package and any(p in name for p in prefixes)
            ]
            if len(owners) != 1:
                problems.append(f"{package} harness {name} matches shards {owners or 'none'}")
    for shard, (package, prefixes) in SHARDS.items():
        for prefix in prefixes:
            if not any(prefix in name for name in found.get(package, [])):
                problems.append(f"shard {shard} prefix {prefix} matches no harness")
    return problems


def run(shard_names: list[str], target_dir: str) -> int:
    for shard in shard_names:
        package, prefixes = SHARDS[shard]
        command = ["cargo", "kani", "-p", package, "--target-dir", target_dir]
        for prefix in prefixes:
            command += ["--harness", prefix]
        print(f"Running Kani shard {shard}: {' '.join(command)}", flush=True)
        status = subprocess.run(command, cwd=REPO_ROOT, check=False).returncode
        if status != 0:
            print(f"Kani shard {shard} failed with status {status}", file=sys.stderr)
            return status
    return 0


def self_test() -> None:
    root = Path("/crate/src")
    assert module_path(root, root / "kani_proofs.rs") == "kani_proofs"
    assert module_path(root, root / "a/b/mod.rs") == "a::b"
    text = "#[kani::proof]\n#[kani::should_panic]\n#[kani::unwind(4)]\nfn one() {}\n#[kani::proof]\npub fn two() {}\n"
    assert [m.group(1) for m in PROOF_RE.finditer(text)] == ["one", "two"]
    assert KANI_VERSION_RE.search("X ?= 1\nKANI_VERSION ?= 0.68.0\n").group(1) == "0.68.0"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("command", choices=["list", "check", "github-outputs", "run"])
    parser.add_argument("shard", nargs="?", default="all")
    parser.add_argument("--target-dir", default="target/kani")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    found, unowned = discover()
    if args.command == "list":
        for shard, (package, prefixes) in SHARDS.items():
            names = [n for n in found[package] if any(p in n for p in prefixes)]
            print(f"{shard} ({package}): {len(names)} harnesses")
            for name in names:
                print(f"  {name}")
        return 0
    problems = check(found, unowned)
    if problems:
        for problem in problems:
            print(f"kani-shards: {problem}", file=sys.stderr)
        return 1
    if args.command == "check":
        total = sum(len(names) for names in found.values())
        print(f"kani shard table passed: {total} harnesses, each in exactly one of {len(SHARDS)} shards")
        return 0
    if args.command == "github-outputs":
        print(f"shards={json.dumps(list(SHARDS))}")
        print(f"kani-version={kani_version()}")
        return 0
    if args.shard == "all":
        return run(list(SHARDS), args.target_dir)
    if args.shard not in SHARDS:
        print(f"unknown shard {args.shard}; known: {', '.join(SHARDS)}", file=sys.stderr)
        return 2
    return run([args.shard], args.target_dir)


if __name__ == "__main__":
    sys.exit(main())
