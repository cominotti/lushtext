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
