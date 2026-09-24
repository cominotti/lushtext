## Context

`services/draft_service/set_aside.rs` is the single owner of every draft body
that left the journal without being applied. Bodies arrive by `move_in`
(unattributable crash leftovers found by reconciliation) and by `keep_copy`
(stale or unrestored bodies, and a body a re-registration would overwrite).
Names are `{draft_id}.{stamp}[-n].draft` and are never reused. The module
contract is "never pruned; only the user's confirmed Delete on
`Preferences > Data` removes one". The Data page group
(`ui/preferences/data_page.rs::render_set_aside_drafts`) lists rows with
Open and Delete. Open reads the body (bounded by `MAX_AUTOMATIC_DRAFT_BYTES`)
into a new untitled tab through `LushtextWindow::open_set_aside_draft`, which
marks it draft-dirty so the journal protects the text while the tab is open.

Facts that shape the design:

- Each body can be up to 64 MiB, and nothing bounds the count. Stale
  file-backed drafts, the commonest source, recur every time a user edits a
  file that another tool then changes.
- `set_aside::list` passes `max_entries: 256` to `visit_directory` and sorts
  afterwards. Past 256 bodies it shows an arbitrary subset as if it were the
  newest, and does not say so. That is a pre-existing defect, and it is fixed
  here.
- The phase-0 goal, and the ownership comment in `set_aside.rs`, say no path
  deletes unsaved user content without a user decision.
- Window status messages (`services/notifications.rs`) carry no action
  buttons, and inline action alerts are editor-scoped. A process-level
  "review" notice therefore needs an action reachable elsewhere.

## Goals / Non-Goals

**Goals:**

- Make set-aside growth visible before it becomes a problem, and give the user
  one efficient, safe way to reclaim space.
- Keep "no deletion without a user decision" as a checked property, not a
  convention.
- Make the listing truthful at any size.

**Non-Goals:**

- Automatic pruning of any body (D1).
- Tracking which bodies the user has opened (D3).
- A zero-copy view shared with local history (still needs an index change,
  per the deferral inventory).
- Changing how or when bodies are set aside.
- Per-body preview on the Data page. Open remains the way to see content.

## Decisions

### D0. A set-aside name identifies a body only with its bytes (E1 fix)

`keep_copy(id, stamp)` used to return early when `{id}.{stamp}.draft`
existed. The stamp is the entry's `saved_at_secs`, and a crash between a body
write and its manifest commit leaves a newer body under that unchanged stamp,
so the early return reported the newer body kept without copying it. The
caller then released the restore hold, and autosave could replace the only
copy.

`place` now probes `{id}.{stamp}[-n].draft` in order, and asks the pure
`journal_core::set_aside_name_step` what to do with each name: a free name is
taken, a byte-identical copy is "already kept" (so retries stay idempotent),
and any other content is left alone while the next name is tried. The size is
compared before any bytes are read; a read failure counts as "different",
which costs at most one extra copy. A move (`move_in`) never dedupes.

A Kani harness (K9) drives that function over an abstract area for one id,
arbitrary stamps, and arbitrary contents, and checks that every body reported
kept is in the area and no kept body is replaced. A `should_panic` twin shows
that the stamp-only rule breaks K9. The journal model keeps preserved content
as a set and cannot see naming, which is why the proofs missed this.

*Alternatives considered:* keying names by a content hash. Rejected: it
changes the set-aside name format that older builds' `parse_name` reads, and
comparing only the few names under one id and stamp is enough.

### D1. No automatic deletion, even of opened bodies

The brief allows auto-pruning of bodies the user already opened or restored.
It is **rejected**. "Opened" does not mean "safe elsewhere":

- Open creates an untitled tab whose journal draft lives only while that tab
  or its draft lives.
- If the user then discards that tab, or saves elsewhere and edits more, the
  set-aside copy is again the only copy of the preserved version.
- Proving that the content survives elsewhere would need content
  fingerprinting across drafts, files, and local history, which is too costly
  and fragile for a hygiene item.

The bound is therefore **soft**: it surfaces a notice and a review path, and
deletion is always a confirmed user action.

*Alternatives considered:*

- (a) An age-based auto-delete of opened bodies after 90 days. Rejected for
  the reason above.
- (b) A hard cap that refuses new set-aside placements. Rejected: refusing a
  placement blocks the journal's preserve-first ordering. Deletion then stops,
  and stale drafts are kept in the journal instead. That is not a bound, just
  a different pile.

### D2. The soft bound and when it is evaluated

The bound is `SOFT_BOUND_BODIES = 100` or `SOFT_BOUND_BYTES = 256 MiB`,
whichever is hit first (four maximum-size bodies, or a hundred typical ones).
Both are constants in the pure core, with no GSettings key: a preference for
"how much preserved work may pile up before you are told" adds a knob without
a use.

Evaluation runs off GTK:

- once per process, after the first window's startup restore settles (the
  `recovery-restore-complete` point);
- after a placement, when the journal reports one.

Totals come from a bounded full scan: at most `MAX_SET_ASIDE_SCAN_ENTRIES`
(10,000) entries. Beyond that, the count is reported as a lower bound and
treated as over the bound.

### D3. No opened-state tracking (maintainer decision, 2026-09-23)

The change records nothing about which bodies the user has opened, and it
introduces **no new persisted file**. There is no per-row opened state, no
opened-only bulk action, and no persisted notice acknowledgment.

Rationale:

- The set-aside area is rare. It fills only through stale, unrestored, or
  unattributable drafts, and most users never see it.
- An opened-vs-unopened record would be a permanent persisted format with a
  lifetime cost out of proportion to that use: a versioned envelope, a
  format-inventory kind with missing/current/future/damaged handling, and
  cross-version compatibility for as long as the file exists.
- By D1, "opened" would never have authorized deletion. It would only have
  made one bulk choice easier, so dropping it loses no safety property.
- Tracking can be added later, as its own change, if the area grows in
  practice.

The one behavior that leaned on persisted state, the notice's rate limit, is
kept in memory instead (D5).

*Alternatives considered:*

- (a) A v1-envelope ledger, `drafts/set-aside/review.json`, holding a map of
  opened file names and the last acknowledgment, registered as a new
  format-inventory kind. Rejected for the lifetime cost above. It was the
  previous design of this change.
- (b) Marking opened by renaming (`….opened.draft`). Rejected: older builds'
  `parse_name` would reject the name and hide those bodies.
- (c) xattrs or mtime. Rejected: not portable across copies and backups.

### D4. Bulk deletion is a pure plan over fingerprints

The new `services/draft_service/set_aside_retention.rs` is GTK-free and
I/O-free, and is the only place the bulk plan and the notice are decided:

- `bound_status(totals) -> BoundStatus { Within, Over { by_count, by_bytes }, Unknown }`
- `notice_due(status, last_notified: Option<Totals>, reviewed_this_process: bool) -> bool`
  (D5).
- `deletion_plan(bodies: &[ListedBody], decision: UserDecision) -> Plan`

`UserDecision` is one of:

- `None`;
- `DeleteAll { confirmed: Fingerprints }`, the exact set the confirmation
  dialog showed.

A body is planned only if its current fingerprint (file name, size, identity
and modification time from metadata) is in `confirmed`. A body that appeared, or changed, after the
dialog opened has a fingerprint outside the set and is kept.

Execution is `set_aside::delete_confirmed(data_dir, fingerprint)`. It runs off
GTK, one body at a time. Immediately before removal it re-reads the metadata,
skips on any mismatch, and syncs the directory once per batch. A failure keeps
the remaining bodies and reports a count. Each deletion goes through
`ensure_inside`. Per-row Delete keeps its existing path.

**Proofs** (programme step 8a), in a `#[cfg(kani)] mod kani_proofs` beside the
module, over 4 bodies with arbitrary fingerprint and change facts and an
arbitrary `UserDecision`:

- **R1** `None` ⇒ empty plan.
- **R2** Every planned body's current fingerprint is in `confirmed`.
- **R3** A body whose fingerprint changed after the confirmation, or that was
  not listed in it, is never planned.
- **R4** `bound_status` and `notice_due` never panic, and no status value
  leads to a plan.

A `should_panic` harness runs a deliberately broken plan that deletes a body
outside the confirmed set (for example, every listed body regardless of
`confirmed`) and shows R2 failing, so the harness is known to catch that
defect. A proptest mirror of R1–R3 runs in `make test-unit`. The harnesses
join the `core-journal-and-write` shard, or a new small shard if that one's
time budget needs it.

### D5. Surfaces: summary row, bulk action, notice, and action

- **Summary row** (`AdwActionRow`) first in the group, for example "37
  preserved drafts · 412 MB · over the suggested limit", followed by "Showing
  the newest 256 of 1,204" when truncated. It carries the "Delete All
  Preserved Drafts…" text button when the group is non-empty.
- **Confirmation** is an `AdwAlertDialog` with Cancel as default and close
  response. Its body states the exact count and total size it will delete, for
  example "Delete 37 preserved drafts (412 MB)? This cannot be undone. Drafts
  preserved after this dialog opened are kept." The destructive response
  deletes only that set (D4).
- **Notice:** a window status warning published through the notification bus
  of the active window. It reads: "LushText is keeping N preserved drafts
  (X MB). Review them with Review Preserved Drafts."
- **Rate limit, in memory only.** The process keeps the totals it last
  notified and whether the user ran the review action. `notice_due` is true
  only while over the bound and when either no notice was published yet in
  this process, or the area has grown ≥ 25 % in count or bytes past the last
  notified totals; it is false for the rest of the process once the review
  action ran. The practical consequence is at most one startup notice per
  launch while the area stays over the bound. That is the accepted cost of D3.
- **Action:** `app.review-preserved-drafts` in `app.rs` presents Preferences
  on the Data page, scrolls the group into view, and marks the review done for
  this process. It goes in the action catalog, so the command palette and
  automation discover it.

The grouped-row, title-line, and accessibility metadata rules of
`.agents/rules/ui.md` apply, with row position metadata and summary value
text. No body text appears anywhere, including logs, announcements, and
automation.

### D6. Listing correctness

`set_aside::list` is split:

- `scan_totals` counts and sums across the whole area within the scan budget;
- `list_newest(limit)` keeps a bounded top-N heap by stamp during the same
  traversal, so memory stays at 256 rows.

`SetAsideListing { rows, total_count, total_bytes, complete }` replaces the
bare `Vec`. When the listing is truncated, the Delete All confirmation covers
only the listed rows, and says so ("the newest 256 of 1,204"); the rest stay
for a later pass.

## Risks / Trade-offs

- [Users who never review keep growing the area] → Mitigation: the notice
  repeats on growth within a process and on each launch while over the bound.
  That is the honest limit of "no deletion without a decision", and it is
  stated as such in the docs.
- [A notice on every launch while over the bound may feel like nagging] →
  Mitigation: it is one 10 s status message, never a dialog, and one Delete
  All clears the cause. A persisted acknowledgment is the follow-up if this
  proves annoying (D3).
- [The notice is missed because status messages expire after 10 s] →
  Mitigation: the summary row states the over-bound state persistently, and
  the action is discoverable in the palette.
- [Delete All removes bodies the user never looked at] → Mitigation: it is an
  explicit destructive confirmation stating the exact count and size, Cancel
  is the default, and per-row Open remains available first.
- [Fingerprint identity on filesystems without stable inodes] → Mitigation:
  size and name must also match, and names are never reused by `place()`. A
  false mismatch only keeps a body.
- [The whole-area scan is slow on a huge area] → Mitigation: it is bounded,
  off GTK, and reports a lower bound past the budget.

## Migration Plan

Nothing to migrate: no persisted format is added or changed. If the area is
already over the bound, the first launch shows one notice. Rollback: an older
build keeps every body and simply lacks the summary, bulk action, and notice.

## Open Questions

- Whether the bulk action should also be offered from the notice's action
  target directly (for example, a filtered dialog). The default is no: the Data
  page is the single review surface.
