## ADDED Requirements

### Requirement: The lane measures highlighted documents at the syntax cutoff
The lane SHALL run a `highlight` group: one synthetic short-line document per
language at 1 MiB (`S_SYNTAX`), for the five chunk-highlighted languages
(markdown, rust, c, python3, xml) and for the seven that keep their gates
(html, json, js, typescript, css, sh, yaml), each with the post-open settle and typing phases of the other
post-open cases. The fixture generator SHALL produce those documents
deterministically from a seed (`short-language`), every line shorter than
`L_SYNTAX`, with the file extension GtkSourceView guesses the language from.

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
