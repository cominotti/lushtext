#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Short private runtime directories for the D-Bus-session smoke drivers.

Mutter, AT-SPI, and PipeWire create Unix sockets under ``XDG_RUNTIME_DIR``, and
a socket path longer than ``sun_path`` (108 bytes) fails, so a runtime
directory inside a deep checkout's artifact tree (a git worktree under
``.claude/worktrees/`` is one) stops the lane before it exercises the app. The
drivers therefore keep it in a short ``mkdtemp`` root and remove it, after the
session's services have let go of it, on every exit path; the artifacts keep a
listing of what it contained.
"""

from __future__ import annotations

import os
import shutil
import tempfile
import time
from pathlib import Path


# The deepest socket a session binds in its runtime directory is the session
# helper's private system bus, and dbus-daemon refuses a socket path over 99
# bytes (gtk-lush-proof-harness `DBUS_SOCKET_PATH_MAX`, below the kernel's 107).
SESSION_DEEPEST_SOCKET = "gtk-lush-session/system-bus"
DBUS_SOCKET_PATH_MAX = 99
# Where the root goes when the temp directory is too deep (the harness's
# `SHORT_RUNTIME_BASE`).
SHORT_RUNTIME_BASE = "/tmp"
# `tempfile.mkdtemp` appends eight random characters to the prefix.
MKDTEMP_SUFFIX_CHARS = 8


def runtime_dir_fits_sockets(runtime_dir: Path) -> bool:
    """Whether a session in runtime_dir can bind its deepest socket."""
    return len(os.fsencode(str(runtime_dir / SESSION_DEEPEST_SOCKET))) <= DBUS_SOCKET_PATH_MAX


def runtime_base(prefix: str, temp_dir: str | None = None) -> str:
    """The directory to create a runtime root in: the temp directory, unless
    a runtime directory inside it could not bind the session's sockets.

    A lane runs inside a ``with-temp-scope.sh`` scope, whose ``TMPDIR`` grows
    with the scope's PID and label, so the check is made per run rather than
    assumed from the prefix.
    """
    base = os.path.realpath(temp_dir or tempfile.gettempdir())
    probe = Path(base) / (prefix + "X" * MKDTEMP_SUFFIX_CHARS) / "runtime"
    return base if runtime_dir_fits_sockets(probe) else SHORT_RUNTIME_BASE


def create_runtime_root(prefix: str) -> tuple[Path, Path]:
    """Create a short private runtime root and its mode-0700 ``runtime`` dir."""
    runtime_root = Path(tempfile.mkdtemp(prefix=prefix, dir=runtime_base(prefix)))
    runtime_dir = runtime_root / "runtime"
    runtime_dir.mkdir()
    os.chmod(runtime_dir, 0o700)
    return runtime_root, runtime_dir


def session_runtime_in_use(runtime_dir: Path) -> bool:
    """Whether any readable process still carries runtime_dir as XDG_RUNTIME_DIR."""
    marker = b"XDG_RUNTIME_DIR=" + os.fsencode(str(runtime_dir))
    for proc in Path("/proc").iterdir():
        if not proc.name.isdigit():
            continue
        try:
            environ = (proc / "environ").read_bytes()
        except OSError:
            continue
        if marker in environ.split(b"\0"):
            return True
    return False


def wait_for_session_release(runtime_dir: Path, timeout: float = 10.0) -> None:
    """Wait until the finished session's services have let go of runtime_dir.

    dbus-run-session returns when its direct child exits, while services its
    private bus activated are still shutting down; xdg-document-portal keeps
    its FUSE mount on doc/ until then, so removing the directory at once fails
    and leaves it behind.
    """
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline and session_runtime_in_use(runtime_dir):
        time.sleep(0.05)


def cleanup_runtime_root(runtime_root: Path, artifact_dir: Path) -> str:
    """Remove the short runtime root after recording what it contained.

    The sockets and mounts in it are dead once the session has exited, so the
    root is removed on failure too; the listing is kept in the artifacts.
    """
    wait_for_session_release(runtime_root / "runtime")
    listing: list[str] = []
    for path in runtime_root.rglob("*"):
        try:
            listing.append(str(path.relative_to(runtime_root)))
        except (ValueError, OSError):
            listing.append(str(path))
    if artifact_dir.is_dir():
        (artifact_dir / "runtime-dir-listing.txt").write_text(
            "\n".join(sorted(listing)) + "\n", encoding="utf-8"
        )
    errors: list[str] = []
    for attempt in range(1, 6):
        try:
            shutil.rmtree(runtime_root)
        except FileNotFoundError:
            return "removed"
        except Exception as exc:
            errors.append(f"attempt {attempt}: {type(exc).__name__}: {exc}")
        if not runtime_root.exists():
            return "removed"
        time.sleep(0.2)

    remaining: list[str] = []
    for path in runtime_root.rglob("*"):
        try:
            remaining.append(str(path.relative_to(runtime_root)))
        except ValueError:
            remaining.append(str(path))
    (artifact_dir / "runtime-dir-cleanup-errors.txt").write_text(
        "\n".join([*errors, "remaining:", *sorted(remaining)]) + "\n",
        encoding="utf-8",
    )
    return "remove_failed"


def _self_test() -> None:
    """The fit rule at dbus's boundary, and the fallback for a deep TMPDIR."""
    suffix = "/" + SESSION_DEEPEST_SOCKET
    fits = "/" + "r" * (DBUS_SOCKET_PATH_MAX - len(suffix) - 1)
    assert runtime_dir_fits_sockets(Path(fits))
    assert not runtime_dir_fits_sockets(Path(fits + "r"))
    # The crash lane's scope, which a 7-digit PID made one byte too long.
    deep = "/tmp/lt-scope-2366576-crash-recovery-mg9Kpb"
    assert runtime_base("lt-crash-rt-", deep) == SHORT_RUNTIME_BASE
    assert runtime_base("lt-crash-rt-", "/tmp") == "/tmp"
    print("smoke_runtime_dir self-test passed.")


if __name__ == "__main__":
    import sys

    if sys.argv[1:] != ["--self-test"]:
        raise SystemExit("usage: smoke_runtime_dir.py --self-test")
    _self_test()
