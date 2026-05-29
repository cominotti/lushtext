#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Validate LushText release-note shape and poem stanza reuse."""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path


REQUIRED_HEADINGS = [
    "Poetic Opening",
    "What's Changed",
    "Manual Actions Needed",
    "Warnings and Deprecations",
    "Bug Fixes",
]

APPROVED_POETS = {
    "rimbaud": "non_english",
    "oscar wilde": "english",
    "wilde": "english",
    "baudelaire": "non_english",
    "edgar allan poe": "english",
    "poe": "english",
    "shakespeare": "english",
    "florbela espanca": "non_english",
    "espanca": "non_english",
}

TRUNCATION_PATTERN = re.compile(
    r"(\.\.\.|…|\[\s*(?:full\s+)?line\s+\d+\s*\]|\[\s*ellipsis\s*\]|\bfragment\b|\bexcerpt\b|\bopening lines\b)",
    re.IGNORECASE,
)


def run(cmd: list[str], cwd: Path, check: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=cwd,
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def repo_root(start: Path) -> Path:
    result = run(["git", "rev-parse", "--show-toplevel"], start)
    if result.returncode != 0:
        return start
    return Path(result.stdout.strip())


def heading_positions(text: str) -> dict[str, tuple[int, int]]:
    positions: dict[str, tuple[int, int]] = {}
    pattern = re.compile(r"(?m)^(#{1,6})\s+(.+?)\s*$")
    for match in pattern.finditer(text):
        title = match.group(2).strip()
        for required in REQUIRED_HEADINGS:
            if title.lower() == required.lower():
                positions.setdefault(required, (match.start(), match.end()))
    return positions


def section_body(text: str, heading: str) -> str:
    positions = heading_positions(text)
    if heading not in positions:
        return ""
    _, body_start = positions[heading]
    following = [
        start
        for other, (start, _) in positions.items()
        if other != heading and start > body_start
    ]
    body_end = min(following) if following else len(text)
    return text[body_start:body_end].strip()


def normalize_for_compare(text: str) -> str:
    lines: list[str] = []
    for raw in text.splitlines():
        line = raw.strip()
        line = re.sub(r"^>\s?", "", line).strip()
        line = re.sub(r"^[*-]\s+", "", line).strip()
        lowered = line.lower()
        if not line:
            continue
        if lowered.startswith(("original:", "english:", "source:", "poem:", "author:", "translation:")):
            continue
        if any(name in lowered for name in APPROVED_POETS):
            continue
        lines.append(line)
    normalized = " ".join(lines).lower()
    normalized = re.sub(r"[^a-z0-9]+", " ", normalized)
    return re.sub(r"\s+", " ", normalized).strip()


def detect_poets(text: str) -> set[str]:
    lowered = text.lower()
    return {name for name in APPROVED_POETS if name in lowered}


def poem_content_lines(text: str) -> list[str]:
    lines: list[str] = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            continue
        line = re.sub(r"^>\s?", "", line).strip()
        lowered = line.lower()
        if lowered.startswith(
            (
                "source checked:",
                "selection:",
                "complete stanza:",
                "complete verse:",
                "original",
                "english",
                "source:",
                "poem:",
                "author:",
                "translation:",
            )
        ):
            continue
        if any(name in lowered for name in APPROVED_POETS):
            continue
        lines.append(line)
    return lines


def extract_poetic_opening(text: str) -> str | None:
    body = section_body(text, "Poetic Opening")
    if not body:
        return None
    return normalize_for_compare(body)


def git_grep_poetic_files(root: Path, ref: str | None = None) -> list[tuple[str, str]]:
    if ref:
        result = run(["git", "grep", "-Il", "Poetic Opening", ref, "--", "."], root)
    else:
        result = run(["git", "grep", "-Il", "Poetic Opening", "--", "."], root)
    if result.returncode not in (0, 1):
        return []
    entries: list[tuple[str, str]] = []
    for line in result.stdout.splitlines():
        if ref:
            prefix = f"{ref}:"
            if line.startswith(prefix):
                entries.append((ref, line[len(prefix) :]))
        else:
            entries.append(("HEAD", line))
    return entries


def read_git_file(root: Path, ref: str, path: str) -> str | None:
    if ref == "HEAD":
        file_path = root / path
        if file_path.exists():
            return file_path.read_text(encoding="utf-8", errors="replace")
        return None
    result = run(["git", "show", f"{ref}:{path}"], root)
    if result.returncode != 0:
        return None
    return result.stdout


def local_history_matches(root: Path, notes_path: Path, stanza: str) -> list[str]:
    matches: list[str] = []
    current_rel: str | None = None
    try:
        current_rel = str(notes_path.resolve().relative_to(root.resolve()))
    except ValueError:
        current_rel = None

    refs = ["HEAD"]
    tag_result = run(["git", "tag", "--list", "v*"], root)
    if tag_result.returncode == 0:
        refs.extend(tag_result.stdout.splitlines())

    seen: set[tuple[str, str]] = set()
    for ref in refs:
        for source_ref, path in git_grep_poetic_files(root, None if ref == "HEAD" else ref):
            key = (source_ref, path)
            if key in seen:
                continue
            seen.add(key)
            if source_ref == "HEAD" and current_rel and path == current_rel:
                continue
            text = read_git_file(root, source_ref, path)
            if not text:
                continue
            other = extract_poetic_opening(text)
            if other and other == stanza:
                matches.append(f"{source_ref}:{path}")
            elif stanza and stanza in normalize_for_compare(text):
                matches.append(f"{source_ref}:{path}")
    return matches


def github_release_matches(root: Path, repo: str, stanza: str) -> list[str]:
    if shutil.which("gh") is None:
        raise RuntimeError("gh is not installed")
    list_result = run(
        ["gh", "release", "list", "--repo", repo, "--limit", "100", "--json", "tagName", "--jq", ".[].tagName"],
        root,
    )
    if list_result.returncode != 0:
        raise RuntimeError(list_result.stderr.strip() or "could not list GitHub releases")
    matches: list[str] = []
    for tag in list_result.stdout.splitlines():
        body_result = run(
            ["gh", "release", "view", tag, "--repo", repo, "--json", "body", "--jq", ".body"],
            root,
        )
        if body_result.returncode != 0:
            raise RuntimeError(body_result.stderr.strip() or f"could not read release {tag}")
        body = body_result.stdout
        opening = extract_poetic_opening(body)
        normalized_body = normalize_for_compare(body)
        if (opening and opening == stanza) or (stanza and stanza in normalized_body):
            matches.append(tag)
    return matches


def validate(args: argparse.Namespace) -> int:
    notes = Path(args.notes_file)
    if not notes.exists():
        print(f"error: notes file does not exist: {notes}", file=sys.stderr)
        return 1

    text = notes.read_text(encoding="utf-8", errors="replace")
    positions = heading_positions(text)
    missing = [heading for heading in REQUIRED_HEADINGS if heading not in positions]
    if missing:
        print("error: missing required heading(s): " + ", ".join(missing), file=sys.stderr)
        return 1

    starts = [positions[heading][0] for heading in REQUIRED_HEADINGS]
    if starts != sorted(starts):
        print("error: required headings are not in the expected order", file=sys.stderr)
        return 1

    for heading in REQUIRED_HEADINGS:
        body = section_body(text, heading)
        if not body:
            print(f"error: section is empty: {heading}", file=sys.stderr)
            return 1
        if "TODO" in body:
            print(f"error: section still contains TODO text: {heading}", file=sys.stderr)
            return 1

    opening = section_body(text, "Poetic Opening")
    lowered_opening = opening.lower()
    if TRUNCATION_PATTERN.search(opening):
        print(
            "error: Poetic Opening appears to use a placeholder, fragment, excerpt, or ellipsis",
            file=sys.stderr,
        )
        return 1
    if "source checked:" not in lowered_opening:
        print("error: Poetic Opening must include a Source checked line", file=sys.stderr)
        return 1
    if "selection:" not in lowered_opening or "complete" not in lowered_opening:
        print(
            "error: Poetic Opening must declare Selection: Complete stanza or complete verse",
            file=sys.stderr,
        )
        return 1
    content_lines = [line for line in opening.splitlines() if line.strip()]
    if len(content_lines) < 2:
        print("error: Poetic Opening needs a real stanza, not a single line", file=sys.stderr)
        return 1
    poem_lines = poem_content_lines(opening)
    if len(poem_lines) < 2:
        print("error: Poetic Opening needs at least two poem lines", file=sys.stderr)
        return 1

    detected = detect_poets(opening)
    if not detected:
        print("error: Poetic Opening does not name an approved poet", file=sys.stderr)
        return 1

    if any(APPROVED_POETS[name] == "non_english" for name in detected):
        lowered = opening.lower()
        if "original" not in lowered or "english" not in lowered:
            print(
                "error: non-English poem openings must include both Original and English labels",
                file=sys.stderr,
            )
            return 1

    stanza = normalize_for_compare(opening)
    if len(stanza) < 40:
        print("error: normalized poetic stanza is too short to check for reuse", file=sys.stderr)
        return 1

    root = Path(args.repo_root).resolve() if args.repo_root else repo_root(notes.parent)
    local_matches = local_history_matches(root, notes, stanza)
    if local_matches:
        print("error: poetic stanza appears to repeat prior local release notes:", file=sys.stderr)
        for match in local_matches:
            print(f"  - {match}", file=sys.stderr)
        return 1

    if args.gh_repo:
        try:
            gh_matches = github_release_matches(root, args.gh_repo, stanza)
        except RuntimeError as exc:
            print(f"error: GitHub release double-check failed: {exc}", file=sys.stderr)
            return 1
        if gh_matches:
            print("error: poetic stanza appears in prior GitHub Release body:", file=sys.stderr)
            for match in gh_matches:
                print(f"  - {match}", file=sys.stderr)
            return 1
        print(f"OK: release notes shape and poem checks passed, including GitHub repo {args.gh_repo}")
    else:
        print("OK: release notes shape and local poem checks passed")
        print("NOTE: run again with --gh-repo cominotti/lushtext before publishing")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("notes_file")
    parser.add_argument("--repo-root")
    parser.add_argument("--gh-repo", help="GitHub repository, for example cominotti/lushtext")
    return validate(parser.parse_args())


if __name__ == "__main__":
    raise SystemExit(main())
