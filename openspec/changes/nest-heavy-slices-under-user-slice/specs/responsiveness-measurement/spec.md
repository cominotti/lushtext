## ADDED Requirements

### Requirement: A lane run through scripts/measure.sh runs on and records the reference CPU set
When `LUSHTEXT_MEASURE_CPUS` is set, the responsiveness lane SHALL run on exactly that set, and SHALL fail before any case when its inherited affinity differs from it or it exceeds the 16-CPU cap, never trimming it. The lane SHALL record the set and its source in `environment.txt`, `run.json` and `report.md`, and SHALL label unpinned runs, overridden sets and merges of parts on different sets as not comparable with reference runs.

#### Scenario: A reference run is recorded
- **WHEN** the lane runs under `scripts/measure.sh` with the default reference 8-23
- **THEN** `environment.txt` has `affinity=8-23`, `measure_cpus=8-23` and `measure_cpus_source=default`, `run.json` has `reference_run: true`, and `report.md` names 8-23 as the reference CPU set

#### Scenario: An unpinned run is labelled
- **WHEN** the lane runs without `LUSHTEXT_MEASURE_CPUS`
- **THEN** it takes at most 16 CPUs from its inherited affinity as before, `run.json` has `reference_run: false`, and `report.md` says the run is not pinned to the reference CPU set and is trend evidence

#### Scenario: An overridden reference is labelled
- **WHEN** the lane runs under `scripts/measure.sh` with `LUSHTEXT_REFERENCE_CPUS=12-23`
- **THEN** `run.json` has `measure_cpus_source: "override"` and `reference_run: false`, and `report.md` says the run is not comparable with reference runs

#### Scenario: A reference over the cap is refused
- **WHEN** `LUSHTEXT_MEASURE_CPUS` names more than 16 CPUs
- **THEN** the lane fails before any case runs and the message names the set and the cap

#### Scenario: A mismatched inherited affinity is refused
- **WHEN** `LUSHTEXT_MEASURE_CPUS` is set and the lane's inherited affinity differs from it
- **THEN** the lane fails before any case runs and the message names both sets

#### Scenario: Parts on different CPU sets merge as mixed
- **WHEN** `merge-runs` joins parts whose `affinity`, `measure_cpus` or `measure_cpus_source` differ
- **THEN** the merged `run.json` has `affinity: "mixed"`, each part's set in `merged_from` and `reference_run: false`, and `report.md` says the parts are not comparable

#### Scenario: A part without the fields counts as unset
- **WHEN** `merge-runs` joins a part whose `run.json` predates the `measure_cpus` fields with an unpinned part on the same affinity
- **THEN** the merged `run.json` keeps that affinity and has `reference_run: false`, and a merge of such a part with a pinned part is mixed

## MODIFIED Requirements

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
- **WHEN** the lane is not run under `scripts/measure.sh` (`LUSHTEXT_MEASURE_CPUS` unset) and the inherited CPU affinity mask allows more than 16 CPUs
- **THEN** the session and app run under an affinity of at most 16 CPUs taken from that mask, never widened beyond it, and the environment report records it

#### Scenario: A known toolkit defect is reproduced
- **WHEN** the run reproduces a runtime warning already owned by a later change in the programme
- **THEN** the report lists it as a finding with its owning change, and no allowlist entry hides it

#### Scenario: A fixed pre-existing blocker reappears
- **WHEN** the run reproduces the runtime warning of a pre-existing blocker that a change in the programme has fixed, such as the scrollbar `GtkGizmo` snapshot warning (ledger A37)
- **THEN** the report lists it as a regression and the lane fails
