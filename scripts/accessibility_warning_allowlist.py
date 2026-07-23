#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later

"""Single source of truth for accessibility-smoke warning classification.

Both the final warning scan and the summary composer in
``scripts/run-accessibility-smoke.sh`` import this predicate so a warning line
cannot be allowlisted by one path and unexpected by the other.
"""

from __future__ import annotations

import re

_ANSI_STYLE_SEQUENCES = re.compile(r"\x1b\[[0-9;]*m")


def warning_line_is_allowlisted(line: str) -> bool:
    """Return whether one captured warning line is expected smoke noise."""
    # Session logs can carry ANSI style sequences; classify on plain text.
    line = _ANSI_STYLE_SEQUENCES.sub("", line)
    if line.startswith("Gdk-Message: ") and line.endswith(
        "Error reading events from display: Broken pipe"
    ):
        return True
    return (
        "ERROR lushtext_core::ui::editor_page::load_save: Failed to read " in line
        and "unreadable-load-target.txt: Permission denied" in line
    )
