# responsiveness-measurement Specification

## Purpose
TBD - created by archiving change probe-long-line-toolkit-costs. Update Purpose after archive.
## Requirements
### Requirement: Responsiveness budgets are named integer policy constants
The project SHALL define its responsiveness budgets once, as integer-microsecond constants in a GTK-free model module. The module SHALL define:
- the interaction-dispatch budget B_interact (working value 8 ms);
- the settle-dispatch budget B_settle (working value 16 ms);
- a fixed reference frame of 8,333 µs (120 Hz), used by every frame-relative limit regardless of the virtual monitor's refresh interval;
- the input-to-presented-frame limit (2 reference frames, 16,666 µs);
- the scroll p95 frame-gap limit (one reference frame, 8,333 µs).

Budget verdicts SHALL come from the reference machine (the maintainer's desktop, headless, at most 16 cores); runs on CI runners SHALL be reported as trend evidence and SHALL NOT fail on a budget.

The budgets SHALL be read against main-loop stalls: the longest run of dispatches the main loop performs without returning to poll, which is what delays input and is never shorter than any single dispatch in it.

The same module SHALL hold the dispatch classification, histogram bucketing and nearest-rank percentile arithmetic, with no floating-point arithmetic. A Kani harness SHALL prove, for every `u64` input and for sample arrays of up to four elements, that:
- the classification is sound against the watchdog's lower and upper stall bounds;
- the arithmetic saturates;
- bucket indices are monotone and in range;
- a percentile lies between the sample minimum and maximum, and is monotone in the rank.

The harness's domain SHALL be stated in the harness and in the functions' rustdoc.

A change to a budget value SHALL cite the measurement run that justifies it.

#### Scenario: A stall is classified conservatively
- **WHEN** the watchdog reports lateness `L` with tick `T` and measured idle wake-up slack `ε`, and the budget is `B`
- **THEN** the classification is `Within` only when `L + T ≤ B`, `Over` only when `L − ε > B`, and `Indeterminate` otherwise

#### Scenario: A later change needs a budget
- **WHEN** a long-line policy or test needs B_interact or B_settle
- **THEN** it reads the constant from this module and does not restate the number

#### Scenario: The measurement revises a budget
- **WHEN** the reference run shows that the baseline cases cannot fit a working budget value
- **THEN** the constant is changed in the same change that records the run, and the programme record states the old value, the new value and the run id

### Requirement: In-process responsiveness probes are compiled only into a measurement build
LushText SHALL provide a main-loop lateness watchdog, frame-clock timing, input-to-presented-frame stamps and a probe input-method context only under a smoke-only cargo feature. No default, release, Meson, Flatpak or Snap build enables that feature. Without the feature, the probe entry point SHALL compile to an empty function, and no environment variable SHALL be read.

The watchdog, the evidence buffers and the record encoder SHALL also compile under the unit-test configuration, so that the default unit-test lane tests them without the feature. Only the installation into the running application, its toolkit hooks and the probe input-method registration SHALL be feature-only.

With the feature, the probe SHALL activate only when its log-path environment variable is set. It SHALL record evidence in bounded memory and write it off the GTK thread through the filesystem boundary. When the writer cannot keep up, it SHALL drop records and count them, never blocking. It SHALL never record document, note, draft or search text.

#### Scenario: A shipped build carries no probe
- **WHEN** LushText is built without the measurement feature
- **THEN** no watchdog source, frame-clock handler, event controller, input-method registration or environment read from the probe exists in the binary

#### Scenario: The watchdog bounds the longest stall
- **WHEN** the measurement build runs with the log path set and a tick of `T` microseconds at `G_PRIORITY_HIGH`, with microsecond ready times
- **THEN** each record carries the observed lateness `L`, and the report states each worst stall as at least `L − ε` and at most `L + T`, where `ε` is the idle wake-up slack the calibration case measured

#### Scenario: The watchdog is tested without a display
- **WHEN** the default unit-test lane runs the watchdog on a private main context with a source that blocks for a known duration `S`
- **THEN** the recorded lateness lies between `S − T` and `S + ε`

#### Scenario: Frame and input timing are recorded
- **WHEN** a key, scroll or button event reaches a LushText window in the measurement build
- **THEN** the evidence records the handler time and the event's compositor timestamp, latency is measured from the driver's injection time when known and otherwise from the compositor timestamp, never from the handler time alone, and the evidence records the first presented frame that started after the handler, the number of frames between them, the refresh interval, and whether presentation or frame-start times were used

#### Scenario: The probe input method requests surrounding text
- **WHEN** the measurement build runs with the probe input method selected
- **THEN** the input method emits `retrieve-surrounding` on focus-in, cursor-location updates, key filtering and reset, and records the call count and the surrounding-text length, never the text

#### Scenario: The evidence writer is saturated
- **WHEN** records arrive faster than the writer thread appends them
- **THEN** excess records are dropped and counted in the next written record, and the GTK thread never waits on the writer

### Requirement: Long-line fixtures are synthetic and deterministic
The project SHALL generate every long-line measurement fixture from a kind, a size and a seed, so that the same inputs produce byte-identical output. Fixtures SHALL be written only into the run's temporary scope. The fixture kinds SHALL cover:
- single giant paragraphs;
- dense sets of lines just under a threshold;
- short-line HTML at a given total size;
- CR-only, CRLF and U+2029 delimiter variants;
- a minified-markup kind that mimics long single-line HTML and script;
- multi-document session and eviction sets.

No user file, and no excerpt of one, SHALL be committed or copied into a fixture.

#### Scenario: A fixture is regenerated
- **WHEN** the generator runs twice with the same kind, size and seed
- **THEN** both outputs have the same SHA-256, and the artifacts record the seed and digest instead of the bytes

#### Scenario: A run leaves no fixtures behind
- **WHEN** a responsiveness run finishes, passes or fails
- **THEN** its generated fixtures are gone and the temporary-scope leak check passes

### Requirement: A responsiveness lane measures the real binary headlessly
The project SHALL provide `make responsiveness-smoke`. It SHALL run the measurement build of the real LushText binary inside a private `dbus-run-session` and `mutter --headless` session, never the developer's live desktop, using at most 16 CPUs. It SHALL drive the binary through the existing automation client and actions, and through the compositor's remote-desktop input for scroll, key and click input.

It SHALL run the long-line sweep:
- paragraph lengths 2k, 10k, 50k, 250k and 1.9M characters, with syntax on and off, wrap on and off, and minimap on and off, refined by bounded bisection;
- dense near-threshold sets, wrap off and on;
- short-line HTML at 1, 2, 5 and 10 MB;
- CR-only, CRLF and U+2029 variants;
- session restore of three or more giant-paragraph documents, with tab switching;
- eviction and reselection;
- an external reload;
- invisibles over a long paragraph;
- baseline empty and small documents.

Each case SHALL run plain, with the AT bridge and an event listener, and with the probe input method. Real IBus SHALL run as a fourth variant when it is present. Every variant SHALL select its input method explicitly and SHALL NOT leave GTK's Wayland text-input method selected under the headless compositor.

The lane SHALL run nightly and on demand. It SHALL NOT be a pull-request gate, and each CI job SHALL stay within 30 minutes.

#### Scenario: An interaction hangs
- **WHEN** an interaction exceeds its per-interaction wall cap
- **THEN** the case records `exceeded_cap` with its partial evidence, the app is killed and relaunched, and the sweep continues

#### Scenario: A host lacks a variant's support
- **WHEN** the AT-SPI bus, remote-desktop input, IBus or a requested renderer is unavailable
- **THEN** the affected cases record `unavailable` or `skipped` with the reason, and the other cases still run

#### Scenario: The lane respects the core cap
- **WHEN** the inherited CPU affinity mask allows more than 16 CPUs
- **THEN** the session and app run under an affinity of at most 16 CPUs taken from that mask, never widened beyond it, and the environment report records it

#### Scenario: A known toolkit defect is reproduced
- **WHEN** the run reproduces a runtime warning already owned by a later change in the programme
- **THEN** the report lists it as a finding with its owning change, and no allowlist entry hides it

#### Scenario: A fixed pre-existing blocker reappears
- **WHEN** the run reproduces the runtime warning of a pre-existing blocker that a change in the programme has fixed, such as the scrollbar `GtkGizmo` snapshot warning (ledger A37)
- **THEN** the report lists it as a regression and the lane fails

### Requirement: The lane never changes a fixture's bytes
The lane SHALL record the SHA-256 of every fixture before the app opens it, and after each case. The lane SHALL fail when a fixture's digest differs from the last bytes the driver itself wrote.

The driver SHALL never send a save shortcut or activate a save action. An app holding typed edits SHALL be ended by killing it, never by a graceful quit that could raise a save prompt. Each case SHALL start from fresh isolated application state, so drafts and sessions never carry from one case into another.

#### Scenario: Typing never reaches the file
- **WHEN** a case types into a giant-paragraph document and the app is closed or killed
- **THEN** the fixture's digest is unchanged

#### Scenario: An external rewrite is expected
- **WHEN** the reload case rewrites a fixture to trigger the file monitor
- **THEN** the lane compares against the digest of the bytes the driver wrote, and the case passes only if the app did not write the file

### Requirement: The lane records storage-stall evidence
After the last case, and at least 15 seconds after it ended, the lane SHALL query `btrfs-stall-report` in JSON form for the run window, and SHALL keep the output in the artifacts. A case whose window the report attributes to a storage stall SHALL be marked storage-contaminated and excluded from derived values. When the tool or its log directory is missing, the lane SHALL record that it is unavailable, and SHALL NOT install the tool.

#### Scenario: Storage was a bottleneck
- **WHEN** the storage report's verdict attributes a stall to a case's window
- **THEN** that case is marked storage-contaminated, and no derived value uses it

#### Scenario: The storage tool is absent
- **WHEN** `btrfs-stall-report` is not installed, for example on a CI runner
- **THEN** the artifacts record `unavailable`, and the run is otherwise unaffected

### Requirement: The lane derives the long-line thresholds and a budget verdict
The lane SHALL write a machine-readable report and a human-readable report. Together they SHALL give candidate values for:
- L_syntax, L_minimap, L_invisibles, L_search_highlight, L_protect and N_visible;
- V_batch, R_batch and N_dense;
- the syntax total-size cutoff S_syntax;
- the upper bound on the Line Inspector window, min(N_visible, L_protect).

Each value SHALL carry the name the document-scale and protected-view changes declare for it.

They SHALL also give a confirm-or-revise verdict for B_interact, B_settle and the input-to-frame limit.

Each derived value SHALL name the rule that produced it and the cases, variants and repetitions that decided it, and the worst variant decides. A value whose deciding cases are unavailable, noisy or storage-contaminated SHALL be reported as not derived, with the reason. Every report SHALL record the run id, the tier, the toolkit versions, the kernel, the CPU model, the CPU affinity, the renderer and the fixture seeds.

#### Scenario: L_protect needs input injection
- **WHEN** remote-desktop input was unavailable for the caret, type and click interactions
- **THEN** L_protect is reported as not derived, and the reason names the missing input path

#### Scenario: Budgets are confirmed
- **WHEN** the baseline cases, excluding window construction, fit B_interact, B_settle and the input-to-frame limit under the watchdog's upper bound on the reference run
- **THEN** the report marks the budgets as confirmed and cites the run

#### Scenario: A noisy case repeats
- **WHEN** a case's maximum exceeds twice its median across repetitions
- **THEN** the case is re-run once, and it is marked noisy if the spread persists

### Requirement: Measured values are recorded in the programme record before the next change
The long-line programme record `docs/next/long-line-documents.md` SHALL record:
- the C0–C5 sequence, with each gate;
- the maintainer decisions, dated;
- the reference run's derived values, with their run id and derivation;
- the budget verdict;
- the axiom ids each later change cites;
- each pre-existing blocker the run reproduced, with its owning change.

The document-scale, protected-view, sliced-install and window-construction changes SHALL NOT be implemented until the record holds derived values for the thresholds and budgets they use. They may be drafted before then, with their constants left as named placeholders.

#### Scenario: C1 is implemented
- **WHEN** implementation of the document-scale change begins
- **THEN** its S_syntax, L_syntax, L_minimap, L_invisibles, L_search_highlight, L_protect, R_batch and V_batch values cite the programme record's reference run instead of an estimate

### Requirement: The lane attributes post-open settle by stage
The measurement build SHALL record one `post_open_*` span per post-open stage:
- the activation turn, editor construction, language catalog and language attach;
- install direct or slice, publish, highlight attach, and the first editor frame.

It SHALL also record a mark for each GtkSourceBuffer `highlight-updated` and for each change of the editor's vertical adjustment `upper`, and a record each time a low-priority settle sentinel idle dispatches. Without the `responsiveness-probes` feature, each of these SHALL be an empty inline function.

The lane SHALL report a post-open settle table. For every case and repetition, it lists every dispatch from the activation turn until `document-settled`, at least 1 ms long, with the stage that contains it. A dispatch that no span covers SHALL be attributed to highlighting or validation when it ends at the matching mark, and otherwise reported as unattributed. The table SHALL be read by span, not by the `open`/`background` phase boundary, because `file-open-complete` can hold before the activation's opens run.

The background phase SHALL last until the document is settled, capped. The driver SHALL compute "settled" from the probe log's marks and sentinel records with the same clauses as the `document-settled` predicate, so that a build without the predicate is measured identically. A cap hit SHALL be a failed phase. A `type-during-settle` phase SHALL inject one key every 50 ms from load readiness, for a named number of keys large enough to span the largest case's install and language attach. It SHALL then stop typing and wait for settle, and report each key's dispatch latency from its compositor event time to its handler. On the dense cases, `wrap-toggle`, `zoom-step` and `font-change` phases SHALL each run until `document-settled` and record their interaction dispatch and every later dispatch.

#### Scenario: A post-open stall is attributed
- **WHEN** a post-open dispatch exceeds `B_SETTLE_US`
- **THEN** the report names the post-open stage, or the highlight or validation mark, that contains it, or states that it is unattributed

#### Scenario: Settle outlasts a fixed sleep
- **WHEN** background highlighting continues for longer than two seconds after open
- **THEN** the background phase continues until `document-settled` or its cap, and every dispatch until then is in the table

#### Scenario: A shipped build carries no post-open span
- **WHEN** LushText is built without the `responsiveness-probes` feature
- **THEN** no post-open span, mark, environment variable or frame-phase extension is compiled in

### Requirement: The post-open settle verdict is measured interleaved
The C6 verdict SHALL run under the measurement lock protocol and the reference precondition. It SHALL interleave a baseline build and the branch build, both measured by the branch's lane driver: baseline, branch, baseline, branch, 7 repetitions per side. The baseline SHALL be `main` plus only the commit that adds the post-open spans, marks and sentinel, with no behaviour change.

`baseline/small` SHALL run in the four construction profiles. The cases SHALL include:
- `baseline/empty`;
- `total-size` 16 kB, 1 MB and 5 MB as `.html` and `.txt`;
- `dense` 120, 500 and 710 with wrap on and off;
- a Markdown document with side-by-side preview;
- an EditorConfig tab width that differs from the setting;
- a three-document session restore, selecting each restored tab in turn and waiting for it to settle;
- a two-draft recovery.

The verdict is met when, on the branch, in every profile and repetition:
- every post-open dispatch until settled is at most `B_SETTLE_US`;
- every `type-during-settle` key is dispatched within `B_INTERACT_US`;
- no post-open stage regresses: per case, the median over the repetitions of each attributed stage's worst dispatch, and the median time from activation to fully laid out, are each at most the baseline's plus one reference frame. Fully laid out is the end of the last layout dispatch (GtkTextView's validation, or an install slice) or change of the view's vertical extent; the only time it excludes is the deferred language attach's wait for input quiet, at most `ATTACH_DEADLINE_US`, and the layout the attach's highlighting causes still counts. The time from activation to settled is reported against the baseline's and is not gated, because the attach waits for input quiet by design (maintainer decision, 2026-11-01).

A dispatch recorded as a toolkit-floor exception against a ledger axiom SHALL be reported on its own line, with the axiom id and upstream report, and SHALL be exempt from the `B_SETTLE_US` condition. The input events whose dispatch overlaps it SHALL be listed on that line with their latencies, and SHALL be exempt from the `B_INTERACT_US` condition. No other dispatch or event is exempt.

Input-to-frame SHALL be reported and not gated. The report and the programme record SHALL document the headless lane's floor: double-buffered 120 Hz presentation (16.7–19.3 ms) plus a full-screen paint. Each case's input-to-frame SHALL be printed next to that floor. The verdict, run ids, load, storage evidence and toolkit versions SHALL be recorded in `docs/next/long-line-documents.md`.

#### Scenario: The verdict is met
- **WHEN** every branch case meets the three conditions in every profile and repetition
- **THEN** the programme record marks C6's gate as met and cites the interleaved run ids

#### Scenario: A toolkit-floor exception remains
- **WHEN** a dispatch over `B_SETTLE_US` remains and its probe shows that LushText cannot split or move it
- **THEN** the record lists it as a toolkit-floor exception with its stage, size, axiom id and drafted upstream report, together with any input event that waited behind it, and every other dispatch and event is still gated

#### Scenario: Input-to-frame sits on the lane floor
- **WHEN** a case's input-to-frame exceeds two reference frames
- **THEN** the report shows it beside the documented floor, and the verdict does not fail on it

### Requirement: The lane proves its inputs reached the document
The measurement build's probe SHALL stamp each key and button input record with where it was headed: the window's focus widget for a key, and the widget under the pointer (with its window coordinates) for a button, and SHALL record each pointer motion with its window coordinates. The target is classed as an editable editor, a read-only editor, another widget, or none. The probe SHALL record a mark after each insertion an editor buffer makes inside a user action. Without the `responsiveness-probes` feature none of this SHALL be compiled in.

The lane SHALL aim every click meant for the editor at a point of the editor text view's rectangle, from the automation snapshot, that no sidebar or bottom sheet covers. It SHALL correct the pointer by where the probe saw it arrive. It SHALL fail the click's phase unless the probe's record of the press names an editor. For each phase that types text keys, it SHALL report how many keys were typed, how many left no input record, where they arrived and how many insertions they made. A phase with a text key that left no input record, whose text keys arrived outside an editor, or that inserted fewer characters than the keys it typed into an editable editor, SHALL fail the run and SHALL be listed in the report. A build whose probe records no targets SHALL be reported as unverified, not failed, for where its recorded keys arrived; its keys without any input record SHALL still fail the phase.

A text key that reached an editable editor SHALL NOT be classed as changing nothing visible. When the first frame after it starts only after the next key was handled while the main loop was free, or after the phase ended, its latency SHALL be measured to that frame, the phase SHALL be listed as a withheld typing frame, and the run SHALL fail. Keys into a read-only (protected) editor SHALL be reported apart from editable typing and SHALL NOT enter its input-to-frame figures. After a relaunch, a driver click that cannot be proven to reach the editor SHALL end the case with a driver failure.

The lane SHALL seed every settings key the case's layout depends on, including `split-view-layout-migrated`, so that the layout it records is the layout rendered. The fresh construction profiles that show no pane SHALL instead seed the legacy `sidebar-visible` as hidden, unmigrated, so that their launch runs the one-shot split-view migration and renders the same layout.

#### Scenario: A focus click lands outside the editor
- **WHEN** a driver click meant for the editor arrives on another widget
- **THEN** its phase fails with the widget class and the window position it arrived at

#### Scenario: Typing reaches no document
- **WHEN** a typing phase's keys arrive at a widget that is not an editor, or insert nothing in an editable editor
- **THEN** the report lists the phase under `misdirected_typing` and the run fails

#### Scenario: A baseline without targets stays measurable
- **WHEN** the measured binary's probe records no input targets
- **THEN** the delivery is reported as unverified and the run is not failed for it

#### Scenario: Keys that never reached the application
- **WHEN** a typing phase's text keys leave no input record at all
- **THEN** the phase is listed under `misdirected_typing` and the run fails

#### Scenario: Typed text drawn in a withheld frame
- **WHEN** a key typed into an editable editor is drawn only in a frame that starts after the next key was handled while the main loop was free
- **THEN** its latency runs to that frame, the phase is listed under `withheld_typing_frames`, and the run fails

### Requirement: The lane measures a save during a validation backlog
The lane SHALL have a case, `save/during-validation`, that runs in both tiers.
It opens a small scratch document that the case writes into its own state
directory, never a fixture. It then opens two 8 MiB short-line fixtures through
a second activation of the running instance, selects the scratch tab, types one
character and activates `win.save` (the action Ctrl+S activates; the lane never
injects a Control+letter chord) while those fixtures are still validating. The
case SHALL record the time from that activation until the scratch file's
bytes change on disk (`save_latency_us`), the phase's watchdog stalls and the
typed key's input-to-frame. The report SHALL list these per run. The case SHALL
verify the fixture digests like every other case, so the lane still never
changes a fixture's bytes.

#### Scenario: The save completes during the backlog
- **WHEN** the case runs against a build whose admission hand-offs and
  completions are held behind validation
- **THEN** `save_latency_us` approaches the backlog's length, seconds
- **AND** against this change it is a few frames, and no dispatch in the phase
  exceeds `B_SETTLE_US` that would not on `main`

#### Scenario: A fixture is never saved
- **WHEN** the case saves
- **THEN** only the scratch file in the case's state directory is written
- **AND** the fixture ledger's digests are unchanged after the case

