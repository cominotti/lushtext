## ADDED Requirements

### Requirement: The lane measures a save during a validation backlog
The lane SHALL have a case, `save/during-validation`, that runs in both tiers.
It opens a small scratch document that the case writes into its own state
directory, never a fixture. It then opens two 8 MiB short-line fixtures through
a second activation of the running instance, selects the scratch tab, types one
character and activates `win.save` (the action Ctrl+S activates; the lane never
injects a Control+letter chord) while those fixtures are still validating. The
case SHALL record the time from that activation until the scratch file's
bytes change on disk (`save_latency_us`), the phase's watchdog stalls and the
typed key's input-to-frame. The report SHALL list these per run. The case SHALL
verify the fixture digests like every other case, so the lane still never
changes a fixture's bytes.

#### Scenario: The save completes during the backlog
- **WHEN** the case runs against a build whose admission hand-offs and
  completions are held behind validation
- **THEN** `save_latency_us` approaches the backlog's length, seconds
- **AND** against this change it is a few frames, and no dispatch in the phase
  exceeds `B_SETTLE_US` that would not on `main`

#### Scenario: A fixture is never saved
- **WHEN** the case saves
- **THEN** only the scratch file in the case's state directory is written
- **AND** the fixture ledger's digests are unchanged after the case
