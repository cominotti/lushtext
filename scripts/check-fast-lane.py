#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Fast-lane eligibility from paths (.agents/rules/parallel-agent-work.md, "Fast lane").

    check-fast-lane.py --base REF        classify the change's paths against REF
    check-fast-lane.py --paths FILE      classify a planned path list (one per line; - for stdin)
    check-fast-lane.py --self-test

Every path gets one class from an allowlist that fails closed:

- eligible: under docs/ or openspec/, or a *.md file at the repository root
  other than the guidance files below;
- declare: agent guidance: .agents/rules/** except the two change-delivery
  rules, .agents/skills/**/*.md, .agents/skills/*/agents/*.yaml, AGENTS.md,
  SOUL.md, CLAUDE.md and .claude/CLAUDE.md. An eligible
  path that a gate names literally (its path string appears in the Makefile,
  .githooks/, .github/ or scripts/) is promoted to declare;
- not eligible: every other path, including any new directory, all of crates/,
  scripts/, .github/, .claude/workflows/, the Makefile (it wires every gate),
  the skill policy file, the Cargo files, and the change-delivery rules parallel-agent-work.md and
  verification-runs.md.

With --base, the paths are `git diff --name-status --find-renames REF...HEAD`
(both sides of a rename or copy), the uncommitted changes against HEAD, and
untracked files git does not ignore.

Exit 0 when every path is eligible or declare (it prints the declare paths,
which need a "Gate effect: none" section in proposal.md), 1 when any path is
not eligible, 2 on a usage or git error.
"""

from __future__ import annotations

import argparse
import importlib.util
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path, PurePosixPath

REPO_ROOT = Path(__file__).resolve().parent.parent
CHANGE_DELIVERY_RULES = {
    ".agents/rules/parallel-agent-work.md",
    ".agents/rules/verification-runs.md",
}
ROOT_GUIDANCE = {"AGENTS.md", "SOUL.md", "CLAUDE.md"}
DECLARE_FILES = {*ROOT_GUIDANCE, ".claude/CLAUDE.md"}
GATE_SOURCES = ("Makefile", ".githooks", ".github", "scripts")
SELF = "scripts/check-fast-lane.py"

ELIGIBLE, DECLARE, NOT_ELIGIBLE = "eligible", "declare", "not eligible"


def scrubbed_env() -> dict[str, str]:
    """The environment without the repository variables a git hook exports."""
    env = dict(os.environ)
    try:
        names = subprocess.run(["git", "rev-parse", "--local-env-vars"], check=True,
                               text=True, capture_output=True).stdout.split()
    except (OSError, subprocess.CalledProcessError):
        names = ["GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY",
                 "GIT_COMMON_DIR", "GIT_PREFIX"]
    for name in names:
        env.pop(name, None)
    return env


def git(repo: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(repo), *args], check=True, text=True,
                          capture_output=True, env=scrubbed_env()).stdout


def base_class(path: str) -> str:
    """The class from the path alone, before the gate-name promotion."""
    parts = PurePosixPath(path).parts
    if not parts or path.startswith("/") or ".." in parts:
        return NOT_ELIGIBLE
    if path in CHANGE_DELIVERY_RULES:
        return NOT_ELIGIBLE
    if path in DECLARE_FILES:
        return DECLARE
    if parts[0] == ".agents":
        if len(parts) >= 3 and parts[1] == "rules":
            return DECLARE
        if len(parts) >= 3 and parts[1] == "skills":
            if path.endswith(".md"):
                return DECLARE
            if len(parts) == 5 and parts[3] == "agents" and path.endswith(".yaml"):
                return DECLARE
        return NOT_ELIGIBLE
    if parts[0] in ("docs", "openspec") and len(parts) >= 2:
        return ELIGIBLE
    if len(parts) == 1 and path.endswith(".md"):
        return ELIGIBLE
    return NOT_ELIGIBLE


_GATE_TEXT: dict[Path, list[str]] = {}


def gate_texts(repo: Path) -> list[str]:
    """The text of every gate source in the working tree (tracked and untracked), cached per repo."""
    if repo not in _GATE_TEXT:
        sources = [source for source in GATE_SOURCES if (repo / source).exists()]
        texts: list[str] = []
        if sources:
            listed = git(repo, "ls-files", "--cached", "--others", "--exclude-standard", "--", *sources)
            for name in sorted(set(listed.splitlines())):
                if not name or name == SELF:
                    continue
                try:
                    texts.append((repo / name).read_text(encoding="utf-8", errors="replace"))
                except (IsADirectoryError, FileNotFoundError):
                    continue
        _GATE_TEXT[repo] = texts
    return _GATE_TEXT[repo]


def named_by_gate(repo: Path, path: str) -> bool:
    """Whether a gate source names the path literally, as a whole path.

    The path must not continue a longer path on either side, so
    `crates/x/README.md` does not name the root `README.md`; a leading `./`
    is allowed.
    """
    pattern = re.compile(r"(?<![A-Za-z0-9_.\-/])(?:\./)?" + re.escape(path) + r"(?![A-Za-z0-9_\-/])")
    return any(pattern.search(text) for text in gate_texts(repo))


def classify(repo: Path, path: str) -> str:
    cls = base_class(path)
    if cls == ELIGIBLE and named_by_gate(repo, path):
        return DECLARE
    return cls


def changed_paths(repo: Path, base: str) -> list[str]:
    paths: set[str] = set()
    for diff in (git(repo, "diff", "--name-status", "--find-renames", f"{base}...HEAD"),
                 git(repo, "diff", "--name-status", "--find-renames", "HEAD")):
        for line in diff.splitlines():
            fields = line.split("\t")
            if len(fields) >= 2:
                paths.update(fields[1:])
    paths.update(line for line in git(repo, "ls-files", "--others", "--exclude-standard").splitlines() if line)
    return sorted(paths)


def report(repo: Path, paths: list[str]) -> int:
    classes = {path: classify(repo, path) for path in paths}
    blocked = [path for path, cls in classes.items() if cls == NOT_ELIGIBLE]
    declare = [path for path, cls in classes.items() if cls == DECLARE]
    for path in paths:
        print(f"{classes[path]}\t{path}")
    if not paths:
        print("fast-lane: no paths; nothing to classify")
        return 1
    if blocked:
        print(f"fast-lane: NOT ELIGIBLE ({len(blocked)} path(s) outside the allowlist)")
        return 1
    if declare:
        print(f"fast-lane: eligible with declarations: {len(declare)} path(s) need \"Gate effect: none\" in proposal.md")
    else:
        print("fast-lane: eligible")
    return 0


def self_test() -> int:
    failures: list[str] = []

    def expect(path: str, want: str, repo: Path = REPO_ROOT) -> None:
        got = classify(repo, path)
        if got != want:
            failures.append(f"{path}: {got}, expected {want}")

    for path in ("docs/memorials/2026-11-18-x.md", "openspec/changes/x/proposal.md",
                 "openspec/changes/x/specs/y/spec.md", "UNNAMED-NOTES.md"):
        expect(path, ELIGIBLE)
    for path in (".agents/rules/build.md", ".agents/skills/x/SKILL.md",
                 ".agents/skills/x/references/y.md", ".agents/skills/x/agents/openai.yaml",
                 "AGENTS.md", "SOUL.md", ".claude/CLAUDE.md"):
        expect(path, DECLARE)
    # The Makefile wires every gate (check, check-policy, the shard lists), so
    # it is a gate implementation, not guidance (Fable review S7).
    # Paths a gate names literally: check-automation-docs.py reads the first,
    # check-filesystem-boundary.sh scans the root README.md.
    expect("docs/automation-reference.md", DECLARE)
    expect("README.md", DECLARE)
    for path in ("crates/lushtext-ui/src/app.rs", "crates/lushtext-ui/src/ui/AGENTS.md",
                 "crates/lushtext/tests/widget/x.rs", "crates/gtk-lush/settle/src/lib.rs",
                 "crates/gtk-lush/proof-harness/src/lib.rs", "resources/ui/x.blp", "Cargo.lock",
                 "Cargo.toml", ".cargo/mutants.toml", ".gitignore",
                 ".agents/rules/parallel-agent-work.md", ".agents/rules/verification-runs.md",
                 "scripts/flatpak-source-tree.sh", "scripts/agent-topology.py",
                 ".agents/skills/skill-policy.toml", ".agents/skills/x/scripts/run.py",
                 ".claude/workflows/x.js", ".github/workflows/ci.yml", ".githooks/pre-commit",
                 "build-aux/cargo.sh", "Makefile", "newtop/file.md", "docs", "../docs/x.md", "/docs/x.md",
                 "crates/lushtext/README.md"):
        expect(path, NOT_ELIGIBLE)

    # Every Kani harness file and every ORACLES module is outside the allowlist.
    harnesses = [line for line in git(REPO_ROOT, "ls-files").splitlines()
                 if PurePosixPath(line).name.startswith("kani_proofs") and line.endswith(".rs")]
    if not harnesses:
        failures.append("no kani_proofs*.rs found in git ls-files")
    for path in harnesses:
        expect(path, NOT_ELIGIBLE)
    spec = importlib.util.spec_from_file_location("kani_shards", REPO_ROOT / "scripts" / "kani-shards.py")
    if spec is None or spec.loader is None:
        failures.append("cannot load scripts/kani-shards.py")
    else:
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
        if not module.ORACLES:
            failures.append("kani-shards.py ORACLES is empty")
        for path in module.ORACLES:
            expect(path, NOT_ELIGIBLE)

    # Diff collection: both sides of a rename, uncommitted and untracked files.
    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp) / "repo"
        env = dict(scrubbed_env(), GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1")

        def run(*args: str) -> None:
            subprocess.run(["git", "-C", str(repo), "-c", "core.hooksPath=/dev/null",
                            "-c", "user.name=t", "-c", "user.email=t@example.invalid",
                            "-c", "commit.gpgsign=false", *args],
                           check=True, capture_output=True, env=env)

        repo.mkdir()
        subprocess.run(["git", "init", "-q", "-b", "main", str(repo)], check=True, env=env)
        (repo / "docs").mkdir()
        (repo / "docs" / "x.md").write_text("one\ntwo\nthree\nfour\n", encoding="utf-8")
        run("add", "docs/x.md")
        run("commit", "-q", "-m", "base")
        run("checkout", "-q", "-b", "change")
        (repo / "scripts").mkdir()
        run("mv", "docs/x.md", "scripts/x.sh")
        run("commit", "-q", "-m", "rename")
        paths = changed_paths(repo, "main")
        if "docs/x.md" not in paths or "scripts/x.sh" not in paths:
            failures.append(f"a rename lists both sides, got {paths}")
        if classify(repo, "scripts/x.sh") != NOT_ELIGIBLE:
            failures.append("the rename's new side is not eligible")
        run("mv", "scripts/x.sh", "docs/x.md")
        run("commit", "-q", "-m", "back")
        (repo / "docs" / "new.md").write_text("new\n", encoding="utf-8")
        paths = changed_paths(repo, "main")
        if "docs/new.md" not in paths:
            failures.append(f"an untracked new file is listed, got {paths}")
        (repo / "newtop").mkdir()
        (repo / "newtop" / "a.txt").write_text("a\n", encoding="utf-8")
        if "newtop/a.txt" not in changed_paths(repo, "main") or classify(repo, "newtop/a.txt") != NOT_ELIGIBLE:
            failures.append("an untracked file in a new top-level directory is not eligible")
        (repo / "Makefile").write_text("check:\n\t./scripts/read docs/gated.md\n", encoding="utf-8")
        if classify(repo, "docs/gated.md") != DECLARE or classify(repo, "docs/free.md") != ELIGIBLE:
            failures.append("the gate-name promotion reads the working tree's Makefile")

    if failures:
        for failure in failures:
            print(f"check-fast-lane self-test FAILED: {failure}", file=sys.stderr)
        return 1
    print(f"check-fast-lane self-test: ok (allowlist classes, gate-named docs promoted, "
          f"{len(harnesses)} harness files and every ORACLES module not eligible, renames, "
          f"untracked and new-directory paths)")
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--base")
    group.add_argument("--paths")
    group.add_argument("--self-test", action="store_true")
    parser.add_argument("--repo", type=Path, default=REPO_ROOT)
    args = parser.parse_args(argv)
    try:
        if args.self_test:
            return self_test()
        if args.base:
            return report(args.repo, changed_paths(args.repo, args.base))
        text = sys.stdin.read() if args.paths == "-" else Path(args.paths).read_text(encoding="utf-8")
        return report(args.repo, sorted({line.strip() for line in text.splitlines() if line.strip()}))
    except (OSError, subprocess.CalledProcessError, RuntimeError) as error:
        print(f"check-fast-lane: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
