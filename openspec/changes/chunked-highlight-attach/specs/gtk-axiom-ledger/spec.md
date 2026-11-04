## ADDED Requirements

### Requirement: The context engine's reported coverage is a ledger axiom
The ledger SHALL record A84: after an insert at the end of a GtkSourceView
buffer whose language is attached, or an attach to a buffer already holding
text, the largest `highlight-updated` end reported since the change reaches
the end of the last line's text once the context engine has analysed the whole
buffer, a `G_PRIORITY_LOW + 1` idle queued after the change always finds it
there, and a chunk the `G_PRIORITY_LOW` worker still analyses is not reported
at the end by a `G_PRIORITY_DEFAULT_IDLE` idle. Designs that pace inserts by
the engine's progress SHALL cite A84. A84 SHALL be pinned by a
`gtk-lush-axioms` probe and a sample with an interactive and a `--check` mode;
the sample SHALL also measure the paced chunked install per language.

#### Scenario: A84 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A84 probe installs short-line HTML chunk by chunk into an
  attached buffer, with `\n`, CRLF and unterminated text, attaches a language
  to a buffer holding one chunk, and checks the reported coverage at a
  `G_PRIORITY_LOW + 1` idle after each change
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe and sample
  in agreement

#### Scenario: GtkSourceView changes what highlight-updated reports
- **WHEN** a GtkSourceView update reports less than the analysed range, or
  reports it after a `G_PRIORITY_LOW + 1` idle
- **THEN** the A84 probe fails
- **AND** the chunked highlighted install of `chunked-highlight-attach` is
  revisited before the probe is changed
