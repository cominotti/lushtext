#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Decide whether a commit may skip the pre-commit hook's Clippy step.

Design D8 of openspec/changes/archive/2026-11-15-reduce-build-disk-writes.

Usage:
  scripts/precommit-scope.py docs-only        # exit 0 only for a docs-only commit
  scripts/precommit-scope.py check-includes   # fail when a Rust build input is allowlisted
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
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ALLOWED_DIRS = ("docs", "openspec", ".claude", ".agents")
SOURCE_ROOTS = ("crates", "fuzz", "workspace-hack")
INCLUDE_MACROS = ("include_str", "include_bytes", "include")


def allowlisted(rel: str) -> bool:
    """Whether a repository-relative path is on the documentation allowlist."""
    rel = rel.replace("\\", "/")
    if rel.startswith("../") or rel.startswith("/"):
        return False
    first = rel.split("/", 1)[0]
    if first in ALLOWED_DIRS and "/" in rel:
        return True
    return "/" not in rel and rel.endswith(".md")


def git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True)


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
    print("precommit-scope self-test: ok")


def main(argv: list[str]) -> int:
    if argv == ["--self-test"]:
        self_test()
        return 0
    if argv == ["docs-only"]:
        ok, why = docs_only(Path.cwd())
        print(f"precommit-scope: {'docs-only' if ok else 'full gate'}: {why}", file=sys.stderr)
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
