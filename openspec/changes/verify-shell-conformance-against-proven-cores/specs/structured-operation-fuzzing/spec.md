## ADDED Requirements

### Requirement: Operation scripts can drive shell conformance
The `operation_script` fuzz target SHALL have a conformance mode, selected by
the input, that decodes bounded bytes into the same operation vocabulary as the
conformance properties. That mode SHALL run the same lockstep driver against
the durable-write shell and the draft service over tiny temporary directories.
It SHALL report a divergence from the model as a failure. The mode SHALL keep
the target's explicit operation-count and input-length bounds. Stable corpus
replay SHALL replay its committed seeds. A divergence the fuzzer finds SHALL be
promoted to a committed seed and a deterministic regression test.

#### Scenario: A conformance seed replays on stable
- **WHEN** `make fuzz-corpus-replay` runs
- **THEN** the committed conformance-mode seeds run through the lockstep driver without nightly, cargo-fuzz, or sanitizers

#### Scenario: A fuzz-found divergence is promoted
- **WHEN** the conformance mode finds an input on which the shell diverges from the model
- **THEN** the input is committed as a seed and the decoded operation script is committed as a deterministic regression test
