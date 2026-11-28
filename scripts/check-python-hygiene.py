#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Zero-dependency static Python hygiene gate for repository scripts.

Catches syntax errors, bare `except:` blocks (python:S5754), duplicate constant
keys in dict literals, and redundant regex alternation branches (python:S5855
style) before CI/SonarQube runs.

Suppress a specific regex line only with an explicit rationale:
  # python-hygiene-ok: <reason>
"""

from __future__ import annotations

import argparse
import ast
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
REGEX_CALLS = frozenset(
    {"compile", "search", "match", "fullmatch", "findall", "finditer", "sub", "subn", "split"}
)
EXEMPTION_MARKER = "python-hygiene-ok:"


@dataclass(frozen=True)
class Finding:
    path: str
    line: int
    rule: str
    message: str


def repo_python_files(root: Path) -> list[Path]:
    files: list[Path] = []
    scripts_dir = root / "scripts"
    if scripts_dir.is_dir():
        files.extend(sorted(scripts_dir.glob("*.py")))
    skills_dir = root / ".agents" / "skills"
    if skills_dir.is_dir():
        files.extend(sorted(skills_dir.glob("*/scripts/*.py")))
    return sorted(set(files))


def split_top_level_alternations(pattern: str) -> list[str]:
    """Split a regex fragment on top-level `|` respecting escapes and char classes."""
    branches: list[str] = []
    current: list[str] = []
    depth = 0
    in_class = False
    class_idx = 0
    class_negated = False
    escaped = False
    i = 0
    while i < len(pattern):
        ch = pattern[i]
        if escaped:
            current.append(ch)
            escaped = False
            if in_class:
                class_idx += 1
            i += 1
            continue
        if ch == "\\":
            current.append(ch)
            escaped = True
            i += 1
            continue
        if in_class:
            current.append(ch)
            if class_idx == 0 and ch == "^":
                class_negated = True
            elif ch == "]" and not (class_idx == 0 or (class_idx == 1 and class_negated)):
                in_class = False
            class_idx += 1
            i += 1
            continue
        if ch == "[":
            in_class = True
            class_idx = 0
            class_negated = False
            current.append(ch)
            i += 1
            continue
        if ch == "(":
            depth += 1
            current.append(ch)
            i += 1
            continue
        if ch == ")" and depth > 0:
            depth -= 1
            current.append(ch)
            i += 1
            continue
        if ch == "|" and depth == 0:
            branches.append("".join(current))
            current = []
            i += 1
            continue
        current.append(ch)
        i += 1
    branches.append("".join(current))
    return branches


def iter_group_bodies(pattern: str) -> list[str]:
    """Extract parenthesized group bodies `(?:...)`, `(...)`, etc. for alternation checks."""
    bodies: list[str] = []
    stack: list[int] = []
    in_class = False
    class_idx = 0
    class_negated = False
    escaped = False
    for i, ch in enumerate(pattern):
        if escaped:
            escaped = False
            if in_class:
                class_idx += 1
            continue
        if ch == "\\":
            escaped = True
            continue
        if in_class:
            if class_idx == 0 and ch == "^":
                class_negated = True
            elif ch == "]" and not (class_idx == 0 or (class_idx == 1 and class_negated)):
                in_class = False
            class_idx += 1
            continue
        if ch == "[":
            in_class = True
            class_idx = 0
            class_negated = False
            continue
        if ch == "(":
            stack.append(i)
        elif ch == ")" and stack:
            start = stack.pop()
            body = pattern[start + 1 : i]
            if body.startswith("?:"):
                body = body[2:]
            elif body.startswith("?P<") and ">" in body:
                body = body.split(">", 1)[1]
            elif body.startswith(("?=", "?!", "?<=", "?<!")):
                continue
            elif body.startswith("?"):
                # Flags or comments like (?x) / (?#...) are not simple alternations.
                continue
            bodies.append(body)
    return bodies


def redundant_alternations(pattern: str) -> list[str]:
    """Return duplicate alternation branch literals within a regex pattern."""
    if pattern.startswith("(?") and "x" in pattern.split(")", 1)[0]:
        return []
    issues: list[str] = []
    candidates = [pattern, *iter_group_bodies(pattern)]
    for candidate in candidates:
        branches = split_top_level_alternations(candidate)
        if len(branches) < 2:
            continue
        seen: set[str] = set()
        for branch in branches:
            if branch in seen:
                issues.append(
                    f"redundant regex alternation branch {branch!r} in {candidate!r}"
                )
            seen.add(branch)
    return issues


def is_re_call(node: ast.Call) -> bool:
    func = node.func
    return (
        isinstance(func, ast.Attribute)
        and isinstance(func.value, ast.Name)
        and func.value.id == "re"
        and func.attr in REGEX_CALLS
    )


def _node_uses_verbose_flag(node: ast.AST) -> bool:
    if isinstance(node, ast.Attribute) and isinstance(node.value, ast.Name) and node.value.id == "re":
        return node.attr in {"VERBOSE", "X"}
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr):
        return _node_uses_verbose_flag(node.left) or _node_uses_verbose_flag(node.right)
    return False


def has_verbose_flag(node: ast.Call) -> bool:
    return any(_node_uses_verbose_flag(arg) for arg in node.args) or any(
        kw.arg == "flags" and _node_uses_verbose_flag(kw.value) for kw in node.keywords
    )


def regex_literal_arg(node: ast.Call) -> ast.Constant | None:
    if node.args:
        first = node.args[0]
        if isinstance(first, ast.Constant) and isinstance(first.value, str):
            return first
    for kw in node.keywords:
        if kw.arg == "pattern" and isinstance(kw.value, ast.Constant) and isinstance(kw.value.value, str):
            return kw.value
    return None


def check_file(path: Path, root: Path = REPO_ROOT) -> list[Finding]:
    try:
        rel = path.relative_to(root).as_posix()
    except ValueError:
        rel = path.as_posix()
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    try:
        tree = ast.parse(text, filename=rel)
    except SyntaxError as exc:
        return [Finding(rel, exc.lineno or 1, "syntax", f"syntax error: {exc.msg}")]

    findings: list[Finding] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.ExceptHandler) and node.type is None:
            findings.append(
                Finding(rel, node.lineno, "bare-except", "bare `except:` catches BaseException; specify an exception type")
            )
        elif isinstance(node, ast.Dict):
            seen_keys: dict[object, int] = {}
            for key in node.keys:
                if isinstance(key, ast.Constant) and isinstance(key.value, (str, int, float, bool, type(None))):
                    # Distinguish 1 vs True in Python dict keys cleanly via (type, value)
                    k = (type(key.value), key.value)
                    if k in seen_keys:
                        findings.append(
                            Finding(
                                rel,
                                key.lineno,
                                "duplicate-dict-key",
                                f"duplicate dict key {key.value!r} (first seen at line {seen_keys[k]})",
                            )
                        )
                    else:
                        seen_keys[k] = key.lineno
        elif isinstance(node, ast.Call) and is_re_call(node):
            if has_verbose_flag(node):
                continue
            arg = regex_literal_arg(node)
            if arg is None:
                continue
            # Check raw call lines or preceding line for exemption marker.
            start = max(0, min(node.lineno, arg.lineno) - 1)
            end = max(node.end_lineno or node.lineno, arg.end_lineno or arg.lineno)
            span_lines = lines[start:end]
            if any(EXEMPTION_MARKER in line for line in span_lines):
                continue
            for issue in redundant_alternations(arg.value):
                findings.append(Finding(rel, arg.lineno, "regex-alternation", issue))

    return sorted(findings, key=lambda f: (f.line, f.rule))


def check_paths(paths: list[Path], root: Path = REPO_ROOT) -> list[Finding]:
    findings: list[Finding] = []
    for path in paths:
        findings.extend(check_file(path, root=root))
    return findings


def run_self_test() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        bad_syntax = root / "bad_syntax.py"
        bad_syntax.write_text("def broken(:\n    pass\n", encoding="utf-8")
        assert any(f.rule == "syntax" for f in check_file(bad_syntax, root))

        bad_except = root / "bad_except.py"
        bad_except.write_text("try:\n    x = 1\nexcept:\n    x = 2\n", encoding="utf-8")
        assert any(f.rule == "bare-except" for f in check_file(bad_except, root))

        bad_dict = root / "bad_dict.py"
        bad_dict.write_text("D = {'a': 1, 'b': 2, 'a': 3}\n", encoding="utf-8")
        assert any(f.rule == "duplicate-dict-key" for f in check_file(bad_dict, root))

        bad_regex = root / "bad_regex.py"
        bad_regex.write_text('import re\nP = re.compile(r"foo|bar|foo")\n', encoding="utf-8")
        assert any(f.rule == "regex-alternation" for f in check_file(bad_regex, root))

        bad_group_regex = root / "bad_group.py"
        bad_group_regex.write_text('import re\nP = re.search(r"^(?:alpha|alpha)$", "alpha")\n', encoding="utf-8")
        assert any(f.rule == "regex-alternation" for f in check_file(bad_group_regex, root))

        exempt_regex = root / "exempt.py"
        exempt_regex.write_text(
            'import re\nP = re.compile(  # python-hygiene-ok: intentional fixture\n    r"foo|foo"\n)\n',
            encoding="utf-8",
        )
        assert not check_file(exempt_regex, root)

        bad_subn_kw = root / "bad_subn.py"
        bad_subn_kw.write_text(
            'import re\nX = re.subn(pattern=r"a|a", repl="b", string="a")\n',
            encoding="utf-8",
        )
        assert any(f.rule == "regex-alternation" for f in check_file(bad_subn_kw, root))

        clean = root / "clean.py"
        clean.write_text(
            'import re\nP = re.compile(r"[a|a]|[]|a]|[^]|b]|(?:one|two)")\n'
            'V = re.compile(r"""a  # a | a\n| b""", re.VERBOSE)\n'
            "try:\n    x = {1: 'a', True: 'b'}\nexcept ValueError:\n    pass\n",
            encoding="utf-8",
        )
        assert not check_file(clean, root), f"unexpected findings on clean fixture: {check_file(clean, root)}"
    print("check-python-hygiene self-test: ok")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="run self-tests first")
    args = parser.parse_args(argv)
    if args.self_test:
        run_self_test()

    findings = check_paths(repo_python_files(REPO_ROOT), REPO_ROOT)
    if findings:
        print("Python hygiene check failed:", file=sys.stderr)
        for finding in findings:
            print(
                f"{finding.path}:{finding.line}: [{finding.rule}] {finding.message}",
                file=sys.stderr,
            )
        return 1

    count = len(repo_python_files(REPO_ROOT))
    print(f"PASS: python hygiene checked {count} Python script(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
