## ADDED Requirements

### Requirement: An axiom whose verdict differs between supported toolkits is version-qualified
When a ledger axiom holds on one supported toolkit version and not on another, the ledger entry SHALL state the behaviour for each supported version range, with the version bound and the upstream change that introduced the difference. The supported versions are the GNOME source floor and the runtime the Flatpak ships. The entry's probe and sample SHALL read the running toolkit's version, assert the statement for that range, and print which range they asserted. They SHALL fail if the behaviour on either range changes again. "Verified against" SHALL record a run on each supported range and where each run happened. An entry SHALL be restated only after the toolkit change is attributed and every dependent design and citing verification envelope is reviewed; a probe SHALL be edited only after its entry is restated. A defect axiom fixed upstream SHALL be restated the same way while any supported toolkit still has the defect.

#### Scenario: GTK 4.24 changes the list's anchor behaviour
- **WHEN** `make gtk-axioms` runs on GTK 4.24 and the A7 and A20 probes observe that a host `set_value` no longer leaves a stray anchor
- **THEN** A7 and A20 state the GTK < 4.24 behaviour and the measured GTK ≥ 4.24 behaviour, attributed to the GTK change that introduced it
- **AND** the probes hold on both the GNOME 50 floor and GTK 4.24, asserting the range they run on

#### Scenario: A defect axiom is fixed upstream
- **WHEN** the A32 probe runs on GtkSourceView 5.22 or later, where detaching a map with a pending tick logs no CRITICAL
- **THEN** A32 states that the defect is present below 5.22.0 and fixed from 5.22.0 (`8d679ccd`)
- **AND** on 5.22 the probe holds only if no CRITICAL is logged, so a return of the defect fails it

#### Scenario: Restating before reviewing
- **WHEN** a probe fails after a toolkit update and its dependent designs or citing envelopes have not yet been reviewed
- **THEN** neither the entry nor the probe is edited

### Requirement: The ledger is verified on the shipped toolkit and on the floor
Every ledger row SHALL record in "Verified against" a run on the toolkit the shipped Flatpak runtime carries. Every row with a version-qualified statement SHALL also record a run on the GNOME source floor. After a runtime change, no row SHALL cite only the previous runtime's toolkit.

#### Scenario: Moving the Flatpak to GNOME 51
- **WHEN** the Flatpak runtime moves from GNOME 50 to GNOME 51
- **THEN** every probed row's "Verified against" names GTK 4.24.x / Adw 1.10.x, copied from a `make gtk-axioms` observation with the place it ran
- **AND** the version-qualified rows also name the GNOME 50 floor run
