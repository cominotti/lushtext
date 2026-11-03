## Context

C6 (`bound-post-open-settle`) replaced `set_editable(false)` with a counted, buffer-level edit lock:
the first-connected `insert-text`, `insert-paintable`, `insert-child-anchor`, `delete-range`, `undo`
and `redo` handlers stop every emission that is not the owning workflow's own edit
(`edit_lock::with_own_edit`). Three workflows hold it: the load installation (from
`begin_load_installation` to `restore_load_installation_state`), the whole-buffer replacement
(`begin_guard` to `restore_guard`) and the save (`SaveViewInteractivity`). A refused edit changes
nothing, and nothing reports it.

A load holds the lock only from its install's first dispatch. Before that, while the worker reads,
decodes and profiles the file (`load_state == Loading`), the buffer accepts typing, and the install's
clear then deletes what was typed (`with_own_edit`, which the lock lets through by design).

Measured on the lane, `type-during-settle` (one key every 50 ms from load readiness) on
`total-size/1000000` and `total-size/5000000` under load: 60 keys delivered to the editor, 0–3
inserted. Most keys fall in the sliced install.

## Goals / Non-Goals

**Goals:** **the user's text is never lost**: no typed character is discarded, a document that
cannot take held text gets it in a new tab, and no close or quit drops it; typed text appears at the
caret the document is shown with, in order, as one undo action; data safety for drafts, the modified state, the lock's ordering,
C2's entry rules and supersession; immediate visible and accessible feedback for anything that is
still refused; the replay dispatch within `B_SETTLE`.

**Non-Goals:** holding edits other than typing (deletions of document text, undo, moves, Replace);
keeping held text across a crash (it is in memory for the install's duration only);
changing the lock for programmatic edits.

## Decisions

### D1. Hold and replay (option (a)), not refuse-with-feedback (option (b))

Option (b) keeps today's refusal and adds a message. It is simple and data-safe, but the user still
loses the characters: they must notice the message and retype. A user who opens a file and starts
typing expects the text to appear where the caret is once the document shows, which is what a word
processor or GNOME Text Editor does when a document is still loading. Option (a) gives that, and it can
be made data-safe because the replay is an ordinary edit made after every workflow has released the
buffer (D4): every invariant that holds for a keystroke typed one moment later holds for the replay.
Option (b) remains the behaviour for everything (a) does not hold (D3), so nothing is refused silently
either way. Decision: **(a)**, with (b) for the remainder.

### D2. What is held: a typed insertion

An insertion is held when all of these hold:

1. the editor is **held**: a workflow holds the edit lock, a load is in flight (`Loading`), or held
   text is waiting for its replay dispatch (the last keeps order: a key typed between the release and
   the replay goes behind the held text, never before it);
2. it is not the owning workflow's own edit;
3. it is inside a **user action** (`begin-user-action`/`end-user-action` depth > 0). GtkTextView's
   typing, input-method commits, `insert-at-cursor` (Return, Tab), paste and GtkSourceView's
   auto-indent all insert through `gtk_text_buffer_insert_interactive*`, which brackets the insert in
   a user action; the workflows' own installs never open one (the measurement probe already relies on
   this for its `editor_user_insert` mark);
4. it is **at the caret** (the location is the insert mark): a primary paste at the pointer, or an
   insert elsewhere, is not typing at the caret;
5. its user action **deleted nothing** first. Typing over a selection, search Replace and Replace All,
   GtkSourceView's move-lines and change-case all delete before inserting in one user action; holding
   only their insert would replay a fragment of a different edit. Such an action is *tainted* by its
   refused deletion and its inserts are refused;
6. it **fits** the held buffer whole, with everything else its user action held (D6).

A held insertion is appended to the editor's held text and its emission is stopped, exactly as the
lock stops it today, so it changes no text, modified flag, undo history or draft.

Outside the lock (a load still planning), conditions 3–6 decide alone: a programmatic edit of another
workflow is not refused there today and is not now. Within the held window, a *user-action* deletion
and undo/redo are refused there too, because the install would undo their effect anyway; they used to
"succeed" on text the install then deleted.

### D3. Backspace and everything else

Backspace while held text exists removes the held text's last character (GtkTextView's `backspace`
keybinding signal, handled before its class handler, stopped). This is what the user means: they are
correcting what they just typed. Backspace with nothing held, Delete, word deletions, cut, undo, redo
and every tainted or misplaced insert are refused, as the lock refuses them now, and publish one
rate-limited status warning (`HeldNotice::EditNotApplied`). The warning says "typed text is kept"
only when text is actually held; otherwise it says only that the edit was not applied. They are not replayed: a deletion's target
was computed on a buffer that the install is replacing, so replaying it at the final caret could
delete text the user never saw.

### D4. When the held text is inserted: the settle dispatch

A **settle dispatch** (a default-idle source, one per editor at a time) is queued when the first
character of an episode is held and again whenever the last lock holder releases while text is held.
It decides with the pure `settle_verdict`:

- **Wait** while a workflow still holds the buffer: the lock is held (its release queues the dispatch
  again) or a load is in flight. A load's terminals queue the dispatch directly: every load state
  change refreshes the editor's accessibility metadata, which calls `sync_held_input_window`, and
  that queues the dispatch at once when held text no longer has an owner. The 16 ms re-arm remains
  only for what has no hook;
- **Wait** while a draft restore is still queued or reading its body for the editor
  (`draft_restore_pending_for`): the recovered text, not the file, is the document the user typed
  into, and replaying first would make the restore's freshness check divert the recovered work to
  set-aside;
- **Wait** while an input-method preedit is in progress, so the replay's caret move does not reset a
  composition the user is in the middle of. The preedit is read from the input method at the
  dispatch (`preedit_string`), not tracked from signals, and a composition still in progress after
  `PREEDIT_DEADLINE_MS` (1,024 ms, a power of two) is reset (`gtk_im_context_reset`, which commits or
  drops it as the input method chooses; a commit is held like any typed text);
- **Replay** when the document is whole and editable (D5);
- **Rescue** otherwise (D5): the text never goes nowhere.

The dispatch runs one above the tasks crate's completion priority
(`gtk_lush_tasks::completion_priority() - 1`, 123; `spawn-completion-priority`), so it is not held
back behind a document's validation batches (`GDK_PRIORITY_REDRAW + 5`, ledger A70). It was 124,
the completion priority itself, until the landing merge measured the alternative: GLib dispatches
every ready source of the top priority in one iteration (A81), so at 124 a replay and a task
completion or a load/save hand-off ready together run back to back and their bounds add. The
replay's worst measured dispatch is 13.6 ms (a replay that enters Protected View, cold, D8) and a
completion's 4.8 ms (`spawn-completion-priority`'s verdict): together past `B_SETTLE` (16 ms). The
lane rows at 124 (interleaved against `main` 44b1f067, idle host) saw no such iteration (worst
iteration overlapping a replay: no late tick; completions 4.6 ms), but the bound is what decides:
at 123 the replay's iteration dispatches nothing else of the application's.

The release itself never replays: `unlock_edits` runs in the middle of a terminal (the load sets
`Loaded`, the caret, the modified flag and the draft baseline around it), so the replay waits for the
dispatch after the terminal has finished.

The replay inserts the held text with `gtk_text_buffer_insert_interactive_at_cursor` (`editable` =
the view's), inside one explicit user action: **one undo action, like typing**; typing that follows
contiguously may coalesce with it, as GtkSourceView coalesces typed characters, but undo never
reaches anything installed before it. Its result is checked: a refused insert (the view stopped
being editable) is rescued (D5), never dropped. It runs after the lock is released
and the projections are restored, so the buffer's `changed` and `modified-changed` handlers mark the
document modified and draft-dirty and the next autosave writes it; the document-scale watch and the
paragraph-wrap and Protected View edit entries see it as an ordinary edit. The view then scrolls the
caret on screen when it has keyboard focus, as typing does.

Where the caret is: the install terminal places it (start of document, or the restored session
position, `apply_restore_position`) before it releases the lock, and the caret was invisible during
the install. So the held text appears where the caret is when the document is shown, which is the
coordinator's stated expectation. A reload of an open document (Reload after an external change,
Replace All's reload, Discard Changes) keeps the caret's line and column and the scroll position
(`reload_preserving_position`), so text typed during it goes where the user was, not to the start.
An episode in which **no** release published a whole document (only `Unchanged`, a save) replays at
a mark placed at the caret when its first character was held (`ReplayAnchor::FirstHold`): the
content did not move, so that position still names where the user typed, whatever a later click
moved the caret to.

### D5. Which document gets the text: the episode's content disposition

Held text belongs to the editor it was typed into, never another tab. Whether that editor's buffer is
still a document the text can go into is tracked per episode, from what each lock release reports
(`unlock_edits(lock, ContentDisposition)`):

- `WholeContent`: the release published a whole document (a completed load install or replacement);
- `Unchanged`: the release left the content as it was (a save, a replacement cancelled before its
  first mutation);
- `Unpublished`: the buffer was left partial or emptied (a superseded or cancelled load install, a
  cancelled replacement that had cleared text).

An episode starts *sound* (the document the user typed into was whole); `Unpublished` makes it
*broken*, `WholeContent` makes it sound again (a superseding load that completes publishes the
document the user meant), `Unchanged` keeps it. At the settle dispatch:

| Document | Verdict |
| --- | --- |
| Protected View, or the view not editable | rescue: "it is view-only because it contains very long lines" |
| evicted | rescue: "it was unloaded from memory" |
| `Failed` load, incomplete installation | rescue: "it did not finish loading" |
| broken episode | rescue: "its content was replaced before it was ready" |
| otherwise (`Loaded`, `Untitled`, a reload failure that kept the old text) | replay |

**No verdict discards the text.** A rescue opens it in a new Untitled tab through the Preserved
Drafts "Open" path (`LushtextWindow::open_set_aside_draft`: a new tab, a bounded replacement,
modified and draft-dirty, so autosave writes its own draft) and publishes a warning that is never
rate-limited, because it is the one place the user learns where the text went: "N typed characters
could not go into “name” (why); they are in a new Untitled tab." A cancelled or failed first load
leaves an empty or partial buffer whose save is already refused (`installation_incomplete`);
inserting the held text there would make a document that consists of only the typed text, and a
later retry would race the user's edit, so the text goes to its own tab instead. An editor in no
window waits (the dispatch re-arms) rather than lose the text.

Protected View (C2): a document whose profile is protected is entered at install begin
(`begin_protected_install`), so its view is not editable from then on and GtkTextView refuses typing
before any signal; Protected View's own pre-emption notice covers those keys. Text held before that
(while the load was planning) is rescued at the settle (the retired `HeldTextNotInserted` notice
said it was not inserted; the rescue warning says where it is). A replay into
an editable document that the replay itself makes protected enters through C1's edit watch like any
other edit (C2's edit entry, with its entry undo).

A close never drops held text (D10), and a tab moved to another window keeps it (D11). A crash
during an install loses at most the held text, which was never shown or drafted; that is recorded,
not prevented. A tab holding typed input is never evicted from memory
(`eligible_for_memory_eviction`, and the eviction's own freshness check between its turns, which
reads `has_held_text` because the eviction's lock is held while it runs).

### D6. The cap and its overflow

`HELD_TEXT_CAP_BYTES` = 4 KiB of UTF-8, kind **cap** (per held episode), a power of two (rule in
`.agents/rules/rust.md`). Derivation: the longest hold is one settle (the lane's cap is 10 s); a fast
typist at 20 characters per second fills 200 bytes of ASCII, 800 of four-byte UTF-8, so 4 KiB is five
times the worst typing. It is also small enough that the replay insert stays far inside `B_SETTLE`
on any document the edit watch lets the user edit (D8). Overflow is decided per **user action**:
the held length is recorded at the outermost `begin-user-action`, and an insertion that does not fit
truncates the held text back to that length and refuses the rest of the action, so a paste that GTK
delivers in several segments is never partly held (half a paste is worse than none). It publishes
`HeldNotice::Overflow` at once, saying that the clipboard still has the text and it can be pasted
again when the document is ready; text held by earlier actions is still replayed.

### D7. Feedback and accessibility

- The first held character of an episode publishes one information status message,
  "Typed text will appear when “name” is ready." Discards and refusals publish warnings, which the
  status bar announces on its throttled lane. Each reason is rate-limited like Protected View's
  notices (`notice_due`).
- While text is held, the editor's accessible description adds "Text you type is kept and inserted
  when the document is ready." The accessible state stays read-only and busy while the lock is held,
  as C6 has it: the buffer does not change until the replay, and AT reads the buffer. The replay's
  insertion is an ordinary text change, which AT reports as inserted text.
- Input method: the preedit is drawn by the view at the caret and never enters the buffer; its commit
  is a typed insertion (D2). The replay waits for a preedit to end, at most 1,024 ms (D4). The
  editor's `retrieve-surrounding` answer (`surrounding_text.rs`) puts the held text, bounded to the
  surrounding window, before the cursor, and the view's `delete-surrounding` handler is replaced the
  same way: a deletion that falls inside the held text edits it. An input method that composes by
  deleting what it committed (Telex for Vietnamese) would otherwise delete document text the user
  never meant, or be refused and leave the held text garbled. A deletion outside the held text keeps
  GTK's behaviour, which the lock and D3 then refuse.

### D8. The replay dispatch fits `B_SETTLE`

The replay is one insert of at most 4 KiB at the caret. Its cost is GtkTextView invalidating and
re-validating the caret's paragraph, which is what a single typed character costs in that paragraph
plus the inserted characters' layout. On documents where a keystroke fits `B_INTERACT` (the edit
watch's rule), 4 KiB more of text in the paragraph fits `B_SETTLE`; in the lane the held text is at
most 60 characters. The lane verdict checks it (tasks 4).

Measured (a throwaway widget probe, debug build, cairo renderer, under the measurement lock, load
5.9–6.9 with a game on 2.4 CPUs, three repetitions, `.claude/handoffs/keystrokes-verdict/budget/`),
against a **4 KiB paste control**: the same text pasted at the same caret into the same document,
loaded. Replay dispatch / paste: `dense` 710 × 400 ASCII 0.7–0.8 / 0.7–2.4 ms, four-byte 0.7 /
0.6 ms, 5 MB one line 0.4 / 0.5–0.8 ms, 256 lines into 5 MB 2.9–3.5 / 3.7–10.7 ms; the dispatches in
the 200 ms after are the same size for both (up to 29 ms replayed, 49 ms pasted, four-byte font
fallback), so "the frame after costs what a paste costs" is measured, not assumed. Two replays that
change the document's scale: one that grows a 48,000-character paragraph past `L_PROTECT_EDIT` and
**enters Protected View** took 2.7–3.1 ms (13.6 ms on the first, cold repetition; the paste 2.2–6.8
ms), and one that grows a `.rs` paragraph past `L_SYNTAX` and **turns syntax off** took 0.9–1.2 ms
(paste 2.5–3.0). All within `B_SETTLE`.

### D9. Lane

The probe classifies a key that arrives while its editor is held (`LushtextEditorPage::holds_typed_input`)
as `held-editor`. Such a key is drawn only at the replay, so its key-to-frame figure measures nothing
the user sees: the driver reports it apart (`held_input_to_frame`), counts it in `to_held_editor`, and
the delivery gate requires `inserted >= to_editor + to_held_editor`, so a held key that is never
replayed fails the phase. The `editor_user_insert` mark is recorded once per inserted **character**
(the replay is one insertion of many), which is what the delivery requirement already says.

### D10. A close never drops held text

Every close collector asks `LushtextEditorPage::has_unsaved_changes()`, which is `is_modified() ||
has_held_text()`: tab close (`handle_tab_close_request`, `confirm_close_tab`), Close Others and bulk
close (`tab_strip/close_execution.rs`, `collect_modified_close_targets`), window close
(`close_request`, `modified_editors`). The close-safety fingerprint includes the held character
count, so typing during the close transaction aborts it like any other change.

Before collecting, a close calls `settle_held_text_for_close`, which asks each editor's
`flush_held_text_for_close()` (the pure `close_disposition`):

- **Nothing held:** the close goes on as before.
- **Held, no workflow owns the buffer** (the gap between a release and its settle dispatch, or an
  active composition): the composition is reset and the settle runs synchronously. The document is
  then modified, so the normal Save Changes dialog asks and its Save includes the text (or the text
  was rescued into its own tab, which the window close then collects).
- **Held, a workflow owns the buffer** (a load, a replacement, a save, or a pending draft restore):
  the close is refused, as `is_saving` already refuses it, with "Typed text is waiting for “name”;
  close it when the document is ready."

`app.quit` (Ctrl+Q) called `GApplication::quit`, which skipped `close_request`, the Save Changes
dialog and the close-safety flush entirely (pre-existing). It now calls `request_quit`, which
closes every window through its own close confirmation; the process ends when the last window is
destroyed, and with no window it quits at once.

### D11. A tab moved to another window is not a teardown

`AdwTabView` reports a close and a transfer to another tab view with the same `page-detached`. The
detach terminal tore every detached page down, so moving a tab to another window cancelled its
load (and its held text was then rescued or lost) and stopped its file monitor (pre-existing). A
close the window confirmed now records its page (`closing_pages`, inserted at
`close_page_finish(.., true)`); a detach of any other page is a transfer: the source window drops
its bookkeeping (open paths, notifications, memory tracking), the editor keeps its load, monitor and
held text, and the receiving window's `page-attached` adopts it with the wiring a page it opened
gets (`adopt_transferred_tab`: open paths, info bar, notes, modified indicator, preview projection,
tab configuration, memory tracking, session). The draft-id claim follows on its own: a claim whose
owner no longer has an editor for the id passes to the asking window.

### D12. A partial buffer is never the user's modification, and never saved

Found while landing (pre-existing, data safety). GtkTextBuffer's modified flag says only that the
buffer changed, and three workflows change it without the user:

- **A sliced load installation.** A tab mid-install reported `is_modified()`, so its close asked
  Save Changes and the window close offered it for saving. Ctrl+S and the dialog's Save were
  refused (`LoadInProgress`), so nothing partial reached disk that way, but the dialog asked about a
  change the user never made.
- **Save As during a load.** A Save As pre-empted the load (`save_may_preempt_pending_load`): it
  cancelled it and wrote the buffer. While the file was still read the buffer was **empty**, so a
  Save As onto the file being opened **truncated it to zero bytes** (failing-first:
  `test_save_as_during_an_install_never_writes_the_partial_buffer`); mid-install it cancelled the
  install and left an emptied tab.
- **A memory eviction that went stale** after its first clear turn (an access, any
  memory-policy change): the cancelled clear emptied the buffer, the eviction terminal ignored the
  cancelled outcome, and the tab was left `Loaded`, not evicted and **modified**, so Ctrl+S or the
  dialog's Save **truncated the file** (failing-first:
  `test_an_eviction_cut_short_never_leaves_an_emptied_buffer_to_save`). Any replacement cancelled
  after it started mutating leaves the same emptied buffer.

The rules:

- `is_modified()` is the buffer's flag **unless** a workflow is changing the buffer right now: a
  sliced install session is live (its cancelled clear included) or a memory eviction is emptying
  it. Both install terminals clear the buffer's flag (the publish, the cancelled clear), so text the
  user types into a buffer an install left behind is modified again and its close asks (the
  cancelled-install autosave guard, `test_incomplete_load_installation_blocks_draft_autosave_over_a_good_draft`,
  depends on it). Held text and recovered drafts still count
  (`has_unsaved_changes`; a draft recovery is a replacement whose terminal sets the flag), so a close
  of an installing tab with neither asks nothing and closes.
- A replacement cancelled after it mutated clears the buffer's modified flag and records
  `left_partial` (cleared by a completed replacement or a published load). A stale eviction is
  treated as an eviction: the page is marked evicted and reloads (at once when it is shown).
- **No save pre-empts a load in flight**: Save As is refused with `LoadInProgress` like Ctrl+S. The
  derivation now only decides whether a save withdraws a reload queued during final projection.
- Every save is refused, at the queue and again at admission after `cancel_load`, while the buffer is
  not a whole document (`buffer_holds_no_whole_document`): an install owns it or left it
  (`install_owns_changes`, set when the chunked session starts and cleared only by the publish or
  the cancelled clear, or `installation_incomplete`: `IncompleteLoadInstallation`), the page is evicted, or a cancelled replacement emptied it and the
  user has not typed since (`DocumentNotInMemory`, new). Typing into an emptied buffer makes it the
  user's text again, which saves.

Widget tests (shard `editor-page`, `held_typing`): a tab mid-install is not modified and closes
without asking; the window-close dialog saves the edited document and never the installing one;
Ctrl+S during an install writes nothing (already refused on `main`: a guard); Save As during the
read and mid-install writes nothing and lets the load finish; a stale eviction leaves no emptied
buffer to save. `test_file_chooser_save_as_cancels_pending_load_result_before_adopting_destination`
(window) pinned the old pre-emption and is rewritten for the new contract
(`test_file_chooser_save_as_during_a_pending_load_is_refused_and_writes_nothing`). All but the Ctrl+S guard failed on the pre-fix tree.

## Risks / Trade-offs

- [The held window covers `Loading`] A load that stays queued behind admission keeps typing held for
  as long, and a close of that tab is refused meanwhile (D10). → The text is kept, not lost, and the
  status message says it will appear; a load always reaches a terminal (completion, failure,
  cancellation, disposal).
- [A rescue opens a tab the user did not ask for] → It is selected and announced; the alternative is
  losing the text.
- [The taint rule refuses typing over a selection] → Refused with a warning; selections are hidden
  with the caret during an install, so this is rare, and replaying only the insert would be wrong.
- [GtkSourceView auto-indent or an input method that inserts then deletes in one action] → The insert
  is held, the later deletion refused and reported; the held text is kept.
- [Key-to-frame for held keys looks slow] → It is reported apart; the user sees the replay.

## Alternatives considered

- **Replay at `unlock_edits`, synchronously.** Rejected: the release runs inside the terminal, before
  `Loaded`, the draft baseline and the content republish (D4).
- **Hold in a capture-phase key controller.** Rejected: an input method delivers text through its
  commit, not as key events, and paste and Return arrive through keybinding signals; the buffer's
  user action is the one place all of typing's routes meet.
- **Replay each held insertion separately.** Rejected: N inserts, N watch runs and N undo actions
  where one does.
- **Replay into the superseding document only when the path is unchanged.** Not needed: supersession
  happens within the same editor, whose identity is the document; `Unpublished` then `WholeContent`
  covers it (D5).
