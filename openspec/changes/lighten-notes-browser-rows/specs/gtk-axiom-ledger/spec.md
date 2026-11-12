## ADDED Requirements

### Requirement: List row cost behaviours are ledger axioms
The ledger SHALL record four axioms, A108–A111, each pinned by an isolated `gtk-lush-axioms` probe, with a sample that has an interactive and a `--check` mode, and verified against the toolkit the change measured (GTK 4.24.1, libadwaita 1.10.0):

- **A108.** In a presented `GtkListView` with about 205 realized rows (A1), changing the text of every realized row's `GtkLabel` makes the next frame measure every realized row again, off-screen rows included. That frame's layout grows with the realized rows and dominates it. When the row's text widget has a measure that does not depend on its text, the same change costs the frame no layout, and the frame costs the same at 205 realized rows as at 16.
- **A109.** `GtkInscription`'s constructor builds its Pango layout, and so computes a style for a node with no parent yet. Constructing 205 of them costs several times constructing 205 `GtkLabel`s.
- **A110.** Removing realized rows from the end of a presented list's model, `items-changed(len - k, k, 0)`, tears down only the removed rows' widgets, at a cost proportional to `k`. This holds while the list is not mapped.
- **A111.** Under `GSK_RENDERER=cairo`, each frame of an `AdwDialog`'s open and close animation costs a paint that does not depend on the dialog's content (8 rows or 205). Under the GL renderer the same frames cost a few milliseconds. The probe records both, and skips the GL half, stating why, where no GL renderer is available.

A probe that records costs SHALL record magnitudes and assert only ratios or bounds stated in its ledger row. The `lighten-notes-browser-rows` design SHALL cite these ids, and the programme record's exceptions E1–E3 and X1 SHALL cite the axiom their remaining cost rests on.

#### Scenario: The row cost axioms are probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A108–A111 probes and samples run under the private headless session, and any CRITICAL or WARNING they provoke fails the lane
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probes and samples in agreement

#### Scenario: A toolkit update changes a row cost
- **WHEN** a GTK update makes a `GtkLabel` row's relayout stop growing with realized rows, or makes `GtkInscription` lay out lazily
- **THEN** the A108 or A109 probe changes verdict, and the ledger, the row text design and the programme record are revisited before any probe is edited
