## ADDED Requirements

### Requirement: A re-test of an upstream fix is versioned, contained and interleaved
When an upstream maintainer asks LushText to re-test a fix, the re-test SHALL first establish:
- the build that was tested: the SDK commit and date, and each toolkit module's source commit from the SDK's manifest;
- for each fix commit or merge request, whether the tested build contains it, checked against the upstream repository's history (merge base), not inferred from dates or version numbers;
- the first release tag, if any, that contains it.

Timing results SHALL come from interleaved rounds of the affected samples only, the tested build alternating with the shipped runtime's SDK. There SHALL be at most four rounds, each started at a 1-minute load below 4, with the load before and after recorded. Each result SHALL be reported as the median and the range across rounds. The upstream-facing reproducer SHALL also be rerun inside the tested build.

#### Scenario: A fix merged through a rebase
- **WHEN** a GtkSourceView merge request's reported head commit is not an ancestor of the nightly, but its commits landed rebased on `master` before the nightly's commit
- **THEN** the record names the landed commits and counts the fix as contained, citing the merge base

#### Scenario: A fix that is in no release
- **WHEN** a fix is on `main` but in no tag
- **THEN** the record says so, and no workaround is changed on its account

### Requirement: Each workaround's retirement condition names a version floor and a measurement
Every row of `docs/upstream-reports.md` whose workaround waits on an upstream fix SHALL state, in its "When resolved" cell:
- the first release that carries the fix, or that no release carries it yet;
- that the release must be the project's floor, and also the shipped runtime when the workaround concerns the shipped Flatpak;
- the measurement that must pass before the workaround is removed, with its budget;
- what stays even then.

A row's axiom id SHALL be the ledger axiom that pins the behaviour named in the row.

#### Scenario: A fix brings a cost under budget only on a newer toolkit
- **WHEN** the nightly with GtkSourceView !465 measures the first html, markdown and json attach at or below `B_SETTLE_US`, while GNOME 51 measures them above it
- **THEN** the gtksourceview#432 row says the C6 toolkit-floor exception is lifted only once GtkSourceView 5.24 or later is the shipped runtime and the floor, and the lane measures the attach at or below `B_SETTLE_US`
- **AND** it says the attach stays deferred to a low-priority idle, because it still exceeds `B_INTERACT_US`

#### Scenario: A fix does not address the defect
- **WHEN** the nightly containing Pango `f60f461` still shows x positions wrapping negative past the overflow length
- **THEN** the pango#582 row records the re-test result and the workaround stays, with no new retirement condition

### Requirement: Upstream text is drafted only within the reporting rules
Replies to an upstream maintainer's feedback or re-test request SHALL NOT be drafted. Instead, the issue's report folder SHALL get a facts sheet with:
- the measured results, versions and source commits;
- the commands that produced them;
- the reproducer output files;
- the questions the maintainer may want to answer.

A new upstream report SHALL be drafted only when the defect still reproduces on the newest nightly build and the tracker has no existing issue or merge request for it. The draft SHALL follow the `upstream-gtk-reports` skill's folder layout and the project's issue template. It SHALL use none of LushText's internal vocabulary, and SHALL end with an AI-assistance disclosure line. Nothing SHALL be filed or posted without the maintainer's explicit approval for that one item.

#### Scenario: A maintainer asks for a re-test
- **WHEN** the gtk#8528 maintainer asks whether GTK !10651 helps
- **THEN** `retest-facts.md` in the issue's report folder lists the nightly's lookup times, the GNOME 51 times, the build and source commits, and the commands, and holds no drafted reply

#### Scenario: A candidate still reproduces on the nightly
- **WHEN** the A102 sample still shows a superseded `forward_async` request that never calls back, with a GIO CRITICAL, on the newest nightly
- **THEN** a GtkSourceView report is drafted in its own folder with a standalone reproducer, its output from inside the nightly SDK, and the disclosure line
- **AND** it is listed under "Drafted, not filed" in `docs/upstream-reports.md`

#### Scenario: A candidate no longer reproduces
- **WHEN** a drafted or candidate defect does not reproduce on the newest nightly
- **THEN** no report is drafted or kept for it, and the row moves to "Followed, not filed" with the build that fixed it
