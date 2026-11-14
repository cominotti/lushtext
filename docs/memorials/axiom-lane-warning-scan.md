# GTK axiom lane: the warning scan and the sample flag guard

Why `scripts/run-gtk-axioms.sh` fails on any warning line, why the runtimes
lane runs SDK sandboxes without a session bus, and why samples call
`support::guard` first. The rules are in
[`.agents/rules/build.md`](../../.agents/rules/build.md), "GTK Axioms and
Toolkit Updates", and [`.agents/rules/testing.md`](../../.agents/rules/testing.md).

## Helper processes log with no domain

Matching only `Gtk-`/`GLib-`/`Adwaita-` prefixes missed helper processes, which
log with no domain: `make gtk-axioms-runtimes` passed while the SDK sandbox's
`xdg-dbus-proxy` printed
`** (process:N): WARNING **: Error writing credentials to socket: Error sending message: Broken pipe`
(a sample's gvfs client exiting mid-handshake) and the session could not remove
the document portal's `doc` mount from its runtime directory. The runtimes lane
therefore runs every SDK sandbox, build and checks alike, with
`--no-session-bus --no-a11y-bus --no-documents-portal` (`SANDBOX_FLAGS`): the
samples need no D-Bus, and without the proxy and portal neither race exists.

## 2026-11-04: a sample opened its window on the live desktop

A sample run with `--help`, which it then ignored, opened its window on the
maintainer's live desktop. Since then a sample with its own measurement modes
declares them in a `SampleFlags` and calls `support::guard(&FLAGS)` as the
first statement of `main`, and `make check-gtk-axioms` fails a sample that
reads flags without it.

## The GNOME 50 and 51 SDK runs

After A23's check learned to allow the one pixel of separate rounding a
fractional line height produces (`retest-upstream-fixes-on-nightly`), the
GNOME 50 and 51 SDKs passed with 91 holds and 5 skips each.
