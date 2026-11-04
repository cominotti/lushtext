## Why

Restoring or opening many large documents spends most of the application's
CPU laying out tabs nobody is looking at. `eviction/reselect` (ten 8 MiB
documents opened at once) installs them in batches 6–11 s apart and does not
become ready within the lane's 20 s cap on `main`. `spawn-completion-priority`
attributed it: 69.5 % of the application's cycles go to GtkTextView's
`incremental_validate_callback`, hidden tabs included.

Two causes, both confirmed by probes in this change:

1. **A hidden tab lays out its whole document.** A `GtkSourceView` on an
   `AdwTabView` page that was never selected is neither mapped nor realized,
   yet it has a text layout (its scrolled window creates one when it hands the
   view its adjustments) and validates everything loaded into its buffer
   (A82). Selection order cannot prevent that. Because GLib co-dispatches every
   ready source of one priority in one iteration (A81, new and general), every
   hidden view still validating also lengthens every iteration of the visible
   one.
2. **Background installs wait behind the visible tab's layout.** C6 paces a
   dense document's install slices below the validation idle (A70), so a
   background document's install, and its 65 MiB transient load permit, waits
   for every validating view's whole backlog (A80). With three such permits
   held, the next documents are not even read.

The user-visible goal: documents restore and become usable promptly, CPU goes
to the visible tab, every dispatch stays within `B_INTERACT_US`/`B_SETTLE_US`,
and no byte is lost or altered.

The change also attributes the `baseline/small/steady-populated`
`type-during-settle` worst-stall shift that `spawn-completion-priority` left
unattributed, and records the general equal-priority co-dispatch axiom the
earlier designs cited A69 for.

## What Changes

- **A hidden tab's view shows an empty placeholder buffer.** Every editor's
  view holds a placeholder from construction until its tab is first shown,
  and again once it is seen laying out while hidden (its adjustment's
  `upper` changes while unmapped). On map it gets its document back.
  `gtk_text_view_set_buffer` to an empty buffer stops validation at once and
  leaves the document buffer untouched (A83). The document buffer stays the
  one every workflow reads and writes: `LushtextEditorPage::buffer()` returns
  it whatever the view shows.
- **The kept top line.** A view detached after being shown keeps the line it
  showed; reattached, it comes back at that line and is held there while it
  lays the document out again (a scroll queued right after a reattach lands
  elsewhere, A83), until it is laid out or the user gives input.
  `visible_top_line` (session persistence) reads the kept line without laying
  anything out; scrolls requested while detached (restore position, search
  navigation, bookmarks, local-history restore, Protected View navigation)
  become the kept line.
- **Pacing.** A paced (dense or protected) install or whole-buffer replacement
  into a background tab queues its slices one priority step below the visible
  tab's (`G_PRIORITY_DEFAULT_IDLE + 1`): nothing lays it out, and the visible
  document installs and lays out first. Slices stay bounded by the same
  budgets.
- **The transient load permit keeps only the decoded body after the read**
  and gives its reading slot back (design D4), so a background install waiting
  behind the visible tab never stops the next document from being read.
- **Axioms** A81 (equal-priority co-dispatch), A82 (a hidden tab page's view
  lays out), A83 (an empty buffer stops it; reattach semantics), each with a
  probe and a sample. The A69 citations for co-dispatch in
  `spawn-completion-priority`, `keep-keystrokes-during-install`, the record
  and four code comments point at A81.
- **Lane.** `eviction/reselect` gains a `switch-hidden` phase: select a tab
  the run never showed, then scroll it.
- **Attribution** of the `steady-populated` shift (design, "Attribution").

## Capabilities

### Modified Capabilities
- `main-thread-responsiveness`: hidden tabs lay out nothing; paced installs
  into hidden tabs are not held behind validation.
- `gtk-axiom-ledger`: A81, A82, A83.
- `responsiveness-measurement`: the `switch-hidden` phase.
- `editor-inline-alerts`: the selected document's alert is announced after the
  opened document's focus handoff (design D6, found by `make test`).

## Impact

`model/file_load.rs` (`retain_for_install`),
`ui/editor_page/view_attachment.rs` (new, cross-cutting, no role), the editor
page's `map`/`unmap`, `buffer()`, the scroll and evidence sites listed in
tasks 3.x, `load/policy.rs` (`install_slice_pacing`), the load and
buffer-replacement slice scheduling, `gtk-lush-axioms`, the ledger, the lane
driver, a new widget-test module. No persisted format, action, D-Bus member or
snapshot field changes.
