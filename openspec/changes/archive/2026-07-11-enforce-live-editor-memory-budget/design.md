## Context

The window currently has a 256 MiB aggregate editor-memory policy and can evict clean, file-backed pages. The estimate is dominated by `file_size`, so an untitled buffer contributes zero and a loaded file that grows after editing remains close to its old on-disk size. Budget enforcement also runs mainly around selection and lifecycle events rather than when a live buffer changes. GTK text must remain on the main thread, while eviction must preserve every unsaved or in-flight state.

## Goals / Non-Goals

**Goals:**

- Make the aggregate estimate conservatively reflect each loaded buffer's current size without copying buffer text on edits.
- Re-run one coalesced eviction policy when live residency can change.
- Evict only clean, recoverable, inactive pages and reject stale eviction decisions.
- Define stable behavior when protected documents alone exceed the budget.
- Keep accounting and policy independently testable as plain Rust calculations.

**Non-Goals:**

- Measuring GTK allocator residency exactly or promising that process RSS never exceeds 256 MiB.
- Evicting modified, untitled, saving, loading, active, or otherwise non-reloadable documents.
- Changing large-file open thresholds, session format, or the shipped 256 MiB budget.
- Adding a generic cache framework or a new crate.

## Decisions

### Use a conservative O(1) live estimate

Each editor will expose `estimated_live_buffer_bytes()` from GTK's constant-time character count using a saturating four-byte UTF-8 upper bound. For a clean loaded file, accounting uses the maximum of that live bound and known file bytes; unloaded/evicted pages contribute only their small fixed bookkeeping estimate. This can overestimate ASCII text, but it cannot miss untitled content or multibyte growth and never copies the buffer merely to count bytes.

Alternatives considered:

- Tracking exact inserted/deleted UTF-8 byte deltas was rejected because large deletions require reading the deleted range and signal ordering makes correctness fragile.
- Sampling process RSS was rejected because it cannot attribute memory to editors or drive deterministic eviction tests.
- Continuing to use file size was rejected because it is stale after edits and absent for untitled documents.

### Coalesce policy evaluation at the window boundary

Editor pages will notify the window when their estimate or eviction eligibility may have changed: accepted load, text-length change, modified-state change, save start/finish, path adoption, activation, eviction/reload, and close. The window will arm one main-loop coalescing callback rather than evaluate all tabs for every keystroke. That callback snapshots small scalar page facts and passes them to a plain Rust policy function.

The policy sorts eligible inactive pages by least-recent use. If total estimated residency exceeds 256 MiB, it selects enough pages to reach a 90% low-water mark. Before applying each decision, the window rechecks page identity, active state, modified state, save/load state, path, and generation.

Alternatives considered:

- A timer-only sweep was rejected because the advertised limit could remain stale for a long editing burst.
- Evicting immediately in every buffer signal was rejected because it would repeatedly scan tabs and could churn around the threshold.

### Treat the budget as soft for protected work

The active editor and any modified, untitled, loading, saving, failed-load, or otherwise non-reloadable editor are ineligible. If their aggregate exceeds the budget, the policy records an over-budget/protected state and stops; it never discards content or loops trying the same candidates. A later eligibility or size transition re-arms evaluation. Logging and test-only policy inspection provide evidence without introducing a noisy user warning for normal protected work.

Alternatives considered:

- Forcing protected pages to disk or drafts was rejected because editor eviction must not become a hidden persistence workflow.
- Raising the budget dynamically was rejected because it would make the limit meaningless and harder to test.

### Keep recency and generations explicit

Each successfully activated or reloaded page receives a monotonically increasing access generation. Each residency/eligibility change advances a policy generation. Selected eviction candidates carry both values, and a candidate is ignored if either page identity or relevant generation has changed before application. This prevents delayed callbacks from evicting a newly active or newly modified page.

## Risks / Trade-offs

- [The four-byte upper bound may evict ASCII-heavy clean tabs earlier than strictly necessary] → Keep the budget soft, use a low-water mark to avoid repeated work, and benchmark representative ASCII and Unicode sessions before considering a more complex exact counter.
- [Coalesced evaluation may briefly exceed the budget] → Run on the next main-loop opportunity after every material transition and avoid any blocking work in the callback.
- [New edit-state notifications may form signal cycles] → Centralize arming in one idempotent window method and cover one-edit/one-evaluation behavior.
- [Eviction could race save, restore, or activation] → Recheck eligibility and generations immediately before invoking the existing eviction path.

## Migration Plan

1. Extract a pure aggregate-budget policy with fixtures for eligibility, recency, hysteresis, and protected over-budget states.
2. Add conservative live estimates and policy-generation notifications to editor pages.
3. Route existing load/selection eviction triggers through the coalesced window evaluator.
4. Add delayed session-restore and stale-callback integration coverage, then remove obsolete file-size-only helpers.
5. Roll back by disabling the coalesced trigger and retaining the existing eviction call sites; no persisted data migration is involved.

## Open Questions

None. Exact-RSS measurement remains intentionally outside this capability unless later profiling shows the conservative estimate is too costly in practice.
