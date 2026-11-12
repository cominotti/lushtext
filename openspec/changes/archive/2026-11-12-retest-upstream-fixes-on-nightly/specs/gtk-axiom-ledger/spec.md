## ADDED Requirements

### Requirement: Nightly re-test results are recorded without restating an axiom
When a ledger axiom is re-tested against a GNOME nightly SDK, its "Verified against" cell SHALL gain a note naming the following, each copied from the run:
- the nightly SDK build commit and date;
- the toolkit versions the sample printed, with the GTK, Pango and GtkSourceView source commits from the SDK's manifest;
- where the sample ran;
- the verdict and the recorded values that the re-test was about.

The note SHALL be labelled as a preview build. It SHALL NOT be counted as a run on a supported toolkit. An axiom that changes verdict only on a nightly SDK SHALL NOT be restated or version-qualified until the change reaches a toolkit the project supports and is attributed. The ledger or the programme record SHALL list such an axiom as an unattributed nightly preview, owned by the next GNOME adoption change. A ledger cell that names a probe SHALL name the probe that the catalogue entry with the same id registers, and `make check-gtk-axioms` SHALL reject a "Pinned by" cell that names another axiom's probe.

#### Scenario: A defect still reproduces on the nightly
- **WHEN** the A79 sample holds on the nightly SDK that contains GTK !10651
- **THEN** A79's "Verified against" gains a nightly note with the build commit, GTK `4.25.x` and its source commit, and the 1, 2 and 4 MiB lookup times
- **AND** the axiom statement is unchanged

#### Scenario: An axiom fails only on the nightly
- **WHEN** the A85 sample fails on the nightly SDK and holds on GNOME 51 and on the GNOME 50 floor
- **THEN** neither A85's statement nor its probe is changed
- **AND** A85 is listed as an unattributed nightly preview for the next GNOME adoption, with the failed check and the build commit

#### Scenario: A ledger cell names another axiom's probe
- **WHEN** the A67 row's "Pinned by" cell names `gtk_lush_axioms::probe_a52`
- **THEN** `make check-gtk-axioms` reports that A67's cell names another axiom's probe
- **AND** the cell is corrected to `probe_a67`, which the A67 catalogue entry registers, as are the A68–A73 cells that name `probe_a53` … `probe_a58`
