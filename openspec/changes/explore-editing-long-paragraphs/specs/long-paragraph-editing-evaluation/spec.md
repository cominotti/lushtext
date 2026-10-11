## ADDED Requirements

### Requirement: Giant paragraphs stay view-only until a recorded decision adopts an editing approach
The system SHALL keep every paragraph above the Protected View threshold
non-editable, as Protected View defines it, until the evaluation record
`docs/next/long-paragraph-editing.md` contains a maintainer-recorded Decision
that adopts a specific editing approach. An adopted approach SHALL be
delivered by a separate follow-up change; the evaluation change itself SHALL
NOT change the source or dependencies of the LushText application crates,
change production behaviour, or add an action, GSettings key, D-Bus member,
automation snapshot field, or persisted format. Its only maintained artefacts
outside documentation SHALL be GTK axiom ledger entries with their
`gtk-lush-axioms` probes and samples, and the root workspace exclusion,
ignore entry, and local-only make target of the disposable spike.

#### Scenario: Editing is proposed without a decision
- **WHEN** a change proposes making a paragraph above the Protected View threshold editable
- **AND** the evaluation record has no Decision section adopting that approach
- **THEN** the change is not accepted until the evaluation and its Decision exist

#### Scenario: The evaluation completes
- **WHEN** the evaluation change is applied
- **THEN** Protected View documents still open non-editable and caret-free
- **AND** no LushText application crate source, dependency, GSettings schema, or persisted format differs because of the evaluation

### Requirement: The evaluation compares every candidate against one rubric
The evaluation SHALL assess at least in-buffer segmentation with an offset
map, a custom virtualized text widget, and windowed edit-through via the Line
Inspector, each against the same rubric: the recorded budgets, byte
integrity, per-consumer obligations, feature parity lost, production cost
(size, dependencies, workflow-convention placement, proof coverage, GTK
axioms needed, upgrade maintenance), and per-document reversibility. A
candidate rejected on paper SHALL have the reason recorded.

#### Scenario: A candidate is rejected before the spike
- **WHEN** the paper analysis shows a candidate cannot meet a rubric criterion
- **THEN** the record states which criterion, with the source or measurement that shows it
- **AND** the candidate is not spiked

#### Scenario: The record omits a candidate
- **WHEN** the record does not assess one of the three named candidates
- **THEN** the evaluation is incomplete and no Decision may be recorded

### Requirement: Every consumer of document coordinates is inventoried per candidate
The evaluation SHALL list every consumer that stores, exchanges, or reads a
document coordinate or the document text — at least save and Save As, drafts,
local history capture, preview and restore, bookmarks, session restore,
workspace search hits, in-tab find, go-to-line and restore position,
clipboard and drag-and-drop, undo and redo, line-ending and encoding
analysis, EditorConfig formatting on save, file-monitor and eviction reload,
the line-number gutter, status-bar position and document statistics, the
live editor memory estimate, print, minimap, invisibles, Markdown preview,
accessibility, input methods, and the automation snapshot — and SHALL state
for each candidate what that consumer must do. Persisted coordinates SHALL
remain file coordinates in every candidate; a candidate that cannot keep them
so SHALL be rejected on paper with that reason.

#### Scenario: A candidate introduces an offset map
- **WHEN** a candidate maps buffer coordinates to file coordinates
- **THEN** the record names, for each inventoried consumer, whether it maps through the pure map, needs no change, or changes behaviour
- **AND** bookmarks, session tabs, and search hits keep their existing persisted line and column semantics

#### Scenario: A candidate needs a persisted format change
- **WHEN** a candidate can only work if bookmarks, session tabs, drafts, local history, or search history store buffer coordinates instead of file coordinates
- **THEN** the record rejects the candidate on paper and names the format it would have changed

### Requirement: Surviving candidates are measured against the recorded budgets
Each candidate that survives the paper analysis SHALL be spiked and measured
on the reference lane (private headless Mutter, `GSK_RENDERER=cairo`, at most
16 cores, never the live desktop) with the in-process lateness watchdog and
frame-clock timing introduced by C0, after a calibration run that reproduces
one case C0 already recorded within C0's recorded run-to-run spread. The
measurement SHALL use the budget constants C0 records (working values: 8 ms
per interaction dispatch, at most 2 frames from input to presented frame,
16 ms per settle dispatch, and the scroll frame-gap limit), SHALL assign each
scripted step to the interaction or settle class before measuring, and SHALL
cover a fixed interaction script — open, scroll, caret or window placement,
character and word motion, typing, Backspace, large selection, copy, a 1 MB
paste, undo and redo, save, a draft autosave tick, a local-history capture,
session restore inside the paragraph, and an external reload — over synthetic
fixtures of at least 50k, 250k, 1.9M and 10M characters in one paragraph,
including a mixed-script variant and CR-only and U+2029-separated variants.
Every scripted run SHALL be repeated with the accessibility bridge and a
listener active and with an input method that requests surrounding text.
Every step SHALL also be measured in the stock editor on a short-line control
fixture of the same total size; a step that misses its budget there SHALL be
recorded as a pre-existing blocker with an owning change in the long-line
programme record, not excused.

#### Scenario: A candidate misses a budget
- **WHEN** any scripted step exceeds its budget class on any fixture or configuration
- **THEN** the record marks the candidate as failing the budget criterion and names the step, fixture, configuration, and measured value

#### Scenario: The harness disagrees with C0
- **WHEN** the calibration run falls outside C0's recorded spread for the same case
- **THEN** no candidate measurement is recorded until the harness is fixed

#### Scenario: Storage noise during a run
- **WHEN** the stall report for a measured run does not conclude that storage was not a bottleneck
- **THEN** the run is repeated before its numbers are recorded

#### Scenario: The user's file is not used
- **WHEN** the spike needs a giant-paragraph document
- **THEN** it uses synthetic fixtures from the shared generator
- **AND** no copy of, or path to, a user document is committed or referenced in the repository

### Requirement: Byte integrity is demonstrated with seeded defects
For each spiked candidate the evaluation SHALL demonstrate: a no-edit round
trip whose saved bytes equal the fixture bytes; a single-edit save equal to
the fixture with exactly the edited file range replaced, checked against an
independent oracle; and draft and local-history bodies equal to what save
would write. It SHALL seed defects specific to the candidate — at least a
lost synthetic-break marker (including one re-inserted untagged by undo) and
an off-by-one offset map for segmentation, a
window cut inside a grapheme for windowed approaches, and a commit against a
stale paragraph generation for edit-through — and SHALL record whether the
checks catch each one. The pure core of a candidate whose mapping or splice
planning is integer arithmetic SHALL be accompanied by a Kani harness plan
and a property test.

#### Scenario: A seeded defect escapes
- **WHEN** a seeded defect is not caught by the candidate's integrity checks
- **THEN** the candidate fails the integrity criterion regardless of its timings

#### Scenario: A candidate passes integrity
- **WHEN** all integrity checks pass and every seeded defect is caught
- **THEN** the record lists the checks, the defects, and the proof plan for the production pure core

### Requirement: The spike is disposable and isolated from production gates
The spike SHALL live in a standalone package outside every production crate,
with its own workspace and lockfile, excluded from the root workspace, building
into a gitignored directory, and invoked only by a local-only make target. It
SHALL NOT be part of CI, `make check`, policy audits, widget shards, mutation
or Kani lanes, the SonarQube scope, or the production dependency graph, and no
production crate SHALL depend on it. Toolkit behaviour the Decision relies on
SHALL instead be recorded as a GTK axiom ledger entry with a `gtk-lush-axioms`
probe and sample, unless an existing entry already pins it.

#### Scenario: Production builds ignore the spike
- **WHEN** the workspace is built, linted, or tested through any maintained gate
- **THEN** no spike code is compiled or executed

#### Scenario: The Decision relies on a toolkit fact
- **WHEN** the Decision's reasoning depends on a GTK, GtkSourceView, or Pango behaviour
- **THEN** that behaviour is a ledger axiom with a probe and a sample, either pre-existing or added by this change

### Requirement: The evaluation ends with a recorded maintainer decision
The evaluation record SHALL end with a Decision section, recorded by the
maintainer, that chooses one of: adopt a named candidate through a named
follow-up change, keep giant paragraphs view-only, or defer. The Decision SHALL
state the evidence it rests on, the open questions it resolved, the fate of the
spike package, and the triggers that would re-open the question. The long-line
programme record SHALL be updated with C5's outcome in the same change.

#### Scenario: Adoption is chosen
- **WHEN** the Decision adopts a candidate
- **THEN** it names the follow-up change, the production placement under the workflow role convention, and the capabilities that change will modify

#### Scenario: View-only is kept or the question is deferred
- **WHEN** the Decision keeps view-only or defers
- **THEN** it records the re-open triggers
- **AND** the programme record states that C5 is closed or deferred, not pending

### Requirement: Adopting segmentation requires Kani harnesses over the segment map and splice planner
If the Decision adopts in-buffer segmentation, the follow-up implementation
change SHALL ship Kani harnesses over the production pure core that maps
buffer offsets to file byte offsets (UTF-8 byte offsets of the decoded text)
and plans segment splices, calling that core rather than a copy, proving at
least: the round trip file offset → buffer offset → file offset is the
identity on every valid file offset; the map is monotone; under bounded edit
sequences that cross segment boundaries, the file reconstructed from the
segmented buffer equals applying the same edits to the flat text; and the
splice planner never produces a segment longer than `N_segment`. Each harness
SHALL state its domain, and the core's rustdoc SHALL state the same domain:
any `u32` offset, at most four breaks, and break widths of 1 and 3 bytes for
the round trip and monotonicity, with the constructor rejecting any map whose
buffer length would overflow `u32` and a harness checking that rejection;
texts of at most 8 bytes, at most three edits given in buffer coordinates,
break widths of 1 and 3 bytes, and byte caps of 1 to 3 for byte preservation,
plus one further insertion of up to three times the cap for the segment cap.
The buffer-to-file-to-buffer direction SHALL be stated as non-injective
across a synthetic break rather than claimed as an identity. Grapheme-boundary
placement of breaks, never cutting inside a UTF-8 sequence, and the
line-number mapping SHALL be covered by property tests, not claimed by Kani. The follow-up SHALL also
keep `should_panic` harnesses showing that removing re-segmentation breaks
the cap, that a synthetic break losing its marker breaks byte preservation,
and that an inclusive break bound breaks the round trip. The evaluation
record SHALL state these obligations before a Decision adopting segmentation
is recorded. If segmentation is spiked and its core fits the spike's time
box, the spike SHALL prototype the same harnesses locally, outside every Kani
lane, and record their verdicts and any counterexample.

#### Scenario: Segmentation is adopted
- **WHEN** the Decision adopts in-buffer segmentation through a named follow-up change
- **THEN** that change's tasks include the round-trip, monotonicity, byte-preservation and segment-cap harnesses and the three `should_panic` regressions
- **AND** each harness is in exactly one Kani shard with a CI-runner budget and in the proof-strength oracle table, and a proof-strength run of the module is recorded in the formal-verification programme record

#### Scenario: An edit crosses a segment boundary
- **WHEN** the byte-preservation harness applies a deletion spanning two or more synthetic breaks, or an insertion that grows a segment past `N_segment`
- **THEN** a `kani::cover!` shows the case is reached
- **AND** the reconstructed file equals the flat-text oracle

#### Scenario: The spike prototype catches the seeded off-by-one
- **WHEN** the spike's prototype map carries the seeded inclusive-bound defect
- **THEN** the prototype round-trip harness fails before the defect is reverted
- **AND** the evaluation record states the verdict and that the run was local and outside the Kani shards

#### Scenario: Edit-through is adopted instead
- **WHEN** the Decision adopts windowed edit-through via the Line Inspector
- **THEN** the record names the splice-planner harnesses — the splice range and window cap, byte preservation against the flat-text oracle, and rejection of a stale paragraph generation — with their domains, and a `should_panic` regression showing that a stale commit is applied without the generation check

#### Scenario: The custom widget is adopted instead
- **WHEN** the Decision adopts the custom virtualized widget
- **THEN** the record names the pure cores that receive equivalent harnesses — the text model's offset translation and byte preservation, the undo journal, the viewport and input-method window planner, and the caret and selection clamp — with their domains
