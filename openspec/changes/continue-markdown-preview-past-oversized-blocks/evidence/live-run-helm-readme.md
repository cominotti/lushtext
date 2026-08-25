# Live `make run` verification: Helm-style README with an oversized values table

Task 10.9 evidence. Captured 2026-08-25 on the Fedora 44 GNOME Wayland host
with the user's explicit approval to close their running LushText instance.

## Session setup

The user's installed Flatpak instance was asked to quit through its own
exported application action, not killed:

```
gdbus call --session --dest dev.cominotti.lushtext \
  --object-path /dev/cominotti/lushtext \
  --method org.gtk.Actions.Activate quit "[]" "{}"
```

It exited cleanly, so no `flatpak kill` and no signal was needed. Draft state
was therefore never at risk.

The session was then recorded with the repo's own live-debug harness so stdout,
stderr, the user journal, and D-Bus traffic were captured together:

```
.claude/skills/gtk-agentic-debugging/scripts/run-gtk-debug-session.sh \
  --cmd "make run build/manual/helm-values-README.md" \
  --pid-pattern '(^| )target/debug/lushtext($| )' \
  --out /tmp/gtk-debug-10-9 --duration 5
```

Two harness notes worth recording, because both could have produced a false
result:

- The documented `--pid-pattern '(^| )target/debug/lushtext($| )'` does **not**
  match the launched process, whose command line is the absolute path
  `/var/home/.../lushtext/target/debug/lushtext` — the character before `target`
  is `/`, not a space or start-of-line. `check-lushtext-live.sh` correctly
  refused to proceed ("No current LushText PID is proven to belong to this debug
  launch"). Re-running with `--pid-pattern '/target/debug/lushtext$'` proved
  `process_live=1`, `current_pids=2301224`, `before_pids=` empty,
  `launched_pids=2301224`: a genuinely fresh debug instance, not a handoff to a
  pre-existing window.
- `make run <path>` treats the path as a **make target**, not an app argument
  (`make: Nothing to be done for 'build/manual/helm-values-README.md'`), so the
  fixture did not open from the command line. It was opened through the standard
  GApplication D-Bus entry point instead:
  `org.gtk.Application.Open(['file:///.../build/manual/helm-values-README.md'])`.

## Fixture

`build/manual/helm-values-README.md` (76 lines, gitignored): an ACME Gateway
Helm chart README with a title, prose, an install fenced code block, a **40-row
x 3-column values table** with realistic keys/defaults/descriptions, then
`## Upgrading`, `### Upgrading from 1.x` with three bullets, `## Uninstalling`
with a second code block, and a final `PAGE-TAIL-MARKER:` line.

The table is well over the 256-event per-slice budget (roughly 8-10 parser
events per cell across 120 body cells) and comfortably inside
`MAX_PREVIEW_TABLE_CELLS`, which is exactly the window this change exists to
fix: before the change, everything after this table was discarded.

## What the live session showed

Preview was enabled through the exported action and both readiness predicates
were awaited:

```
scripts/lushtext-automation.py action win.set-preview-mode --bool true
scripts/lushtext-automation.py wait visual-geometry-settled
scripts/lushtext-automation.py wait idle
```

`GetSnapshot` reported `idle: true` with `idle_blocker: null`, and
`surfaces.preview_mode: true`.

Assertions from the AT-SPI tree (`atspi-dump-tree.py`, 276 nodes):

| Claim | Observed |
| --- | --- |
| the values table is **one continuous widget** | `role='table' name='Markdown table'` x **1** |
| every data row rendered | `role='table cell'` x **120** = 40 rows x 3 columns |
| nothing was omitted | 0 nodes matching `omitted` |
| nothing degraded to a fallback | 0 nodes matching `not rendered` |

The full preview text interface was read directly (`characterCount=650`) and
ends with:

```
Upgrading from 1.x

• gateway.listenPort was renamed to service.port.
• gateway.tlsSecret moved under ingress.tls.
• The bundled Redis subchart was removed; point cache.url at your own.

Uninstalling

PAGE-TAIL-MARKER: if you can read this line, the tail survived.
```

**The tail marker is present**, along with every section after the values table.
The blank runs are the anchored code-block and table widgets, which are child
widgets rather than buffer text.

The layout path was then stressed, because a partially applied table is exactly
the shape that can produce a zero-width `Gtk::Grid::attach`. Four transitions
were driven, each awaiting `visual-geometry-settled` and `idle`:
preview-only off -> side-by-side on -> side-by-side off -> preview-only on. A
second AT-SPI dump after that churn still reported **1 table, 120 cells, 0
omissions, 0 fallbacks**, and the tail marker was still present.

The app was closed through the same graceful `quit` action.

## stderr scan

`gtk-launch` leaves the app's stderr connected to the launcher's PTY, so
`app.typescript` is the complete session stderr from launch (11:09:15) to exit
(11:14:19). It is **12 lines**, in full:

- `Script started` / `Script done` harness markers
- `Building LushText (debug)`, `cargo build`, `Finished dev profile`,
  `Running LushText...`, the `run-dev-app.sh` invocation
- `make: Nothing to be done for 'build/manual/helm-values-README.md'`
- `WARNING: radv is not a conformant Vulkan implementation, testing use only.`
  — a Mesa driver notice emitted before GTK starts, not an app diagnostic
- `WARN lushtext_core::ui::window::recent_open: recent document pruned
  /tmp/lushtext-glyph-highlight-on.IwwTmW/empty.txt: unsupported status Missing`
- `ERROR lushtext_core::ui::editor_page::load_save: Cannot stat
  /tmp/lushtext-glyph-highlight-on.IwwTmW/empty.txt: No such file or directory`

**None of the 10.9 target signatures appeared.** Zero matches for
`Gtk-WARNING`, `Gtk-CRITICAL`, `GLib-GObject-WARNING`, `GLib-GObject-CRITICAL`,
`GLib-CRITICAL`, `Gdk-WARNING`, `Gdk-CRITICAL`, `*** BUG ***`, `pixman`,
`Grid`/`gtk_grid` criticals, or `assertion` — in `app.typescript` or in the
captured journal. The harness's own dedicated check agrees: *"No GtkBox
measurement warnings were detected."* In particular there was **no zero-width
`Gtk::Grid::attach` critical**, which is the defect class a sub-sliced table
could have introduced.

The two LushText messages that do appear are pre-existing session state, not
regressions from this change: a restored tab and a recent-document entry both
point at `/tmp/lushtext-glyph-highlight-on.IwwTmW/empty.txt`, a temp file from
an unrelated earlier glyph-debugging session that no longer exists. The app
handles both correctly — it prunes the stale recent entry and reports the
missing file — and neither touches Markdown preview.

Everything else the harness summary lists under "Top Warnings" belongs to
unrelated host processes (Chrome, FreeRDP under Flatpak, a `fractal` coredump
whose loaded-module manifest is the only reason the string `pixman` appears
anywhere in the journal).

## Screenshot

Deliberately **not** included. The available portal capture path on this host is
full-screen, and the capture it produced contained unrelated private windows
from the user's live desktop, so it was deleted rather than retained as
evidence. Screenshot proof for this change comes from the isolated headless
lanes instead: `make visual-geometry-smoke` (80/80 cases, unfiltered) and
`make visual-smoke`, both of which run under private Mutter sessions with no
access to the user's desktop.

## Artifacts

Left outside the repo at `/tmp/gtk-debug-10-9/`: `app.typescript`,
`journal.log`, `dbus.log`, `summary.md`, `atspi-preview.txt`,
`atspi-preview2.txt`, `preview-text.txt`.
