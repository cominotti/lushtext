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

**Goals:** no typed character silently lost; typed text appears at the caret the document is shown
with, in order, as one undo step; data safety for drafts, the modified state, the lock's ordering,
C2's entry rules and supersession; immediate visible and accessible feedback for anything that is
still refused; the replay dispatch within `B_SETTLE`.

**Non-Goals:** holding edits other than typing (deletions of document text, undo, moves, Replace);
keeping held text across a crash or a tab close (it is in memory for the install's duration only);
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
6. it **fits** the held buffer whole (D6).

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
rate-limited status warning (`HeldNotice::EditNotApplied`). They are not replayed: a deletion's target
was computed on a buffer that the install is replacing, so replaying it at the final caret could
delete text the user never saw.

### D4. When the held text is inserted: the settle dispatch

A **settle dispatch** (a default-idle source, one per editor at a time) is queued when the first
character of an episode is held and again whenever the last lock holder releases while text is held.
It decides with the pure `settle_verdict`:

- **Wait** while a workflow still holds the buffer: the lock is held (its release queues the dispatch
  again) or a load is in flight (the dispatch re-arms itself on a 16 ms timeout, so a load's failure,
  cancellation or completion is seen without a hook in every terminal);
- **Wait** while an input-method preedit is in progress, so the replay's caret move does not reset a
  composition the user is in the middle of (the same 16 ms re-arm);
- **Replay** when the document is whole and editable (D5);
- **Discard** otherwise (D5).

The release itself never replays: `unlock_edits` runs in the middle of a terminal (the load sets
`Loaded`, the caret, the modified flag and the draft baseline around it), so the replay waits for the
dispatch after the terminal has finished.

The replay inserts the held text with `gtk_text_buffer_insert_interactive_at_cursor` (`editable` =
the view's), inside one explicit user action: GtkSourceView's undo manager records one action, so
**one Ctrl+Z removes the replay** and nothing installed before it. It runs after the lock is released
and the projections are restored, so the buffer's `changed` and `modified-changed` handlers mark the
document modified and draft-dirty and the next autosave writes it; the document-scale watch and the
paragraph-wrap and Protected View edit entries see it as an ordinary edit. The view then scrolls the
caret on screen when it has keyboard focus, as typing does.

Where the caret is: the install terminal places it (start of document, or the restored session
position, `apply_restore_position`) before it releases the lock, and the caret was invisible during
the install. So the held text appears where the caret is when the document is shown, which is the
coordinator's stated expectation.

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
| Protected View, or the view not editable | discard, Protected View notice `HeldTextNotInserted` |
| broken episode, `Failed` load, incomplete installation, evicted | discard, status warning naming the count |
| otherwise (`Loaded`, `Untitled`, a reload failure that kept the old text) | replay |

A cancelled or failed first load leaves an empty or partial buffer whose save is already refused
(`installation_incomplete`), but inserting the held text would make a document that consists of only
the typed text; a later retry would then race the user's edit. Discarding with a warning is the safe
choice; the warning says how many characters were not inserted.

Protected View (C2): a document whose profile is protected is entered at install begin
(`begin_protected_install`), so its view is not editable from then on and GtkTextView refuses typing
before any signal; Protected View's own pre-emption notice covers those keys. Text held before that
(while the load was planning) is discarded at the settle with the Protected View notice. A replay into
an editable document that the replay itself makes protected enters through C1's edit watch like any
other edit (C2's edit entry, with its entry undo).

Closing a tab cancels its load and drops its held text with the page, as it drops the load: the user
asked for the document to go. A crash during an install loses at most the held text, which was never
shown or drafted. Both are recorded, not prevented.

### D6. The cap and its overflow

`HELD_TEXT_CAP_BYTES` = 4 KiB of UTF-8, kind **cap** (per held episode), a power of two (rule in
`.agents/rules/rust.md`). Derivation: the longest hold is one settle (the lane's cap is 10 s); a fast
typist at 20 characters per second fills 200 bytes of ASCII, 800 of four-byte UTF-8, so 4 KiB is five
times the worst typing. It is also small enough that the replay insert stays far inside `B_SETTLE`
on any document the edit watch lets the user edit (D8). Overflow: an insertion that does not fit
**whole** is refused whole (never split: half a paste is worse than none) and publishes
`HeldNotice::Overflow` at once; the held text already kept is still replayed.

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
  is a typed insertion (D2). The replay waits for a preedit to end (D4). `retrieve-surrounding`
  answers from the buffer, which does not contain held text; an input method that reads surrounding
  text sees the document as it is, which is the same as for a refused key today.

### D8. The replay dispatch fits `B_SETTLE`

The replay is one insert of at most 4 KiB at the caret. Its cost is GtkTextView invalidating and
re-validating the caret's paragraph, which is what a single typed character costs in that paragraph
plus the inserted characters' layout. On documents where a keystroke fits `B_INTERACT` (the edit
watch's rule), 4 KiB more of text in the paragraph fits `B_SETTLE`; in the lane the held text is at
most 60 characters. The lane verdict checks it (tasks 4).

### D9. Lane

The probe classifies a key that arrives while its editor is held (`LushtextEditorPage::holds_typed_input`)
as `held-editor`. Such a key is drawn only at the replay, so its key-to-frame figure measures nothing
the user sees: the driver reports it apart (`held_input_to_frame`), counts it in `to_held_editor`, and
the delivery gate requires `inserted >= to_editor + to_held_editor`, so a held key that is never
replayed fails the phase. The `editor_user_insert` mark is recorded once per inserted **character**
(the replay is one insertion of many), which is what the delivery requirement already says.

## Risks / Trade-offs

- [The held window covers `Loading`] A load that stays queued behind admission keeps typing held for
  as long. → The text is kept, not lost, and the status message says it will appear; a load always
  reaches a terminal (completion, failure, cancellation, disposal).
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
