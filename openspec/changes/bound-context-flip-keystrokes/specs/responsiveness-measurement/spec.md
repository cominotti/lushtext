## ADDED Requirements

### Requirement: The lane types ordinary and flipping keys into highlighted documents at their gates
The responsiveness lane SHALL carry a `reanalysis` group: for each measured
language, one blank-line-free `short-language` document at that language's
C6 total-size gate, and for c and markdown also one generated with
`--flavor no-closers`, each opened, settled and typed into after settle with
the standard interactions; each SHALL also exist as a `/flip` case whose
type-start and type-middle bursts type the language's context opener, seven
letters, its closer and three letters. The keysyms the bursts need SHALL be
added to the driver's injection allowlist, never with Control, and the
driver's self-test SHALL check that every burst character may be injected.
The verdict of a change that alters how edits are re-analysed SHALL come
from this group, interleaved with `main`.

#### Scenario: A flip case types its opener and closer
- **WHEN** the lane runs `reanalysis/c/86016/no-closers/flip`
- **THEN** its type-start phase delivers `/*abcdefg*/hij` to the editor, and
  the phase's input delivery counts 14 inserted keys
