## ADDED Requirements

### Requirement: An edit's re-analysis cost can reduce syntax highlighting at run time
The document scale SHALL carry a run-time reduction reason,
`ReductionReason::EditReanalysis`, set only by the paced re-analysis of an
edit (`main-thread-responsiveness`) and never derived from a profile. Applying
it SHALL turn syntax highlighting off and leave every other feature as it was,
bracket matching included. Like every presentation reduction it SHALL survive
the session join, so no edit, save or replacement turns highlighting back on;
a reopen or an external reload starts a new document session from the
document's profile. The reduced-features notice, the document-properties row
and the editor's accessible description SHALL report it as "Syntax
highlighting is off because re-highlighting after an edit was too slow." when
no other reason reduces syntax highlighting. Kani SHALL prove that the
reduction turns syntax highlighting off, re-enables nothing, keeps the byte
tiers and bracket matching, and survives the join in both directions, for
every profile, language and Markdown chunked cap.

#### Scenario: The reduction is announced once
- **WHEN** an edit's re-analysis removes syntax highlighting
- **THEN** the status bar shows one warning naming the reason, and the
  document-properties row names it

#### Scenario: A save cannot restore it
- **WHEN** the document is saved after the reduction
- **THEN** syntax highlighting stays off until the document is reopened or
  reloaded
