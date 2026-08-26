# Live run: real saves through the migrated workflow

Recorded while the run happened, not reconstructed afterwards. Raw artifacts are
under `build/live-run/save/` (ignored build output): `run.txt`, `stderr.log`,
`stdout.log`, `snapshot-initial.json`, `snapshot-final.json`,
`fixtures-before.sha256` / `fixtures-after.sha256`, `fixtures-before.stat` /
`fixtures-after.stat`, and `stderr-findings.txt`.

## Pre-authorized substitution, and why it was used

Save replaces the user's file bytes, so this must never be pointed at the
maintainer's real documents. Following slot 2b's precedent, the run used a real
GNOME Wayland session — the app is a real GUI process on the real compositor,
not a headless one — but with **throwaway fixture files** inside **isolated**
`LUSHTEXT_DATA_DIR`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, and
`XDG_STATE_HOME`. `XDG_RUNTIME_DIR` was deliberately *not* overridden, because
the Wayland socket lives there and overriding it would have silently downgraded
this to something that is not a live-session run.

Before launching, the run checked for an existing `dev.cominotti.lushtext` owner
via `flatpak ps`, `busctl --user list`, and a `/target/debug/lushtext$` process
scan. **No owner was present**, so nothing was raced or asked to quit, and the
maintainer's own session was never disturbed. `scripts/clear-lushtext-xdg.sh`
was not run at any point.

Session type recorded from the app's own environment: `wayland`.

## What was run

1. Launched `target/debug/lushtext` on `fixtures/alpha.txt`
   (`alpha original line\n`), with `fixtures/beta.txt` present as an untouched
   control file.
2. Waited for the app to answer a read-only Automation1 `snapshot`. Initial tab
   state: `load_state=loaded modified=False saving=False`.
3. **Clobbered `alpha.txt` on disk behind the app's back** with
   `EXTERNAL CLOBBER THAT THE SAVE MUST OVERWRITE\n`, then activated the real
   `win.save` action over D-Bus and waited on the `save-complete` readiness
   predicate.
4. Clobbered it a second time and saved again, to prove the admission lane
   settles and re-admits rather than wedging after one payload.
5. Captured a final snapshot, quit through `app.quit`, and scanned stderr.

The clobber-then-save shape is what makes this a real durable-write proof rather
than a no-op: the only way the file can come back to the buffer's content is if
the full capture → worker → atomic temp-then-rename → accept path actually ran.

## Result

| Check | Outcome |
| --- | --- |
| First `win.save` action | accepted; `save-complete` reported `ready` |
| File content after save | `alpha original line\n` — the **buffer's** content, overwriting the external clobber |
| Inode before save (clobbered file) | `30138254` |
| Inode after save | `30138494`, then `30138505` after the second save |
| Second `win.save` after a second clobber | accepted; content restored again |
| `alpha.txt` sha256, start vs end | `79c57dc3…` both times — the buffer content survived two external clobbers |
| `beta.txt` (control) | sha256 `36b00e4c…` and inode `30138255` unchanged; the save touched only its own destination |
| Final tab state | `load_state=loaded modified=False saving=False file_size=20` |
| Final readiness | `idle=True`, `idle_blocker=None` |
| stderr findings | **0** |

**A new inode on every save is the atomic-replace contract visible from
outside.** `filesystem::write::atomic_replace` writes a temp file and renames it
over the destination, so the destination necessarily gets a fresh inode; an
in-place rewrite would have kept `30138254`. The identity-metadata preservation
that compensates for the new inode is covered by the unit tests in
`services/durable_write.rs`, which this change does not touch.

## stderr

`stderr.log` contains exactly one line for the whole session:

```
WARNING: radv is not a conformant Vulkan implementation, testing use only.
```

That is a Mesa driver notice emitted before GTK starts and is unrelated to the
application. The scan for `Gtk-WARNING`, `Gtk-CRITICAL`, `GLib-GObject-WARNING`,
`GLib-CRITICAL`, `Adw-WARNING`, `Gdk-WARNING`, pixman `*** BUG ***`, and
`Trying to measure` produced **zero** findings (`stderr-findings.txt` is empty).

## What this run does not cover

Stated explicitly rather than implied, because the substitution is what makes
these gaps possible:

- **Save As was not exercised live.** `win.save-as` opens a `GtkFileChooser`,
  and driving a live chooser is exactly the step the programme-level actuation
  seam deferral exists for. Save As is covered by widget tests through the
  preserved chooser-bound seams (`select_save_as_destination_for_test`,
  `select_save_as_uri_for_test`, `cancel_save_as_destination_for_test`) and by
  `complete_save_as` window tests, including the symlink and stale-canonical
  cases.
- **Close-with-changes was not exercised live**, for the same reason: it runs
  through an `AdwAlertDialog`. It is covered by widget tests
  (`test_close_modified_file_tab_save_writes_then_closes`,
  `test_multi_tab_close_admits_one_save_payload_at_a_time`, and the close-batch
  `BeforeRename` / `AfterRename` pair).
- **A modified-buffer save was not driven from the keyboard.** The buffer text
  itself was not edited live: an AT-SPI `EditableText` insertion was attempted
  first and the accessible tree walk did not locate the source view under this
  session's a11y configuration, so the run switched to the clobber-then-save
  shape instead of silently skipping the durable write. The saves performed were
  therefore clean-buffer saves — which still traverse the entire workflow, since
  a clean buffer is queued with `required_modified = false` and is not dropped by
  the staleness predicate (asserted directly by
  `test_save_of_a_clean_unmodified_buffer_still_writes_the_buffer`). Modified-buffer
  saves, save formatting, and the buffer mirror-back are covered by widget tests.
- **Durability-failure classification was not exercised live**, because it needs
  fault injection; it is covered by the editor-level `BeforeRename` /
  `AfterRename` tests and the close-batch pair.

The run was **not** silently downgraded to a headless one: it used the real
session compositor, and the artifacts record the session type.
