## Context

This is C5 of the long-line stream in `docs/next/long-line-responsiveness.md`.
C0 measures toolkit costs and records the budgets; C1 adds the paragraph
profile and `DocumentScale`; C2 adds **Protected View** (non-editable,
caret-free, display-only tail fold, Line Inspector, worker find); C3 and C4
are conditional and unrelated to editing. The maintainer recorded on
2026-10-11 that Protected View ships first and that making giant paragraphs
editable is an optional, exploratory C5.

Facts this design rests on (from PROPOSAL.md; verified against the GTK 4.22.5
sources under `target/huge-file-explore/src/` unless marked):

- `compute_log_attrs` runs over the whole logical paragraph, hidden text
  included, and is invalidated by every character change
  (gtktextbuffer.c:4469-4503, 4532-4537). Cursor motion, word motion and
  Backspace depend on it. Measured 92–99 ms on a 1.9M-character paragraph.
- A paragraph's layout is one PangoLayout over its visible text (A23, A24);
  the fold bounds layout only.
- IM retrieve-surrounding slices the caret's paragraph (gtktextview.c:8996-9059)
  and is armed by caret moves, keys and clicks (:5785, :5853, :6856).
- Visible-cursor iteration over hidden runs is linear in the hidden length
  (gtktextiter.c:3210-3236, reviewer-cited).
- AT-SPI contents/offsets and TextChanged payloads span the paragraph
  (reviewer-cited; A35).
- GTK paragraph boundaries are `\n`, `\r`, `\r\n`, U+2029 (C1 makes LushText's
  boundary function match).
- Text inserted strictly inside a tagged run is tagged; at either boundary it
  is not (A25, probe pending in C0).
- Tag apply/remove emit no `changed`, set no `modified`, record no history
  (A27, probe pending in C0).
- The undo history records text only, not tags ("Changes to tags and marks
  are not tracked", gtktextbuffer.c:5153-5154): undoing a deletion re-inserts
  the deleted characters without the tags they carried. There is no way to
  insert text unrecorded without discarding history: both
  `begin_irreversible_action`/`end_irreversible_action` and disabling undo
  clear the undo and redo queues (gtktexthistory.c:1011-1050, 1211-1229).

Budgets (C0 records the final values as policy constants; working values):
B_interact = 8 ms per interaction dispatch, input-to-presented-frame ≤ 2
frames, B_settle = 16 ms per settle dispatch, scroll p95 frame gap within one
frame on the reference lane. C0 stores them in `model/responsiveness_budget.rs`. Data integrity is absolute: the bytes on disk
change only where the user edited, and never by a mechanism the user cannot
see.

Every place that holds a document coordinate or the document text today
reads it from the GtkSourceBuffer as if buffer = file:

| Consumer | Coordinate or text it holds |
|---|---|
| Save / Save As | `text(start, end, true)` (`ui/buffer_snapshot.rs:630`, chunked path) |
| Drafts | body = buffer text; restore installs it |
| Local history | snapshot bodies; preview installs them; restore replaces the buffer |
| Bookmarks | `BookmarkRecord.line` (zero-based line), `GtkSourceMark` |
| Session restore | `SessionTab.cursor_line/cursor_col/scroll_line` |
| Workspace search | `SearchMatch.line_number` (1-based) + excerpt, computed from file bytes |
| In-tab find | C2's worker find (line+column records over decoded bytes) |
| Go-to-line, restore position | line/column → iter |
| Line-number gutter, status-bar position, document statistics | buffer line numbers, cursor line/column, character and line counts |
| Live editor memory estimate | `char_count()` (`model/editor_memory.rs`) |
| Print | buffer text (C2 refuses print in Protected View) |
| Clipboard, DnD | buffer text of a range |
| Undo/redo | GtkSourceBuffer history |
| Line endings, encoding, file health | analysed on load from decoded text, re-applied on save |
| EditorConfig formatting on save | operates on the saved text |
| File monitor reload, eviction reload | reinstall from file |
| Minimap, invisibles, Markdown preview | read buffer text/lines |
| Accessibility, IM | GTK reads buffer offsets/paragraphs |
| Automation snapshot | counts, cursor line/column, never content |

## Goals / Non-Goals

**Goals:**
- Decide, on measurement, whether paragraphs above `L_protect` can be made
  editable within B_interact / B_settle / 2 frames with absolute byte
  integrity, and by which approach.
- Give every candidate the same criteria, the same consumer inventory, and
  the same fixtures, so the comparison is fair.
- Leave a decision record a future session can act on without re-deriving the
  analysis, including what would re-open it.

**Non-Goals:**
- Shipping editing, a production widget, a production offset map, a new
  action, setting, D-Bus member, snapshot field or persisted format.
- Changing Protected View's behaviour, thresholds or capability.
- Revisiting C0–C4 decisions (budgets, `L_protect`, fold semantics).
- Upstream GTK/Pango fixes as a dependency (reports may be filed; the
  decision may not wait for them).
- Using the user's file. Fixtures are synthetic, from C0's generator.

## Decisions

### D1. Three candidates, one rubric

Candidates A (segmentation + offset map), B (custom virtualized widget) and C
(windowed edit-through) are evaluated against the same rubric:

1. **Budgets** on the reference lane, per interaction in the spike's script
   (D4), with and without the AT bridge and a surrounding-text IM.
2. **Byte integrity**: no-edit round trip; single edit changes exactly the
   edited file range; line endings and encoding preserved as today.
3. **Consumer obligations**: what each row of the inventory must do, and
   whether that is a bounded, pure mapping or a behaviour change.
4. **Feature parity lost**: syntax, undo granularity, search/replace,
   bookmarks, multi-cursor-free selection, IM/CJK, screen reader.
5. **Cost**: estimated production size, new dependencies (`cargo deny`),
   workflow-convention placement, Kani/property coverage, axioms needed,
   maintenance against GTK upgrades.
6. **Reversibility**: can it be switched off per document without data risk.

*Alternatives considered:* evaluating only the two candidates PROPOSAL.md
named. Rejected because candidate C fell out of C2's own design (inspector +
non-editable main view) and may avoid the offset map entirely; omitting it
would bias the decision toward the expensive options.

### D2. Candidate A: in-buffer segmentation with an offset map

A giant paragraph is installed with synthetic paragraph breaks at most every
`N_segment` bytes of UTF-8 text (a byte cap also bounds characters; D10), chosen
on grapheme boundaries where one exists in the cap, so GTK never sees a
paragraph above `L_protect`. Each break is a real paragraph delimiter
character (GTK only recognises `\n`, `\r`, `\r\n`, U+2029) carrying a
non-editable `lushtext-synthetic-break` tag. A pure `SegmentMap` in `model/`
maps buffer offset ↔ file offset and buffer line ↔ (file line, column).

What the paper analysis must settle before any spike:
- **Delimiter choice.** U+2029 or `\n` both collide with real content; the
  tag, not the character, identifies a synthetic break. Every text read
  (`text(…)`, clipboard, DnD, snapshot chunks, drafts, local-history capture,
  print) must strip tagged breaks, and every write path must never strip an
  untagged one. A tag lost through undo, paste or tag-table churn is silent
  corruption, which is why the spike seeds exactly that defect.
- **Line identity.** Bookmarks, session, search hits and go-to-line use line
  numbers; with synthetic breaks the buffer line count differs from the file
  line count. Either every consumer maps through `SegmentMap`, or the
  persisted coordinates stay file coordinates and only the GTK adapter maps.
  The latter keeps formats unchanged and is the only acceptable shape.
- **Editing across a break.** Backspace at a segment start, selections that
  span breaks, and typing that grows a segment past `N_segment` (re-segment in
  the same emission, `connect_after("insert-text")`, as C1 does for
  downgrades).
- **Undo.** GTK cannot insert a break unrecorded without clearing the undo
  and redo queues (Context). The analysis must choose between recording
  re-segmentation inside the triggering user action (undo then reverts the
  edit and its breaks together, and the map must follow) and accepting that
  every re-segmentation discards history; files above the undo threshold
  already run inside `begin_irreversible_action`. Because history records
  text only, undoing a deletion that removed a synthetic break re-inserts the
  delimiter **untagged**, which save would then write as a real line break:
  this is the concrete tag-loss path the seeded defect models, and the
  analysis must show it unreachable (breaks are never deleted through a
  recorded action) or repaired before any read.
- **Costs that remain:** log attrs now span one segment (bounded); word
  motion stops at synthetic breaks (visible behaviour difference); a synthetic
  break forces a visual line break, so wrapping differs from the file; the
  line-number gutter, status-bar position and document statistics count
  buffer lines unless they map through `SegmentMap`; syntax still sees
  segments as lines (C1's gate keeps it off anyway).

*Pure core and proof:* `SegmentMap`, the strip function and the splice
planner are integer/offset arithmetic over a sorted break list, a good Kani
fit; D10 states the harnesses (K-SEG1–K-SEG4 and their `should_panic`
regressions) that become required if A is adopted. No break inside a
grapheme is out of scope for Kani and goes to a property test.

### D3. Candidate B: custom virtualized text widget

A LushText-owned widget used only when `DocumentScale.protected_view` holds:
a text model (rope or piece table; candidate crates are named in the record,
none is added to the workspace by this change), viewport-only layout that
builds PangoLayouts for the visible window of each visible paragraph, its own
caret, selection, word motion (bounded ICU-free segmentation over a window),
`GtkIMContext` with surrounding text bounded to a window, a
`GtkAccessibleText` implementation (GTK ≥ 4.14; confirm in the 4.22 sources)
exposing bounded contents, and its own undo.

The paper analysis must state honestly what is lost or rebuilt:
GtkSourceView features (search context, marks/gutter for bookmarks, space
drawer, minimap, completion), the Markdown preview hook, the existing
editor_page workflows (load, save, buffer_replacement, minimap) that are typed
against `sourceview5::Buffer`, accessibility parity, and bidi/complex-script
layout of a partial paragraph (a window cut mid-run changes shaping). The
spike measures only the core: open, scroll, caret motion, type, Backspace,
select, paste, undo, with AT bridge and IM, on the giant fixtures.

*Rejected sub-option:* subclassing GtkTextView with overridden layout. The
whole-paragraph costs live in GtkTextBuffer/GtkTextLayout private code, not
in overridable vfuncs.

### D4. Candidate C: windowed edit-through via the Line Inspector

The main view stays exactly C2's Protected View (non-editable, caret-free,
folded). "Edit Line…" opens the Line Inspector in edit mode over a copied
window `[s, e)` of one paragraph, cut on grapheme boundaries, at most
`N_edit` characters long (C2's read-only `N_inspect` may differ; see Open
Question 3). The inspector buffer is small, so its own log attrs, IM and
AT costs are bounded. Commit (explicit, or on window move) applies one
`SpliceRecord { paragraph, file_range, replacement }` to the main buffer as a
single user action: delete `[s, e)` and insert the replacement, with the tail
fold maintained at insertion. Strictly inside a hidden run the text inherits
the tag by A25; at either boundary of the run (the head/tail boundary, or the
paragraph end when `e` is the paragraph's last character) it does not, and the
executor tags explicitly. A splice that grows the visible head past
`N_visible` re-plans the fold, moving the excess into the tail by tag apply
(A27: no history, no `changed`). Undo and redo of a splice re-insert text
without tags (Context), so the fold executor re-applies the fold after every
undo and redo step; an unfolded giant run reaching a frame is a budget
failure the spike must detect.

Why it may fit the budgets: the main buffer is still the file text, so save,
drafts, local history, bookmarks (paragraph granularity unchanged), session
and search need **no offset map**; the main view never computes log attrs
because it has no caret (A34); the remaining main-buffer cost is one btree
delete/insert in a giant paragraph, the fold-tag bookkeeping, re-layout of the
visible head only when the splice touches it, and one AT TextChanged with the
replacement text. All of these are measurable and the spike measures them.

What the analysis must settle: the main buffer becomes modified (drafts and
autosave run on a protected document, so the draft body is a giant-paragraph
snapshot through the existing chunked path, and its budget must be measured);
undo granularity is per commit; stale windows when the file monitor reloads or
the buffer changes elsewhere (a generation-guarded `*Ticket` seam, rejecting a
commit whose paragraph generation moved); editing across window edges;
whether the inspector's local undo and the main undo compose.

*Pure core and proof:* splice planning (window bounds, grapheme-safe cut
indices given boundary candidates, the resulting file range, the generation
check) is integer arithmetic, a Kani fit; grapheme correctness is a property
test against `unicode-segmentation` or the GTK cursor positions of the window.

### D5. Spike isolation

The spike lives in `spikes/long-paragraph-editing/`, a standalone Cargo
package with its own `[workspace]` and `Cargo.lock`, listed in the root
`Cargo.toml` `exclude`, building into `build/long-paragraph-editing/`
(gitignored), invoked by a local-only `make long-paragraph-editing-spike
SPIKE_TARGET=a|b|c|all`, never in CI, `make check`, policy audits, Sonar
scope, widget shards, hakari or `cargo deny`'s production graph. It may
depend on `lushtext-core` by path (as `formal/evaluation/stateright` does)
for C1's paragraph boundary function (`model/paragraph_boundary.rs`) and C0's
budget constants (`model/responsiveness_budget.rs`), and it invokes C0's
fixture generator, `scripts/generate-long-line-fixtures.py`, as a script.
C0's watchdog and frame timing live in `ui/responsiveness_probe.rs` behind
the `responsiveness-probes` feature; the spike enables that feature through
its path dependency if the module is reachable, and otherwise reimplements
the same two measures. Production crates never depend on the spike. This
mirrors the `formal/evaluation/` precedent of the Stateright and Quint/TLA+
evaluations.

It runs under the same reference lane as C0 (private `mutter --headless`,
`GSK_RENDERER=cairo`, `taskset` to ≤ 16 cores, never the live desktop) and
uses C0's in-process `G_PRIORITY_HIGH` lateness watchdog and frame-clock
`after-paint` timing, so its numbers are comparable with C0's. Comparability is checked, not
assumed: before any candidate run, the harness re-measures one case C0
already recorded (a stock GtkTextView with one unfolded 250k-character
paragraph, Right ×100) and the record states both values; a harness whose
result falls outside C0's recorded run-to-run spread is fixed before any
candidate number counts. Each run
records `btrfs-stall-report --json` for its window, and a run whose verdict is
not "Storage was NOT a bottleneck" is repeated before it counts.

*Alternatives considered:* spiking inside `crates/lushtext-core` behind a
feature flag (rejected: production crates must not carry disposable code,
and every gate would have to learn to skip it); spiking in the adoption lab
(rejected: it is a maintained second consumer of GTK Lush, not a scratch
area).

### D6. Interaction script and fixtures

Fixtures from C0's generator (10M is a size parameter, not a new fixture
kind): one paragraph of 50k, 250k, 1.9M and 10M characters (ASCII, and a mixed-script variant with combining marks, emoji ZWJ
sequences and RTL runs); CR-only and U+2029-separated variants; a document
with eight giant paragraphs among short lines.

Script per candidate, each step timed by dispatch and input-to-frame: open;
scroll to the middle; place the caret (or open the window) at the paragraph
middle; Right ×100; Ctrl+Right ×20; type 200 characters; Backspace ×50;
select 10k characters by Shift+motion; copy; paste 1 MB into the paragraph;
undo ×5; redo; save; draft autosave tick; local-history capture; restore the
session with the caret inside the paragraph; external reload through the file
monitor. Each run once with the AT bridge and an AT-SPI listener
(accessibility-smoke infrastructure) and once with an IM requesting
surrounding text.

Budget classes are fixed before measuring: open, session restore, external
reload, save, the draft autosave tick and the local-history capture are
settle steps (B_settle per dispatch); every other step is an interaction step
(B_interact per dispatch and input-to-presented-frame ≤ 2 frames); scrolling
also has the frame-gap limit. Work a step legitimately moves off GTK is timed
by its GTK dispatches, with its completion recorded separately.

Every step also runs on a **control** fixture: an ordinary document of the
same total size with short lines, in the stock editor. A step the stock
editor already misses on the control does not discriminate between
candidates; the record lists it and enters it in the long-line programme
record's pre-existing-blocker list with an owning change, instead of
excusing it.

A candidate **passes** only if every step meets its budget class on every
fixture, and the integrity checks (D7) pass.

### D7. Integrity checks are seeded, not assumed

For every surviving candidate the spike runs:
- no-edit round trip on every fixture: saved bytes equal the fixture bytes;
- single-edit check: the saved file equals the fixture with exactly the edited
  file range replaced (oracle computed independently of the candidate);
- draft and local-history bodies equal what save would write;
- seeded defects that must be caught: A — a synthetic break whose tag is
  dropped (including by an undo that re-inserts it untagged), a break inserted inside a grapheme, an off-by-one in `SegmentMap`;
  B — a window cut inside a grapheme, an undo that drops a span; C — a commit
  against a stale paragraph generation, an off-by-one splice range.

A candidate whose checks miss a seeded defect fails the integrity criterion
regardless of its timings.

### D8. Toolkit facts go to the ledger

Any GTK/GtkSourceView/Pango behaviour the decision depends on (for C:
btree delete/insert cost in a giant paragraph, tag inheritance on splice at
both run boundaries, TextChanged payload of a splice, undo re-inserting text
untagged; for A: tag loss through undo and paste, history clearing on
unrecorded insertion, log-attr scope ending at a synthetic break) is either an existing ledger
axiom (C0's A23–A36) or a new ledger entry with a `gtk-lush-axioms` probe and
an interactive/`--check` sample, run through `make gtk-axioms` and
`make check-gtk-axioms`. This is the one maintained artefact the change may
add, because the growth rule requires it and a later adoption change would
need it anyway.

### D9. The decision record

`docs/next/long-paragraph-editing.md` holds: the rubric table per candidate
with measured numbers, the consumer inventory with per-candidate obligations,
integrity results, seeded-defect results, the proposed production placement
of the winner under the workflow role convention (for C: a `line_inspector`
editing workflow with `policy.rs` for splice planning, a `journal`-free
`execution.rs`, an `evidence.rs`; for A: `model/segment_map.rs` plus a
cross-cutting GTK adapter), the Kani/property plan, and a **Decision** section
the maintainer fills: *adopt A/B/C via follow-up change `<name>`*, *keep
view-only*, or *defer*, each with re-open triggers (for example a GTK release
that bounds log attrs to the visible run). The long-line programme record is
updated in the same change.

### D10. Kani harnesses that adoption carries

The evaluation adds no production harness (Non-Goals; the spike is outside
every Kani lane). It fixes, before any adoption, which harnesses the
follow-up change must ship, so a Decision to adopt cannot arrive without a
proof plan. Everything here follows the normative Kani rules
(`openspec/specs/formal-verification-kani/spec.md`, `.agents/rules/build.md`
"Formal Verification (Kani)"); only the candidate-specific content is stated.

**If segmentation (A) is adopted.** Production pure core:
`crates/lushtext-core/src/model/segment_map.rs` (GTK-free, I/O-free), with
`#[cfg(kani)] mod kani_proofs;` and the harnesses in
`crates/lushtext-core/src/model/segment_map/kani_proofs.rs`, calling the real
`SegmentMap`, `strip_synthetic_breaks` and `plan_segment_splice`, never a
copy. Refactors the follow-up makes so the core is checkable:

- Offsets are **UTF-8 byte offsets of the decoded text** (equal to file byte
  offsets for UTF-8 files; other encodings keep today's encode-on-save layer,
  outside the map), held in `u32` (the 500 MB refuse limit is below
  `u32::MAX`). `SegmentMap::new` rejects a map whose buffer length
  `file_len + breaks × break_width` would exceed `u32::MAX`, so no overflow is
  assumed away. A synthetic break occupies `break_width` buffer bytes, passed
  in (1 for `\n`, 3 for U+2029) rather than hard-coded, so the harnesses
  cover both delimiter choices of D2. A file offset exactly at a break maps to
  the buffer offset **before** the synthetic delimiter; the rustdoc states
  this convention, which K-SEG1 depends on.
- The segment cap is a parameter `n_segment` of the planner, in **bytes**;
  production passes `N_segment` as Open Question 3 settles it (derived from
  C0's measured per-character costs; C0 records no `N_segment` itself). A byte
  cap bounds the segment's character count too, conservatively for multibyte
  text. The harness uses small symbolic caps; that the planner's behaviour
  depends only on lengths relative to the cap is the bound argument below,
  not a proved property, so the rustdoc claims only the proved caps.
- The planner receives the permitted cut positions (the adapter's grapheme
  boundaries) as input and, when none lies within the cap, falls back to the
  last UTF-8 character boundary within it. K-SEG4 is proved with a symbolic
  permitted-cut set over ASCII text, so the cap holds even when no permitted
  cut exists; preferring a grapheme boundary and never cutting inside a UTF-8
  sequence are property tests, not Kani claims.
- The planner is split into `apply_splice` (edit the segment list) and
  `resegment` (restore the cap), both pure, so a `should_panic` harness can
  remove the second stage.
- The break list is the only state; tags, iters and the GtkTextBuffer stay in
  the GTK adapter, which only executes the plan.

| Id | Property | Domain (stated in the harness and the rustdoc) |
|---|---|---|
| K-SEG1 round trip | for every valid file offset `f` (`0 ..= file_len`), `file_of_buffer(buffer_of_file(f)) == f`; for every buffer offset `b` (`0 ..= buffer_len`, not strictly inside a 3-byte break), `buffer_of_file(file_of_buffer(b))` is `b`, or the start of the break when `b` is the end of a synthetic break (the map is constant across a break, so it is not injective there); `new` rejects exactly the inputs whose buffer length overflows `u32` | any `u32` offsets and `file_len`; sorted, strictly increasing break list of length ≤ 4 with every break in `1 .. file_len`; `break_width ∈ {1, 3}`; `#[kani::unwind(6)]` |
| K-SEG2 monotonicity | `f1 < f2 ⇒ buffer_of_file(f1) < buffer_of_file(f2)`; `file_of_buffer` is non-decreasing, constant exactly across a break | same as K-SEG1 |
| K-SEG3 byte preservation | for any edit sequence, reconstructing the file from the segmented buffer (`strip_synthetic_breaks` over the planned segments) equals applying the same edits, mapped to file coordinates through `file_of_buffer`, to the flat text; with zero edits, `strip(insert_breaks(t)) == t` | flat text ≤ 8 bytes over a 2-symbol ASCII alphabet plus the real delimiter bytes (`\n` and `\r` when `break_width = 1`, so real delimiters collide with synthetic ones and a real `\r` can precede a synthetic `\n`; the 3 bytes of U+2029 as one symbol when `break_width = 3`); ≤ 3 edits, each in **buffer** coordinates (as the adapter receives them; endpoints not strictly inside a 3-byte break): a delete of any in-range span, including exactly one synthetic break (Backspace at a segment start, a no-op on the file), plus an insert of ≤ 2 bytes; `n_segment ∈ 1..=3`; `break_width ∈ {1, 3}`; unwind one more than the longest reachable buffer (≤ 14 bytes of text plus its breaks) |
| K-SEG4 cap | after `plan_segment_splice`, every segment (span between synthetic breaks) is `≤ n_segment` bytes and non-empty except the sole segment of an empty text, and the segment lengths sum to the text length (a real delimiter inside a segment is content; it only shortens GTK's paragraphs further) | same as K-SEG3, plus one further insertion of up to `3 × n_segment` bytes so growth needs two or more new breaks; symbolic permitted-cut set |

`kani::cover!` on each interesting branch, so a vacuous harness fails: an
offset exactly at a break, and a buffer offset at a break's end; an edit
wholly inside a segment; a delete of exactly one synthetic break (Backspace at
a segment start); a delete spanning two or more breaks; a delete emptying a
segment (merge); growth needing one and two new breaks; a cut with no
permitted position in the cap (fallback); a real delimiter byte adjacent to a
synthetic break, including a real `\r` directly before a synthetic `\n`; the
overflow rejection; the empty text.

The real `\r` before a synthetic `\n` is a line-identity hazard as well as a
byte one: GTK treats `\r\n` as one delimiter, so the buffer line count no
longer follows from the break count. K-SEG3 shows the bytes survive it; the
line ↔ (file line, column) mapping D2 needs is derived from the byte map and
C1's boundary function, and the paper analysis must settle the case (choose
U+2029, or never place a synthetic `\n` directly after `\r`) with a property
test, not a Kani claim.

`should_panic` regressions (each pins a guard D2 and D7 say is essential):
- `segment_cap_fails_without_resegment` — `apply_splice` without `resegment`
  violates K-SEG4 (typing past `N_segment`).
- `bytes_fail_when_a_break_loses_its_tag` — reconstruction from a break list
  missing one entry (the undo that re-inserts a synthetic break untagged)
  violates K-SEG3, so the adapter's repair-before-read step is required.
- `round_trip_fails_with_inclusive_break_bound` — the D7 off-by-one (`<=`
  instead of `<` when counting breaks before an offset) violates K-SEG1.

*Why the bounds are enough (an argument, recorded in the harness doc
comments, not a proof).* The map is linear between breaks: its value at an
offset depends only on how many breaks precede it, so K-SEG1/K-SEG2 run over
full-width `u32` offsets and the only bound is the break count; four breaks
reach every relative arrangement (before the first, between, at, after the
last, adjacent breaks). For K-SEG3/K-SEG4, the planner's decisions depend on
an edit's position relative to segment boundaries and on lengths relative to
`n_segment`; with caps 1–3 and texts of 8 bytes there are up to 8 segments,
and three edits reach every case the cover list names, including an edit after
a re-segmentation. A larger text or longer sequence adds only repetitions of
these cases. Any cover the bounds cannot reach is a bound error and fails the
harness, not a gap to accept.

*Expected cost.* K-SEG1/K-SEG2 are a handful of symbolic `u32` comparisons
over at most four breaks: seconds. K-SEG3/K-SEG4 carry the cost risk
(symbolic `Vec` growth through three or four edits, both break widths): the
spike prototype's local times (task 5.6) are the first estimate. If a
projected shard exceeds its margin, the follow-up fits it in the normative
order — split the harness (per `break_width`, per edit count) and the shard,
then another solver, and only as a last resort a reduced bound recorded in
the programme record and the harness doc comment with the property it
weakens.

Follow-up obligations: every harness in exactly one shard of
`scripts/kani-shards.py` (a new or existing `core-*` shard, gated
`scheduled` unless its CI measurement is ≤ 15 min) with `ci_minutes`,
`ci_peak_gib` and `measured_in` from a dispatched CI-runner run (the larger of
two), within 25 min / 12 GiB; `make check-kani-shards` passing; the `ORACLES`
entry `crates/lushtext-core/src/model/segment_map.rs` listing every harness,
cheapest first, with measured local seconds and a `ci` or `local` tier; a
`make proof-strength PROOF_STRENGTH_MODULE=crates/lushtext-core/src/model/segment_map.rs`
run recorded in `docs/next/formal-verification.md`, with every Kani survivor
triaged; any counterexample triaged there too (code, stated domain, or
envelope).

**Spike prototype.** If A is spiked and its core is small enough (Open
Question 7's time box), the spike carries the same four harnesses and three
`should_panic` regressions over its own prototype core, run locally with the
pinned Kani version inside the spike package (`cargo kani` with the spike's
manifest, never `make kani`, no shard, no `ORACLES` entry, because the spike
is outside every Kani lane by D5). The D7 seeded off-by-one must make K-SEG1
fail before it is reverted (failing-first). Verdicts, local wall time and
peak memory go to the evaluation record as estimates only (developer-machine
timing is not a shard measurement); a counterexample against the prototype
is triaged in `docs/next/formal-verification.md` and in the record.

**If the custom widget (B) is adopted.** It has no offset map, but four pure
cores get equivalent harnesses, under its role home's `policy.rs` or under
`model/` (the follow-up places them under the workflow convention):
- *Text model* (piece table or rope adapter, LushText-owned arithmetic over
  piece lengths, not the third-party crate's internals): logical offset ↔
  (piece, offset) round trip and monotonicity; byte preservation of a bounded
  edit sequence against the flat-text oracle (same domain shape as K-SEG3).
- *Undo journal*: undo of `k ≤ 3` edits restores the exact prior bytes; redo
  re-applies them; an edit after undo discards exactly the redo tail (texts
  ≤ 8 bytes, edits of the K-SEG3 shape without synthetic breaks).
- *Viewport and IM window planner*: the layout window and the surrounding-text
  window lie inside the paragraph, contain the caret, and never exceed their
  caps (`N_visible` for layout; the surrounding-text cap the follow-up names,
  since C0 records none), for any `u32` paragraph length, caret and cap ≥ 1.
- *Caret/selection clamp*: after any edit, caret and selection bounds lie in
  `0 ..= len` and the selection is ordered (any `u32` length, bounds and edit
  span).
Grapheme and bidi correctness stay property tests. The same shard, `ORACLES`,
proof-strength and triage obligations apply, and each harness's domain is
stated in its rustdoc as for A.

**If edit-through (C) is adopted.** The splice planner of D4 gets the
equivalent harnesses, with the same obligations: the planned `file_range` lies
inside the paragraph and equals `[s, e)`; both cut indices are taken from the
supplied boundary candidates (or the window is refused), and the window never
exceeds `N_edit`; applying the `SpliceRecord` to the flat paragraph changes
exactly `file_range` (byte preservation against the flat-text oracle, K-SEG3's
text and edit bounds); a commit whose paragraph generation differs from its
ticket's is rejected (any `u64` generations). One `should_panic` regression
pins the generation check: without it a stale commit is applied (the D7
seeded defect).

## Risks / Trade-offs

- [Spike numbers flatter a candidate because the spike omits production
  machinery (drafts, minimap gate, session)] → the script includes save,
  autosave, local-history capture, session restore and reload, and the record
  lists every production surface the spike stubbed.
- [Candidate C's main-buffer splice still pays an O(paragraph) cost somewhere
  unexpected, e.g. btree segment merge (A36) or AT TextChanged] → measured on
  the 10M fixture with the AT bridge on; the axiom entry pins it.
- [Candidate A's tag-identified breaks fail silently] → seeded tag-loss defect
  is a mandatory check; failure to catch it fails A.
- [Candidate B's cost is underestimated because the spike builds only the
  core] → the record's cost section lists every GtkSourceView feature that
  must be rebuilt, with an estimate, before the decision is taken.
- [Measurement on one reference machine] → the record states the machine and
  lane; per PROPOSAL.md, any validation on the user's session is a separate,
  consented isolated-XDG run.
- [Scope creep into shipping] → the spec forbids production changes; any
  adoption is a separate change.

## Migration Plan

None for users. The spike package and its make target are deleted, or kept as
disposable evidence, as the Decision section states; no rollback is needed
because no production behaviour changes.

## Open Questions

1. **Is editing giant paragraphs wanted at all?** The maintainer recorded C5
   as optional. If the answer after reading the paper analysis is "no", the
   spike may stop after D1–D4 and the Decision records "keep view-only".
2. **Is a narrower editing contract acceptable?** Candidate C gives
   commit-based editing through a secondary view (no in-place typing in the
   main view, undo per commit). Does that count as "editable", or must typing
   happen in place?
3. **Value of `N_edit` / `N_segment`.** Candidate C's editable window may
   differ from C2's read-only `N_inspect` (which never exceeds `min(N_visible, L_protect)`).
   Derive both from C0's layout and log-attr cost per character against
   B_interact = 8 ms, or set them independently?
4. **Draft and local-history capture on a modified protected document** must
   snapshot a giant paragraph; if the existing chunked snapshot path misses
   B_settle at 10M characters, is that a C5 blocker or a separate fix?
5. **New production dependency** (a rope crate) for candidate B: acceptable in
   principle, subject to `cargo deny`, or excluded up front?
6. **Spike retention.** Delete the spike package after the decision, or keep
   it as disposable evidence like `formal/evaluation/`?
7. **Time box.** How much effort may the spike take before the Decision must
   be recorded with whatever has been measured?
