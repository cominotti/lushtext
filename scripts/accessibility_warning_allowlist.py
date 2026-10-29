#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Single source of truth for accessibility-smoke warning classification.

Both the final warning scan and the summary composer in
``scripts/run-accessibility-smoke.sh`` import this predicate so a warning line
cannot be allowlisted by one path and unexpected by the other. The shared
cross-lane families (for example Gdk broken-pipe teardown) come from
``smoke_warning_classifiers`` so an adjustment cannot diverge from the other
smoke lanes; only the accessibility-lane-specific entries live here.
"""

from __future__ import annotations

import re

from smoke_warning_classifiers import is_benign_session_noise

_ANSI_STYLE_SEQUENCES = re.compile(r"\x1b\[[0-9;]*m")


def warning_line_is_allowlisted(line: str) -> bool:
    """Return whether one captured warning line is expected smoke noise."""
    # Shared cross-lane families: Gdk broken-pipe teardown and the headless
    # container session services (see smoke_warning_classifiers).
    if is_benign_session_noise(line):
        return True
    # Lane-specific: the accessibility smoke deliberately fails to load a
    # target (read-fault binary) to exercise the permission-denied path, as plain text
    # (`editor-failed-load`) and as Markdown (`preview-mode-document-alert`).
    # Classify on plain text because session logs can carry ANSI style
    # sequences.
    line = _ANSI_STYLE_SEQUENCES.sub("", line)
    return "ERROR lushtext_core::ui::editor_page::load::execution: Failed to read " in line and (
        "unreadable-load-target.txt: Permission denied" in line
        or "unreadable-load-target.md: Permission denied" in line
    )
