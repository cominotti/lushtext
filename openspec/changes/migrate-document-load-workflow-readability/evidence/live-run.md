# Live run

Written during the run, not reconstructed afterwards.

## Precondition check, before launching anything

A live run must not race the maintainer's own instance, so this was checked
first rather than assumed:

```
$ flatpak ps | grep -i lushtext            → no flatpak lushtext
$ busctl --user list | grep -i lushtext     → no session-bus lushtext owner
$ pgrep -af '/target/debug/lushtext'        → no dev lushtext process
```

No owner existed, so the run proceeded. Had one existed, the run would have been
left unchecked with a note rather than the maintainer's session disturbed.

## Isolation

Everything the app could persist was redirected into a throwaway root at
`/tmp/l3blive`, and every file opened was a generated fixture. **No document of
the maintainer's was opened.**

```
XDG_DATA_HOME=/tmp/l3blive/xdg/data      XDG_CONFIG_HOME=/tmp/l3blive/xdg/config
XDG_CACHE_HOME=/tmp/l3blive/xdg/cache    XDG_STATE_HOME=/tmp/l3blive/xdg/state
LUSHTEXT_DATA_DIR=/tmp/l3blive/appdata
GSETTINGS_SCHEMA_DIR=<repo>/data
```

The short `/tmp` root is deliberate: an earlier attempt to run a lane from the
session scratchpad path failed with
`libmutter-ERROR **: Failed to create socket`, which is the runtime-path length
limit `.agents/rules/build.md` warns about.

| Fixture | Bytes | Purpose |
| --- | --- | --- |
| `small.txt` | 36 | direct install |
| `notes.md` | 45 | second tab, Markdown language detection |
| `large.txt` | 12,800,000 | chunked install, many slices |
| `windows1252.txt` | 5 | bytes that are not valid UTF-8 (`0xE9`) |

Launched directly as `target/debug/lushtext <fixtures>` rather than through
`make run`: that target stages a development desktop entry and icons into the
maintainer's session, which this run has no reason to touch.

## What was run, in order

Every step was driven through the read-only D-Bus automation surface
(`scripts/lushtext-automation.py`), and stderr was marked before each step so new
output could be attributed.

| Step | Entry point exercised | Result |
| --- | --- | --- |
| 1 | Launch with two file arguments → `HANDLES_OPEN` activation → `open_document` → `load_file_async` | `wait app-startup` → `ready`. Snapshot: `small.txt` `loaded` (36 B), `notes.md` `loaded` (45 B), `tab_count 2`. **Both `load_state` values came through the new `LoadEvidence` projection.** |
| 2 | `win.open-recent` → recent-documents popover | `open_popover_visible: true` |
| 3 | `win.set-open-popover-query "notes"` → recent-list filtering | accepted; popover still visible |
| 4 | `win.show-encoding-controls` → the reopen-with-encoding entry surface | dialog presented |
| 5 | Second activation with `large.txt` → chunked install | polled the snapshot; caught `large.txt` in `load_state: loading` on the first poll |
| 6 | `win.close-tab` **while that load was still installing** → `tabs.rs`'s `cancel_load()` and then page disposal | tab closed, `tab_count` back to 2, remaining tabs still `loaded` and unmodified |
| 7 | Third activation with `windows1252.txt` | `wait file-open-complete` → `ready`; tab `loaded` at 5 B, so the encoding fallback decoded it rather than failing |
| 8 | Fourth activation, `large.txt` again, allowed to finish this time | `wait file-open-complete` → `ready`; `tab_count 4`, `large.txt` `loaded` at 12,800,000 B |
| 9 | `app.quit` | exited cleanly; the pid was gone within 3 s |

## stderr

**One line, for the whole session:**

```
WARNING: radv is not a conformant Vulkan implementation, testing use only.
```

That is the host Mesa/RADV driver's own notice about this machine's GPU stack,
emitted before any LushText code runs. It is not an application warning.

Scanned for every class `.agents/rules/build.md` treats as blocking:

```
$ grep -nE "Gtk-WARNING|Gtk-CRITICAL|GLib-GObject-WARNING|\*\*\* BUG \*\*\*|\
Trying to measure|pixman_region32_init_rect|Gdk-CRITICAL|GLib-CRITICAL|Adw-WARNING" stderr.log
CLEAN: none of the forbidden warning classes appear
```

No `Trying to measure ... needs at least ...`, no pixman `*** BUG ***`, no
`Gtk-CRITICAL`, no `GLib-GObject-WARNING`.

## What this covers, and what it does not

**Covered:** the activation open path (the same `open_document` →
`set_file_path_for_pending_load` → `load_file_async` sequence the chooser and
sidebar reach), direct install, chunked install of a 12.8 MB file to completion,
the recent-documents popover and its query, the encoding-controls surface,
**cancellation of a live chunked installation** through tab close, an
undecodable-as-UTF-8 file taking the encoding fallback, and clean shutdown.

**Not covered by this run, stated rather than implied:**

- **The `GtkFileChooser` itself.** `win.open-file` presents a portal file chooser
  with no automatable selection outside `test-utils`; the three chooser-bound
  seams (`select_open_file_for_test`, `select_open_file_uri_for_test`,
  `cancel_open_file_for_test`) exist precisely because this step cannot be driven
  from a release-shaped build. The code path *after* the chooser returns is the
  activation path exercised in step 1.
- **Reopen-with-a-chosen-encoding as a completed round trip.** Step 4 opened the
  controls; choosing an encoding in the dialog needs pointer input. The
  reopen-with-encoding load itself is covered by
  `editor_page::test_reopen_with_a_different_encoding_replaces_the_loaded_content`.
- **Sidebar row activation.** Needs a double-click on a realized tree row;
  covered by the widget suite instead.
- **A user-visible "Loading Cancelled" inline alert.** Step 6 cancelled by
  *closing* the tab, which is the silent disposal path by design. The
  user-visible cancellation terminal is covered by
  `editor_page::test_chunked_load_cancellation_clears_partial_text_and_releases_admission`.

## Artifacts

`/tmp/l3blive/stderr.log`, `/tmp/l3blive/stdout.log`, `/tmp/l3blive/run.sh`, and
the generated fixtures. They live outside the repository because the run's whole
point was to keep its state out of both the repo and the maintainer's session.
