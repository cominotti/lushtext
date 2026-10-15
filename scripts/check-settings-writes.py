#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Every LushText GSettings access goes through the coalescing settings writer.

`bound-window-construction-dispatch` made `ui/settings_writes.rs` the one
place that owns the application's settings instance and writes to the
GSettings backend: a keyfile-backend write rewrites and fsyncs the settings
file on the calling thread (ledger A38), so an imperative write anywhere else
would put that cost back on window construction or an interaction dispatch,
and a second `gio::Settings` instance would neither see unflushed values nor
notify its subscribers when they are written (A39, A41).

Outside `crates/lushtext-core/src/ui/settings_writes.rs`, this check fails on:

- `Settings::new(...APP_ID...)`: a second instance of the application schema
  (use `settings_writes::shared()`);
- an imperative write or reset of an application key on a settings object
  (`.set_boolean(keys::...)`, `.set_string(config::keys::...)`, `.reset(keys::...)`
  and the other `set_*` forms): use `settings_writes::set_*` / `reset`.

Reads, `connect_changed` and `Settings::bind` stay allowed: they go through the
shared instance, so a bound Preferences row writes into the writer's delayed
instance too and needs no allowlist.

`--self-test` proves the check red on deliberate violations.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
SCAN_ROOTS = ("crates/lushtext-core/src", "crates/lushtext/src")
WRITER = "crates/lushtext-core/src/ui/settings_writes.rs"

SECOND_INSTANCE = re.compile(r"Settings::new\(\s*(?:crate::)?(?:config::)?APP_ID\s*\)", re.S)
DIRECT_WRITE = re.compile(
    r"\.(?:set_(?:boolean|int|int64|uint|uint64|double|string|strv|enum|flags|value)|reset)"
    r"\(\s*(?:crate::)?(?:config::)?keys::",
    re.S,
)


def strip_comments(text: str) -> str:
    """Drop `//` comments so prose that names the pattern does not trip it."""
    return "\n".join(line.split("//", 1)[0] if "//" in line and '"' not in line.split("//", 1)[0] else line for line in text.splitlines())


def findings_for(path: str, text: str) -> list[str]:
    if path == WRITER:
        return []
    code = strip_comments(text)
    found = []
    for pattern, message in (
        (SECOND_INSTANCE, "a second application settings instance; use settings_writes::shared()"),
        (DIRECT_WRITE, "a direct settings write; use settings_writes::set_* / reset"),
    ):
        for match in pattern.finditer(code):
            line = code.count("\n", 0, match.start()) + 1
            found.append(f"{path}:{line}: {message}")
    return found


def scan() -> list[str]:
    findings: list[str] = []
    for root in SCAN_ROOTS:
        for path in sorted((REPO_ROOT / root).rglob("*.rs")):
            relative = path.relative_to(REPO_ROOT).as_posix()
            findings.extend(findings_for(relative, path.read_text(encoding="utf-8")))
    return findings


def self_test() -> int:
    cases = [
        ("crates/lushtext-core/src/ui/x.rs", "let s = gio::Settings::new(config::APP_ID);", 1),
        ("crates/lushtext-core/src/ui/x.rs", "let s = gtk4::gio::Settings::new(crate::config::APP_ID);", 1),
        ("crates/lushtext-core/src/ui/x.rs", "let _ = settings.set_boolean(keys::WINDOW_MAXIMIZED, true);", 1),
        ("crates/lushtext-core/src/ui/x.rs", "window\n    .imp()\n    .settings\n    .set_double(\n        keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, 0.3);", 1),
        ("crates/lushtext-core/src/ui/x.rs", "prefs.imp().settings.reset(keys::WORKSPACE_EXCLUDED_NAMES);", 1),
        ("crates/lushtext-core/src/ui/x.rs", "settings_writes::set_boolean(keys::WINDOW_MAXIMIZED, true);", 0),
        ("crates/lushtext-core/src/ui/x.rs", "settings.bind(keys::WORD_WRAP, &row, \"active\").build();", 0),
        ("crates/lushtext-core/src/ui/x.rs", "// gio::Settings::new(config::APP_ID) in prose", 0),
        ("crates/lushtext-core/src/ui/x.rs", "adjustment.set_value(lower);", 0),
        (WRITER, "let s = gio::Settings::new(config::APP_ID); s.set_value(key, v);", 0),
    ]
    failures = []
    for path, text, expected in cases:
        got = len(findings_for(path, text))
        if got != expected:
            failures.append(f"{text!r}: expected {expected} finding(s), got {got}")
    if failures:
        print("check-settings-writes self-test failed:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print(f"check-settings-writes self-test passed ({len(cases)} cases).")
    return 0


def main(argv: list[str]) -> int:
    if argv[1:] == ["--self-test"]:
        return self_test()
    findings = scan()
    if findings:
        print("GSettings access outside the coalescing settings writer:", file=sys.stderr)
        for finding in findings:
            print(f"  {finding}", file=sys.stderr)
        return 1
    print("Every LushText GSettings write goes through ui/settings_writes.rs.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
