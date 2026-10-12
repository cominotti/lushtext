# gtk-lush-axioms Specification

## Purpose
Keep every pinnable GTK axiom observable in isolation: a governed GTK Lush leaf crate with a catalogue, typed observations, one interactive and headless sample per axiom, a headless probe runner, CI, and a toolkit-update procedure that runs the probes first.
## Requirements
### Requirement: Axioms are catalogued in a governed GTK Lush family crate
The workspace SHALL contain a GTK Lush family crate `gtk-lush-axioms` at
`crates/gtk-lush/axioms/`, versioned `0.0.0` and governed by the GTK Lush
family policy. It SHALL expose a catalogue of every ledger axiom, with its
stable id, its statement and its dependent designs. Every pinnable entry SHALL
carry a probe function. A probe SHALL return an `Observation` that records a
verdict (the axiom held, was violated, or the fixture never reached the state
the axiom describes), each value the probe measured, and the running GTK and
Libadwaita versions. The crate MUST NOT depend, in any dependency section, on
another GTK Lush crate or on a LushText crate.

#### Scenario: A stock gtk-rs application runs a probe
- **WHEN** an application with GTK and Libadwaita initialized calls a probe function
- **THEN** the probe builds its own fixture, drives the main loop to a bounded end, closes the fixture, and returns an `Observation` with its verdict, measured values and toolkit versions

#### Scenario: A probe's control step fails
- **WHEN** a probe's fixture does not reach the state its axiom talks about
- **THEN** the verdict is fixture-invalid, not violated

#### Scenario: The family policy accepts the crate
- **WHEN** `make check-gtk-lush-policy` and `make gtk-lush-adoption-matrix` run
- **THEN** `gtk-lush-axioms` passes the same checks as every other family crate

### Requirement: Every pinnable axiom has one sample with an interactive and a headless mode
Each pinnable axiom SHALL have one runnable example in the crate that uses the
same fixture builder as that axiom's probe. By default it SHALL open a window
that shows the fixture, the axiom's statement, controls that trigger the
behaviour, and the measured values. With `--check` it SHALL run the probe,
print the `Observation` as one JSON line, and exit 0 when the axiom holds, 1
when it is violated and 2 when the fixture is invalid. `--check` SHALL refuse
to run outside a private headless session.

#### Scenario: A person watches an axiom
- **WHEN** a developer runs `make gtk-axiom-sample AXIOM=A9` on a desktop session
- **THEN** a window shows the list, a control that nudges its adjustment before `scroll_to`, and the resulting position

#### Scenario: A machine checks an axiom
- **WHEN** a sample runs with `--check` under `make gtk-axiom-sample AXIOM=<id> CHECK=1`
- **THEN** it prints one JSON observation and exits with the code of its verdict

#### Scenario: A check is started on a live desktop
- **WHEN** a sample runs with `--check` outside the private headless session
- **THEN** it opens no window and exits 77

### Requirement: Probes run headless as one test binary and in CI
A probe test binary built on `gtk-lush-proof-harness`, outside the family
(in `gtk-lush-adoption-lab`), SHALL run every catalogued probe as a separately
isolated test, print each observation, and fail unless the verdict is holds.
`make gtk-axioms` SHALL run that binary and every sample's `--check` headless,
never against the developer's live desktop session. CI SHALL run `make
gtk-axioms` on every pull request, within the 30-minute job cap. The binary
SHALL NOT run in lanes that have no headless compositor.

#### Scenario: A probe fails
- **WHEN** a probe's axiom does not hold
- **THEN** its test fails and prints every measured value and the toolkit versions, so the failure shows how the behaviour changed

#### Scenario: The non-widget lane runs
- **WHEN** `cargo nextest run --workspace` or `make gtk-lush-adoption-lab` runs
- **THEN** the probe binary is not executed

### Requirement: A toolkit update runs the probes first
The documented procedure for any GTK or Libadwaita version change SHALL run
`make gtk-axioms` before other work. A failing probe SHALL be handled as an
axiom change under the ledger's rule, before any probe is edited.

#### Scenario: A GNOME SDK bump
- **WHEN** the GNOME SDK floor or the CI container's GTK version changes
- **THEN** `make gtk-axioms` runs, and every failing axiom's entry, dependent designs, and citing envelopes are reviewed before the bump is accepted

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

