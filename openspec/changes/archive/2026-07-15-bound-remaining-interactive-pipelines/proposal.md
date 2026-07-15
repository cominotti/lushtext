## Why

The recent boundedness and safety changes materially improved LushText, but a final set of user-triggered pipelines can still retain document-sized payloads before admission, overlap superseded workers, or perform scale-dependent GTK work in one turn. Closing these gaps now gives the application one consistent bounded-work contract across saving, Markdown preview, workspace search, editor residency, and encoding analysis without reopening the architecture broadly.

## What Changes

- Admit close-triggered and ordinary saves by conservative byte weight before capturing a complete document snapshot; keep queued work compact, serialize the multi-tab close flow, and preserve dirty state, Save As identity, durability warnings, draft recovery, and close ordering on every failure path.
- Split Markdown preview into a GTK-free bounded render plan and generation-owned GTK application slices, with explicit source/event/embed budgets, bounded image admission, placeholders for excess embeds, and stale-completion rejection.
- Make workspace content search single-flight with one active worker group and one latest compact request, while retaining cancellation, bounded result streaming, and exact readiness semantics.
- Share accepted search matches with Replace Preview instead of cloning the full result set on GTK, and retire large result models and caches in generation-safe bounded slices.
- Make live editor-memory accounting incremental so ordinary edits update scalar aggregate state in constant time; build the full eviction candidate snapshot only after a threshold crossing or an uncertain lifecycle transition.
- Replace per-scalar lossy-encoding probes with an exact reusable analysis path, including an immediate lossless result for UTF-16, while preserving counts and diagnostic positions.
- Add deterministic policy, integration, widget, benchmark, and high-water evidence for the new concurrency, memory, responsiveness, freshness, and equivalence guarantees.
- Do not add a new generic scheduler, repository layer, crate, or dependency; extend the existing UI/service/model boundaries and GTK Lush task/settle primitives only where their current contracts fit.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `document-save-safety`: Saves, including multi-tab close saves, gain pre-snapshot byte-weighted admission and ordered close completion without weakening existing durability or recovery behavior.
- `main-thread-responsiveness`: Markdown rendering, search replacement handoff/teardown, save snapshot admission, and encoding analysis gain explicit bounded-turn and stale-result contracts.
- `live-editor-memory-budget`: Residency accounting becomes incrementally maintained and save payloads join process-wide byte-weighted transient admission.
- `search-replace-safety`: Workspace search becomes single-flight and accepted search generations remain identity-stable through preview, teardown, and apply.
- `encoding-toolkit`: Lossy-save analysis must remain exact while avoiding per-scalar allocation/encoder setup and recognizing Unicode encodings that represent every Rust scalar.
- `performance-regression-coverage`: The repository gains repeatable latency, retained-payload, worker-overlap, and throughput evidence for the remaining interactive pipelines.

## Impact

The change primarily affects `ui/window/dialogs.rs`, `ui/editor_page/load_save.rs` and save runtime state, `ui/markdown_preview/`, `ui/search_panel/`, editor-memory accounting in `ui/window/` and `ui/editor_page/`, `services/editor_io.rs`, their GTK-free policy models, tests, benchmarks, and benchmark evidence. Public application behavior and file formats remain compatible; no dependency or GTK Lush public-API change is expected unless implementation proves a narrowly reusable admission primitive is required.
