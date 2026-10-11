## ADDED Requirements

### Requirement: Text, Pango and GtkSourceView probes keep structural verdicts
The `gtk-lush-axioms` crate SHALL be able to build GtkTextView, GtkTextBuffer, Pango and GtkSourceView fixtures. It MAY depend on the `pango` and `sourceview5` workspace crates, and still SHALL NOT depend on any GTK Lush or LushText crate.

A probe's verdict SHALL be decided only by structural observations: positions, geometry, tag membership, signal and call counts, captured log messages, and buffer state. Durations SHALL be recorded as observation values and SHALL NOT decide a verdict.

A probe that needs an input method SHALL register its own probe `GtkIMContext` on the `gtk-im-module` extension point inside the probe's process, and SHALL select it only for its own view, through the view's `im-module` property, leaving the runner's `GTK_IM_MODULE` environment and the display's `gtk-im-module` setting unchanged. A probe that captures toolkit warnings SHALL restore the previous log handling before it returns.

A structural verdict whose fixture only reaches its state after a toolkit time limit, such as the context engine's 2000 ms cut-off, SHALL grow the fixture until the state is reached, up to a fixed cap, and SHALL report `FixtureInvalid` rather than `Violated` when the cap is reached first, so that runner speed cannot flip a verdict.

#### Scenario: A layout-cost axiom is probed
- **WHEN** the A23 probe lays out paragraphs of increasing visible length
- **THEN** its verdict comes from the paragraph's geometry, and the per-character layout times appear only as recorded values

#### Scenario: A defect axiom captures a CRITICAL
- **WHEN** the A32 probe detaches a source map with an update tick pending
- **THEN** the probe captures the critical message as an observed value, the verdict holds while the critical is emitted, and the previous log handling is restored

#### Scenario: The input-method axiom is probed
- **WHEN** the A30 probe moves the caret, types, clicks and scrolls in a text view inside the probe's process
- **THEN** its probe input method counts surrounding-text requests and their lengths, and the runner's `GTK_IM_MODULE` and the display's `gtk-im-module` setting are unchanged afterwards

#### Scenario: A time-limited fixture runs on a fast runner
- **WHEN** the A29 probe's first line finishes analysis before the 2000 ms cut-off
- **THEN** the probe doubles the line and retries, and reports `FixtureInvalid` only if its cap is reached without the cut-off firing

### Requirement: Long-line probes stay inside the pull-request probe budget
The added long-line probes SHALL use the smallest fixture sizes that exhibit their structure. The change that adds them SHALL record the added runtime of `make gtk-axioms`. A probe whose fixture must run for seconds, such as the context-engine cut-off, SHALL be marked slow in its catalogue entry if the pull-request `GTK Axioms` job would otherwise approach its `timeout-minutes`; the timeout SHALL NOT be raised instead. The runner SHALL skip slow probes unless an explicit include switch is set, and SHALL report each skipped probe by id. A slow probe SHALL still run in `make gtk-axioms` locally and in a nightly CI job. `make check-gtk-axioms` SHALL reject a slow entry whose ledger row does not say where it runs.

#### Scenario: The probe job stays under its cap
- **WHEN** CI runs `make gtk-axioms` on a pull request after A23–A36 are added
- **THEN** the job finishes within its existing `timeout-minutes` (20 at the time of writing), and its recorded duration is in the change's evidence

#### Scenario: A slow probe is skipped on pull requests
- **WHEN** a probe is marked slow
- **THEN** the pull-request job reports it as skipped by design, and the nightly lane runs it and records its observation
