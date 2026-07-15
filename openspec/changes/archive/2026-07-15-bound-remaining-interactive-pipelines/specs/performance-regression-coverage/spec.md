## ADDED Requirements

### Requirement: Remaining interactive pipelines have deterministic boundedness evidence
The project SHALL maintain runnable policy, integration, GTK/widget, and benchmark evidence for save admission, Markdown render/application and image limits, workspace-search single-flight ownership, result retirement/handoff, incremental editor-memory accounting, and lossy-encoding analysis. Evidence MUST measure the resource bound directly where possible rather than treating elapsed time alone as proof.

#### Scenario: Multi-tab save high-water evidence runs
- **WHEN** a fixture closes or saves several large modified documents
- **THEN** evidence records queued compact requests, admitted bytes, exclusive overweight behavior, and maximum simultaneously retained save payloads
- **AND** the observed high-water mark satisfies sequential close-save and byte-budget contracts

#### Scenario: Dense Markdown and image pressure evidence runs
- **WHEN** fixtures render dense event streams and more local images than the configured budgets
- **THEN** evidence records maximum events/nodes per GTK slice and outstanding image count/bytes
- **AND** stale generations and limited placeholders reach exact terminal states

#### Scenario: Rapid workspace queries run at scale
- **WHEN** a fixture supersedes slow searches repeatedly
- **THEN** evidence observes no more than one active controller/walker group and one latest compact request
- **AND** only the latest current generation can publish terminal results

#### Scenario: Large result handoff and retirement run at scale
- **WHEN** a maximum-sized accepted result set enters Replace Preview and is then superseded
- **THEN** evidence records zero whole-vector GTK clones and bounded rows/cache entries retired per slice
- **AND** preview/check/apply identities remain generation-correct

#### Scenario: Many-tab edits remain below enforcement threshold
- **WHEN** a scale fixture performs ordinary edits with many open tabs below the memory upper threshold
- **THEN** evidence records one scalar record update without a full-tab scan or candidate-vector allocation per edit
- **AND** threshold crossing still triggers deterministic current eviction policy

#### Scenario: Encoding analysis equivalence and throughput run
- **WHEN** benchmarks and property fixtures analyze lossless and lossy UTF-16, Windows-1252, and Shift_JIS inputs at representative sizes
- **THEN** optimized results remain semantically equivalent to actual encoding
- **AND** benchmark evidence records throughput without per-scalar allocation/setup amplification
