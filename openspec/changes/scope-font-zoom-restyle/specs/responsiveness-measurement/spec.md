## ADDED Requirements

### Requirement: The lane measures a restyle in a populated window
The responsiveness lane SHALL carry, beside the dense restyle cases, one case
that runs the same word-wrap, zoom-step and font-change phases on a dense
document with the workspace sidebar shown over one restored folder of 300
synthetic entries, and its control with the same window geometry and the
sidebar shown over an empty folder, the search panel closed in both; and a
variant of the populated case with the search panel listing results, whose
font-following labels are reported separately. The fixtures are generated and
isolated like every other lane fixture. The verdict of a change that alters how the
editor font or zoom is applied SHALL come from the dense restyle cases and
this case, interleaved with `main`.

#### Scenario: The populated restyle case runs every restyle phase
- **WHEN** the lane runs the populated restyle case
- **THEN** its report lists the wrap-toggle, zoom-step and font-change
  interaction dispatches and every later dispatch beside those of its
  empty-folder control and of the search-results variant, with the difference
  to the control per phase
