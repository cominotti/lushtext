## ADDED Requirements

### Requirement: Equal-priority co-dispatch is a ledger axiom
The ledger SHALL record A81: GLib dispatches every ready source of the top
priority in one main-loop iteration, so that iteration lasts the sum of their
callbacks, while a ready source one step lower, or one of the same priority
added during the dispatch, runs in a later iteration. Designs that rely on
equal-priority co-dispatch SHALL cite A81. A81 SHALL be pinned by a
`gtk-lush-axioms` probe and a sample with an interactive and a `--check` mode.

#### Scenario: A81 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A81 probe arms three busy idles at one priority, one a step
  lower and one added from a callback, and records which ran in each iteration
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe and sample
  in agreement

### Requirement: A hidden tab page's text view lays out its buffer
The ledger SHALL record A82: an `AdwTabView` page never selected is neither
mapped nor realized, yet a `GtkSourceView` on it has a text layout and validates
text loaded into its buffer in full while hidden, as does a page selected once
and hidden again; only a buffer no view shows is not laid out; and the first
page appended to an empty tab view is selected by the tab view. A82 SHALL be
pinned by a probe and a sample.

#### Scenario: GTK stops laying out never-shown pages
- **WHEN** a GTK or Libadwaita update makes a never-selected page's view skip
  validation
- **THEN** the A82 probe fails
- **AND** the placeholder detach of `defer-background-tab-layout` is revisited
  before the probe is changed

### Requirement: An empty buffer stops a hidden view's layout
The ledger SHALL record A83: `gtk_text_view_set_buffer` to an empty buffer stops
a hidden view's validation at once and leaves the document buffer's text,
modified flag, undo state and caret unchanged; setting the document back lays
it out again from nothing and brings the view back at the buffer start; a
scroll queued right after the reattach lands at another line once the lines
above get heights, while scrolling to the target again on every `upper` change
keeps the top near it and one more scroll once laid out lands
on it; an unmapped validating view notifies its vertical adjustment's `upper`.
A83 SHALL be pinned by a probe and a sample; its accessibility consequence
(`set_buffer`'s whole-content text change) is recorded from source.

#### Scenario: A83 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A83 probe detaches a hidden validating view, reattaches it hidden
  and shown, and records the iterations and top lines
