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

- **Headless container session services.** The scheduled smoke workflow runs
  its lanes in a Fedora container with no system D-Bus, no ``/dev/fuse``, and
  no portal backend configured for the private session. The services the
  private session bus activates then report exactly that, once each: Mutter
  cannot reach ``org.freedesktop.locale1`` or colord, ``xdg-desktop-portal``
  cannot reach RealtimeKit or mount the document portal and falls back to the
  GTK backend, ``xdg-desktop-portal-gtk`` cannot connect to the system bus, and
  the portal loses PipeWire when the session tears down. None of these involves
  LushText. Each line is matched by its whole message text, so a different
  warning from the same service still fails the lane. The widget runner
  (``scripts/run-widget-tests.sh``) classifies the same session lines, which is
  how the widget shards pass in the same container image. Observed in the
  accessibility, automation, crash-recovery, and visual lanes.

Shell consumers import this the same way ``run-accessibility-smoke.sh`` imports
``accessibility_warning_allowlist`` (add ``scripts/`` to ``sys.path``), or pipe
log text through the ``--drop-gdk-broken-pipe`` or ``--drop-benign`` filter
mode (the shell equivalent of ``grep -E -v`` against the shared patterns).
"""

from __future__ import annotations

import re

_ANSI_STYLE_SEQUENCES = re.compile(r"\x1b\[[0-9;]*m")
_GDK_BROKEN_PIPE = re.compile(
    r"^Gdk-Message: .*Error reading events from display: Broken pipe$"
)
_TIME = r"(?:[0-9:.]+: )?"
_HEADLESS_CONTAINER_SESSION_SERVICE = re.compile(
    "|".join(
        (
            rf"^\(mutter:[0-9]+\): mutter-WARNING \*\*: {_TIME}"
            r"Failed to acquire org\.freedesktop\.locale1 proxy: Could not connect: No such file or directory$",
            rf"^\(mutter:[0-9]+\): libmutter-WARNING \*\*: {_TIME}"
            r"Failed to connect to colord daemon: Could not connect: No such file or directory$",
            rf"^\(xdg-desktop-portal-gtk:[0-9]+\): xdg-desktop-portal-gtk-WARNING \*\*: {_TIME}"
            r"error: Could not connect: No such file or directory$",
            rf"^\(/usr/libexec/xdg-desktop-portal:[0-9]+\): xdg-desktop-portal-WARNING \*\*: {_TIME}"
            r"(?:Failed to create RealtimeKit proxy: Could not connect: No such file or directory"
            r"|Document portal fuse mount point unknown: GDBus\.Error:org\.freedesktop\.DBus\.Error\.ServiceUnknown: "
            r"The name :[0-9.]+ was not provided by any \.service files"
            r"|Choosing gtk\.portal for org\.freedesktop\.impl\.portal\.[A-Za-z]+ as a last-resort fallback"
            r"|Caught PipeWire error: connection error"
            r"|Failed connect to PipeWire: Couldn't connect to PipeWire)$",
        )
    )
)


def _plain(line: str) -> str:
    """Strip ANSI style sequences and trailing newline before classifying."""
    return _ANSI_STYLE_SEQUENCES.sub("", line).rstrip("\n")


def is_gdk_broken_pipe_teardown(line: str) -> bool:
    """Return whether ``line`` is the benign Gdk broken-pipe teardown warning."""
    return bool(_GDK_BROKEN_PIPE.match(_plain(line)))


def is_headless_container_session_noise(line: str) -> bool:
    """Return whether ``line`` is a session service reporting the container's
    missing system bus, FUSE device, or portal configuration."""
    return bool(_HEADLESS_CONTAINER_SESSION_SERVICE.match(_plain(line)))


def is_benign_session_noise(line: str) -> bool:
    """Return whether ``line`` belongs to any shared benign family above."""
    return is_gdk_broken_pipe_teardown(line) or is_headless_container_session_noise(line)


def _self_test() -> int:
    benign = (
        "Gdk-Message: 14:26:35.271: Error reading events from display: Broken pipe",
        "(mutter:3709): mutter-WARNING **: 14:26:33.177: Failed to acquire org.freedesktop.locale1 proxy: "
        "Could not connect: No such file or directory",
        "(mutter:3709): libmutter-WARNING **: 14:26:33.181: Failed to connect to colord daemon: "
        "Could not connect: No such file or directory",
        "(xdg-desktop-portal-gtk:3761): xdg-desktop-portal-gtk-WARNING **: 14:26:33.413: error: "
        "Could not connect: No such file or directory",
        "(/usr/libexec/xdg-desktop-portal:3742): xdg-desktop-portal-WARNING **: 14:26:33.424: "
        "Failed to create RealtimeKit proxy: Could not connect: No such file or directory",
        "(/usr/libexec/xdg-desktop-portal:3742): xdg-desktop-portal-WARNING **: 14:26:33.366: "
        "Document portal fuse mount point unknown: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: "
        "The name :1.14 was not provided by any .service files",
        "(/usr/libexec/xdg-desktop-portal:3742): xdg-desktop-portal-WARNING **: 14:26:33.425: "
        "Choosing gtk.portal for org.freedesktop.impl.portal.FileChooser as a last-resort fallback",
        "(/usr/libexec/xdg-desktop-portal:3742): xdg-desktop-portal-WARNING **: 14:26:35.302: "
        "Caught PipeWire error: connection error",
        "(/usr/libexec/xdg-desktop-portal:3742): xdg-desktop-portal-WARNING **: 14:26:35.304: "
        "Failed connect to PipeWire: Couldn't connect to PipeWire",
    )
    defects = (
        "(lushtext:12): Gtk-WARNING **: 14:26:33.000: Trying to snapshot GtkGizmo without a current allocation",
        "(lushtext:12): Gtk-CRITICAL **: gtk_widget_unparent: assertion 'GTK_IS_WIDGET (widget)' failed",
        "(xdg-desktop-portal-gtk:3761): xdg-desktop-portal-gtk-WARNING **: 14:26:33.413: error: "
        "Could not open the file chooser",
        "(/usr/libexec/xdg-desktop-portal:3742): xdg-desktop-portal-WARNING **: 14:26:33.424: "
        "Failed to create RealtimeKit proxy: Permission denied",
        "(mutter:3709): mutter-WARNING **: 14:26:33.177: Failed to acquire org.freedesktop.locale1 proxy: "
        "Could not connect: No such file or directory and more",
    )
    failures = [line for line in benign if not is_benign_session_noise(line)]
    failures += [line for line in defects if is_benign_session_noise(line)]
    for line in failures:
        print(f"smoke_warning_classifiers self-test misclassified: {line}")
    if failures:
        return 1
    print("PASS: smoke_warning_classifiers self-test")
    return 0


def _main(argv: list[str]) -> int:
    import sys

    if "--self-test" in argv:
        return _self_test()
    if "--drop-benign" in argv:
        # Copy stdin to stdout, dropping every shared benign family.
        for line in sys.stdin:
            if not is_benign_session_noise(line):
                sys.stdout.write(line)
        return 0
    if "--drop-gdk-broken-pipe" in argv:
        # Copy stdin to stdout, dropping the shared Gdk broken-pipe teardown
        # lines. This is the shell-pipeline equivalent of the former
        # per-script ``grep -E -v '^Gdk-Message: ...Broken pipe$'``.
        for line in sys.stdin:
            if not is_gdk_broken_pipe_teardown(line):
                sys.stdout.write(line)
        return 0
    sys.stderr.write("usage: smoke_warning_classifiers.py --drop-gdk-broken-pipe | --drop-benign | --self-test\n")
    return 2


if __name__ == "__main__":
    import sys

    raise SystemExit(_main(sys.argv[1:]))
