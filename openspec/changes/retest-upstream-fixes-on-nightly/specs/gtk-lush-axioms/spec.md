## MODIFIED Requirements

### Requirement: Text, Pango and GtkSourceView probes keep structural verdicts
The `gtk-lush-axioms` crate SHALL be able to build GtkTextView, GtkTextBuffer, Pango and GtkSourceView fixtures. It MAY depend on the `pango` and `sourceview5` workspace crates, and still SHALL NOT depend on any GTK Lush or LushText crate.

A probe's verdict SHALL be decided only by structural observations: positions, geometry, tag membership, signal and call counts, captured log messages, and buffer state. Durations SHALL be recorded as observation values and SHALL NOT decide a verdict. A geometric comparison SHALL allow only the rounding that the observed fractional line positions explain, and SHALL record the values it compared.

A probe that needs an input method SHALL register its own probe `GtkIMContext` on the `gtk-im-module` extension point inside the probe's process, and SHALL select it only for its own view, through the view's `im-module` property, leaving the runner's `GTK_IM_MODULE` environment and the display's `gtk-im-module` setting unchanged. A probe that captures toolkit warnings SHALL restore the previous log handling before it returns.

A structural verdict whose fixture only reaches its state after a toolkit time limit, such as the context engine's 2000 ms cut-off, SHALL grow the fixture until the state is reached, up to a fixed length cap and a fixed wall-time budget. It SHALL also stop when one call runs past twice the toolkit's limit without the state being reached. It SHALL report `FixtureInvalid` rather than `Violated` when any of these bounds is reached first, with every attempt recorded, so that runner speed cannot flip a verdict and a toolkit that removed the limit cannot make the probe run unbounded.

#### Scenario: A layout-cost axiom is probed
- **WHEN** the A23 probe lays out paragraphs of increasing visible length
- **THEN** its verdict comes from the paragraph's geometry, and the per-character layout times appear only as recorded values

#### Scenario: A defect axiom captures a CRITICAL
- **WHEN** the A32 probe detaches a source map with an update tick pending
- **THEN** the probe captures the critical message as an observed value, the verdict holds while the critical is emitted, and the previous log handling is restored

#### Scenario: The input-method axiom is probed
- **WHEN** the A30 probe moves the caret, types, clicks and scrolls in a text view inside the probe's process
- **THEN** its probe input method counts surrounding-text requests and their lengths, and the runner's `GTK_IM_MODULE` and the display's `gtk-im-module` setting are unchanged afterwards

#### Scenario: A time-limited fixture runs on a fast runner
- **WHEN** the A29 probe's first line finishes analysis before the 2000 ms cut-off
- **THEN** the probe doubles the line and retries, and reports `FixtureInvalid` only if its cap is reached without the cut-off firing

#### Scenario: A toolkit no longer reaches the time limit
- **WHEN** the A29 probe runs against a toolkit whose context engine never logs the cut-off, and its attempts pass the 60 s wall budget, or one call passes 4000 ms, before the length cap
- **THEN** the probe stops, reports `FixtureInvalid` with every attempt's length and duration, and the run ends within about the budget

#### Scenario: The last character of a paragraph sits on a fractional line position
- **WHEN** the A23 probe runs with a font whose display lines start at fractional positions, and the last character's bottom edge rounds one pixel past the paragraph height
- **THEN** the verdict holds when the last character's display line is the paragraph's last one and the overshoot is within the recorded rounding, and is violated otherwise

## ADDED Requirements

### Requirement: The SDK runtimes lane runs a chosen subset and reports what a sandbox cannot run
`scripts/run-gtk-axioms.sh runtimes` SHALL accept `--only <ids>` and then build and check only the named samples in each SDK. An unknown id SHALL fail the run. Inside an SDK sandbox, the runtimes mode SHALL set `GTK_LUSH_AXIOMS_SANDBOX=1`. A sample whose fixture needs the compositor's RemoteDesktop SHALL read that variable before connecting, print `skipped (needs RemoteDesktop): <id>` and exit 0. The lane SHALL count that line as a result, as it counts a slow skip. The host lane and CI SHALL NOT set the variable, so a missing RemoteDesktop there still reports `fixture-invalid`. After each SDK, the lane SHALL print one summary line per sample, with its id, its verdict or skip reason, and the toolkit versions the sample printed. The lane SHALL exit non-zero if any sample in any SDK neither holds nor is skipped by one of these two rules.

#### Scenario: Re-testing only the affected samples
- **WHEN** `scripts/run-gtk-axioms.sh runtimes --only A28,A79 org.gnome.Sdk//master` runs
- **THEN** only the A28 and A79 samples are built and checked in that SDK, and the run expects exactly two results

#### Scenario: A RemoteDesktop sample inside an SDK sandbox
- **WHEN** the runtimes mode runs the A59 sample in `org.gnome.Sdk//51`
- **THEN** the sample prints `skipped (needs RemoteDesktop): A59`, exits 0, and the summary lists it as skipped, not as fixture-invalid

#### Scenario: The same sample on the host
- **WHEN** `make gtk-axioms` runs the A59 sample under a host compositor without RemoteDesktop
- **THEN** it reports `fixture-invalid` and the lane fails

#### Scenario: The shipped runtime's SDK passes
- **WHEN** `make gtk-axioms-runtimes` runs against `org.gnome.Sdk//51` on a host where every sample holds on the GNOME 51 toolkit
- **THEN** that SDK's summary has no violated or fixture-invalid sample
