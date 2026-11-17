#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Print the rustc-wrapper cargo would use from a directory, ignoring ours.

scripts/heavy.sh puts scripts/rustc-frontend-threads.sh in RUSTC_WRAPPER,
which overrides any configured wrapper, so it resolves the one cargo would
otherwise have run (CST's sccache wrapper on this workstation) and the
front-end wrapper hands every call on to it. Prints an absolute path, a bare
program name (looked up on PATH, as cargo does), or nothing for no wrapper.

    scripts/next-rustc-wrapper.py [--self-test] [<dir>]

Resolution follows cargo: the RUSTC_WRAPPER environment variable, then
CARGO_BUILD_RUSTC_WRAPPER, then `build.rustc-wrapper` in the first config
file that sets it, nearest first: `.cargo/config.toml` (or the legacy
`.cargo/config`) in <dir> and each ancestor, then $CARGO_HOME (default
~/.cargo). An empty value means "no wrapper". A configured value containing a
slash is relative to the directory holding `.cargo`. A value naming the
front-end wrapper itself is skipped, so a nested heavy.sh never loops.
"""

from __future__ import annotations

import os
import sys
import tempfile
import tomllib
from pathlib import Path

OURS = "rustc-frontend-threads.sh"


def _is_ours(value: str) -> bool:
    return Path(value).name == OURS


def _config_files(start: Path, cargo_home: Path) -> list[tuple[Path, Path]]:
    """(config file, directory its relative paths resolve against), nearest first."""
    found: list[tuple[Path, Path]] = []
    seen: set[Path] = set()
    for directory in [start, *start.parents]:
        for name in ("config.toml", "config"):
            candidate = directory / ".cargo" / name
            if candidate.is_file():
                found.append((candidate, directory))
                seen.add(candidate.resolve())
                break
    for name in ("config.toml", "config"):
        candidate = cargo_home / name
        if candidate.is_file():
            if candidate.resolve() not in seen:
                found.append((candidate, cargo_home.parent))
            break
    return found


def resolve(start: Path, environ: dict[str, str]) -> str:
    for variable in ("RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER"):
        if variable in environ:
            value = environ[variable]
            if not _is_ours(value):
                return value
    cargo_home = Path(environ.get("CARGO_HOME") or Path(environ.get("HOME", "/")) / ".cargo")
    for config, base in _config_files(start, cargo_home):
        try:
            data = tomllib.loads(config.read_text(encoding="utf-8"))
        except (OSError, tomllib.TOMLDecodeError) as error:
            raise SystemExit(f"next-rustc-wrapper: cannot read {config}: {error}") from error
        build = data.get("build")
        if not isinstance(build, dict) or "rustc-wrapper" not in build:
            continue
        value = str(build["rustc-wrapper"])
        if not value:
            return ""
        if _is_ours(value):
            continue
        if "/" in value and not os.path.isabs(value):
            return str(base / value)
        return value
    return ""


def self_test() -> int:
    failures = 0

    def check(name: str, got: str, want: str) -> None:
        nonlocal failures
        if got == want:
            print(f"ok - {name}")
        else:
            print(f"FAIL - {name}: got {got!r}, want {want!r}")
            failures += 1

    with tempfile.TemporaryDirectory(prefix="lt-next-wrapper-") as raw:
        root = Path(raw)
        home = root / "home"
        (home / ".cargo").mkdir(parents=True)
        project = root / "work" / "project"
        (project / "crate").mkdir(parents=True)
        env = {"HOME": str(home)}

        check("no config, no wrapper", resolve(project / "crate", env), "")
        (home / ".cargo" / "config.toml").write_text(
            '[build]\nrustc-wrapper = "/opt/cst/rustc-wrapper"\n', encoding="utf-8"
        )
        check("the home config", resolve(project / "crate", env), "/opt/cst/rustc-wrapper")
        check(
            "CARGO_HOME replaces ~/.cargo",
            resolve(project, {**env, "CARGO_HOME": str(root / "elsewhere")}),
            "",
        )
        (root / "work" / ".cargo").mkdir()
        (root / "work" / ".cargo" / "config.toml").write_text(
            '[build]\nrustc-wrapper = "tools/wrap"\n', encoding="utf-8"
        )
        check(
            "a nearer config wins, relative to the directory holding .cargo",
            resolve(project / "crate", env),
            str(root / "work" / "tools" / "wrap"),
        )
        (project / ".cargo").mkdir()
        (project / ".cargo" / "config.toml").write_text(
            '[build]\njobs = 2\n', encoding="utf-8"
        )
        check(
            "a nearer config without the key is skipped",
            resolve(project / "crate", env),
            str(root / "work" / "tools" / "wrap"),
        )
        (project / ".cargo" / "config.toml").write_text(
            '[build]\nrustc-wrapper = "sccache"\n', encoding="utf-8"
        )
        check("a bare name stays a PATH lookup", resolve(project, env), "sccache")
        (project / ".cargo" / "config.toml").write_text(
            '[build]\nrustc-wrapper = ""\n', encoding="utf-8"
        )
        check("an empty value means no wrapper", resolve(project, env), "")
        (project / ".cargo" / "config.toml").write_text(
            '[build]\nrustc-wrapper = "/x/scripts/rustc-frontend-threads.sh"\n', encoding="utf-8"
        )
        check(
            "the front-end wrapper itself is skipped",
            resolve(project, env),
            str(root / "work" / "tools" / "wrap"),
        )
        check(
            "RUSTC_WRAPPER wins over config",
            resolve(project, {**env, "RUSTC_WRAPPER": "/env/wrap"}),
            "/env/wrap",
        )
        check(
            "an empty RUSTC_WRAPPER means no wrapper",
            resolve(project, {**env, "RUSTC_WRAPPER": ""}),
            "",
        )
        check(
            "CARGO_BUILD_RUSTC_WRAPPER wins over config",
            resolve(project, {**env, "CARGO_BUILD_RUSTC_WRAPPER": "/b/wrap"}),
            "/b/wrap",
        )
        check(
            "our own RUSTC_WRAPPER (a nested heavy.sh) falls through",
            resolve(
                project,
                {**env, "RUSTC_WRAPPER": "/repo/scripts/rustc-frontend-threads.sh"},
            ),
            str(root / "work" / "tools" / "wrap"),
        )

    if failures:
        print(f"next-rustc-wrapper self-test: {failures} failure(s)", file=sys.stderr)
        return 1
    print("next-rustc-wrapper self-test: all checks passed")
    return 0


def main(argv: list[str]) -> int:
    if argv[1:] == ["--self-test"]:
        return self_test()
    if len(argv) > 2:
        print(__doc__, file=sys.stderr)
        return 2
    start = Path(argv[1] if len(argv) == 2 else os.getcwd()).resolve()
    print(resolve(start, dict(os.environ)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
