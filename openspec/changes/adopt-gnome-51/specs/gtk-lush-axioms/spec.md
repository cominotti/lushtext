## MODIFIED Requirements

### Requirement: Probes run headless as one test binary and in CI
A probe test binary built on `gtk-lush-proof-harness`, outside the family (in `gtk-lush-adoption-lab`), SHALL run every catalogued probe as a separately isolated test, print each observation, and fail unless the verdict is holds. For a version-qualified axiom, holds means it holds for the running toolkit's version range. `make gtk-axioms` SHALL run that binary and every sample's `--check` headless, never against the developer's live desktop session. CI SHALL run `make gtk-axioms` on every pull request within the 30-minute job cap, once on the toolkit the shipped Flatpak runtime carries and once on the GNOME source floor's toolkit. The binary SHALL NOT run in lanes that have no headless compositor.

#### Scenario: A probe fails
- **WHEN** a probe's axiom does not hold
- **THEN** its test fails and prints every measured value and the toolkit versions, so the failure shows how the behaviour changed

#### Scenario: The non-widget lane runs
- **WHEN** `cargo nextest run --workspace` or `make gtk-lush-adoption-lab` runs
- **THEN** the probe binary is not executed

#### Scenario: Both supported toolkits are probed in CI
- **WHEN** a pull request runs CI after the Flatpak moved to GNOME 51
- **THEN** a `GTK Axioms` job runs `make gtk-axioms` in a `fedora:45` container (GTK 4.24) and another in a `fedora:44` container (GTK 4.22, the GNOME 50 floor)

### Requirement: A toolkit update runs the probes first
The documented procedure for any GTK or Libadwaita version change SHALL run `make gtk-axioms` before other work. A failing probe SHALL be handled as an axiom change under the ledger's rule, before any probe is edited. The procedure SHALL cover a change of the Flatpak manifest's runtime, of the development toolbox or CI container toolkit, and of the GNOME source floor. Before a new runtime is shipped, `make gtk-axioms-runtimes` SHALL check the samples against that runtime's SDK; its default SDK list SHALL include the floor's SDK, the shipped runtime's SDK and the nightly SDK.

#### Scenario: A GNOME SDK bump
- **WHEN** the GNOME SDK floor or the CI container's GTK version changes
- **THEN** `make gtk-axioms` runs, and every failing axiom's entry, dependent designs, and citing envelopes are reviewed before the bump is accepted

#### Scenario: The shipped runtime moves ahead of the floor
- **WHEN** the Flatpak manifest's `runtime-version` moves from 50 to 51 while the gtk-rs feature floor stays GNOME 50
- **THEN** `make gtk-axioms` runs on the GNOME 51 toolkit and on the GNOME 50 floor, and `make gtk-axioms-runtimes` checks `org.gnome.Sdk//51`, before the manifest change is accepted
