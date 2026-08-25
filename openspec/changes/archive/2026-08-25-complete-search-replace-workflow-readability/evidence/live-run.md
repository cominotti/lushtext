# Task 10.10 — live-session Replace All and undo

**Provenance, stated first.** This file was written **post-hoc**, from the session
transcript and one preserved artifact, after a code review found that task 10.10
had no evidence file. What is verbatim and what is reconstructed is marked
throughout. The run itself happened; only this record is retrospective.

- **Preserved artifact:** `live-run-stderr.log`, beside this file — the app's
  complete stderr for the session, copied at the time.
- **Reconstructed:** the fixture description, the seeded `workspaces.json`, and
  the `sha256sum` methodology. These are re-stated from the transcript rather than
  from files, because the `/tmp` fixture tree was deliberately removed at the end
  of the run. The observed values quoted below were recorded at the time.
- **Not preserved:** the snapshot JSON at each step, and the digests themselves.
  A future live-run task should write its artifacts into `evidence/` as it goes
  rather than into a scratch directory.

## Deviation from the literal target, and why

Task 10.10 names `make run`. On this maintainer's machine `make run` launches
against their real `$XDG_DATA_HOME/lushtext/workspaces.json`, so the app restores
**their own workspace folders** — and Replace All rewrites files. The task's own
guard ("Replace All mutates files: it must never be pointed at the maintainer's
real workspace folders") therefore rules out the literal command. There is also no
automation action that adds a workspace folder, so a fixture folder cannot be
introduced after launch; it has to be seeded before startup, which requires an
isolated data directory.

Per the slot-1 precedent for exactly this situation, the substitution is recorded
rather than silently made.

## What was run

The freshly built debug binary on the maintainer's **live GNOME Wayland session** —
a real GUI window on the real compositor, not headless:

```
cargo build
LUSHTEXT_DATA_DIR=<tmp>/data \
XDG_CONFIG_HOME=<tmp>/config XDG_CACHE_HOME=<tmp>/cache XDG_STATE_HOME=<tmp>/state \
  ./target/debug/lushtext
```

`make run` was **not** used, so no development desktop entry or icon was staged;
that was confirmed after the run by checking for
`~/.local/share/applications/dev.cominotti.lushtext*.desktop` (absent). The
maintainer's own app data and workspace folders were never read or written.

**Fixture (reconstructed).** `<tmp>/fixture/` held three files:

| File | Contents |
| --- | --- |
| `first.txt` | `alpha needle beta\nplain line\n` |
| `second.txt` | `needle only\n` |
| `third.txt` | `no match here\n` — a non-matching control |

`<tmp>/data/workspaces.json` was seeded with the v1 envelope
(`kind: dev.cominotti.lushtext.workspace-state`) naming one workspace,
`Replace Fixture`, with that one folder.

**Digest methodology (reconstructed).** `sha256sum <tmp>/fixture/*.txt` was
recorded to `<tmp>/original.sha256` **before** launching the app, and verified
after the undo with `sha256sum -c <tmp>/original.sha256`. That is what makes the
restoration claim byte-exact rather than eyeballed: the check compares digests of
the pre-replacement bytes against the post-undo files, and reports per file.

## What was exercised, and what was observed

Driven entirely through the real read-only D-Bus automation client
(`scripts/lushtext-automation.py`), whose `action` verbs go through the same GTK
actions a user's keyboard does.

| Step | Observed (recorded at the time) |
| --- | --- |
| workspace restore | `workspace.scope_workspace_name = "Replace Fixture"`, `folder_count = 1` |
| `action set-search-panel-visible --bool true` then `action set-search-panel-query --string needle` | real streaming search: `match_count = 2`, `file_count = 2`, `query = "needle"` |
| `action set-search-panel-replace-query --string thread` then `action preview-search-panel-replacements` | `replace_preview_mode = true`, `replace_preview_count = 2`, `checked_replacement_count = 2`, `replace_preview_pending = false` |
| `action confirm-search-panel-replacements` | **fixture files really rewritten**: `first.txt` → `alpha thread beta\nplain line\n`, `second.txt` → `thread only\n`; `third.txt` untouched. Status bar: `Replaced 2 of 2 matches in 2 files`. Durable journal present on disk: two per-file entry files plus `manifest.json`. `has_undo_backup = true` |
| `action undo-search-panel-replacements` | **`sha256sum -c` reported `OK` for all three files.** Status bar: `Reverted 2 files`. Journal directory removed. `has_undo_backup = false` |
| `action undo-search-panel-replacements` again | refused: digests still `OK`, no second write, no transaction left claimed |

Note that the first automation attempt used the wrong action
(`begin-search`, which is the in-editor find bar) and then
`set-search-panel-query`, which correctly **no-ops while the panel revealer is
hidden** — the panel-visibility guard doing its job. Revealing the panel first
with `set-search-panel-visible` produced the search above. That is recorded
because it is a real behavior of the automation surface, not a flake.

## stderr

The complete session stderr, preserved verbatim in `live-run-stderr.log`, is one
line:

```
WARNING: radv is not a conformant Vulkan implementation, testing use only.
```

That is the host Mesa/Vulkan driver banner, not an app message. **Zero**
`Gtk-WARNING`, `Gtk-CRITICAL`, `GLib-GObject-WARNING`, pixman `*** BUG ***`, and
`Trying to measure` output — the five patterns task 10.10 names.

## What remains uncovered by this substitution

Only the app-data profile. The run did not exercise restoring the maintainer's
real `workspaces.json`, session, or drafts. Everything the task asks about — a
real GUI process on a real compositor performing a real workspace search, a real
Replace All that mutates files on disk, a real undo that restores them byte-exactly,
a refused second undo, and clean stderr — was covered.

The fixture tree and the isolated data directory were removed after the run.
