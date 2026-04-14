# Research: File Peek

## Decision 1: Keep peek ownership inside `ui/sidebar/workspace_section/`

- Decision: Implement peek as a section-owned workflow with one reusable preview
  surface per `LushtextWorkspaceSection`.
- Rationale: The selected row widget, `GtkListView`, `SingleSelection`, row
  recycling, drill-down roots, and workspace-filter invalidation already live in
  the section. Keeping the popover and request lifecycle there preserves the
  existing sidebar ownership contract and avoids creating a second global
  controller for row-local behavior.
- Alternatives considered: A top-level sidebar manager or a window-owned
  preview controller. These would still need to reach into section-local row
  widgets for anchoring and invalidation, so they would increase coupling
  without simplifying ownership.

## Decision 2: Use a dedicated bounded snapshot service instead of the editor load path

- Decision: Add a GTK-free `services/file_peek.rs` module that performs
  metadata inspection, bounded prefix reads, UTF-8 eligibility checks, and
  fallback classification for preview.
- Rationale: `services/editor_io.rs` is optimized for opening real editor tabs.
  It reads whole files, returns editor-oriented errors, and participates in a
  workflow that later wires monitors, drafts, and buffer state. Peek needs a
  smaller read-only path with deterministic unit coverage and no accidental
  editor side effects.
- Alternatives considered: Reusing `editor_io::load_text_file()` and truncating
  afterward, or performing `std::fs` reads directly inside GTK callbacks.
  Reusing the full load path would over-read large files and blur the
  editor-versus-peek boundary. Doing file I/O in UI code would violate the
  repo's layer split and make testing harder.

## Decision 3: Present peek as an anchored `GtkPopover`, not a pane or tab

- Decision: Use a floating `GtkPopover`-style card anchored beside the realized
  sidebar row.
- Rationale: The spec explicitly requires an overlay that does not consume pane
  width. `GtkPopover` already matches the repository's existing context-menu
  pattern, can flip near window edges, and keeps the feature visually distinct
  from persistent panels or tabs.
- Alternatives considered: Reusing the existing markdown preview pane, adding a
  third persistent split child, or opening transient tabs. Those approaches
  either violate the "no pane resize" contract or recreate the tab-noise problem
  the feature is trying to solve.

## Decision 4: Keep keyboard ownership on the sidebar list while peek is visible

- Decision: The sidebar `GtkListView` remains the default focus owner during
  keyboard-triggered peek. Selection changes drive preview refresh, and dismissal
  explicitly restores focus to the sidebar list when promotion did not occur.
- Rationale: The primary value of peek is fast scanning. If the preview steals
  focus, Up and Down stop working naturally and the user has to context-switch
  between list navigation and preview interaction. This would break the
  specified interaction contract.
- Alternatives considered: Letting the popover own focus, or creating a second
  key controller on the preview body. Both make simple scanning slower and raise
  the risk of focus getting stranded on popover children or sidebar buttons.

## Decision 5: Treat stale-result suppression as a first-class requirement

- Decision: Every preview request carries a generation or token so async
  completions can be ignored when the user changes selection, hides the section,
  or closes peek before the background read finishes.
- Rationale: The feature is selection-driven. Without stale-result suppression,
  holding Up and Down through a tree will intermittently show the wrong file or
  revive a popover that the user already dismissed.
- Alternatives considered: No guard because reads are bounded, or a simple
  "latest path wins" comparison in the UI. A pure path comparison fails when the
  same file is reselected after an earlier close; a monotonic generation gives a
  stricter and easier-to-test ordering rule.

## Decision 6: Verification must span service, widget, and live runtime layers

- Decision: Verify peek with unit tests for snapshot behavior, widget tests for
  interaction and focus flows, and live `make run` validation for overlay
  positioning and warning-free behavior.
- Rationale: The service logic is deterministic and should not be tested only
  through GTK. Conversely, popover anchoring, focus restoration, and width-preset
  behavior cannot be trusted from unit tests alone.
- Alternatives considered: Widget-only coverage or service-only coverage.
  Widget-only would make core eligibility logic slower and more brittle to
  validate. Service-only would miss the exact class of focus and allocation
  regressions the constitution treats as product defects.
