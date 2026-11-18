#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Only the session helper may start a private headless session.

`quiet-headless-session-services` made the `session` module of
`gtk-lush-proof-harness` the one implementation of the private headless test
session: the session bus, the private system bus, the compositor and the quiet
media daemons. Before it, about twenty call sites started those processes on
their own and drifted apart silently (WirePlumber's `main` profile probing the
host's sound cards and bluetoothd, module-rt draining the host's rtkit burst
limit). This gate keeps that from coming back.

It scans every tracked file (`git ls-files`), classified by kind, not by
directory, so a new lane under a new path is scanned the day it is added:

- shell (`.sh`/`.bash`, a `sh`/`bash` shebang, `.githooks/*`), Makefile recipe
  lines and variable values, GitHub workflow `run:` blocks, and
  `Containerfile`/`Dockerfile` `RUN`/`CMD`/`ENTRYPOINT` instructions. The
  shell rule is fail-closed: a word equal to a forbidden name, or a path
  ending in `/<name>`, fails **in any position**. The exempt contexts are
  explicit and few: comments, package-manager install commands with their
  backslash continuation lines, the elements of an array whose name ends in
  `packages`, `case` pattern labels, fully quoted words outside command
  position (except the string of `sh -c`/`bash -c`/`eval`, which is scanned
  as shell), and heredoc bodies (except one fed to a shell; command
  substitutions inside an unquoted heredoc are still scanned).
- Python (`ast`, docstrings excluded) and Rust (string literals, after
  stripping comments): a literal fails when it equals a forbidden name, ends
  in `/<name>`, or has such a name or path as its first whitespace-separated
  word.

It also fails when:

- a scanned file other than the shared classifier module and the
  parity-tested `cargo-gtk-proof` warning scanner contains a distinctive
  message text of the shared "headless session services" warning family, so a
  runner cannot regrow its own copy of it;
- the helper's quiet configuration drifts (WirePlumber profile, module-rt,
  hardware monitors, private homes, the private system bus);
- the helper is missing, or has zero sanctioned spawns, so a renamed helper or
  a gate that matches nothing turns red instead of passing.

The helper module and this gate are exempt by **path**; renaming either
without updating the gate makes the renamed file a violator (fail-closed).

`--self-test` proves every rule red on deliberate violations and green on the
benign forms, using fixtures in the run's temporary directory.
"""

from __future__ import annotations

import argparse
import ast
import bisect
import importlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPTS_DIR = REPO_ROOT / "scripts"

FORBIDDEN = (
    "dbus-run-session",
    "dbus-daemon",
    "dbus-launch",
    "mutter",
    "pipewire",
    "pipewire-pulse",
    "wireplumber",
)
FORBIDDEN_SET = frozenset(FORBIDDEN)

# Path-keyed, fail-closed exemptions (testing.md, "Path-keyed gates").
HELPER_PREFIX = "crates/gtk-lush/proof-harness/src/session/"
HELPER_SOURCE = HELPER_PREFIX + "mod.rs"
HELPER_PROFILES = HELPER_PREFIX + "profiles.rs"
HELPER_PIPEWIRE_FRAGMENT = HELPER_PREFIX + "pipewire-quiet.conf"
HELPER_WIREPLUMBER_FRAGMENT = HELPER_PREFIX + "wireplumber-quiet.conf"
GATE_PATH = "scripts/check-session-hygiene.py"
# The spawns the helper must contain; zero of them means the gate is keyed on
# nothing and would pass vacuously.
SANCTIONED_SPAWNS = ("dbus-run-session", "dbus-daemon", "mutter", "pipewire", "wireplumber")

# The shared warning family and the scanner that keeps a parity-tested copy.
CLASSIFIER_MODULE = "scripts/smoke_warning_classifiers.py"
CLASSIFIER_PARITY_COPY = "crates/cargo-gtk-proof/src/warnings.rs"
CLASSIFIER_FAMILY = "headless-session-services"
# A literal run shorter than this is too generic to identify a copy.
MIN_FAMILY_FRAGMENT = 24

PACKAGE_MANAGERS = frozenset(
    {"dnf", "dnf5", "microdnf", "yum", "apt", "apt-get", "zypper", "pacman", "brew", "rpm-ostree"}
)
SHELLS = frozenset({"sh", "bash", "dash", "zsh"})


@dataclass(frozen=True, order=True)
class Finding:
    path: str
    line: int
    message: str

    def render(self) -> str:
        return f"{self.path}:{self.line}: {self.message}"


def is_forbidden_word(word: str) -> str | None:
    """Return the forbidden name `word` is, or ends in as a path, else None."""

    if word in FORBIDDEN_SET:
        return word
    if "/" in word:
        base = word.rsplit("/", 1)[1]
        if base in FORBIDDEN_SET:
            return base
    return None


def forbidden_literal(value: str) -> str | None:
    """The Python and Rust literal rule: equal, `/name` suffix, or first word."""

    hit = is_forbidden_word(value)
    if hit:
        return hit
    words = value.split()
    if words:
        return is_forbidden_word(words[0])
    return None


# ---------------------------------------------------------------------------
# Shell


class _Lines:
    def __init__(self, text: str) -> None:
        self.starts = [0]
        for index, char in enumerate(text):
            if char == "\n":
                self.starts.append(index + 1)

    def line(self, offset: int) -> int:
        return bisect.bisect_right(self.starts, offset)


@dataclass
class _Word:
    value: str
    offset: int
    quoted_all: bool
    quoted_any: bool


def _find_closing(text: str, index: int, open_char: str, close_char: str) -> int:
    """Index of the `close_char` matching the opener just before `index`."""

    depth = 1
    i = index
    while i < len(text):
        char = text[i]
        if char == "\\":
            i += 2
            continue
        if char == "'":
            end = text.find("'", i + 1)
            i = len(text) if end < 0 else end + 1
            continue
        if char == '"':
            i = _skip_double_quoted(text, i + 1)
            continue
        if char == "#" and (i == 0 or text[i - 1] in " \t\n;(|&"):
            end = text.find("\n", i)
            i = len(text) if end < 0 else end
            continue
        if char == open_char:
            depth += 1
        elif char == close_char:
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return len(text)


def _skip_double_quoted(text: str, index: int) -> int:
    i = index
    while i < len(text):
        char = text[i]
        if char == "\\":
            i += 2
            continue
        if char == '"':
            return i + 1
        if text.startswith("$(", i):
            i = _find_closing(text, i + 2, "(", ")") + 1
            continue
        if char == "`":
            end = text.find("`", i + 1)
            i = len(text) if end < 0 else end + 1
            continue
        i += 1
    return len(text)


class ShellScanner:
    """A small fail-closed bash lexer: enough to tell a spawn from prose."""

    def __init__(self, path: str, text: str, line_map=None) -> None:
        self.path = path
        self.text = text
        self.lines = _Lines(text)
        self.line_map = line_map
        self.findings: list[Finding] = []

    def _line(self, offset: int) -> int:
        line = self.lines.line(offset)
        if self.line_map is not None:
            return self.line_map.get(line, line)
        return line

    def report(self, offset: int, name: str, context: str) -> None:
        self.findings.append(
            Finding(
                self.path,
                self._line(offset),
                f"`{name}` {context}; start sessions through scripts/headless-session "
                "or the gtk-lush-proof-harness session library",
            )
        )

    # -- driver ------------------------------------------------------------

    def scan(self) -> list[Finding]:
        self._scan_region(0, len(self.text))
        return self.findings

    def _scan_region(self, start: int, end: int) -> None:
        state = _CommandState()
        case_stack: list[str] = []  # "pattern" or "body"
        pending_heredocs: list[tuple[str, bool, bool, bool]] = []
        i = start
        word: list[str] = []
        word_start = start
        quoted_all = True
        quoted_any = False
        has_chars = False

        def flush_word() -> None:
            nonlocal word, quoted_all, quoted_any, has_chars
            if has_chars:
                state.words.append(
                    _Word("".join(word), word_start, quoted_all and quoted_any, quoted_any)
                )
            word = []
            quoted_all = True
            quoted_any = False
            has_chars = False

        def finish_command(separator: str) -> None:
            flush_word()
            self._evaluate_command(state, case_stack)
            if case_stack and case_stack[-1] == "body" and separator in (";;", ";&", ";;&"):
                case_stack[-1] = "pattern"
            state.reset()

        while i < end:
            char = self.text[i]
            # Case pattern mode: everything up to the unquoted `)` is a label.
            if case_stack and case_stack[-1] == "pattern" and not has_chars and not state.words:
                j = i
                while j < end and self.text[j] in " \t\n;":
                    j += 1
                if self.text.startswith("esac", j) and (
                    j + 4 >= end or not (self.text[j + 4].isalnum() or self.text[j + 4] in "_-")
                ):
                    case_stack.pop()
                    i = j + 4
                    continue
                if j < end and self.text[j] == "#":
                    newline = self.text.find("\n", j)
                    i = end if newline < 0 or newline > end else newline + 1
                    continue
                close = self._case_label_end(j, end)
                if close is not None:
                    case_stack[-1] = "body"
                    i = close + 1
                    continue
            if char == "\\":
                if i + 1 < end and self.text[i + 1] == "\n":
                    i += 2
                    continue
                if not has_chars:
                    word_start = i
                word.append(self.text[i + 1 : i + 2])
                quoted_all = False
                has_chars = True
                i += 2
                continue
            if char in " \t":
                flush_word()
                i += 1
                continue
            if char == "\n":
                finish_command("\n")
                i += 1
                if pending_heredocs:
                    i = self._consume_heredocs(i, end, pending_heredocs)
                    pending_heredocs = []
                continue
            if char == "#" and not has_chars:
                newline = self.text.find("\n", i)
                i = end if newline < 0 or newline > end else newline
                continue
            if char == "'":
                close = self.text.find("'", i + 1)
                close = end if close < 0 or close > end else close
                if not has_chars:
                    word_start = i
                word.append(self.text[i + 1 : close])
                quoted_any = True
                has_chars = True
                i = close + 1
                continue
            if char == '"':
                close = _skip_double_quoted(self.text, i + 1)
                inner_end = min(close - 1, end)
                self._scan_substitutions(i + 1, inner_end)
                if not has_chars:
                    word_start = i
                word.append(self.text[i + 1 : inner_end])
                quoted_any = True
                has_chars = True
                i = close
                continue
            if char == "`":
                close = self.text.find("`", i + 1)
                close = end if close < 0 or close > end else close
                self._scan_region(i + 1, close)
                if not has_chars:
                    word_start = i
                word.append("`sub`")
                quoted_all = False
                has_chars = True
                i = close + 1
                continue
            if self.text.startswith("$(", i):
                close = _find_closing(self.text, i + 2, "(", ")")
                self._scan_region(i + 2, min(close, end))
                if not has_chars:
                    word_start = i
                word.append("$(sub)")
                quoted_all = False
                has_chars = True
                i = close + 1
                continue
            if self.text.startswith("${", i):
                close = _find_closing(self.text, i + 2, "{", "}")
                if not has_chars:
                    word_start = i
                word.append(self.text[i : close + 1])
                quoted_all = False
                has_chars = True
                i = close + 1
                continue
            if char == "(":
                current = "".join(word)
                if has_chars and re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*\+?=", current):
                    close = _find_closing(self.text, i + 1, "(", ")")
                    name = current.rstrip("+=")
                    self._scan_array(name, i + 1, min(close, end))
                    word = []
                    has_chars = False
                    quoted_all = True
                    quoted_any = False
                    i = close + 1
                    continue
                finish_command("(")
                i += 1
                continue
            if char == ")":
                finish_command(")")
                i += 1
                continue
            if char in ";&|":
                op = char
                if self.text.startswith(";;&", i):
                    op = ";;&"
                elif self.text.startswith(";;", i) or self.text.startswith(";&", i):
                    op = self.text[i : i + 2]
                elif self.text.startswith("&&", i) or self.text.startswith("||", i):
                    op = self.text[i : i + 2]
                finish_command(op)
                i += len(op)
                continue
            if self.text.startswith("<<", i) and not self.text.startswith("<<<", i):
                flush_word()
                j = i + 2
                strip_tabs = False
                if j < end and self.text[j] == "-":
                    strip_tabs = True
                    j += 1
                while j < end and self.text[j] in " \t":
                    j += 1
                match = re.match(r"""(['"]?)([A-Za-z_][A-Za-z0-9_]*)\1""", self.text[j:end])
                if match:
                    delimiter = match.group(2)
                    quoted = bool(match.group(1))
                    fed_to_shell = any(
                        w.value.rsplit("/", 1)[-1] in SHELLS or w.value == "eval"
                        for w in state.words
                    )
                    pending_heredocs.append((delimiter, quoted, strip_tabs, fed_to_shell))
                    i = j + match.end()
                    continue
                i = j
                continue
            if not has_chars:
                word_start = i
            word.append(char)
            quoted_all = False
            has_chars = True
            i += 1
        finish_command("\n")

    def _case_label_end(self, index: int, end: int) -> int | None:
        i = index
        while i < end:
            char = self.text[i]
            if char == "\\":
                i += 2
                continue
            if char == "'":
                close = self.text.find("'", i + 1)
                i = end if close < 0 else close + 1
                continue
            if char == '"':
                i = _skip_double_quoted(self.text, i + 1)
                continue
            if char == ")":
                return i
            if char == "\n":
                return None
            i += 1
        return None

    def _consume_heredocs(self, index: int, end: int, heredocs) -> int:
        i = index
        for delimiter, quoted, strip_tabs, fed_to_shell in heredocs:
            body_start = i
            body_end = end
            while i < end:
                newline = self.text.find("\n", i)
                line_end = end if newline < 0 or newline > end else newline
                line = self.text[i:line_end]
                if (line.lstrip("\t") if strip_tabs else line) == delimiter:
                    body_end = i
                    i = line_end + 1
                    break
                i = line_end + 1
            else:
                body_end = end
            if fed_to_shell:
                self._scan_region(body_start, body_end)
            elif not quoted:
                self._scan_substitutions(body_start, body_end)
        return i

    def _scan_substitutions(self, start: int, end: int) -> None:
        """Scan `$(...)` and backtick substitutions in double-quoted text."""

        i = start
        while i < end:
            if self.text[i] == "\\":
                i += 2
                continue
            if self.text.startswith("$(", i):
                close = _find_closing(self.text, i + 2, "(", ")")
                self._scan_region(i + 2, min(close, end))
                i = close + 1
                continue
            if self.text[i] == "`":
                close = self.text.find("`", i + 1)
                close = end if close < 0 or close > end else close
                self._scan_region(i + 1, close)
                i = close + 1
                continue
            i += 1

    def _scan_array(self, name: str, start: int, end: int) -> None:
        if name.lower().endswith("packages"):
            # The package-inventory convention (setup-dev-tools.sh).
            return
        sub = ShellScanner(self.path, self.text, self.line_map)
        sub.lines = self.lines
        state = _CommandState()
        sub_words = sub._words_only(start, end)
        for item in sub_words:
            hit = is_forbidden_word(item.value)
            if hit:
                self.report(item.offset, hit, f"is an element of array `{name}`")
        del state

    def _words_only(self, start: int, end: int) -> list[_Word]:
        words: list[_Word] = []
        i = start
        while i < end:
            char = self.text[i]
            if char in " \t\n":
                i += 1
                continue
            if char == "#":
                newline = self.text.find("\n", i)
                i = end if newline < 0 or newline > end else newline
                continue
            begin = i
            parts: list[str] = []
            while i < end and self.text[i] not in " \t\n":
                char = self.text[i]
                if char == "'":
                    close = self.text.find("'", i + 1)
                    close = end if close < 0 or close > end else close
                    parts.append(self.text[i + 1 : close])
                    i = close + 1
                elif char == '"':
                    close = _skip_double_quoted(self.text, i + 1)
                    self._scan_substitutions(i + 1, close - 1)
                    parts.append(self.text[i + 1 : close - 1])
                    i = close
                elif char == "\\":
                    parts.append(self.text[i + 1 : i + 2])
                    i += 2
                else:
                    parts.append(char)
                    i += 1
            words.append(_Word("".join(parts), begin, False, False))
        self.findings.extend([])
        return words

    # -- command evaluation -------------------------------------------------

    def _evaluate_command(self, state: "_CommandState", case_stack: list[str]) -> None:
        words = state.words
        if not words:
            return
        values = [w.value for w in words]
        if values[0] == "case":
            # `case WORD in`: the subject is scanned, the labels are not.
            for w in words[1:]:
                if w.value == "in":
                    break
                self._check_word(w, "is named in a `case` subject")
            case_stack.append("pattern")
            return
        bases = [v.rsplit("/", 1)[-1] for v in values]
        if any(b in PACKAGE_MANAGERS for b in bases) and any(
            v in ("install", "reinstall") or re.fullmatch(r"-S[a-z]*", v) for v in values
        ):
            return
        command_index = 0
        while command_index < len(words) and re.fullmatch(
            r"[A-Za-z_][A-Za-z0-9_]*\+?=.*", words[command_index].value
        ) and not words[command_index].quoted_all:
            command_index += 1
        is_shell_c = any(b in SHELLS for b in bases)
        is_eval = "eval" in values
        for index, w in enumerate(words):
            previous = values[index - 1] if index else ""
            if is_shell_c and re.fullmatch(r"-[a-z]*c[a-z]*", previous) and w.quoted_any:
                sub = ShellScanner(self.path, self.text, self.line_map)
                sub.lines = self.lines
                sub._scan_quoted_word(w)
                self.findings.extend(sub.findings)
                continue
            if is_eval and index > values.index("eval") and w.quoted_any:
                sub = ShellScanner(self.path, self.text, self.line_map)
                sub.lines = self.lines
                sub._scan_quoted_word(w)
                self.findings.extend(sub.findings)
                continue
            if w.quoted_all and index != command_index:
                continue
            self._check_word(w, "is named in a shell command")

    def _scan_quoted_word(self, word: _Word) -> None:
        sub = ShellScanner(self.path, word.value, None)
        for finding in sub.scan():
            self.findings.append(
                Finding(self.path, self._line(word.offset), finding.message.replace("shell command", "`sh -c`/`eval` string"))
            )

    def _check_word(self, w: _Word, context: str) -> None:
        candidates = [w.value]
        assignment = re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*\+?=(.*)", w.value)
        if assignment and not w.quoted_all:
            candidates.append(assignment.group(1))
        for candidate in candidates:
            hit = is_forbidden_word(candidate)
            if hit:
                self.report(w.offset, hit, context)
                return


class _CommandState:
    def __init__(self) -> None:
        self.words: list[_Word] = []

    def reset(self) -> None:
        self.words = []


def scan_shell(path: str, text: str, line_map=None) -> list[Finding]:
    return ShellScanner(path, text, line_map).scan()


# ---------------------------------------------------------------------------
# Embedded shell: Makefile, workflows, container builds


def makefile_shell(text: str) -> str:
    """Recipe lines and variable values as shell; other lines blank."""

    out = []
    for line in text.split("\n"):
        if line.startswith("\t"):
            body = line[1:].lstrip()
            body = body.lstrip("@-+")
            out.append(body.replace("$$", "$"))
            continue
        match = re.match(r"^[A-Za-z_][A-Za-z0-9_.]*\s*(?::=|\?=|\+=|=|::=)\s*(.*)$", line)
        if match and not line.lstrip().startswith("#"):
            out.append(match.group(1).replace("$$", "$"))
            continue
        out.append("")
    return "\n".join(out)


def workflow_shell(text: str) -> str:
    lines = text.split("\n")
    out = [""] * len(lines)
    i = 0
    while i < len(lines):
        line = lines[i]
        match = re.match(r"^(\s*)(?:-\s+)?run:\s*(.*)$", line)
        if not match:
            i += 1
            continue
        indent = len(match.group(1))
        rest = match.group(2).strip()
        if rest and rest[0] in "|>":
            i += 1
            block_indent = None
            while i < len(lines):
                candidate = lines[i]
                if candidate.strip() == "":
                    i += 1
                    continue
                current = len(candidate) - len(candidate.lstrip(" "))
                if current <= indent:
                    break
                if block_indent is None:
                    block_indent = current
                if current < block_indent:
                    break
                out[i] = candidate[block_indent:]
                i += 1
            continue
        if (rest.startswith('"') and rest.endswith('"')) or (rest.startswith("'") and rest.endswith("'")):
            rest = rest[1:-1]
        out[i] = rest
        i += 1
    return "\n".join(out)


def containerfile_parts(text: str) -> tuple[str, list[tuple[int, str]]]:
    """Shell-form instructions as shell, plus exec-form array elements."""

    lines = text.split("\n")
    out = [""] * len(lines)
    exec_words: list[tuple[int, str]] = []
    i = 0
    while i < len(lines):
        match = re.match(r"^\s*(RUN|CMD|ENTRYPOINT)\s+(.*)$", lines[i], re.IGNORECASE)
        if not match:
            i += 1
            continue
        body = match.group(2)
        if body.lstrip().startswith("["):
            try:
                for item in json.loads(body):
                    exec_words.append((i + 1, str(item)))
            except json.JSONDecodeError:
                exec_words.append((i + 1, body))
            i += 1
            continue
        # Drop BuildKit flags such as `--mount=...`.
        body = re.sub(r"^(?:--[a-z]+=\S+\s+)*", "", body)
        out[i] = body
        while out[i].rstrip().endswith("\\") and i + 1 < len(lines):
            i += 1
            out[i] = lines[i]
        i += 1
    return "\n".join(out), exec_words


# ---------------------------------------------------------------------------
# Python and Rust literals


def scan_python(path: str, text: str) -> list[Finding]:
    try:
        tree = ast.parse(text, filename=path)
    except SyntaxError as error:
        return [Finding(path, error.lineno or 1, f"cannot parse Python: {error.msg}")]
    docstrings: set[int] = set()
    for node in ast.walk(tree):
        if isinstance(node, (ast.Module, ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            body = getattr(node, "body", [])
            if body and isinstance(body[0], ast.Expr) and isinstance(body[0].value, ast.Constant):
                if isinstance(body[0].value.value, str):
                    docstrings.add(id(body[0].value))
    inner: set[int] = set()
    findings = []
    for node in ast.walk(tree):
        if isinstance(node, ast.JoinedStr):
            for index, value in enumerate(node.values):
                inner.add(id(value))
                if index == 0 and isinstance(value, ast.Constant) and isinstance(value.value, str):
                    words = value.value.split()
                    if words and not value.value[:1].isspace():
                        hit = is_forbidden_word(words[0]) if len(node.values) > 1 or len(words) > 1 else forbidden_literal(value.value)
                        if hit:
                            findings.append(_literal_finding(path, node.lineno, hit))
    for node in ast.walk(tree):
        if not isinstance(node, ast.Constant) or not isinstance(node.value, str):
            continue
        if id(node) in docstrings or id(node) in inner:
            continue
        hit = forbidden_literal(node.value)
        if hit:
            findings.append(_literal_finding(path, node.lineno, hit))
    return findings


def _literal_finding(path: str, line: int, name: str) -> Finding:
    return Finding(
        path,
        line,
        f"string literal names `{name}`; start sessions through scripts/headless-session "
        "or the gtk-lush-proof-harness session library, and take versions and paths "
        "from `check-tools --json`",
    )


_RUST_CHAR = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]{1,6}\}|.)|[^\\'\n])'")


def rust_string_literals(text: str) -> list[tuple[int, str]]:
    """(offset, value) of every string literal, comments excluded."""

    literals = []
    i = 0
    n = len(text)
    while i < n:
        char = text[i]
        if text.startswith("//", i):
            newline = text.find("\n", i)
            i = n if newline < 0 else newline
            continue
        if text.startswith("/*", i):
            depth = 1
            i += 2
            while i < n and depth:
                if text.startswith("/*", i):
                    depth += 1
                    i += 2
                elif text.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    i += 1
            continue
        raw = re.match(r"(?:b|c)?r(#*)\"", text[i : i + 300])
        if raw and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            hashes = raw.group(1)
            start = i + raw.end()
            close = text.find('"' + hashes, start)
            close = n if close < 0 else close
            literals.append((i, text[start:close]))
            i = close + 1 + len(hashes)
            continue
        if char == '"' or (char in "bc" and text.startswith('"', i + 1) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_"))):
            start = i + (1 if char == '"' else 2)
            j = start
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            literals.append((i, text[start:j]))
            i = j + 1
            continue
        if char == "'":
            match = _RUST_CHAR.match(text, i)
            i = match.end() if match else i + 1
            continue
        i += 1
    return literals


def scan_rust(path: str, text: str) -> list[Finding]:
    lines = _Lines(text)
    findings = []
    for offset, value in rust_string_literals(text):
        hit = forbidden_literal(value)
        if hit:
            findings.append(_literal_finding(path, lines.line(offset), hit))
    return findings


# ---------------------------------------------------------------------------
# File classification


def file_kind(path: str, text: str) -> str | None:
    name = path.rsplit("/", 1)[-1]
    if path.endswith(".md") or path.startswith("openspec/"):
        return None
    if path.endswith((".sh", ".bash")) or path.startswith(".githooks/"):
        return "shell"
    if name == "Makefile" or name.endswith(".mk"):
        return "make"
    if path.startswith(".github/") and path.endswith((".yml", ".yaml")):
        return "workflow"
    if name.startswith(("Containerfile", "Dockerfile")):
        return "container"
    if path.endswith(".py"):
        return "python"
    if path.endswith(".rs"):
        return "rust"
    first = text.split("\n", 1)[0] if text.startswith("#!") else ""
    if first:
        interpreter = first[2:].split()
        words = [w.rsplit("/", 1)[-1] for w in interpreter]
        if words and words[0] == "env" and len(words) > 1:
            words = [w for w in words[1:] if not w.startswith("-")]
        if words and words[0] in SHELLS:
            return "shell"
        if words and words[0].startswith("python"):
            return "python"
    return None


def is_exempt_path(path: str) -> bool:
    return path.startswith(HELPER_PREFIX) or path == GATE_PATH


def scan_file(path: str, text: str) -> list[Finding]:
    if is_exempt_path(path):
        return []
    kind = file_kind(path, text)
    if kind == "shell":
        return scan_shell(path, text)
    if kind == "make":
        return scan_shell(path, makefile_shell(text))
    if kind == "workflow":
        return scan_shell(path, workflow_shell(text))
    if kind == "container":
        shell, exec_words = containerfile_parts(text)
        findings = scan_shell(path, shell)
        for line, word in exec_words:
            hit = forbidden_literal(word)
            if hit:
                findings.append(Finding(path, line, f"`{hit}` is named in an exec-form instruction"))
        return findings
    if kind == "python":
        return scan_python(path, text)
    if kind == "rust":
        return scan_rust(path, text)
    return []


# ---------------------------------------------------------------------------
# Classifier copies


def family_fragments(pattern: str) -> list[str]:
    """Distinctive literal runs of a regex: the texts a copy would repeat."""

    fragments = []
    current: list[str] = []
    i = 0

    def close() -> None:
        run = "".join(current).strip()
        if len(run) >= MIN_FAMILY_FRAGMENT:
            fragments.append(run)
        current.clear()

    while i < len(pattern):
        char = pattern[i]
        if char == "\\" and i + 1 < len(pattern):
            nxt = pattern[i + 1]
            if nxt.isalnum():
                close()
            else:
                current.append(nxt)
            i += 2
            continue
        if char in "^$|()[]{}?*+.":
            if char == "[":
                close_index = pattern.find("]", i + 1)
                i = len(pattern) if close_index < 0 else close_index + 1
                if i < len(pattern) and pattern[i] in "*+?":
                    i += 1
                close()
                continue
            if char == "(" and pattern.startswith("(?:", i):
                close()
                i += 3
                continue
            close()
            i += 1
            continue
        current.append(char)
        i += 1
    close()
    # Log-prefix runs ("(mutter:", "): libmutter-WARNING **:") name a process
    # and a level, not a message: they are not what a copy is recognised by.
    return sorted(
        {
            fragment
            for fragment in fragments
            if "WARNING" not in fragment and "**" not in fragment and not fragment.startswith("(")
        }
    )


def load_family_fragments() -> tuple[list[str], str | None]:
    sys.path.insert(0, str(SCRIPTS_DIR))
    try:
        module = importlib.import_module("smoke_warning_classifiers")
    except Exception as error:  # noqa: BLE001 - reported as a finding
        return [], f"cannot import {CLASSIFIER_MODULE}: {error}"
    finally:
        sys.path.pop(0)
    pattern = None
    getter = getattr(module, "family_pattern", None)
    if callable(getter):
        try:
            pattern = getter(CLASSIFIER_FAMILY)
        except KeyError:
            pattern = None
    if pattern is None:
        compiled = getattr(module, "_HEADLESS_CONTAINER_SESSION_SERVICE", None)
        pattern = getattr(compiled, "pattern", None)
    if not pattern:
        return [], f"{CLASSIFIER_MODULE} exports no `{CLASSIFIER_FAMILY}` family"
    fragments = family_fragments(pattern)
    if not fragments:
        return [], f"the `{CLASSIFIER_FAMILY}` family yields no distinctive text"
    return fragments, None


def scan_classifier_copies(path: str, text: str, fragments: list[str]) -> list[Finding]:
    if path in (CLASSIFIER_MODULE, CLASSIFIER_PARITY_COPY) or is_exempt_path(path):
        return []
    if file_kind(path, text) is None:
        return []
    normalized = text.replace("\\", "")
    findings = []
    for fragment in fragments:
        index = normalized.find(fragment)
        if index >= 0:
            line = normalized.count("\n", 0, index) + 1
            findings.append(
                Finding(
                    path,
                    line,
                    f"copies the shared `{CLASSIFIER_FAMILY}` warning text {fragment!r}; "
                    f"read the family from {CLASSIFIER_MODULE} (--print-regex) instead",
                )
            )
    return findings


# ---------------------------------------------------------------------------
# Drift of the helper's quiet configuration


def check_helper_drift(read) -> list[Finding]:
    """`read(path)` returns the file text or None; paths are repo-relative."""

    findings: list[Finding] = []
    source = read(HELPER_SOURCE)
    profiles = read(HELPER_PROFILES)
    pipewire = read(HELPER_PIPEWIRE_FRAGMENT)
    wireplumber = read(HELPER_WIREPLUMBER_FRAGMENT)
    for path, text in (
        (HELPER_SOURCE, source),
        (HELPER_PROFILES, profiles),
        (HELPER_PIPEWIRE_FRAGMENT, pipewire),
        (HELPER_WIREPLUMBER_FRAGMENT, wireplumber),
    ):
        if text is None:
            findings.append(
                Finding(path, 1, "session helper file is missing; if it moved, re-key this gate")
            )
    if None in (source, profiles, pipewire, wireplumber):
        return findings

    profile = re.search(r'WIREPLUMBER_PROFILE:\s*&str\s*=\s*"([^"]*)"', profiles)
    if not profile or profile.group(1) != "policy":
        findings.append(
            Finding(HELPER_PROFILES, 1, "WirePlumber profile drifted: WIREPLUMBER_PROFILE must be \"policy\"")
        )
    for path, text in ((HELPER_WIREPLUMBER_FRAGMENT, wireplumber), (HELPER_PIPEWIRE_FRAGMENT, pipewire)):
        code = "\n".join(line.split("#", 1)[0] for line in text.split("\n"))
        hardware = re.search(r"hardware\.[A-Za-z0-9_.-]+\s*=\s*(required|optional)", code)
        if hardware:
            findings.append(Finding(path, 1, f"hardware monitor enabled: {hardware.group(0)!r}"))
        if path == HELPER_WIREPLUMBER_FRAGMENT:
            if "libpipewire-module-rt" in code:
                findings.append(Finding(path, 1, "the WirePlumber fragment lists libpipewire-module-rt"))
            if "override.context.modules" not in code:
                findings.append(Finding(path, 1, "the WirePlumber fragment lost `override.context.modules`"))
            if "wireplumber.components.rules" not in code or "client-context" not in code:
                findings.append(
                    Finding(path, 1, "the WirePlumber fragment lost the `client-context` components rule")
                )
            if not re.search(r"module\.rt\s*=\s*false", code):
                findings.append(Finding(path, 1, "the WirePlumber fragment must set `module.rt = false`"))
        else:
            if not re.search(r"module\.rt\s*=\s*false", code):
                findings.append(Finding(path, 1, "the PipeWire fragment must set `module.rt = false`"))
    homes = re.search(r"DAEMON_PRIVATE_HOMES:\s*\[&str;\s*\d+\]\s*=\s*\[([^\]]*)\]", source)
    listed = set(re.findall(r'"([A-Z_]+)"', homes.group(1))) if homes else set()
    for home in ("XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME"):
        if home not in listed:
            findings.append(
                Finding(HELPER_SOURCE, 1, f"the media daemons' {home} is not helper-private (DAEMON_PRIVATE_HOMES)")
            )
    for match in re.finditer(r"SystemBus::Host\b(\s*\{[^}]*\})?", profiles):
        body = match.group(1) or ""
        reason = re.search(r'reason:\s*"([^"]+)"', body)
        line = profiles.count("\n", 0, match.start()) + 1
        if not reason:
            findings.append(Finding(HELPER_PROFILES, line, "a profile uses the host system bus without a recorded reason"))
        tail = profiles[match.end() : match.end() + 200]
        if re.search(r"media:\s*Media::Quiet", tail) or re.search(
            r"media:\s*Media::Quiet", profiles[max(0, match.start() - 200) : match.start()]
        ):
            findings.append(Finding(HELPER_PROFILES, line, "a host-system-bus profile starts media"))
    return findings


def check_sanctioned_spawns(read) -> list[Finding]:
    texts = []
    for path in tracked_files():
        if path.startswith(HELPER_PREFIX) and path.endswith(".rs"):
            text = read(path)
            if text is not None:
                texts.append(text)
    return sanctioned_spawn_findings(texts)


def sanctioned_spawn_findings(texts: list[str]) -> list[Finding]:
    found = set()
    for text in texts:
        for _offset, value in rust_string_literals(text):
            hit = forbidden_literal(value)
            if hit:
                found.add(hit)
    missing = [name for name in SANCTIONED_SPAWNS if name not in found]
    if missing:
        return [
            Finding(
                HELPER_PREFIX,
                1,
                "the session helper has no sanctioned spawn of "
                + ", ".join(f"`{name}`" for name in missing)
                + "; a gate keyed on nothing passes vacuously (re-key it if the helper moved)",
            )
        ]
    return []


# ---------------------------------------------------------------------------
# Driver


def tracked_files() -> list[str]:
    output = subprocess.run(
        ["git", "ls-files", "-z"], cwd=REPO_ROOT, check=True, capture_output=True
    ).stdout.decode("utf-8", "surrogateescape")
    return [path for path in output.split("\0") if path]


def read_repo(path: str) -> str | None:
    try:
        return (REPO_ROOT / path).read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        return None


def scan_repo() -> list[Finding]:
    findings: list[Finding] = []
    fragments, family_error = load_family_fragments()
    if family_error:
        findings.append(Finding(CLASSIFIER_MODULE, 1, family_error))
    if not (REPO_ROOT / GATE_PATH).is_file():
        findings.append(Finding(GATE_PATH, 1, "the gate is not at its keyed path"))
    for path in tracked_files():
        absolute = REPO_ROOT / path
        if not absolute.is_file() or absolute.is_symlink():
            continue
        try:
            text = absolute.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        findings.extend(scan_file(path, text))
        findings.extend(scan_classifier_copies(path, text, fragments))
    findings.extend(check_helper_drift(read_repo))
    findings.extend(check_sanctioned_spawns(read_repo))
    return sorted(set(findings))


# ---------------------------------------------------------------------------
# Self-test

SPAWN_FIXTURES = (
    ("lanes/a.sh", "#!/bin/sh\nexec env A=1 timeout 5 wireplumber &\n", "wireplumber"),
    ("lanes/b.sh", "taskset -c 0 pipewire\n", "pipewire"),
    ("lanes/c.sh", "scripts/heavy.sh -- mutter --headless\n", "mutter"),
    ("lanes/d.sh", "if ! dbus-run-session -- x; then\n  exit 1\nfi\n", "dbus-run-session"),
    ("lanes/e.sh", "sh -c 'wireplumber &'\n", "wireplumber"),
    ("lanes/f.sh", 'out=$(dbus-run-session -- x)\n', "dbus-run-session"),
    ("lanes/g.sh", 'echo "$(mutter --version)"\n', "mutter"),
    ("lanes/h.sh", "bash <<EOF\nmutter --headless\nEOF\n", "mutter"),
    ("lanes/i.sh", "/usr/bin/pipewire &\n", "pipewire"),
    ("lanes/j.sh", "x && wireplumber\n", "wireplumber"),
    ("lanes/k.sh", 'eval "mutter --headless"\n', "mutter"),
    ("lanes/l.sh", 'cmd=(\n  dbus-run-session --\n  "$x"\n)\n', "dbus-run-session"),
    ("lanes/m.sh", '"mutter" --headless\n', "mutter"),
    ("lanes/n.sh", "MUTTER=mutter\n", "mutter"),
    ("lanes/o", "#!/usr/bin/env bash\nnohup pipewire-pulse &\n", "pipewire-pulse"),
    ("lanes/p.sh", "case $x in\n  a) wireplumber ;;\nesac\n", "wireplumber"),
    ("lanes/q.sh", "cat <<EOF\n$(dbus-launch)\nEOF\n", "dbus-launch"),
    ("Makefile", "run:\n\t@dbus-run-session -- mutter --headless -- x\n", "dbus-run-session"),
    ("sub/rules.mk", "X := mutter\n", "mutter"),
    (".github/workflows/w.yml", "jobs:\n  a:\n    steps:\n      - run: |\n          wireplumber &\n", "wireplumber"),
    (".github/workflows/v.yml", "jobs:\n  a:\n    steps:\n      - name: x\n        run: mutter --headless\n", "mutter"),
    ("build-aux/x/Containerfile", "FROM fedora\nRUN mutter --headless --version\n", "mutter"),
    ("build-aux/y/Dockerfile", 'FROM fedora\nCMD ["pipewire"]\n', "pipewire"),
    (".githooks/post-merge", "wireplumber\n", "wireplumber"),
    ("tools/a.py", 'import subprocess\nsubprocess.run(["mutter", "--headless"])\n', "mutter"),
    ("tools/b.py", 'P = "/usr/bin/pipewire"\n', "pipewire"),
    ("tools/c.py", 'import subprocess\nsubprocess.run("mutter --headless", shell=True)\n', "mutter"),
    ("tools/d.py", 'x = f"dbus-run-session -- {y}"\n', "dbus-run-session"),
    ("tools/e", "#!/usr/bin/env python3\nX = 'wireplumber'\n", "wireplumber"),
    ("crates/x/src/a.rs", 'fn f() { std::process::Command::new("dbus-run-session"); }\n', "dbus-run-session"),
    ("crates/x/src/b.rs", 'fn f() -> String { "wireplumber".to_string() }\n', "wireplumber"),
    ("crates/x/src/c.rs", 'const X: &str = r#"mutter --headless"#;\n', "mutter"),
    ("crates/x/src/d.rs", "fn f<'a>(x: &'a str) -> char { let _ = \"pipewire\"; 'c' }\n", "pipewire"),
    # Argument forms: a prerequisite check, a `for` list and a command array.
    ("lanes/r.sh", "require_command mutter\n", "mutter"),
    ("lanes/s.sh", "smoke_require_command wireplumber\n", "wireplumber"),
    ("lanes/t.sh", "for c in dbus-run-session mutter; do\n  echo \"$c\"\ndone\n", "dbus-run-session"),
    ("lanes/u.sh", "command -v pipewire >/dev/null\n", "pipewire"),
    ("lanes/v.sh", "required_commands=(\n  python3\n  wireplumber\n)\n", "wireplumber"),
    # A never-listed path in a never-listed directory.
    ("brand/new/lane/run.sh", "wireplumber &\n", "wireplumber"),
)

BENIGN_FIXTURES = (
    ("lanes/b1.sh", "dnf install -y pipewire wireplumber\n"),
    ("lanes/b2.sh", "sudo dnf install -y \\\n  gtk4-devel \\\n  mutter \\\n  wireplumber\nmake check\n"),
    ("lanes/b3.sh", "packages=(\n  mutter\n  pipewire\n  # comment\n  wireplumber\n)\n"),
    ("lanes/b4.sh", "case \"$c\" in\n  dbus-run-session) echo dbus ;;\n  mutter|pipewire) echo media ;;\n  *) ;;\nesac\n"),
    ("lanes/b5.sh", "printf 'dbus-daemon'\n"),
    ("lanes/b6.sh", 'echo "running under mutter --headless"\n'),
    ("lanes/b7.sh", "usage() {\n  cat <<EOF\nUsage: run under mutter --headless\nEOF\n}\n"),
    ("lanes/b8.sh", "# start mutter --headless here\nx=1 # wireplumber\n"),
    ("lanes/b9.sh", "cp pipewire.log out/ && gst-launch-1.0 pipewiresrc ! fakesink\napt-get install -y pipewire-gstreamer\n"),
    ("lanes/b10.sh", 'die "missing required command: mutter"\n'),
    ("lanes/b11.sh", "cat <<'EOF'\n$(mutter --headless)\nEOF\n"),
    ("lanes/b12.sh", "run --mutter-child x\n"),
    ("Makefile", "test:\n\t@echo \"Running widget tests under mutter --headless...\"\n"),
    (".github/workflows/ok.yml", "jobs:\n  a:\n    steps:\n      - run: |\n          dnf install -y \\\n            dbus-daemon \\\n            mutter \\\n            wireplumber\n          make test\n"),
    ("build-aux/z/Containerfile", "FROM fedora\nRUN dnf install -y \\\n    mutter \\\n    pipewire \\\n && dnf clean all\n"),
    ("tools/ok.py", '"""Runs mutter --headless."""\nimport sys\nprint("missing required command: mutter")\nX = "pipewiresrc"\nY = "pipewire.log"\nZ = f"{n} mutter processes"\n# mutter\n'),
    ("crates/x/src/ok.rs", '//! Runs `mutter --headless`.\n/// dbus-run-session docs\nfn f() { let _ = "missing mutter"; let _ = "--mutter-child"; /* "mutter" */ }\n'),
    ("docs/notes.md", "dbus-run-session -- mutter --headless\n"),
    ("openspec/changes/x/tasks.md", "wireplumber &\n"),
)


def _helper_sample() -> dict[str, str]:
    """Copies of the real helper data when present, else a minimal sample."""

    sample = {
        HELPER_SOURCE: (
            'const X: &str = "dbus-run-session"; const Y: &str = "dbus-daemon";\n'
            'const Z: &str = "mutter"; const P: &str = "pipewire"; const W: &str = "wireplumber";\n'
            'const DAEMON_PRIVATE_HOMES: [&str; 3] = ["XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME"];\n'
        ),
        HELPER_PROFILES: (
            'pub const WIREPLUMBER_PROFILE: &str = "policy";\n'
            "const P: Profile = Profile { system_bus: SystemBus::Private, media: Media::Off };\n"
        ),
        HELPER_PIPEWIRE_FRAGMENT: "context.properties = { module.rt = false }\n",
        HELPER_WIREPLUMBER_FRAGMENT: (
            "context.properties = { module.rt = false }\n"
            "override.context.modules = [ { name = libpipewire-module-protocol-native } ]\n"
            'wireplumber.components.rules = [ { matches = [ { name = "client-context" } ] } ]\n'
        ),
    }
    for path in list(sample):
        real = read_repo(path)
        if real is not None:
            sample[path] = real
    return sample


DRIFT_MUTATIONS = (
    ("profile main", HELPER_PROFILES, r'WIREPLUMBER_PROFILE:\s*&str\s*=\s*"policy"', 'WIREPLUMBER_PROFILE: &str = "main"', "WirePlumber profile drifted"),
    ("hardware.bluetooth required", HELPER_WIREPLUMBER_FRAGMENT, r"\Z", "\nwireplumber.profiles = { policy = { hardware.bluetooth = required } }\n", "hardware monitor enabled"),
    ("module.rt true (PipeWire)", HELPER_PIPEWIRE_FRAGMENT, r"module\.rt\s*=\s*false", "module.rt = true", "PipeWire fragment must set"),
    ("module.rt missing (PipeWire)", HELPER_PIPEWIRE_FRAGMENT, r"module\.rt\s*=\s*false", "", "PipeWire fragment must set"),
    ("module-rt in WirePlumber", HELPER_WIREPLUMBER_FRAGMENT, r"\Z", "\n# x\noverride.context.modules = [ { name = libpipewire-module-rt } ]\n", "lists libpipewire-module-rt"),
    ("user state home", HELPER_SOURCE, r'"XDG_STATE_HOME",?\s*', "", "XDG_STATE_HOME is not helper-private"),
    ("host bus without reason", HELPER_PROFILES, r"\Z", "\nconst H: Profile = Profile { system_bus: SystemBus::Host, media: Media::Off };\n", "without a recorded reason"),
)


def run_self_test(stub_rules: bool = False) -> int:
    """Prove every rule; `stub_rules` runs the fixtures with no rule (red-first)."""

    failures: list[str] = []
    scan = (lambda _p, _t: []) if stub_rules else scan_file
    with tempfile.TemporaryDirectory(prefix="session-hygiene-self-test-") as scratch:
        root = Path(scratch)
        for path, text, expected in SPAWN_FIXTURES:
            fixture = root / path
            fixture.parent.mkdir(parents=True, exist_ok=True)
            fixture.write_text(text, encoding="utf-8")
            found = scan(path, fixture.read_text(encoding="utf-8"))
            if not any(f"`{expected}`" in f.message for f in found):
                failures.append(f"spawn fixture {path} was not caught for `{expected}`: {found}")
            fixture.unlink()
        for path, text in BENIGN_FIXTURES:
            fixture = root / path
            fixture.parent.mkdir(parents=True, exist_ok=True)
            fixture.write_text(text, encoding="utf-8")
            found = scan_file(path, fixture.read_text(encoding="utf-8"))
            if found:
                failures.append(f"benign fixture {path} was flagged: {[f.render() for f in found]}")
            fixture.unlink()

        # Classifier copies: a runner holding any family text fails.
        fragments, error = load_family_fragments()
        if error:
            failures.append(error)
        for fragment in [] if stub_rules else fragments:
            found = scan_classifier_copies("scripts/run-new-lane.sh", f"NOISE='{fragment}'\n", fragments)
            if not found:
                failures.append(f"classifier copy {fragment!r} was not caught")
        if stub_rules:
            failures.append("classifier copies were not caught (no rule)")
        colord = "There is no colord server available"
        if fragments and colord in "".join(fragments):
            if not scan_classifier_copies("scripts/run-x.sh", f"X='{colord}'\n", fragments):
                failures.append("the colord copy was not caught")
        if scan_classifier_copies(CLASSIFIER_MODULE, "".join(fragments), fragments):
            failures.append("the classifier module itself was flagged")

        # A renamed copy of this gate is a violator: its exemption is path-keyed.
        gate_copy = Path(__file__).read_text(encoding="utf-8")
        if stub_rules or not scan("scripts/check-session-hygiene-copy.py", gate_copy):
            failures.append("a renamed copy of the gate was not flagged")
        if scan_file(GATE_PATH, gate_copy):
            failures.append("the gate at its keyed path was flagged")

        # Drift: each mutation of a copy of the helper data fails.
        sample = _helper_sample()
        clean = [] if stub_rules else check_helper_drift(sample.get)
        if clean:
            failures.append(f"the unmutated helper data drifted: {[f.render() for f in clean]}")
        for label, path, pattern, replacement, expected in DRIFT_MUTATIONS:
            mutated = dict(sample)
            mutated[path] = re.sub(pattern, replacement, sample[path], count=1)
            if mutated[path] == sample[path]:
                failures.append(f"drift mutation {label!r} did not apply")
                continue
            found = [] if stub_rules else check_helper_drift(mutated.get)
            if not any(expected in f.message for f in found):
                failures.append(f"drift mutation {label!r} was not caught: {found}")

        # An empty helper (zero sanctioned spawns) fails.
        if stub_rules or not sanctioned_spawn_findings(["fn f() {}\n"]):
            failures.append("an empty helper passed the sanctioned-spawn check")
        if not stub_rules and sanctioned_spawn_findings([sample[HELPER_SOURCE]]) and read_repo(HELPER_SOURCE) is None:
            failures.append("the sample helper has no sanctioned spawn")

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        print(f"session hygiene self-test: {len(failures)} failure(s)", file=sys.stderr)
        return 1
    print(
        f"PASS: session hygiene self-test ({len(SPAWN_FIXTURES)} spawn, "
        f"{len(BENIGN_FIXTURES)} benign, {len(DRIFT_MUTATIONS)} drift fixtures)"
    )
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument(
        "--self-test-without-rules",
        action="store_true",
        help="run the self-test fixtures with every rule stubbed out; must fail (red-first proof)",
    )
    args = parser.parse_args(argv)
    if args.self_test_without_rules:
        status = run_self_test(stub_rules=True)
        return 0 if status else 1
    if args.self_test:
        return run_self_test()
    findings = scan_repo()
    for finding in findings:
        print(finding.render())
    if findings:
        print(
            f"session hygiene: {len(findings)} finding(s); every private headless session "
            "starts through scripts/headless-session or the gtk-lush-proof-harness session library",
            file=sys.stderr,
        )
        return 1
    print("session hygiene: no session spawns outside the helper")
    return 0


if __name__ == "__main__":
    os.environ.setdefault("PYTHONDONTWRITEBYTECODE", "1")
    sys.exit(main(sys.argv[1:]))
