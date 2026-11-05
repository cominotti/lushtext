## ADDED Requirements

### Requirement: Provider reloads and class swaps are a ledger axiom
The ledger SHALL record A86: after `gtk_css_provider_load_from_string` on a
provider added for the display, even with unchanged text, the next frame
revalidates the CSS of every widget, and its cost grows with the number of
widgets, while a load whose rules match no widget's classes changes no
computed font and re-lays out no text; after a CSS class is swapped on a set of widgets whose font rules are
keyed by that class, those widgets compute the new font size, and the next
frame's cost does not grow with widgets outside the set. Designs that change
the editor font or zoom SHALL cite A86. A86 SHALL be pinned by a
`gtk-lush-axioms` probe and a sample with an interactive and a `--check` mode.

#### Scenario: A86 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A86 probe measures a reload with unchanged text and a class
  swap on a source view and `.monospace` labels, each in a window without and
  with 1,500 unrelated rows, checks that the reload's frame grows with the rows
  and the class swap's does not, that the view's and labels' Pango font size
  follows the class, and that loading rules keyed by classes no widget carries
  leaves the view's Pango font size and its layout unchanged
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe and sample
  in agreement

#### Scenario: GTK changes how a class swap restyles
- **WHEN** a GTK update makes a class swap on a widget revalidate widgets
  outside its subtree, or a provider reload stop revalidating every widget
- **THEN** the A86 probe fails
- **AND** the zoom classes of `scope-font-zoom-restyle` are revisited before
  the probe is changed
