#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Decide whether a commit may skip the pre-commit hook's Clippy step.

Design D8 of openspec/changes/archive/2026-11-15-reduce-build-disk-writes.

Usage:
  scripts/precommit-scope.py docs-only                 # exit 0 only for a staged docs-only commit
  scripts/precommit-scope.py docs-only-range BASE HEAD # exit 0 only when BASE...HEAD is non-empty and all docs
  scripts/precommit-scope.py stamp-write               # record clean git write-tree stamp after make check
  scripts/precommit-scope.py stamp-valid               # exit 0 only when clean tree matches recent stamp
  scripts/precommit-scope.py check-includes            # fail when a Rust build input is allowlisted
  scripts/precommit-scope.py --self-test

docs-only exits 0 only when the commit is not a merge (`MERGE_HEAD` absent),
the index differs from HEAD in at least one path (`git diff --cached
--name-only --no-renames`, which lists both sides of a rename and deletions),
and every such path is on the documentation allowlist:

  docs/**, openspec/**, .claude/**, .agents/**, and *.md at the repository root.

Any other commit, an empty one, and any git error exit 1, so the hook falls
back to the full `make pre-commit`. It reads the index git hands the hook
(`GIT_INDEX_FILE`), so it sees exactly what is being committed.

check-includes keeps the allowlist honest: it fails when a compile-time input
of a Rust source under crates/, fuzz/ or workspace-hack/ resolves to an
allowlisted path, because a docs-only commit would then change what Clippy
reports. Compile-time inputs are the argument of include_str!, include_bytes!
and include! and a #[path = "..."] attribute, resolved against the source
file's directory as rustc does, and every string literal of a build.rs (which
covers cargo:rerun-if-changed and files the build script reads). An include
argument that is not one literal (concat!, env!) fails closed when any literal
in it names an allowlisted location. Runtime reads of documentation, and
string literals elsewhere, are not build inputs and stay allowed.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ALLOWED_DIRS = ("docs", "openspec", ".claude", ".agents")
SOURCE_ROOTS = ("crates", "fuzz", "workspace-hack")
GTK_LUSH_PREFIXES = (
    "crates/gtk-lush/",
    "crates/gtk-lush-adoption-lab/",
    "fixtures/gtk-lush-adoption/",
    ".cargo/",
    "workspace-hack/",
)
GTK_LUSH_EXACT = frozenset(
    {
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "Makefile",
        ".github/workflows/ci.yml",
        "scripts/run-gtk-axioms.sh",
        "scripts/check-gtk-axioms.py",
        "scripts/check-gtk-lush-policy.py",
        "scripts/check-gtk-lush-adoption.py",
        "scripts/install-cargo-public-api.sh",
        "scripts/headless-session",
        "scripts/bounded-log-scan.sh",
        "scripts/smoke_warning_classifiers.py",
        "scripts/with-temp-scope.sh",
        "scripts/precommit-scope.py",
    }
)
INCLUDE_MACROS = ("include_str", "include_bytes", "include")
STAMP_FILENAME = "lushtext-precommit-check-stamp"
STAMP_TTL_SECONDS = 1800


def is_gtk_lush_path(rel: str) -> bool:
    """Whether a path can affect GTK Lush crates, axioms, MSRV, or API surface."""
    norm = rel.replace("\\", "/")
    if norm in GTK_LUSH_EXACT:
        return True
    return norm.startswith(GTK_LUSH_PREFIXES)


def allowlisted(rel: str) -> bool:
    """Whether a repository-relative path is on the documentation allowlist."""
    rel = rel.replace("\\", "/")
    if rel.startswith("../") or rel.startswith("/"):
        return False
    # Runtime smoke helper scripts executed by run-accessibility-smoke.sh and
    # run-visual-smoke.sh must stay off the docs-only allowlist so verify-local
    # and pre-commit cannot short-circuit them.
    if rel.startswith(".agents/skills/gtk-agentic-debugging/scripts/"):
        return False
    first = rel.split("/", 1)[0]
    if first in ALLOWED_DIRS and "/" in rel:
        return True
    return "/" not in rel and rel.endswith(".md")


def _git_env() -> dict[str, str]:
    env = dict(os.environ)
    # Strip repo-locating overrides so `git -C <repo>` cannot follow a decoy
    # checkout, while keeping GIT_INDEX_FILE for partial-stage hook runs.
    for var in (
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_PREFIX",
    ):
        env.pop(var, None)
    return env


def git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, env=_git_env())


def docs_only(repo: Path) -> tuple[bool, str]:
    if git(repo, "rev-parse", "--is-inside-work-tree").returncode != 0:
        return False, "not inside a git work tree"
    if git(repo, "rev-parse", "-q", "--verify", "MERGE_HEAD").returncode == 0:
        return False, "a merge is in progress"
    diff = git(repo, "diff", "--cached", "--name-only", "--no-renames", "-z")
    if diff.returncode != 0:
        return False, "git diff --cached failed: " + diff.stderr.decode(errors="replace").strip()
    paths = [os.fsdecode(p) for p in diff.stdout.split(b"\0") if p]
    if not paths:
        return False, "nothing is staged"
    outside = [p for p in paths if not allowlisted(p)]
    if outside:
        return False, f"{outside[0]} is outside the documentation allowlist"
    return True, f"{len(paths)} staged path(s), all documentation"


def docs_only_range(repo: Path, base_ref: str, head_ref: str) -> tuple[bool, str]:
    if not base_ref or not head_ref:
        return False, "base and head refs are required"
    if git(repo, "rev-parse", "--is-inside-work-tree").returncode != 0:
        return False, "not inside a git work tree"
    if (
        git(repo, "rev-parse", "-q", "--verify", f"{base_ref}^{{commit}}").returncode != 0
        or git(repo, "rev-parse", "-q", "--verify", f"{head_ref}^{{commit}}").returncode != 0
    ):
        return False, "base or head commit could not be resolved"
    diff = git(repo, "diff", "--name-only", "--no-renames", "-z", f"{base_ref}...{head_ref}")
    if diff.returncode != 0:
        # Shallow clones may lack a merge-base for `...`; fallback to two-dot diff only then.
        if git(repo, "merge-base", base_ref, head_ref).returncode != 0:
            diff = git(repo, "diff", "--name-only", "--no-renames", "-z", base_ref, head_ref)
    if diff.returncode != 0:
        return False, "git diff range failed: " + diff.stderr.decode(errors="replace").strip()
    paths = [os.fsdecode(p) for p in diff.stdout.split(b"\0") if p]
    if not paths:
        return False, "range is empty"
    outside = [p for p in paths if not allowlisted(p)]
    if outside:
        return False, f"{outside[0]} is outside the documentation allowlist"
    return True, f"{len(paths)} changed path(s) in range, all documentation"


def gtk_lush_changed_range(repo: Path, base_ref: str, head_ref: str) -> tuple[bool, str]:
    """Fail-closed: return True on any gtk-lush path, empty diff, or git error."""
    if not base_ref or not head_ref:
        return True, "base and head refs are required"
    if git(repo, "rev-parse", "--is-inside-work-tree").returncode != 0:
        return True, "not inside a git work tree"
    if (
        git(repo, "rev-parse", "-q", "--verify", f"{base_ref}^{{commit}}").returncode != 0
        or git(repo, "rev-parse", "-q", "--verify", f"{head_ref}^{{commit}}").returncode != 0
    ):
        return True, "base or head commit could not be resolved"
    diff = git(repo, "diff", "--name-only", "--no-renames", "-z", f"{base_ref}...{head_ref}")
    if diff.returncode != 0 and git(repo, "merge-base", base_ref, head_ref).returncode != 0:
        diff = git(repo, "diff", "--name-only", "--no-renames", "-z", base_ref, head_ref)
    if diff.returncode != 0:
        return True, "git diff range failed: " + diff.stderr.decode(errors="replace").strip()
    paths = [os.fsdecode(p) for p in diff.stdout.split(b"\0") if p]
    if not paths:
        return True, "range is empty"
    matched = [p for p in paths if is_gtk_lush_path(p)]
    if matched:
        return True, f"{matched[0]} affects gtk-lush"
    return False, f"{len(paths)} changed path(s) in range, none affect gtk-lush"


def _porcelain_paths(repo: Path) -> tuple[bool, list[str]]:
    res = git(repo, "status", "--porcelain=v1", "--untracked-files=all", "-z")
    if res.returncode != 0:
        return False, []
    entries = res.stdout.split(b"\0")
    out: list[str] = []
    i = 0
    while i < len(entries):
        entry = entries[i]
        i += 1
        if not entry:
            continue
        if len(entry) < 4:
            return False, []
        xy = entry[:2].decode(errors="replace")
        path = os.fsdecode(entry[3:])
        out.append(path)
        if "R" in xy or "C" in xy:
            if i < len(entries) and entries[i]:
                out.append(os.fsdecode(entries[i]))
            i += 1
    return True, out


def gtk_lush_changed_worktree(repo: Path, base_ref: str | None = None) -> tuple[bool, str]:
    """Fail-closed worktree + optional branch diff query for GTK Lush scope."""
    if git(repo, "rev-parse", "--is-inside-work-tree").returncode != 0:
        return True, "not inside a git work tree"
    ok_wt, paths = _porcelain_paths(repo)
    if not ok_wt:
        return True, "failed to inspect working tree status"
    all_paths = list(paths)
    if base_ref:
        if git(repo, "rev-parse", "-q", "--verify", f"{base_ref}^{{commit}}").returncode != 0:
            return True, f"base ref '{base_ref}' could not be resolved"
        diff = git(repo, "diff", "--name-only", "--no-renames", "-z", f"{base_ref}...HEAD")
        if diff.returncode != 0 and git(repo, "merge-base", base_ref, "HEAD").returncode != 0:
            diff = git(repo, "diff", "--name-only", "--no-renames", "-z", base_ref, "HEAD")
        if diff.returncode != 0:
            return True, "git diff failed"
        all_paths.extend(os.fsdecode(p) for p in diff.stdout.split(b"\0") if p)
    matched = [p for p in all_paths if is_gtk_lush_path(p)]
    if matched:
        return True, f"{matched[0]} affects gtk-lush"
    return False, f"{len(all_paths)} changed path(s), none affect gtk-lush"


def stamp_path(repo: Path) -> Path:
    res = git(repo, "rev-parse", "--path-format=absolute", "--git-dir")
    if res.returncode != 0:
        raise RuntimeError(res.stderr.decode(errors="replace").strip() or "git-dir unavailable")
    return Path(res.stdout.decode().strip()) / STAMP_FILENAME


def _remove_stamp(path: Path) -> None:
    try:
        path.unlink()
    except FileNotFoundError:
        pass


def _clean_index_tree(repo: Path) -> tuple[bool, str, str | None]:
    if git(repo, "rev-parse", "--is-inside-work-tree").returncode != 0:
        return False, "not inside a git work tree", None
    if git(repo, "rev-parse", "-q", "--verify", "MERGE_HEAD").returncode == 0:
        return False, "a merge is in progress", None
    if git(repo, "diff", "--quiet").returncode != 0:
        return False, "working tree has unstaged edits", None
    untracked = git(repo, "ls-files", "--others", "--exclude-standard", "-z")
    if untracked.returncode != 0:
        return False, "failed to inspect untracked files", None
    if untracked.stdout:
        return False, "working tree has untracked files", None
    wt = git(repo, "write-tree")
    if wt.returncode != 0:
        return False, "git write-tree failed", None
    return True, "clean", wt.stdout.decode().strip()


def _working_tree_hash(repo: Path) -> tuple[bool, str, str | None]:
    """Hash all tracked and non-ignored untracked files via a throwaway index."""
    if git(repo, "rev-parse", "--is-inside-work-tree").returncode != 0:
        return False, "not inside a git work tree", None
    if git(repo, "rev-parse", "-q", "--verify", "MERGE_HEAD").returncode == 0:
        return False, "a merge is in progress", None
    with tempfile.TemporaryDirectory() as tmp:
        env = _git_env()
        env["GIT_INDEX_FILE"] = str(Path(tmp) / "index")
        add = subprocess.run(
            ["git", "-C", str(repo), "add", "-A"],
            capture_output=True,
            env=env,
        )
        if add.returncode != 0:
            return False, "failed to hash working tree", None
        wt = subprocess.run(
            ["git", "-C", str(repo), "write-tree"],
            capture_output=True,
            env=env,
        )
        if wt.returncode != 0:
            return False, "git write-tree failed on working tree", None
    return True, "hashed", wt.stdout.decode().strip()


def stamp_write(repo: Path) -> tuple[bool, str]:
    try:
        sp = stamp_path(repo)
    except RuntimeError as exc:
        return False, str(exc)
    ok, why, tree_sha = _working_tree_hash(repo)
    if not ok or tree_sha is None:
        _remove_stamp(sp)
        return False, f"no stamp written ({why})"
    sp.write_text(f"{tree_sha}\t{int(time.time())}\n", encoding="utf-8")
    return True, f"stamped working tree {tree_sha[:12]}"


def stamp_valid(repo: Path, now: int | None = None) -> tuple[bool, str]:
    if os.environ.get("LUSHTEXT_SKIP_PRECOMMIT_STAMP") == "1":
        return False, "LUSHTEXT_SKIP_PRECOMMIT_STAMP=1"
    try:
        sp = stamp_path(repo)
    except RuntimeError as exc:
        return False, str(exc)
    ok, why, tree_sha = _clean_index_tree(repo)
    if not ok or tree_sha is None:
        return False, why
    if not sp.is_file():
        return False, "no pre-commit stamp found"
    try:
        raw = sp.read_text(encoding="utf-8").strip()
        recorded_tree, recorded_epoch_str = raw.split("\t", 1)
        recorded_epoch = int(recorded_epoch_str)
    except (OSError, ValueError):
        return False, "malformed pre-commit stamp"
    if recorded_tree != tree_sha:
        return False, "index tree changed since stamp"
    current_time = int(time.time()) if now is None else now
    age = current_time - recorded_epoch
    if age < 0 or age > STAMP_TTL_SECONDS:
        return False, f"pre-commit stamp expired (age={age}s)"
    return True, f"matches clean tree {tree_sha[:12]} (age={age}s)"


# --- check-includes -------------------------------------------------------------

TOKEN = re.compile(
    r"""
    (?P<ws>\s+)
  | (?P<line_comment>//[^\n]*)
  | (?P<block_comment>/\*)
  | (?P<raw>b?r(?P<hashes>\#*)")
  | (?P<str>b?"(?:\\.|[^"\\])*")
  | (?P<char>b?'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^'\\\n])')
  | (?P<lifetime>'[A-Za-z_][A-Za-z0-9_]*)
  | (?P<ident>[A-Za-z_][A-Za-z0-9_]*)
  | (?P<number>[0-9][0-9A-Za-z_.]*)
  | (?P<punct>.)
    """,
    re.X | re.S,
)


def unescape(body: str) -> str:
    return re.sub(r"\\(.)", lambda m: {"n": "\n", "t": "\t", "0": "\0"}.get(m.group(1), m.group(1)), body)


def tokens(text: str) -> list[tuple[str, str]]:
    """A small Rust lexer: (kind, value), comments dropped, string values unescaped."""
    out: list[tuple[str, str]] = []
    pos = 0
    while pos < len(text):
        m = TOKEN.match(text, pos)
        kind = m.lastgroup if m.lastgroup != "hashes" else "raw"
        if m.group("raw") is not None:
            kind = "raw"
        if kind == "block_comment":
            depth, pos = 1, m.end()
            while depth and pos < len(text):
                if text.startswith("/*", pos):
                    depth, pos = depth + 1, pos + 2
                elif text.startswith("*/", pos):
                    depth, pos = depth - 1, pos + 2
                else:
                    pos += 1
            continue
        if kind == "raw":
            close = '"' + m.group("hashes")
            end = text.find(close, m.end())
            end = len(text) if end < 0 else end
            out.append(("str", text[m.end() : end]))
            pos = end + len(close)
            continue
        if kind == "str":
            body = m.group(0)
            out.append(("str", unescape(body[body.index('"') + 1 : -1])))
        elif kind not in ("ws", "line_comment"):
            out.append((kind, m.group(0)))
        pos = m.end()
    return out


def names_allowlisted(literal: str) -> bool:
    """Whether a literal names an allowlisted location anywhere in it."""
    parts = re.split(r"[/\\=]", literal)
    return any(p in ALLOWED_DIRS for p in parts) or literal.endswith(".md")


def resolve(repo: Path, source: Path, literal: str) -> str | None:
    """The repository-relative path a literal resolves to from `source`'s directory."""
    if os.path.isabs(literal):
        return None
    full = os.path.normpath(os.path.join(str(source.parent), literal))
    rel = os.path.relpath(full, str(repo))
    return None if rel.startswith("..") else rel


def include_findings(repo: Path, source: Path, text: str) -> list[str]:
    findings = []
    toks = tokens(text)
    rel_source = source.relative_to(repo)
    for i, (kind, value) in enumerate(toks):
        # include_str!( ... ), balanced parentheses
        if kind == "ident" and value in INCLUDE_MACROS and toks[i + 1 : i + 3] == [("punct", "!"), ("punct", "(")]:
            depth, j, inner = 1, i + 3, []
            while j < len(toks) and depth:
                if toks[j] == ("punct", "("):
                    depth += 1
                elif toks[j] == ("punct", ")"):
                    depth -= 1
                if depth:
                    inner.append(toks[j])
                j += 1
            literals = [v for k, v in inner if k == "str"]
            if len(inner) == 1 and literals:
                target = resolve(repo, source, literals[0])
                if target is not None and allowlisted(target):
                    findings.append(f"{rel_source}: {value}!(\"{literals[0]}\") includes {target}")
            elif any(names_allowlisted(lit) for lit in literals):
                findings.append(
                    f"{rel_source}: {value}! built from {literals} names an allowlisted location and cannot be resolved"
                )
        # #[path = "..."] / #![path = "..."]
        if kind == "punct" and value == "#":
            j = i + 1
            if toks[j : j + 1] == [("punct", "!")]:
                j += 1
            seq = toks[j : j + 5]
            if (
                len(seq) == 5
                and seq[0] == ("punct", "[")
                and seq[1] == ("ident", "path")
                and seq[2] == ("punct", "=")
                and seq[3][0] == "str"
                and seq[4] == ("punct", "]")
            ):
                target = resolve(repo, source, seq[3][1])
                if target is not None and allowlisted(target):
                    findings.append(f"{rel_source}: #[path = \"{seq[3][1]}\"] names {target}")
    if source.name == "build.rs":
        for kind, value in toks:
            if kind != "str":
                continue
            literal = re.sub(r"^cargo::?[a-z-]+=", "", value)
            target = resolve(repo, source, literal)
            if names_allowlisted_dirs(literal) or (target is not None and allowlisted(target)):
                findings.append(f"{rel_source}: build script names \"{value}\", an allowlisted location")
    return findings


def names_allowlisted_dirs(literal: str) -> bool:
    return any(p in ALLOWED_DIRS for p in re.split(r"[/\\]", literal))


def rust_sources(repo: Path) -> list[Path]:
    out = []
    for root in SOURCE_ROOTS:
        base = repo / root
        if not base.is_dir():
            continue
        for dirpath, dirnames, filenames in os.walk(base):
            dirnames[:] = [d for d in dirnames if d not in ("target", ".git")]
            out += [Path(dirpath) / f for f in filenames if f.endswith(".rs")]
    return sorted(out)


def check_includes(repo: Path) -> list[str]:
    findings = []
    for source in rust_sources(repo):
        findings += include_findings(repo, source, source.read_text(encoding="utf-8", errors="replace"))
    return findings


# --- self-test ------------------------------------------------------------------


def self_test() -> None:
    for name in ("GIT_DIR", "GIT_INDEX_FILE", "GIT_WORK_TREE", "GIT_PREFIX", "GIT_OBJECT_DIRECTORY", "GIT_COMMON_DIR"):
        os.environ.pop(name, None)

    def run(repo: Path, *args: str) -> None:
        subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True)

    def fresh(scratch: Path, name: str) -> Path:
        repo = scratch / name
        run(scratch, "init", "-q", str(repo))
        for key, value in (("user.name", "t"), ("user.email", "t@example.invalid"), ("commit.gpgsign", "false")):
            run(repo, "config", key, value)
        for rel in ("docs/a.md", "openspec/x.md", "README.md", ".claude/notes.md", "crates/c/README.md", "crates/c/src/lib.rs"):
            (repo / rel).parent.mkdir(parents=True, exist_ok=True)
            (repo / rel).write_text("x\n")
        run(repo, "add", "-A")
        run(repo, "commit", "-q", "-m", "fixture")
        return repo

    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp)
        cases = [
            ("docs/ edit", lambda r: (r / "docs/a.md").write_text("y\n"), True),
            ("openspec/ edit", lambda r: (r / "openspec/x.md").write_text("y\n"), True),
            ("root *.md edit", lambda r: (r / "README.md").write_text("y\n"), True),
            (".claude/ deletion", lambda r: (r / ".claude/notes.md").unlink(), True),
            ("crates/**/*.md edit", lambda r: (r / "crates/c/README.md").write_text("y\n"), False),
            (".rs edit", lambda r: (r / "crates/c/src/lib.rs").write_text("y\n"), False),
            (".rs deletion", lambda r: (r / "crates/c/src/lib.rs").unlink(), False),
            ("rename out of docs/", lambda r: (r / "docs/a.md").rename(r / "crates/a.md"), False),
            ("docs/ edit plus .rs edit", lambda r: [(r / "docs/a.md").write_text("y\n"), (r / "crates/c/src/lib.rs").write_text("y\n")], False),
            ("empty index", lambda r: None, False),
        ]
        for n, (label, edit, expected) in enumerate(cases):
            repo = fresh(scratch, f"r{n}")
            edit(repo)
            run(repo, "add", "-A")
            got, why = docs_only(repo)
            assert got == expected, f"{label}: docs_only={got} ({why})"
        # A merge in progress always runs the full gate.
        repo = fresh(scratch, "merge")
        (repo / "docs/a.md").write_text("y\n")
        run(repo, "add", "-A")
        head = subprocess.run(["git", "-C", str(repo), "rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
        (repo / ".git/MERGE_HEAD").write_text(head + "\n")
        assert docs_only(repo) == (False, "a merge is in progress")
        # A git error (no repository) refuses.
        not_repo = scratch / "plain"
        not_repo.mkdir()
        assert docs_only(not_repo)[0] is False
        # The command exits 1 on refusal, 0 on a docs-only commit.
        me = [sys.executable, str(Path(__file__).resolve()), "docs-only"]
        assert subprocess.run(me, cwd=not_repo, capture_output=True).returncode == 1
        ok = fresh(scratch, "cmd")
        (ok / "docs/a.md").write_text("z\n")
        run(ok, "add", "-A")
        assert subprocess.run(me, cwd=ok, capture_output=True).returncode == 0

        # check-includes: compile-time inputs that resolve into the allowlist fail.
        fixture = scratch / "tree"
        (fixture / "crates/c/src").mkdir(parents=True)
        (fixture / "docs").mkdir()
        (fixture / "docs/x.md").write_text("x\n")
        (fixture / "README.md").write_text("x\n")
        cases = {
            'pub const X: &str = include_str!("../../docs/x.md");\n': True,
            'pub const X: &str = include_str!(\n    "../../docs/x.md"\n);\n': True,
            '#![doc = include_str!("../../README.md")]\n': True,
            '#[path = "../../docs/m.rs"]\nmod m;\n': True,
            '#![doc = include_str!("../README.md")]\n': False,
            'const X: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/x.md"));\n': True,
            'const X: &str = include_str!(r#"../../openspec/a.md"#);\n': True,
            # Not compile-time inputs: runtime reads, plain literals, comments.
            'fn f() { let _ = std::fs::read_to_string("docs/x.md"); }\n': False,
            '// include_str!("../../docs/x.md")\nfn g() {}\n': False,
            'const S: &str = "include_str!(\\"../../docs/x.md\\")";\n': False,
            'const X: &str = include_str!("../fixtures/a.json");\n': False,
            'const X: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/r.gresource"));\n': False,
            "fn h<'a>(x: &'a str) -> char { let _ = x; 'x' }\n": False,
        }
        for n, (source, expected) in enumerate(cases.items()):
            path = fixture / f"crates/c/case{n}.rs"  # crates/c/, so ../../ is the root
            path.write_text(source)
            got = include_findings(fixture, path, source)
            assert bool(got) == expected, f"case {source!r}: {got}"
            path.unlink()
        build = fixture / "crates/c/build.rs"
        for source, expected in (
            ('fn main() { println!("cargo:rerun-if-changed=../../openspec/specs"); }\n', True),
            ('fn main() { println!("cargo:rerun-if-changed=../../README.md"); }\n', True),
            ('fn main() { println!("cargo:rerun-if-changed=README.md"); }\n', False),
            ('fn main() { println!("cargo:rerun-if-changed=../../resources"); }\n', False),
        ):
            build.write_text(source)
            got = include_findings(fixture, build, source)
            assert bool(got) == expected, f"build.rs {source!r}: {got}"
        build.unlink()

    # The current tree passes.
    findings = check_includes(REPO_ROOT)
    assert not findings, findings

    # Failing-first tests for docs-only-range and stamp-write / stamp-valid.
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp)
        range_repo = fresh(scratch, "range")
        base_sha = subprocess.run(
            ["git", "-C", str(range_repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        (range_repo / "docs/a.md").write_text("docs only change\n")
        run(range_repo, "add", "docs/a.md")
        run(range_repo, "commit", "-q", "-m", "docs")
        docs_head = subprocess.run(
            ["git", "-C", str(range_repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        ok_range, why_range = docs_only_range(range_repo, base_sha, docs_head)
        assert ok_range, f"docs-only range failed: {why_range}"

        # Mixed commit (docs + Rust) -> False.
        (range_repo / "crates/c/src/lib.rs").write_text("pub fn z() {}\n")
        run(range_repo, "add", "crates/c/src/lib.rs")
        run(range_repo, "commit", "-q", "-m", "rust")
        mixed_head = subprocess.run(
            ["git", "-C", str(range_repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        ok_mixed, _ = docs_only_range(range_repo, base_sha, mixed_head)
        assert not ok_mixed, "mixed range unexpectedly passed docs_only_range"

        # Empty range / invalid ref -> False (fail-closed).
        assert not docs_only_range(range_repo, mixed_head, mixed_head)[0]
        assert not docs_only_range(range_repo, "0" * 40, mixed_head)[0]

        # Rename out of docs/ -> False.
        rename_repo = fresh(scratch, "rename")
        r_base = subprocess.run(
            ["git", "-C", str(rename_repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        (rename_repo / "docs/a.md").rename(rename_repo / "crates/moved.md")
        run(rename_repo, "add", "-A")
        run(rename_repo, "commit", "-q", "-m", "rename out")
        r_head = subprocess.run(
            ["git", "-C", str(rename_repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        assert not docs_only_range(rename_repo, r_base, r_head)[0]

        # Stamp lifecycle: edit -> stamp-write -> git add -> stamp-valid HITS;
        # poison attack & partial staging MISS; expired/MERGE_HEAD/env bypass MISS.
        stamp_repo = fresh(scratch, "stamp")
        (stamp_repo / "crates/c/src/lib.rs").write_text("pub fn a() {}\n")
        wrote, why_write = stamp_write(stamp_repo)
        assert wrote, f"working-tree stamp_write before git add failed: {why_write}"
        # Still unstaged -> stamp_valid MUST refuse.
        assert not stamp_valid(stamp_repo)[0], "unstaged edits matched stamp before git add"
        run(stamp_repo, "add", "crates/c/src/lib.rs")
        valid, why_valid = stamp_valid(stamp_repo)
        assert valid, f"edit -> stamp_write -> git add -> stamp_valid missed: {why_valid}"

        # Partial staging: edit A & B -> stamp_write -> add only A -> MUST miss.
        (stamp_repo / "crates/c/src/lib.rs").write_text("pub fn part_a() {}\n")
        (stamp_repo / "README.md").write_text("part b\n")
        assert stamp_write(stamp_repo)[0]
        run(stamp_repo, "add", "crates/c/src/lib.rs")
        assert not stamp_valid(stamp_repo)[0], "partial stage matched working-tree stamp"

        # Poisoning attack: stage A -> dirty B -> stamp_write -> checkout -- B -> commit A MUST miss!
        run(stamp_repo, "-c", "core.hooksPath=/dev/null", "commit", "-q", "-m", "a")
        run(stamp_repo, "checkout", "--", "README.md")
        (stamp_repo / "crates/c/src/lib.rs").write_text("pub fn staged_a() {}\n")
        run(stamp_repo, "add", "crates/c/src/lib.rs")
        (stamp_repo / "README.md").write_text("dirty b\n")
        assert stamp_write(stamp_repo)[0]
        run(stamp_repo, "checkout", "--", "README.md")
        assert not stamp_valid(stamp_repo)[0], "poisoned stamp matched after reverting B"

        # Clean tree stamp hit + invalidation conditions.
        run(stamp_repo, "add", "crates/c/src/lib.rs")
        assert stamp_write(stamp_repo)[0]
        assert stamp_valid(stamp_repo)[0]
        (stamp_repo / "crates/c/src/lib.rs").write_text("pub fn unstaged() {}\n")
        assert not stamp_valid(stamp_repo)[0], "unstaged edit did not invalidate stamp"
        run(stamp_repo, "checkout", "--", "crates/c/src/lib.rs")
        assert stamp_valid(stamp_repo)[0]

        (stamp_repo / "untracked.txt").write_text("x\n")
        assert not stamp_valid(stamp_repo)[0], "untracked file did not invalidate stamp"
        (stamp_repo / "untracked.txt").unlink()
        assert stamp_valid(stamp_repo)[0]

        (stamp_repo / "crates/c/src/lib.rs").write_text("pub fn changed_stage() {}\n")
        run(stamp_repo, "add", "crates/c/src/lib.rs")
        assert not stamp_valid(stamp_repo)[0], "staged content change did not invalidate stamp"
        assert stamp_write(stamp_repo)[0]
        assert stamp_valid(stamp_repo)[0]

        # Expired timestamp (> 1800s).
        sp = stamp_path(stamp_repo)
        tree_sha = sp.read_text(encoding="utf-8").split("\t", 1)[0]
        sp.write_text(f"{tree_sha}\t1\n", encoding="utf-8")
        assert not stamp_valid(stamp_repo)[0], "expired stamp was accepted"
        assert stamp_write(stamp_repo)[0]

        # MERGE_HEAD invalidates both stamp_valid and stamp_write.
        s_head = subprocess.run(
            ["git", "-C", str(stamp_repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        (stamp_repo / ".git/MERGE_HEAD").write_text(s_head + "\n")
        assert not stamp_valid(stamp_repo)[0], "MERGE_HEAD did not invalidate stamp"
        assert not stamp_write(stamp_repo)[0], "stamp_write succeeded during merge"
        (stamp_repo / ".git/MERGE_HEAD").unlink()
        assert stamp_write(stamp_repo)[0]

        # Env bypass.
        os.environ["LUSHTEXT_SKIP_PRECOMMIT_STAMP"] = "1"
        try:
            assert not stamp_valid(stamp_repo)[0], "env skip did not invalidate stamp"
        finally:
            os.environ.pop("LUSHTEXT_SKIP_PRECOMMIT_STAMP", None)

        # Worktree isolation: linked worktree gets its own stamp separate from main repo.
        wt_path = scratch / "wt-linked"
        run(stamp_repo, "worktree", "add", "-q", "-b", "wt-branch", str(wt_path))
        assert stamp_path(wt_path) != stamp_path(stamp_repo), "linked worktree shared stamp path"
        (wt_path / "crates/c/src/lib.rs").write_text("pub fn wt_only() {}\n")
        run(wt_path, "add", "crates/c/src/lib.rs")
        assert not stamp_valid(wt_path)[0], "linked worktree inherited main repo stamp"
        assert stamp_write(wt_path)[0]
        assert stamp_valid(wt_path)[0]
        run(stamp_repo, "worktree", "remove", "--force", str(wt_path))

        res = subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), "stamp-valid"],
            cwd=stamp_repo,
            capture_output=True,
        )
        assert res.returncode == 0, f"CLI stamp-valid failed: {res.stderr.decode()}"

        # gtk-lush scope classification (fail-closed on missing/invalid refs).
        assert gtk_lush_changed_range(stamp_repo, "missing-ref", "HEAD")[0]
        assert not gtk_lush_changed_worktree(stamp_repo, None)[0]
        (stamp_repo / "crates/lushtext-ui/src").mkdir(parents=True, exist_ok=True)
        (stamp_repo / "crates/lushtext-ui/src/u.rs").write_text("pub fn u() {}\n")
        run(stamp_repo, "add", "crates/lushtext-ui/src/u.rs")
        run(stamp_repo, "-c", "core.hooksPath=/dev/null", "commit", "-q", "-m", "ui-only")
        head_after_ui = subprocess.run(
            ["git", "-C", str(stamp_repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        assert not gtk_lush_changed_range(stamp_repo, f"{head_after_ui}~1", head_after_ui)[0]
        (stamp_repo / "crates/gtk-lush/widgets/src").mkdir(parents=True, exist_ok=True)
        (stamp_repo / "crates/gtk-lush/widgets/src/w.rs").write_text("pub fn w() {}\n")
        assert gtk_lush_changed_worktree(stamp_repo, f"{head_after_ui}~1")[0]
        run(stamp_repo, "add", "crates/gtk-lush/widgets/src/w.rs")
        run(stamp_repo, "-c", "core.hooksPath=/dev/null", "commit", "-q", "-m", "gtk-lush")
        head_after_gl = subprocess.run(
            ["git", "-C", str(stamp_repo), "rev-parse", "HEAD"],
            capture_output=True, text=True, check=True,
        ).stdout.strip()
        assert gtk_lush_changed_range(stamp_repo, head_after_ui, head_after_gl)[0]

    print("precommit-scope self-test: ok")


def main(argv: list[str]) -> int:
    if argv == ["--self-test"]:
        self_test()
        return 0
    if argv == ["docs-only"]:
        ok, why = docs_only(Path.cwd())
        print(f"precommit-scope: {'docs-only' if ok else 'full gate'}: {why}", file=sys.stderr)
        return 0 if ok else 1
    if len(argv) == 3 and argv[0] == "docs-only-range":
        ok, why = docs_only_range(Path.cwd(), argv[1], argv[2])
        print(f"precommit-scope: {'docs-only' if ok else 'full gate'}: {why}", file=sys.stderr)
        return 0 if ok else 1
    if len(argv) == 3 and argv[0] == "gtk-lush-changed-range":
        changed, why = gtk_lush_changed_range(Path.cwd(), argv[1], argv[2])
        print(f"precommit-scope: gtk-lush {'changed' if changed else 'unchanged'}: {why}", file=sys.stderr)
        return 0 if changed else 1
    if argv and argv[0] == "gtk-lush-changed-worktree" and len(argv) in (1, 2):
        base = argv[1] if len(argv) == 2 and argv[1] else None
        changed, why = gtk_lush_changed_worktree(Path.cwd(), base)
        print(f"precommit-scope: gtk-lush {'changed' if changed else 'unchanged'}: {why}", file=sys.stderr)
        return 0 if changed else 1
    if argv == ["stamp-write"]:
        ok, why = stamp_write(Path.cwd())
        print(f"precommit-scope: {why}", file=sys.stderr)
        return 0
    if argv == ["stamp-valid"]:
        ok, why = stamp_valid(Path.cwd())
        print(f"precommit-scope: {'stamp hit' if ok else 'stamp miss'}: {why}", file=sys.stderr)
        return 0 if ok else 1
    if argv == ["check-includes"]:
        findings = check_includes(REPO_ROOT)
        for finding in findings:
            print(f"precommit-scope: {finding}", file=sys.stderr)
        if findings:
            print(
                "precommit-scope: a documentation path is a Rust build input, so docs-only commits could change "
                "what Clippy reports; move the file out of the allowlist or narrow the allowlist in "
                "scripts/precommit-scope.py and .githooks/pre-commit",
                file=sys.stderr,
            )
            return 1
        print(f"precommit-scope: no Rust build input is on the documentation allowlist ({len(rust_sources(REPO_ROOT))} sources)")
        return 0
    print(__doc__.split("\n\n")[1], file=sys.stderr)
    return 64


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
