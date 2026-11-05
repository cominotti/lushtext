## ADDED Requirements

### Requirement: An edit's re-analysis is paced and bounded before any worker slice
After an edit in a document whose language is attached and highlighted, the
editor SHALL re-analyse the edit itself, before the context engine's first
update, in steps dispatched from an idle at `G_PRIORITY_HIGH_IDLE - 1` (ledger
A85): each step SHALL call `gtk_source_buffer_ensure_highlight` on an empty
range at a bound, sized by `model::reanalysis_pacing` towards a quarter of
`B_INTERACT_US` at the rate the previous step measured, at most twice the
previous step and within 128 and 1 Mi characters, the first 512. A step whose
report ends short of its bound, or that reports nothing, or whose bound is the
buffer end, SHALL end the session. Edits during a session SHALL join it
without resetting what it spent. A session that has spent more than
`REANALYSIS_ALLOWANCE_US` (`B_SETTLE_US`) without merging, or run 256 steps,
SHALL remove syntax highlighting for the document session
(`ReductionReason::EditReanalysis`) from the step idle, before any
`G_PRIORITY_LOW` worker slice runs. No step SHALL run while a load
installation or a buffer replacement owns the buffer, or while the language or
highlighting is off. Kani SHALL prove that a session continues only within its
allowance and step bound, that every step size stays within its bounds and at
most doubles, and that a session that never merges downgrades within the step
bound.

#### Scenario: An ordinary key keeps highlighting
- **WHEN** a key is typed in the middle of a small highlighted Rust document
- **THEN** its re-analysis merges in the paced steps, the context engine's
  first update reports nothing, no worker slice runs, and the language stays

#### Scenario: An edit the engine cannot re-analyse within the allowance
- **WHEN** a key is typed at the start of 10 KiB of blank-line-free
  TypeScript, below its total-size gate
- **THEN** syntax highlighting is removed with the reason `EditReanalysis`
  before any `G_PRIORITY_LOW` worker emission, bracket matching stays, the text
  is the edited text, and later keys start no pacing

#### Scenario: Context-flipping keys on both fixture families
- **WHEN** a key that changes the syntax context of the rest of the document
  (a comment, string, docstring or HTML comment opener, then its closer) is
  typed at the start or the middle of a document at its language's gate, with
  or without a closer after the cursor
- **THEN** every dispatch until quiet stays within `B_SETTLE_US`, and every
  ordinary key typed after it within `B_INTERACT_US`

#### Scenario: Plain text starts no pacing
- **WHEN** a key is typed in a document without a language
- **THEN** no pacing session starts
