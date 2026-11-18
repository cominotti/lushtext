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

- **Headless session services, through the private system bus.** Lanes now
  start through the session helper (``scripts/headless-session``), whose
  private system bus answers but runs no locale1, colord or RealtimeKit
  service. Mutter then reports ``Failed to retrieve locale properties`` and
  ``There is no colord server available``, and the portal frontend ``Failed
  to load RealtimeKit property``, each with the bus's exact ``ServiceUnknown``
  text. They join the family above (one family, not a parallel one).

Shell consumers import this the same way ``run-accessibility-smoke.sh`` imports
``accessibility_warning_allowlist`` (add ``scripts/`` to ``sys.path``), or pipe
log text through the ``--drop-gdk-broken-pipe`` or ``--drop-benign`` filter
mode (the shell equivalent of ``grep -E -v`` against the shared patterns). A
runner that builds one ``grep -E`` expression reads a family with
``--print-regex <family>`` (POSIX ERE) and its self-test samples with
``--print-samples <family>``, so no runner embeds a copy of a family line
(``make check-session-hygiene`` fails on such a copy). Families:
``headless-session-services`` and ``gdk-broken-pipe``.
"""

from __future__ import annotations

import re

_ANSI_STYLE_SEQUENCES = re.compile(r"\x1b\[[0-9;]*m")
_GDK_BROKEN_PIPE = re.compile(
    r"^Gdk-Message: .*Error reading events from display: Broken pipe$"
)
# Each family pattern is written in the common subset of Python `re` and POSIX
# ERE (plain groups, bracket expressions, backslash-escaped metacharacters, no
# `(?:`, `\d` or lazy quantifiers), so `--print-regex` hands shell consumers the
# very text Python matches with, and the self-test proves it with `grep -E`.
_TIME = r"([0-9:.]+: )?"
_SERVICE_UNKNOWN = r"GDBus\.Error:org\.freedesktop\.DBus\.Error\.ServiceUnknown: "
_HEADLESS_SESSION_SERVICES_ERE = "|".join(
    (
        rf"^\(mutter:[0-9]+\): mutter-WARNING \*\*: {_TIME}"
        r"Failed to acquire org\.freedesktop\.locale1 proxy: Could not connect: No such file or directory$",
        rf"^\(mutter:[0-9]+\): libmutter-WARNING \*\*: {_TIME}"
        r"Failed to connect to colord daemon: Could not connect: No such file or directory$",
        # The private system bus of the session helper: reachable, but with
        # no locale1, colord or RealtimeKit service behind it.
        rf"^\(mutter:[0-9]+\): mutter-WARNING \*\*: {_TIME}"
        rf"Failed to retrieve locale properties: {_SERVICE_UNKNOWN}"
        r"The name org\.freedesktop\.locale1 was not provided by any \.service files$",
        rf"^\(mutter:[0-9]+\): libmutter-WARNING \*\*: {_TIME}There is no colord server available$",
        rf"^\(xdg-desktop-portal-gtk:[0-9]+\): xdg-desktop-portal-gtk-WARNING \*\*: {_TIME}"
        r"error: Could not connect: No such file or directory$",
        rf"^\(/usr/libexec/xdg-desktop-portal:[0-9]+\): xdg-desktop-portal-WARNING \*\*: {_TIME}"
        r"(Failed to create RealtimeKit proxy: Could not connect: No such file or directory"
        rf"|Failed to load RealtimeKit property: {_SERVICE_UNKNOWN}"
        r"The name org\.freedesktop\.RealtimeKit1 was not provided by any \.service files"
        rf"|Document portal fuse mount point unknown: {_SERVICE_UNKNOWN}"
        r"The name :[0-9.]+ was not provided by any \.service files"
        r"|Choosing gtk\.portal for org\.freedesktop\.impl\.portal\.[A-Za-z]+ as a last-resort fallback"
        r"|Caught PipeWire error: connection error"
        r"|Failed connect to PipeWire: Couldn't connect to PipeWire)$",
    )
)
_HEADLESS_CONTAINER_SESSION_SERVICE = re.compile(_HEADLESS_SESSION_SERVICES_ERE)
_GDK_BROKEN_PIPE_ERE = _GDK_BROKEN_PIPE.pattern

# Sample lines of each family, one per alternative: the self-tests of every
# consumer read them through `--print-samples` instead of embedding copies.
_HEADLESS_SESSION_SERVICES_SAMPLES = (
    "(mutter:3709): mutter-WARNING **: 14:26:33.177: Failed to acquire org.freedesktop.locale1 proxy: "
    "Could not connect: No such file or directory",
    "(mutter:3709): libmutter-WARNING **: 14:26:33.181: Failed to connect to colord daemon: "
    "Could not connect: No such file or directory",
    "(mutter:3709): mutter-WARNING **: 23:40:12.502: Failed to retrieve locale properties: "
    "GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: The name org.freedesktop.locale1 was not "
    "provided by any .service files",
    "(mutter:3709): libmutter-WARNING **: 23:40:12.498: There is no colord server available",
    "(xdg-desktop-portal-gtk:3761): xdg-desktop-portal-gtk-WARNING **: 14:26:33.413: error: "
    "Could not connect: No such file or directory",
    "(/usr/libexec/xdg-desktop-portal:3742): xdg-desktop-portal-WARNING **: 14:26:33.424: "
    "Failed to create RealtimeKit proxy: Could not connect: No such file or directory",
    "(/usr/libexec/xdg-desktop-portal:3742): xdg-desktop-portal-WARNING **: 23:40:13.010: "
    "Failed to load RealtimeKit property: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: "
    "The name org.freedesktop.RealtimeKit1 was not provided by any .service files",
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
_GDK_BROKEN_PIPE_SAMPLES = ("Gdk-Message: 14:26:35.271: Error reading events from display: Broken pipe",)

# Near misses: same service, different text. Each must stay a defect.
_HEADLESS_SESSION_SERVICES_NEAR_MISSES = (
    "(mutter:3709): mutter-WARNING **: 14:26:33.177: Failed to acquire org.freedesktop.locale1 proxy: "
    "Could not connect: No such file or directory and more",
    "(mutter:3709): mutter-WARNING **: 23:40:12.502: Failed to retrieve locale properties: "
    "GDBus.Error:org.freedesktop.DBus.Error.AccessDenied: The name org.freedesktop.locale1 was not "
    "provided by any .service files",
    "(mutter:3709): libmutter-WARNING **: 23:40:12.498: There is no colord server available yet",
    "(xdg-desktop-portal-gtk:3761): xdg-desktop-portal-gtk-WARNING **: 14:26:33.413: error: "
    "Could not open the file chooser",
    "(/usr/libexec/xdg-desktop-portal:3742): xdg-desktop-portal-WARNING **: 14:26:33.424: "
    "Failed to create RealtimeKit proxy: Permission denied",
    "(/usr/libexec/xdg-desktop-portal:3742): xdg-desktop-portal-WARNING **: 23:40:13.010: "
    "Failed to load RealtimeKit property: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: "
    "The name org.freedesktop.RealtimeKit2 was not provided by any .service files",
)

# Family name -> (POSIX ERE, benign samples, near-miss defects).
_FAMILIES = {
    "headless-session-services": (
        _HEADLESS_SESSION_SERVICES_ERE,
        _HEADLESS_SESSION_SERVICES_SAMPLES,
        _HEADLESS_SESSION_SERVICES_NEAR_MISSES,
    ),
    "gdk-broken-pipe": (_GDK_BROKEN_PIPE_ERE, _GDK_BROKEN_PIPE_SAMPLES, ()),
}


def family_pattern(name: str) -> str:
    """The family's pattern, valid as both Python `re` and POSIX ERE."""
    return _FAMILIES[name][0]


def family_samples(name: str) -> tuple[str, ...]:
    """One benign sample line per alternative of the family."""
    return _FAMILIES[name][1]


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


def _grep_e_matches(pattern: str, lines: tuple[str, ...]) -> list[bool] | None:
    """Which `lines` POSIX `grep -E` matches with `pattern`; None without grep."""
    import shutil
    import subprocess

    if shutil.which("grep") is None:
        return None
    matches = []
    for line in lines:
        result = subprocess.run(["grep", "-qE", pattern], input=line + "\n", text=True, check=False)
        if result.returncode not in (0, 1):
            raise RuntimeError(f"grep -E rejected the family pattern (exit {result.returncode})")
        matches.append(result.returncode == 0)
    return matches


def _self_test() -> int:
    benign = _GDK_BROKEN_PIPE_SAMPLES + _HEADLESS_SESSION_SERVICES_SAMPLES
    defects = (
        "(lushtext:12): Gtk-WARNING **: 14:26:33.000: Trying to snapshot GtkGizmo without a current allocation",
        "(lushtext:12): Gtk-CRITICAL **: gtk_widget_unparent: assertion 'GTK_IS_WIDGET (widget)' failed",
    ) + _HEADLESS_SESSION_SERVICES_NEAR_MISSES
    failures = [line for line in benign if not is_benign_session_noise(line)]
    failures += [line for line in defects if is_benign_session_noise(line)]
    # Every alternative of a family has a sample, so a pattern edit cannot
    # leave one untested.
    alternatives = _HEADLESS_SESSION_SERVICES_ERE.count("|^") + 1
    if alternatives + 5 != len(_HEADLESS_SESSION_SERVICES_SAMPLES):
        failures.append(f"headless-session-services: {alternatives} anchored alternatives (+5 portal texts) "
                        f"but {len(_HEADLESS_SESSION_SERVICES_SAMPLES)} samples")
    # The shell consumers match with `grep -E` over `--print-regex`: the same
    # verdict on every benign sample and every near miss.
    for name, (pattern, samples, near_misses) in _FAMILIES.items():
        verdicts = _grep_e_matches(pattern, samples + near_misses)
        if verdicts is None:
            continue
        expected = [True] * len(samples) + [False] * len(near_misses)
        for line, got, want in zip(samples + near_misses, verdicts, expected):
            if got != want:
                failures.append(f"{name} (grep -E): {line}")
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
    for flag, emit in (("--print-regex", lambda name: [family_pattern(name)]),
                       ("--print-samples", family_samples)):
        if flag in argv:
            index = argv.index(flag)
            name = argv[index + 1] if index + 1 < len(argv) else ""
            if name not in _FAMILIES:
                sys.stderr.write(f"{flag} needs a family: {', '.join(sorted(_FAMILIES))}\n")
                return 2
            for line in emit(name):
                sys.stdout.write(line + "\n")
            return 0
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
    sys.stderr.write(
        "usage: smoke_warning_classifiers.py --drop-gdk-broken-pipe | --drop-benign | --self-test"
        " | --print-regex FAMILY | --print-samples FAMILY\n"
    )
    return 2


if __name__ == "__main__":
    import sys

    raise SystemExit(_main(sys.argv[1:]))
