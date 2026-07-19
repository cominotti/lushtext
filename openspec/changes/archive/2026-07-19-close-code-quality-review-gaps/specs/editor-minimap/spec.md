## ADDED Requirements

### Requirement: Wrapped-layout admission uses conservative live bytes
Minimap wrapped-layout analysis SHALL use the existing O(1) conservative live-buffer estimate: the greater of known file bytes and character count multiplied by four with saturating arithmetic. It MUST NOT scan or copy buffer text merely to classify size, the exact 2 MiB budget MUST remain eligible, and an estimate one byte above the budget MUST enter bounded long-line analysis when wrapping is enabled.

#### Scenario: Multibyte or untitled content exceeds the threshold
- **WHEN** a modified or untitled buffer has no sufficiently large file-size floor but its character count multiplied by four exceeds 2 MiB
- **THEN** wrapped-layout admission starts bounded long-line analysis
- **AND** it does not treat Unicode scalar count as byte count

#### Scenario: Known file size is the conservative floor
- **WHEN** known file bytes exceed the character-derived estimate
- **THEN** the known file size controls wrapped-layout admission
- **AND** the calculation remains O(1)

#### Scenario: Estimate is exactly at the threshold
- **WHEN** the conservative estimate equals the 2 MiB budget
- **THEN** the ordinary eligible path remains available
- **AND** only an estimate above the budget triggers the large-buffer analysis policy

#### Scenario: Arithmetic would overflow
- **WHEN** the character-count estimate cannot be multiplied by four without overflow
- **THEN** saturating arithmetic classifies it conservatively as large
- **AND** no text scan or allocation is introduced

#### Scenario: Wrapping is disabled or generation becomes stale
- **WHEN** wrapping is disabled, the editor is evicted, or the minimap generation changes
- **THEN** existing disabled, eviction, cancellation, and stale-result behavior remains in force
- **AND** no obsolete analysis result changes the minimap
