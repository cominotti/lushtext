#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Cross-lane classifiers for benign toolkit warnings in headless smoke lanes.

Some benign toolkit warnings appear in more than one smoke lane. Each such
shared warning *family* gets a single classifier here so the next allowlist
adjustment cannot silently diverge across lanes. Lane-specific allowlist entries
stay owned by their own lane (for example the accessibility lane's
permission-denied load error in ``accessibility_warning_allowlist.py`` and the
automation lane's portal/PipeWire warnings in ``automation-smoke-driver.py``);
only families genuinely observed across lanes belong here.

Shared families:

- **Gdk broken-pipe teardown.** The private headless compositor tears down the
  display socket during shutdown, so a late ``Gdk-Message`` about a broken pipe
  while reading display events is expected teardown noise, not an app defect.
  Observed in the accessibility, crash-recovery, automation, visual, and
  visual-geometry lanes.

Shell consumers import this the same way ``run-accessibility-smoke.sh`` imports
``accessibility_warning_allowlist`` (add ``scripts/`` to ``sys.path``), or pipe
log text through the ``--drop-gdk-broken-pipe`` filter mode (the shell
equivalent of ``grep -E -v`` against the shared pattern).
"""

from __future__ import annotations

import re

_ANSI_STYLE_SEQUENCES = re.compile(r"\x1b\[[0-9;]*m")
_GDK_BROKEN_PIPE = re.compile(
    r"^Gdk-Message: .*Error reading events from display: Broken pipe$"
)


def _plain(line: str) -> str:
    """Strip ANSI style sequences and trailing newline before classifying."""
    return _ANSI_STYLE_SEQUENCES.sub("", line).rstrip("\n")


def is_gdk_broken_pipe_teardown(line: str) -> bool:
    """Return whether ``line`` is the benign Gdk broken-pipe teardown warning."""
    return bool(_GDK_BROKEN_PIPE.match(_plain(line)))


def _main(argv: list[str]) -> int:
    import sys

    if "--drop-gdk-broken-pipe" in argv:
        # Copy stdin to stdout, dropping the shared Gdk broken-pipe teardown
        # lines. This is the shell-pipeline equivalent of the former
        # per-script ``grep -E -v '^Gdk-Message: ...Broken pipe$'``.
        for line in sys.stdin:
            if not is_gdk_broken_pipe_teardown(line):
                sys.stdout.write(line)
        return 0
    sys.stderr.write("usage: smoke_warning_classifiers.py --drop-gdk-broken-pipe\n")
    return 2


if __name__ == "__main__":
    import sys

    raise SystemExit(_main(sys.argv[1:]))
