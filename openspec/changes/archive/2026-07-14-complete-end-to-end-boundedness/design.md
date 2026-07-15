## Context

The preceding portfolio established strong inner-stage contracts: byte-weighted transient file admission, chunked normal file installation, mutation-safe buffer snapshots, bounded watcher mailboxes, bounded top-k palette scoring, latest-only query ownership, conservative draft cleanup, and generation-safe recovery ordering. The remaining findings occur where a caller prepares input before those bounds, applies a result after them, or cannot resume a bounded pass beyond the same prefix.

The affected code crosses `model`, GTK-free `services`, and GTK driving adapters, but it does not justify a new crate or a generic application scheduler. The implementation must preserve the filesystem boundary, background blocking-I/O rule, GTK lifecycle ownership, durable recovery semantics, current automation/readiness contracts, and the workflow-specific generations that prevent stale state from becoming visible.

The current `finish-search-pipeline-hardening` implementation is prerequisite context. Its bounded scorer and one-active/one-latest query coordinator remain valid; this change completes source construction and canonical exclusion around them.

## Goals / Non-Goals

**Goals:**

- Make palette identity and source construction bounded before top-k scoring begins.
- Reuse one narrow editor-owned state machine for every large GTK clear/replace operation.
- Preserve draft and local-history safety while avoiding unnecessary full-body ownership.
- Make bounded orphan cleanup eventually traverse the complete draft directory across restarts.
- Bound large workspace-tree reconciliation per GTK turn without weakening generation or readiness semantics.
- Add evidence that distinguishes retained-state bounds, cancellation, per-turn work, and terminal correctness.

**Non-Goals:**

- Replacing the existing palette fuzzy scorer, transient-load coordinator, draft pipeline, watcher mailbox, or task adapter.
- Introducing a generic job framework, repository/manager traits, a new crate, or a new dependency.
- Changing user-visible search ranking, workspace source priority, normal save formats, draft body format, local-history retention policy, or file-size support thresholds.
- Moving filesystem access into GTK adapters or making LushText portals-only.
- Treating a partial buffer or tree mutation as a completed workflow merely to simplify scheduling.

## Decisions

### 1. Canonical identity is stored separately from display and activation paths

`IndexedFile` and `PaletteFileEntry` will carry a canonical identity captured through the filesystem metadata boundary in background indexing or from the editor's already-known stable file facts. The existing path remains the user-visible and activation path. File-index insertion will deduplicate canonical identities while retaining the first workspace/folder context in configured order.

Grouped search will build the exclusion set from every open-tab canonical identity before workspace scoring. The workspace scorer will accept an exclusion predicate so excluded candidates never enter its bounded heap. This preserves the requested result count and deterministic rank instead of filtering duplicates after top-k selection.

Alternative considered: canonicalize rows after scoring. Rejected because it performs blocking identity work too late, permits aliases to occupy bounded slots, and cannot repair an underfilled group without rescoring.

### 2. Palette source construction gets workflow-specific latest-only coordinators

File-index construction will use an index-specific cancellable traversal. Each directory scan will retain only the best rows that fit the remaining 100,000-entry capacity, check cancellation while visiting entries, and stop recursive expansion once capacity is filled. Canonical deduplication remains bounded by the admitted index.

Note-source construction will enforce two aggregate limits: 10,000 retained entries and 64 MiB of searchable UTF-8 note text. Existing per-sidecar recovery limits remain the first line of defense. The loader will account text before constructing duplicate row strings, stop admitting additional bodies in deterministic source order, and return typed truncation evidence alongside ordinary recovery diagnostics.

File-index and note-source refreshes will each own one active cancellation token and one latest compact request. A pending file request contains folder paths and scope identity; a pending note request contains the scope snapshot plus bounded open-editor metadata, never loaded sidecar bodies. Stale completions release their inventories without reaching GTK.

Alternative considered: reuse the query coordinator directly. Rejected because source construction has blocking I/O, different request/result shapes, and different cancellation checkpoints. The ownership pattern is reused, not forced through one generic coordinator abstraction.

### 3. One editor-owned bounded replacement session handles large clear and replace workflows

A focused editor-page module will own a single active `BufferReplacementSession`. It will reuse the calibrated normal-load policies: direct replacement only when both old and new content are small, clear at most 64 Ki characters per turn, and insert at most 256 KiB on UTF-8 boundaries per turn. The session owns its GLib source, weak editor, retained replacement text, projection-suppression guard, workflow ticket, and typed terminal callback.

Callers remain responsible for workflow semantics:

- Memory eviction revalidates eligibility and marks the editor evicted only after clear-only completion.
- Draft recovery retains its full restore ticket and publishes restored/modified state only after complete installation.
- Local-history restore and undo retain history/path generations and create the required reversible baseline before mutation.
- Save-time formatting retains save/path/load/edit generations and publishes formatted live state only for the accepted save.

The editor stays non-editable and the owning window treats it as non-saveable while a replacement is partial. Syntax, minimap, history, draft, monitor, cursor, modified state, memory accounting, and readiness finalization occur once at the terminal callback rather than on every slice. Disposal or a stale ticket cancels remaining work and releases ownership exactly once.

Alternative considered: call `set_text()` from one idle callback per workflow. Rejected because it bounds workflow count but not document-sized GTK work; repository calibration already shows a visible pause at 16 MiB. Alternative considered: a process-wide mutation scheduler. Rejected because editor ownership and workflow-specific generations are the important safety boundary.

### 4. Draft restore classifies and transfers incoming ownership before GTK mutation

Draft recovery will classify local-history availability from incoming UTF-8 byte size rather than the old buffer's file size. Bodies in the SaveOnly or unavailable history tiers will not be cloned for an automatic baseline. When an eligible incoming body must seed history and remain available for installation, the path will use transferable/shared immutable ownership rather than cloning a second full `String` merely to satisfy two callbacks.

The replacement session owns the installation body until terminal completion. Draft dirty/restored flags, whole-buffer modified marks, minimap refresh, and inline feedback stay deferred. This preserves the invariant that partial recovery is neither user-editable nor accepted as protected work.

Alternative considered: lower the 64 MiB automatic draft limit. Rejected because it changes an established recovery promise without addressing the synchronous GTK problem shared by history, eviction, and save formatting.

### 5. Draft cleanup uses a durable lexicographic continuation

The draft manifest payload will gain optional cleanup-continuation metadata with Serde defaults, preserving the public v1 envelope and backward compatibility. The continuation records the last completed filename boundary and whether a wraparound cycle remains pending; it does not identify private content or authorize deletion.

The filesystem service will provide a bounded page scan that visits directory entries off GTK, retains at most the next configured page in lexicographic order after the cursor, and reports whether entries remain. Persistable native directory offsets are not portable or stable across restart, so a filename boundary is used even though selecting a page may scan the directory with bounded memory.

Inspection still produces inode/stable-target evidence, and execution still reloads the complete latest manifest under the process-wide write lock before mutation. Continuation advances durably only with the cleanup outcome's accepted manifest update. Insertions or renames before the cursor are reconsidered after wraparound; failures remain retained and are reconsidered in a later cycle rather than stalling all later entries. Untrusted manifest recovery disables cleanup and continuation use.

Alternative considered: repeatedly restart at the first 2,048 entries. Rejected because it cannot guarantee coverage. Alternative considered: persist an OS directory stream offset. Rejected because it is not a stable cross-process identity. Alternative considered: keep an unbounded set of visited filenames. Rejected because cleanup itself must remain bounded.

### 6. Tree refresh plans from a plain mirror and applies bounded GTK batches

Each materialized child store will retain a bounded plain mirror of row identity and projection-relevant state. A refresh worker can compare the mirror with newly scanned rows and return a compact reconciliation plan. GTK applies large replacement ranges in the existing 256-row batch cadence, checking scan generation, section lifetime, and store ownership before every slice.

Selection and expansion identities are captured before the first mutation. Row caches, surviving state restoration, watcher-target publication, and `workspace-refresh-complete` readiness finalize once after the last current batch. A superseded plan stops without announcing completion. Small changed ranges retain the direct path after benchmark calibration.

Alternative considered: build a complete shadow GObject model and swap it at once. Rejected because GObject construction still belongs to GTK, a wholesale swap weakens visible row identity, and it discards the current bounded-splice behavior.

### 7. Compatibility-loader documentation follows observable recovery behavior

The legacy `session_service::load` and `draft_service::load_manifest` wrappers recover through their diagnostic-aware loaders and return default/repaired values rather than propagating read/parse errors. Their `# Errors` documentation will be removed or rewritten to describe only errors that can actually escape. No behavior change or new strict loader is introduced unless a current caller demonstrably needs one.

## Risks / Trade-offs

- **Canonicalization can fail for a transient file** → Keep a typed unavailable-identity outcome, skip unsafe cross-source deduplication for that candidate, and preserve deterministic diagnostics rather than blocking GTK or inventing identity.
- **A 64 MiB note budget can truncate a legitimate very large corpus** → Use deterministic ordering, visible diagnostics, documented constants, and benchmarks; explicit activation targets for admitted rows remain intact.
- **Multi-turn replacement creates a transient partial GTK buffer** → Make the editor non-editable/non-saveable, suppress projections, and expose completion only through the terminal callback. Cancellation is limited to stale/disposed workflows where partial content cannot become accepted user state.
- **Persisting cleanup continuation adds manifest metadata** → Use an optional backward-compatible field under the existing envelope, include it in durable manifest ordering, and preserve it across autosave updates.
- **Lexicographic cleanup paging may rescan a huge directory** → Keep all scanning off GTK, bound retained memory, delay passes, and benchmark page selection. Correct eventual coverage is preferred over unstable native offsets.
- **Batched tree splices may show an in-progress list** → Preserve interaction, suppress completion/readiness until the final current batch, and restore selection/expansion once rather than after every slice.
- **One umbrella change touches several workflows** → Keep implementation commits and task groups ordered by shared foundation, with focused tests after each group and no cross-domain generic abstraction.

## Migration Plan

1. Reconcile and test canonical palette identity before archiving `finish-search-pipeline-hardening`.
2. Add GTK-free palette inventory policies/coordinators and their tests before wiring window refreshes.
3. Add the editor replacement session and prove its terminal paths before migrating eviction, recovery, history, and save formatting one workflow at a time.
4. Add backward-compatible cleanup continuation, service tests, and recovery-format fixtures before enabling deferred restart resume.
5. Add the plain tree mirror and batched reconciliation behind current generation/readiness ownership.
6. Refresh benchmark evidence, widget/property tests, automation/readiness documentation if observable fields change, and proof artifacts required by source fingerprints.

Rollback is code-local: retain compatibility for manifests without continuation metadata, keep small direct GTK paths, and revert individual caller adoption without changing persisted draft bodies or user documents. A rollback MUST ignore but preserve safe optional continuation state rather than interpreting it as cleanup authority.

## Open Questions

None. Exact synchronous tree-reconciliation thresholds may be calibrated during implementation, but the bounded large-path contract and existing 256-row batch size are fixed by this design.
