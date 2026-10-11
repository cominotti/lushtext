## ADDED Requirements

### Requirement: Sliced protected installation behaviours are ledger axioms
Every GTK text-buffer behaviour that the sliced protected install relies on SHALL be a ledger entry with a new stable id, allocated after the ids taken by the long-line probe and protected-view changes. Existing ids SHALL NOT be reused. At minimum the ledger SHALL cover:

- **Segment separation.** Two character segments separated by a zero-byte mark segment are not merged by the line cleanup. Text inserted at a left-gravity mark's iterator is placed after the mark, while a right-gravity mark moves after it.
- **Hidden append re-validation.** Re-validating a paragraph after text carrying an invisible tag is appended to it costs time bounded by the paragraph's visible text and its segment count, not by its hidden bytes.
- **Accessible change payloads**, extending the existing accessibility-payload axiom. With the accessibility bridge on and a listener attached, the text-change event for an insertion or deletion is built from the inserted or deleted range only, so its size and cost are bounded by that run and do not depend on the text that remains in the paragraph. The insert event carries the inserted text as it was before the tag is applied; the delete event of a hidden range carries no hidden characters.

The existing segment-merge axiom, that merging adjacent character segments copies both strings, remains the axiom these entries mitigate. The segment-separation and hidden-append entries SHALL each be pinned by an isolated headless probe in the `gtk-lush-axioms` crate, using a minimal GtkTextBuffer or GtkTextView fixture that uses no LushText or GTK Lush widget, with a catalogue entry and a sample. Because segment structure is not public, the segment-separation probe SHALL observe mark gravity structurally and the absent merge by per-append cost, and its ledger row SHALL say so. The accessible-payload entry SHALL be pinned the way the existing accessibility-payload axiom is, by the responsiveness lane's accessibility case, because the probe runner starts no AT-SPI bus, and its ledger row SHALL say so. Each entry's statement SHALL be written from what its probe or lane case observes. A failing probe SHALL block adoption of the sliced protected install, or switch it to the recorded fallback.

#### Scenario: The segment-separation probe runs
- **WHEN** `make gtk-axioms` runs on the pinned toolkit
- **THEN** the segment-separation probe inserts tagged runs at the end of a paragraph across left-gravity marks, observes that each run lands after the previous mark, and records a per-append time that does not grow with the installed tail, while the same appends without marks show a growing per-append time

#### Scenario: A toolkit update breaks a sliced-install axiom
- **WHEN** a sliced-install axiom probe fails after a GTK update
- **THEN** the sliced protected install and its fallback are re-reviewed against the ledger before the failure is resolved
