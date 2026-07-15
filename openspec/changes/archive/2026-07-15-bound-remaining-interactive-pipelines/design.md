## Context

LushText now has strong durable-write, freshness, chunked-buffer, transient-load, watcher, palette, and recovery contracts. The remaining review findings are not evidence of a missing architectural layer; they are places where otherwise-correct workflows acquire large payloads too early, allow superseded worker groups to overlap, or perform scale-dependent GTK work without a terminally owned sliced session.

The affected paths share a lifecycle even though their domain semantics differ:

1. a compact request is accepted on GTK;
2. expensive or document-sized work is admitted by a domain policy;
3. payload ownership remains charged until consumption or discard;
4. GTK projection happens in bounded slices where necessary;
5. a generation/lifetime check owns every completion and terminal state.

The design keeps those lifecycle rules explicit in each owning workflow. It does not introduce a universal job manager or hide save, search, preview, and eviction semantics behind one generic abstraction.

## Goals / Non-Goals

**Goals:**

- Bound retained save payloads before buffer snapshotting, including multi-tab window close.
- Bound Markdown preview parsing, GTK projection, and embedded-image work by source, event, embed, time/slice, and generation.
- Limit workspace content search to one active worker group plus one latest compact request.
- Remove whole-result cloning and one-turn teardown from search/Replace Preview.
- Make normal editor-residency updates constant-time and defer full tab walks until enforcement is necessary.
- Preserve exact lossy-encoding diagnostics while materially reducing repeated setup and allocation.
- Provide deterministic evidence for payload high-water marks, worker overlap, GTK slice bounds, stale completion rejection, semantic equivalence, and throughput.
- Preserve all existing data-safety, close, recovery, readiness, accessibility, and filesystem-boundary contracts.

**Non-Goals:**

- Replacing `gtk-lush-tasks`, GLib sources, or domain-specific generation checks with a new general scheduler.
- Changing file formats, workspace-search semantics, Markdown syntax support, encoding choices, or the 256 MiB live-editor budget.
- Evicting protected editor state or weakening durable-write and draft recovery guarantees to meet a memory target.
- Adding dependencies, crates, or GTK Lush public APIs unless implementation demonstrates a small reusable primitive with a second consumer and governance evidence.
- Treating documented best-effort detached temporary-file cleanup as a defect.

## Decisions

### 1. Admit save payloads before snapshot capture

Introduce a plain-Rust byte-weighted save-admission policy and a process-owned GTK adapter. A request contains only weak editor identity, save/close priority, generation, destination identity, and a conservative charge derived from constant-time live-buffer residency plus worst-case snapshot, line-ending, normalization, encoding, and writer staging overhead. No complete `String`, encoded byte vector, or GTK object is retained by a queued request.

An admission permit is acquired before direct or chunked snapshot capture starts and remains owned until encoded content has been consumed by the durable writer or the operation is cancelled/discarded. Ordinary saves may use the available bounded concurrency; an overweight supported save runs exclusively. Close-triggered batch saves are additionally sequenced one editor at a time so a later selected tab cannot snapshot while the preceding save still owns its payload. Close completion, tab destruction, and draft cleanup occur only after every selected save reaches its existing successful terminal state.

Queued requests are revalidated against editor lifetime, save generation, modified state, destination/path identity, and close-session identity before admission. A stale request consumes no worker or payload capacity. Close cancellation or save failure leaves remaining tabs open and recoverable under the existing contract.

Alternative considered: rely on the fixed `gtk-lush-tasks` worker count. Rejected because queued closures can already own complete document payloads, so worker concurrency alone does not bound retained bytes.

### 2. Represent Markdown preview as a generation-owned bounded session

Preview refresh first obtains text using the existing live-size direct/chunked snapshot policy. A worker parses a GTK-free render plan subject to explicit source-byte, emitted-event, structural-node, and local-image descriptor budgets. A budget terminal produces a deterministic limited-state plan rather than an unbounded partial object graph.

GTK applies the accepted plan in time/row-bounded idle slices. The render generation exclusively owns its plan cursor, inserted widgets/tags, placeholder state, and outstanding image descriptors. Any edit, mode switch, page close, or newer preview invalidates the session and prevents later slices or image completions from projecting.

Local images use a small byte/count-weighted admission lane owned by the current render generation. Excess or oversized embeds become accessible placeholders. Descriptor admission occurs lazily near projection instead of enqueuing all embeds during parse. Terminal readiness distinguishes parsing, applying, image work, limited, complete, cancelled, and failed states exactly.

Alternative considered: raise the existing large-buffer pause threshold. Rejected because dense Markdown and many embeds below a byte threshold can still generate disproportionate GTK work and queued payloads.

### 3. Make workspace search single-flight and keep results immutable by generation

The search panel owns at most one active controller/walker group and one latest pending `SearchRequest`. A newer query cancels the active token and replaces the pending compact request, but it does not start another worker group until the active result channel reaches a disconnected/terminal state. The pending request is revalidated against panel lifetime and query generation before launch.

Accepted matches are sealed once into `Arc<[SearchMatch]>` (or an equivalent immutable shared snapshot). List projection, Replace Preview, checked-row identity, and apply planning reference that generation-owned snapshot rather than cloning the full vector on GTK. Stable match IDs remain the authority; omitted/unchecked preview rows cannot become implicitly applicable.

Alternative considered: keep overlapping cancellation and reduce ripgrep walker threads. Rejected because cancellation observation latency still permits an unbounded number of controller groups under rapid typing.

### 4. Retire large search projections through bounded disposal sessions

Clearing or replacing a large result generation detaches it from current visible/search state immediately, then removes list rows and auxiliary cache entries in bounded idle slices. A disposal session owns the retired generation and cannot clear entries belonging to a newer generation. Starting another search may supersede visible state without synchronously draining the old model.

Replace Preview construction receives the shared immutable match snapshot on the worker path. Its current preview/check/apply generation and byte/row caps remain unchanged.

Alternative considered: replace `remove_all()` with a differently implemented GTK model. Rejected as unnecessary; bounded retirement preserves current model behavior and keeps the change local.

### 5. Maintain editor residency incrementally

Each editor publishes a scalar residency record keyed by stable page identity. The window keeps aggregate loaded bytes and eligibility-relevant uncertainty using saturating delta updates on accepted load, edit, save, restore, clear/evict, reload, attach, detach, and destruction transitions. A normal edit below the upper threshold updates one record and the aggregate without allocating or walking all tabs.

The window builds and sorts a full eviction snapshot only when aggregate residency crosses the upper threshold, an enforcement session is already active, or attach/detach/stale-state detection marks accounting uncertain. Candidate freshness and eligibility are still revalidated immediately before and during bounded clear. Periodic/debug reconciliation may assert the incremental aggregate against a full scan without becoming an ordinary input-path cost.

Alternative considered: only debounce the current full scan more aggressively. Rejected because the cost remains proportional to open tabs and retains avoidable allocation on routine edits.

### 6. Analyze encoding representability with one exact pass

Move representability analysis behind a GTK-free analyzer that reuses encoder/scratch state for the whole string and records only total issue count plus the first eight source positions. UTF-8 and UTF-16 selections return lossless immediately because every valid Rust `char` is representable. Windows-1252 and Shift_JIS use an exact no-replacement/representability path without allocating a temporary `String` or constructing an encoder for each scalar.

Line, column, and Unicode-scalar diagnostics are derived from the original input in the same pass and must remain byte-for-byte equivalent to the existing public result for all inputs. The save encoder remains the final authority; property/equivalence tests compare analysis against actual no-replacement encoding behavior.

Alternative considered: cache only the previous analysis. Rejected because it does not fix the fundamental per-scalar setup cost and becomes fragile across edits and encoding changes.

### 7. Treat evidence as part of the implementation contract

Plain policy tests cover admission, latest-wins, budgets, and accounting arithmetic. Service tests cover search/encoding equivalence and cancellation. GTK integration/widget tests cover sliced progress, lifecycle invalidation, close ordering, protected data, and terminal readiness. Benchmarks record save high-water payload ownership, dense Markdown plan/apply cost, rapid-query maximum worker overlap, 10,000-result handoff/retirement, many-tab below-threshold edit cost, and lossless/lossy encoding throughput.

Evidence must report measured counters rather than infer bounds from elapsed time alone: admitted bytes, queued compact requests, active worker groups, rows/events applied per slice, outstanding image payloads, full memory-policy scans, and copied match payloads.

## Risks / Trade-offs

- **Save admission can delay an urgent close save behind an already admitted load or save** -> Give close saves the highest pending priority, never revoke an active payload unsafely, surface progress, and test eventual admission and cancellation.
- **Conservative save weights may reduce concurrency more than necessary** -> Centralize documented saturating estimates, record high-water counters, and tune only from reproducible evidence without weakening exclusivity for overweight payloads.
- **Sliced preview/search teardown can expose partially built or retiring state** -> Use explicit pending/limited/terminal UI states, detach retired generations immediately, and make readiness include every owned slice/source.
- **Single-flight search may make a replacement query wait for slow cancellation** -> Keep cancellation checks at traversal/stream boundaries, drain/disconnect promptly, retain only the latest query, and expose cancelling versus searching readiness.
- **Incremental residency can drift after an unmodeled lifecycle transition** -> Funnel state changes through one record update API, mark uncertainty on exceptional paths, and reconcile with debug/property/widget coverage before enforcement.
- **Optimized encoding analysis could misreport a rare mapping** -> Require exhaustive codec-boundary fixtures plus property equivalence against actual no-replacement encoding before replacing the current implementation.
- **One umbrella change has a broad implementation surface** -> Land in ordered vertical slices with independent tests, keep capability boundaries explicit, and require the full integrated proof matrix before archive.

## Migration Plan

1. Add plain policy/value types and deterministic tests without changing runtime call sites.
2. Integrate save admission and sequential close saves, then prove failure/recovery ordering before proceeding.
3. Introduce Markdown plan/session ownership and bounded image admission behind the existing preview surface.
4. Convert search launch, immutable result ownership, preview handoff, and sliced retirement as one generation-coherent slice.
5. Replace full-walk residency updates with incremental records plus reconciliation assertions.
6. Swap in the exact encoding analyzer after equivalence tests pass.
7. Run the full unit, integration, GTK/widget, accessibility/visual policy, benchmark, strict OpenSpec, and agent-doc checks required by the touched surfaces; refresh benchmark evidence and any durable architecture guidance.

Each runtime slice can be reverted independently to its prior implementation because no persistent format changes are introduced. Reversion must retain any newly discovered safety test and must not bypass existing durable-write or recovery behavior.

## Open Questions

None. Exact numeric slice sizes and conservative charge multipliers are implementation constants to be selected from existing thresholds and recorded benchmark evidence; changing them does not alter the capability contract as long as the stated bounds remain deterministic.
