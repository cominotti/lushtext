## ADDED Requirements

### Requirement: An edit's re-analysis and its pacing are a ledger axiom
The ledger SHALL record A85: after an edit in a GtkSourceView buffer whose
language is attached, an idle queued at `G_PRIORITY_HIGH_IDLE` before the edit
dispatches immediately before the context engine's first update, whose
`highlight-updated` comes from a `G_PRIORITY_HIGH_IDLE` source; an idle queued
at `G_PRIORITY_LOW - 1` after the edit dispatches after it and before the first
worker slice; `gtk_source_buffer_ensure_highlight` on an empty range at a
bound, from an idle at `G_PRIORITY_HIGH_IDLE - 1`, re-analyses the edit to the
line after the bound, applies no style tag (only the context-class tags every
analysis maintains), and reports the range in one `highlight-updated`, whose
end is short of the bound exactly when the analysis merged, after which the
first update and the worker find nothing; and removing the language from such
an idle removes the pending first update and worker and changes no text.
Designs that pace an edit's re-analysis SHALL cite A85. A85 SHALL be pinned by
a `gtk-lush-axioms` probe and a sample with an interactive and a `--check`
mode; the sample SHALL also measure ordinary and context-flipping keys on any
document, plain and paced.

#### Scenario: A85 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A85 probe types an ordinary key into a small C document and
  `/*` at the start of 512 KiB of C with no closer, checks the idle order
  around the first update, that a pending worker is removed with the language
  from a `G_PRIORITY_LOW - 1` idle, that paced steps leave the first update
  and the worker nothing, that each step makes exactly one `highlight-updated`
  and no style-tag change (against a control in which `ensure_highlight` over
  the whole document does tag it), and that removing the language between steps
  removes both
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe and sample
  in agreement

#### Scenario: GtkSourceView changes how an edit is re-analysed
- **WHEN** a GtkSourceView update makes `ensure_highlight` apply style tags
  for an empty range, report more than one range per call, or leave the first
  update work after a merged step
- **THEN** the A85 probe fails
- **AND** the paced re-analysis of `bound-context-flip-keystrokes` is
  revisited before the probe is changed
