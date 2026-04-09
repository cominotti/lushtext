---
title: 'Search panel results reveal animation'
type: 'bugfix'
created: '2026-04-09'
status: 'in-review'
baseline_commit: '98e47fa'
context: []
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** The "Search in files" panel grows abruptly when result rows first appear because the result widgets are toggled visible immediately instead of participating in the panel's reveal animation. That snap makes the panel feel unstable right when the user is typing and scanning the first matches.

**Approach:** Move the result-growth path under a dedicated `GtkRevealer` so the panel height expands and collapses smoothly, using the same reveal style as the panel itself. Add widget tests that cover the reveal configuration and the important state transitions for searching, first-result arrival, and clearing results.

## Boundaries & Constraints

**Always:** Preserve the current search workflow, debounce behavior, progress footer text, and result-height clamp. Keep the change localized to the search panel UI/state management and widget tests. Match the panel's existing reveal direction and duration rather than inventing a new motion pattern.

**Ask First:** If a robust fix requires changing the outer window layout, replacing `GtkRevealer`, or weakening the existing search panel animation semantics.

**Never:** Add arbitrary sleeps to production code. Remove progress/no-results feedback. Reduce widget-test coverage to a single structure-only assertion.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| First match arrives | Search is running, footer may already show progress, first file match is appended | Results area becomes revealed through the configured animation instead of snapping visible | N/A |
| Query cleared | Panel has visible results from a prior search, user clears the query | Results revealer collapses and result widgets reset to hidden state | N/A |
| No-results search | Search completes with zero matches | Footer/status messaging still appears without forcing the results list scroller visible | N/A |

</frozen-after-approval>

## Code Map

- `resources/ui/search-panel.ui` -- Search panel structure; add a revealer around the results body so height changes animate cleanly
- `crates/lushtext-core/src/ui/search_panel/imp.rs` -- Template child wiring for any new revealer widget
- `crates/lushtext-core/src/ui/search_panel/mod.rs` -- Result visibility state transitions during search start, first match, no-results completion, and clear
- `crates/lushtext/tests/widget/search_panel.rs` -- Widget tests for reveal configuration and result-area state transitions

## Tasks & Acceptance

**Execution:**
- [x] `resources/ui/search-panel.ui` -- Wrap the feedback area and results body in dedicated `GtkRevealer`s that use the same transition type and duration as the outer search panel reveal
- [x] `crates/lushtext-core/src/ui/search_panel/imp.rs` -- Expose the new results revealers as template children for state management and tests
- [x] `crates/lushtext-core/src/ui/search_panel/mod.rs` -- Route results visibility through the revealers so fresh searches expand once, follow-up searches preserve expansion while in flight, and confirmed no-results states contract
- [x] `crates/lushtext/tests/widget/search_panel.rs` -- Add coverage for reveal configuration, fixed-height first-result reveal, no-results behavior, follow-up search preservation, and clearing results

**Acceptance Criteria:**
- Given the search panel is open and a query starts producing matches, when the first result batch arrives, then the panel height grows through a revealer transition instead of snapping larger in one frame
- Given a query is in progress with no matches yet, when progress text is shown, then the footer can appear without forcing the result scroller visible
- Given prior search results are visible, when the query is cleared or a new search resets state, then the animated results section collapses and the scroller is hidden again
- Given prior search results are visible, when a follow-up search is still running, then the results body stays expanded until the new outcome is known, and if that follow-up search finishes with no results, then the widget contracts to the footer-only state
- Given widget tests run, when they inspect the search panel animation path, then they verify both the revealer configuration and the key visibility-state transitions that protect against regressions

## Spec Change Log

## Verification

**Commands:**
- `cargo test -p lushtext --test widget search_panel -- --nocapture` -- expected: search panel widget tests pass, including new reveal-state coverage
- `cargo test -p lushtext --test widget` -- expected: full widget suite remains green after the search panel transition changes
- `cargo fmt --check` -- expected: formatting is clean after the patch
