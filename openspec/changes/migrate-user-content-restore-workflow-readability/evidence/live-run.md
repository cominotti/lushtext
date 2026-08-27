# Live run (task 10.10) — PARTIAL, then DEFERRED BY USER DIRECTIVE

**Status: the live-session acceptance gate for this change is OUTSTANDING.** Task
10.10 is marked `[~]`, not ticked. Do not read this document as the gate having
been cleared.

## Why it stopped

A live-session run was started after confirming no LushText instance was running:

```
$ flatpak ps          # only the user's own apps: Fastmail, Steam, Fractal
$ busctl --user list | grep -i lushtext   # no lushtext bus name owned
```

It ran under fully isolated state so it could not touch the user's real data:

```
XDG_DATA_HOME=/tmp/lt-live/data  XDG_CONFIG_HOME=/tmp/lt-live/config
XDG_CACHE_HOME=/tmp/lt-live/cache  XDG_STATE_HOME=/tmp/lt-live/state
LUSHTEXT_DATA_DIR=/tmp/lt-live/data/lushtext
GSETTINGS_SCHEMA_DIR=$PWD/data
```

**It nevertheless interfered with the user's active fullscreen desktop session,
and the user directed that it stop.** Isolating an app's *state* does not isolate
its *window*: a real Wayland launch maps a real surface and takes focus on the
session the user is working in. That is the lesson, and it is the reason the
directive is now "headless only".

Everything was stopped on receipt: every isolated instance was terminated
(`SIGTERM`, then `SIGKILL` for any survivor), `ydotoold` was stopped and
`$XDG_RUNTIME_DIR/.ydotool_socket` removed, and `pgrep -x lushtext` returns
nothing. **The user's own applications were never signalled** — every kill was
matched against a PID from this change's own launches, and `flatpak ps` after the
cleanup still shows Fastmail, Steam, and Fractal running.

## One process note worth recording against future sessions

Synthetic **global** input (`ydotool`) was tried first and is the wrong tool
twice over. It types into whatever the compositor currently focuses, which in a
live session is a hazard, and it is unverifiable — the first attempt landed
nowhere at all, which only became apparent because the fixture file and the
`modified` flag were checked afterwards and both were untouched. It was abandoned
and the daemon stopped.

The correct mechanism, and the one the repo's own `crash-recovery-smoke` and
`accessibility-smoke` lanes already use, is **targeted AT-SPI**:
`Atspi.EditableText.insert_text` addresses one accessible object inside one
application. Every observation below came from that path plus the read-only
automation client — never from global input.

**And the repo already ships that tooling, which should have been used instead of
hand-rolling it.** `.agents/skills/gtk-agentic-debugging/scripts/` contains
`atspi-set-text.py`, `atspi-click-button.py`, `atspi-accessible-action.py`,
`atspi-dump-tree.py`, `check-lushtext-live.sh`, and `capture-lushtext-mutter.py`.
`atspi-set-text.py` is precisely the text insertion written from scratch here, and
`atspi-click-button.py` is what the inline-alert Discard/Save actuation needed.
Those five scripts are also **fingerprinted inputs** to the accessibility policy
gate, which is a strong signal that they are the sanctioned path. A future live or
AT-SPI-driven check should start from that directory — the tool existed, and not
finding it first cost time and produced a worse mechanism.

## What was genuinely proved before the stop

This is real-session evidence — a real compositor, real GPU, real durable writes,
a real `SIGKILL` — and it covers the recovery half of the task. It is kept
because discarding it would lose honest proof, not because it completes the gate.

### The first dirty edit reaches durable storage

Text was inserted into the open document through AT-SPI, and the snapshot then
reported the buffer dirty:

```
modified: True | draft_present: True | load_state: loaded
```

The first-dirty autosave lane then wrote **both** a body and a durable manifest
entry, unprompted, on the real timer:

```
/tmp/lt-live/data/lushtext/drafts/fbe33761f1c98eb1.draft   (52 bytes)
/tmp/lt-live/data/lushtext/drafts/manifest.json            (296 bytes)

{ "kind": "dev.cominotti.lushtext.draft-manifest", "version": 1,
  "data": { "drafts": [ { "draft_id": "fbe33761f1c98eb1",
    "original_path": "/tmp/lt-live/ws/notes.md",
    "original_mtime_secs": 1787773068, "saved_at_secs": 1787773749 } ] } }
```

This exercises `drafts/autosave_execution.rs` and `drafts/journal.rs` in a real
process: the timer, the snapshot, the worker handoff, the body write, and the
durable manifest upsert with its identity fields.

### `SIGKILL` and relaunch recover the crash-time content verbatim

```
$ kill -9 <pid>       # process gone, no clean shutdown path taken
$ ls .../drafts/      # fbe33761f1c98eb1.draft  manifest.json   (both survived)
$ <relaunch>
```

After relaunch the restored tab came back **dirty**, and the buffer read through
AT-SPI contained the crash-time edit exactly:

```
tab: notes.md | modified: True | draft_present: True | load_state: loaded
TEXT['LIVE-RUN AUTOSAVE EDIT\nline one\nline two\nline three\n']
idle_blocker: None
```

That is `drafts/restore_execution.rs` and the startup half of
`session_restore/journal.rs` working end to end in a real process, and it is the
strongest single piece of evidence in this change that the draft-recovery
migration preserved behavior: the user's unsaved work came back byte for byte
after an unclean kill.

### The migrated local-history projection answers correctly in a live process

```
local_history: {'active_document_file_backed': True,
                'automatic_capture_available': True,
                'availability': 'full', 'browse_available': True}
```

These are the four fields that changed **source** in this slot — they now project
from `LocalHistoryEvidence` rather than re-deriving from widgets — and they report
the same values a live document should produce. `win.show-local-history` was also
activated through the automation client and the browser opened with **zero**
`Gtk-CRITICAL`, `Gtk-WARNING`, `GLib-GObject-WARNING`, pixman, or
`Trying to measure` lines in the log.

### A live observation that corroborates a documented decision

`draft_present` read `true` on a tab with an empty drafts directory. That was
checked rather than assumed, and it is correct: the field projects
`draft_id().is_some()` — an assigned draft *identity slot*, not a body on disk —
and `git diff origin/main` confirms **this change does not touch that
projection**. It independently corroborates the finding in
`evidence/automation-no-widening.md` that `draft_present` is a per-tab
document-identity fact and must not be fabricated as a projection of the
window-level `DraftEvidence` surface.

## Stderr, over the portion that ran

Across the pre-crash and post-crash logs the only line emitted was the host's
Mesa notice, which is a property of the machine's Vulkan driver and not of the
application:

```
WARNING: radv is not a conformant Vulkan implementation, testing use only.
```

**Zero** occurrences of `Trying to measure GtkBox ...`, `*** BUG *** In
pixman_region32_init_rect`, `Gtk-CRITICAL`, `Gtk-WARNING`, or
`GLib-GObject-WARNING`. This is a genuine result for the paths that ran, and it
is **not** the paned-warning gate, because the paths most likely to emit those
warnings are exactly the ones that did not run.

## What remains, precisely

The gate is outstanding for these four, all of which need a live session:

1. **Discard and save a restored draft from the inline alert.** The
   `LushtextInfoBar` `Discard...` / `Save...` actuation was next when the stop
   arrived. This is the one remaining behavior in the draft workflow whose live
   path is unproved here; it *is* covered by widget tests and by
   `make crash-recovery-smoke`.
2. **Browse and restore a local-history snapshot.** Browse was proved; the
   restore commit was not.
3. **Resize the window while the sidebar animates.** This is the actual
   paned-warning gate — `.agents/rules/widget-wiring.md` is explicit that
   "widget green + live warning is a failed fix, not a partial success", and this
   change touches windows that host the animated sidebar.
4. **A full clean-shutdown session-restore round trip** across quit and relaunch.

One incidental observation for whoever runs it: an instance launched with `nohup`
from a tool call did not survive past the launching call, exiting cleanly with no
diagnostic. It was being reaped by the shell lifecycle, not failing — there was
no panic and no warning in its log. Use `setsid` with `< /dev/null`, or run it
under the smoke lanes' own supervision.

## What does cover this behavior in the meantime

Not a substitute for the live gate, but not nothing either:

- `make crash-recovery-smoke` — **PASS** on the changed tree. A real GTK process,
  real `SIGKILL`, real relaunch, recovery verified through AT-SPI plus app-owned
  metadata, with `crash-recovery-smoke-driver.py` unmodified.
- The widget lane, including the seven tests added by this change — the
  data-safety regression, three reentrancy proofs, and three disposal proofs.
- `make automation-smoke` — 22 of 22 artifacts byte-identical to `origin/main`.

The gap those leave is specifically **live compositor allocation behavior during
animation**, which is why item 3 above is the one that matters most.
