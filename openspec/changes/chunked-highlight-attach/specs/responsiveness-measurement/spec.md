## ADDED Requirements

### Requirement: The lane measures highlighted documents at the syntax cutoff
The lane SHALL run a `highlight` group: one synthetic short-line document per
language at 512 KiB, the chunked cap, for the two chunk-highlighted languages
(markdown, c) and for the ten that keep their gates (rust, python3, xml, html,
json, js, typescript, css, sh, yaml), each with the post-open settle and
typing phases of the other post-open cases. The C6 verdict SHALL report, not
gate, an activation-to-laid-out regression of a case marked `chunked`, and
list it apart from the regressions. The fixture generator SHALL produce those documents
deterministically from a seed (`short-language`), every line shorter than
`L_SYNTAX`, with the file extension GtkSourceView guesses the language from.

#### Scenario: A chunked case's later layout is reported
- **WHEN** a combined, side-labelled run finds a chunked case laid out more than one reference frame later than the baseline
- **THEN** the verdict lists it under the exempt regressions and still gates every other clause

#### Scenario: The highlight group runs
- **WHEN** the lane runs the `highlight/*` cases
- **THEN** each row carries the post-open dispatches and the typing phase of
  its document

### Requirement: The chunked highlighted install's verdict is measured interleaved
`chunked-highlight-attach` SHALL be judged on an interleaved comparison with
`main` through `scripts/measure.sh`, at least three repetitions per side, of
the `highlight` group, the construction profiles (first painted frame and
content fill-in) and C6's `total-size` cases, with the host load recorded per
run. Every post-open dispatch SHALL be within `B_SETTLE_US` and every input
dispatch during settle within `B_INTERACT_US`, or the miss SHALL be attributed
and recorded with its date and reason; activation to laid out on the highlight
group SHALL be reported, not gated.

#### Scenario: The verdict is recorded
- **WHEN** the interleaved runs finish
- **THEN** the programme record holds the branch and `main` figures, the load
  of every run, and each recorded exception
