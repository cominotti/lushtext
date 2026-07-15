## 1. Baseline and Policy Calibration

- [x] 1.1 Record current file-load phases, cancellation points, worker/result ownership, session-restore concurrency, and direct `TextBuffer` installation timing for representative small and large Unicode files.
- [x] 1.2 Use existing benchmarks to choose and document the transient shared budget, conservative weight calculation, exclusive-oversize rule, and synchronous installation threshold with saturating arithmetic.
- [x] 1.3 Add a pure load-admission policy covering ordinary concurrency, exclusive oversize, fairness, active priority, protected over-budget state, cancellation, and permit release.

## 2. Bound Planning and Ingestion

- [x] 2.1 Split metadata/canonical-identity planning into a compact `FileLoadPlan` returned from background work before payload admission.
- [x] 2.2 Add a filesystem-boundary bounded read that enforces the supported limit plus sentinel overhead during ingestion and checks cancellation while streaming.
- [x] 2.3 Revalidate the planned stable identity/facts before accepting bytes, and return typed grown-too-large, changed, cancelled, metadata, read, and decode outcomes.
- [x] 2.4 Add service/property tests for growth after metadata, replacement/rename races, short reads, awkward encodings, exact-limit files, cancellation, and boundary compliance.

## 3. Coordinate Byte-Weighted Load Admission

- [x] 3.1 Add an app-specific process-wide runtime coordinator that queues only weak/scalar current load requests and admits payload work before generic worker dispatch.
- [x] 3.2 Retain an RAII admission token across read/decode, completed worker result, GTK installation, finalization, and every cancellation/error path.
- [x] 3.3 Remove stale/dead queued requests before admission and schedule one coalesced queue drain when capacity is released.
- [x] 3.4 Add deterministic multi-window/session-restore interleavings proving the shared budget, exclusive oversize, fair progress, exact-once release, and no worker-slot waiting.

## 4. Install Large Buffers Responsively

- [x] 4.1 Add a generation-bound chunked installation session that owns a stable end mark, retained decoded text, source ID, cancellation, and admission token.
- [x] 4.2 Keep the editor loading/non-editable and suspend minimap, history, draft, syntax, monitor, and other amplifying projections until complete installation.
- [x] 4.3 Finalize cursor/scroll, encoding, file health, language, clean history seed, monitor mtime, modified state, memory accounting, and interactivity exactly once for the current generation.
- [x] 4.4 On cancellation or failure, stop remaining slices, release decoded text and admission, avoid exposing a partially loaded document as successful, and preserve a visible retry/error path.
- [x] 4.5 Add widget tests for exact Unicode contents, cancellation between slices, close/reload, active-tab changes, failed loads, projection suppression, and GTK warning absence.

## 5. Scale Evidence and Repository Verification

- [x] 5.1 Add Criterion/policy fixtures for many small loads, concurrent large loads, one exclusive near-limit load, stale queued requests, and permit high-water accounting.
- [x] 5.2 Extend performance smoke evidence with active payload weight, queued scalar count, installation slices, main-loop progress, and final editor residency without asserting noisy RSS as a hard gate.
- [x] 5.3 Update root/nested guidance and benchmark documentation for the two-phase load/admission/install contract and any new cohesive module.
- [x] 5.4 Run focused service/property/widget tests, `make test-unit`, relevant headless runtime and responsiveness proofs, and large-file/manual smoke where automation cannot safely synthesize the maximum case.
- [x] 5.5 Run `make check`, `make lint-advisory`, `make pre-commit`, accessibility/visual-geometry/automation readiness proofs, `git diff --check`, and strict OpenSpec validation.
- [x] 5.6 Perform final architecture, performance, comment, and scoped data-safety reviews confirming `ui -> services -> model`, filesystem-boundary use, no generic worker starvation, and protected-editor invariants.
