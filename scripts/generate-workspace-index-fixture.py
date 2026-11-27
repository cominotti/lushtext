#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Generate a monorepo-shaped workspace tree for the command palette's file index.

`scale-file-index` design D9: a seeded, deterministic tree of empty files whose
directory depth, files per directory and name lengths follow the shape
measured on real checkouts plus a published Chromium-scale shape (about ten
files per directory, depth up to ten), with one symlinked directory and one
symlinked file. Everything is synthetic: no user file is ever read or copied.

The algorithm is shared, step for step, with the Rust fixture
`lushtext_services::palette::workspace_index_fixture`, so a Criterion
bench, a unit test and the responsiveness lane index the same tree for the
same `--files` and `--seed`. `--self-test` checks the determinism, the counts
and the shape, and runs from `make check-responsiveness-lane`.

    generate-workspace-index-fixture.py --files 131072 --out DIR [--seed N] [--no-symlinks]
    generate-workspace-index-fixture.py --self-test
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
from pathlib import Path

MASK = (1 << 64) - 1
DEFAULT_SEED = 0x5CA1E_F11E

# Directory and file stems in the style of a large C++/Rust/Python monorepo.
WORDS = (
    "base", "browser", "renderer", "layout", "editing", "network", "content",
    "views", "frame", "autofill", "storage", "service", "public", "common",
    "core", "platform", "media", "gpu", "input", "test", "utils", "android",
    "devtools", "metrics", "policy", "sync", "history", "bookmarks", "ash",
    "wayland", "compositor", "sandbox",
)
EXTENSIONS = ("cc", "h", "rs", "py", "md", "json", "ts", "gn", "mojom", "java")

# The deepest directory below the workspace folder, and the shallowest a new
# branch aims for (design D9: depth 6–10).
MAX_DEPTH = 10
MIN_TARGET_DEPTH = 6
# Files per created directory: uniform in 0..FILES_PER_DIRECTORY_SPAN, mean 9.5.
FILES_PER_DIRECTORY_SPAN = 20
# A new branch returns to the workspace root with probability 1/BRANCH_ROLL and
# to the first level with probability SECOND_LEVEL_ROLLS/BRANCH_ROLL; otherwise
# it climbs one to three levels. That gives a few dozen top-level directories at
# 2^17 files and subtrees of thousands of files, as a monorepo has.
BRANCH_ROLL = 256
SECOND_LEVEL_ROLLS = 8
# Symlinks added at the workspace root.
LINKED_DIRECTORY = "linked_dir"
LINKED_FILE = "linked_file.txt"


class SplitMix64:
    """The PRNG the Rust fixture uses (`splitmix64`)."""

    def __init__(self, seed: int) -> None:
        self.state = seed & MASK

    def next(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)

    def below(self, bound: int) -> int:
        return self.next() % bound


def base36(value: int) -> str:
    digits = "0123456789abcdefghijklmnopqrstuvwxyz"
    if value == 0:
        return "0"
    out = []
    while value:
        value, digit = divmod(value, 36)
        out.append(digits[digit])
    return "".join(reversed(out))


def shape(files: int, seed: int):
    """Yield ("dir", relative) and ("file", relative) entries, parents first."""
    rng = SplitMix64(seed)
    stack: list[str] = []
    serial = 0
    emitted = 0
    while emitted < files:
        roll = rng.below(BRANCH_ROLL)
        if roll == 0:
            keep = 0
        elif roll <= SECOND_LEVEL_ROLLS:
            keep = min(1, len(stack))
        else:
            keep = max(0, len(stack) - (1 + rng.below(3)))
        del stack[keep:]
        target = max(MIN_TARGET_DEPTH + rng.below(MAX_DEPTH - MIN_TARGET_DEPTH + 1), keep + 1)
        while len(stack) < target and emitted < files:
            stack.append(f"{WORDS[rng.below(len(WORDS))]}_{base36(serial)}")
            serial += 1
            directory = "/".join(stack)
            yield ("dir", directory)
            count = min(rng.below(FILES_PER_DIRECTORY_SPAN), files - emitted)
            for ordinal in range(count):
                stem = WORDS[rng.below(len(WORDS))]
                if rng.below(4) != 0:
                    stem = f"{stem}_{WORDS[rng.below(len(WORDS))]}"
                extension = EXTENSIONS[rng.below(len(EXTENSIONS))]
                yield ("file", f"{directory}/{stem}_{ordinal}.{extension}")
                emitted += 1


def generate(out: Path, files: int, seed: int, symlinks: bool) -> dict[str, object]:
    out.mkdir(parents=True, exist_ok=True)
    directories = 0
    name_bytes = 0
    first_directory = None
    first_file = None
    for kind, relative in shape(files, seed):
        path = out / relative
        if kind == "dir":
            path.mkdir()
            directories += 1
            if first_directory is None:
                first_directory = relative
        else:
            os.close(os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o644))
            name_bytes += len(path.name.encode("utf-8"))
            if first_file is None:
                first_file = relative
    if symlinks and first_directory is not None and first_file is not None:
        os.symlink(first_directory.split("/")[0], out / LINKED_DIRECTORY)
        os.symlink(first_file, out / LINKED_FILE)
    return {
        "root": str(out),
        "files": files,
        "directories": directories,
        "average_file_name_bytes": round(name_bytes / max(files, 1), 2),
        "seed": seed,
        "symlinks": bool(symlinks),
    }


def tree_digest(root: Path) -> str:
    digest = hashlib.sha256()
    for directory, names, leaves in sorted(os.walk(root)):
        names.sort()
        rel = os.path.relpath(directory, root)
        for name in sorted(leaves) + names:
            entry = Path(directory) / name
            target = os.readlink(entry) if entry.is_symlink() else ""
            digest.update(f"{rel}/{name}->{target}\n".encode())
    return digest.hexdigest()


# FNV-1a 64 of the 4096-file shape for DEFAULT_SEED, one "kind:path\n" line per
# entry. The Rust fixture's unit test asserts the same digest, so the two
# implementations cannot drift apart.
SHAPE_DIGEST_4096 = 0xA8CAF9524F0EC2F8


def shape_digest(files: int, seed: int) -> int:
    value = 0xCBF29CE484222325
    for kind, relative in shape(files, seed):
        for byte in f"{kind}:{relative}\n".encode("utf-8"):
            value ^= byte
            value = (value * 0x100000001B3) & MASK
    return value


def self_test() -> int:
    failures = []
    if shape_digest(4096, DEFAULT_SEED) != SHAPE_DIGEST_4096:
        failures.append("the shape no longer matches the Rust fixture's digest")
    with tempfile.TemporaryDirectory(prefix="workspace-index-fixture-") as scratch:
        first = generate(Path(scratch) / "a", 4096, DEFAULT_SEED, True)
        second = generate(Path(scratch) / "b", 4096, DEFAULT_SEED, True)
        other = generate(Path(scratch) / "c", 4096, DEFAULT_SEED + 1, False)
        if tree_digest(Path(scratch) / "a") != tree_digest(Path(scratch) / "b"):
            failures.append("the same seed must give the same tree")
        if tree_digest(Path(scratch) / "a") == tree_digest(Path(scratch) / "c"):
            failures.append("another seed must give another tree")
        regular = 0
        deepest = 0
        for directory, _names, leaves in os.walk(Path(scratch) / "a"):
            rel = Path(directory).relative_to(Path(scratch) / "a")
            deepest = max(deepest, len(rel.parts))
            for leaf in leaves:
                if not (Path(directory) / leaf).is_symlink():
                    regular += 1
        if regular != 4096:
            failures.append(f"expected 4096 regular files, found {regular}")
        if not (Path(scratch) / "a" / LINKED_DIRECTORY).is_symlink():
            failures.append("the symlinked directory is missing")
        if not (Path(scratch) / "a" / LINKED_FILE).is_symlink():
            failures.append("the symlinked file is missing")
        if (Path(scratch) / "c" / LINKED_FILE).exists():
            failures.append("--no-symlinks must not add symlinks")
        if deepest > MAX_DEPTH:
            failures.append(f"depth {deepest} exceeds {MAX_DEPTH}")
        per_directory = first["files"] / max(first["directories"], 1)
        if not 7.0 <= per_directory <= 12.0:
            failures.append(f"files per directory {per_directory:.2f} is not about ten")
        if not 14.0 <= first["average_file_name_bytes"] <= 20.0:
            failures.append(
                f"average name {first['average_file_name_bytes']} B is not about 18 bytes"
            )
        if other["files"] != 4096:
            failures.append("the count must not depend on the seed")
    for failure in failures:
        print(f"workspace-index fixture self-test: {failure}", file=sys.stderr)
    if not failures:
        print("workspace-index fixture self-test: ok")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--files", type=int)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED)
    parser.add_argument("--no-symlinks", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.files is None or args.out is None or args.files < 0:
        parser.error("--files N and --out DIR are required")
    if args.out.exists() and any(args.out.iterdir()):
        parser.error(f"{args.out} exists and is not empty")
    summary = generate(args.out, args.files, args.seed, not args.no_symlinks)
    print(json.dumps(summary, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
