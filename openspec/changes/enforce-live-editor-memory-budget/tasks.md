## 1. Policy and Accounting Model

- [ ] 1.1 Add named 256 MiB upper-budget and 90% lower-water policy constants plus plain Rust editor-residency and eviction-candidate value objects.
- [ ] 1.2 Implement the pure aggregate policy for saturating totals, protected pages, least-recent use, lower-water convergence, and no-progress outcomes.
- [ ] 1.3 Add pure policy tests for zero/one/many pages, exact thresholds, recency ties, saturation, insufficient candidates, and protected over-budget states.

## 2. Live Editor Estimates

- [ ] 2.1 Add a conservative O(1) live-buffer estimate to `LushtextEditorPage` using current character count, known file bytes, and explicit evicted-state accounting.
- [ ] 2.2 Add access and residency/eligibility generations that advance on accepted load, edit-length, modified, save, path, activation, eviction/reload, and close transitions.
- [ ] 2.3 Route estimate-change notifications through narrowly scoped editor/window callbacks and document why no buffer text is copied for accounting.
- [ ] 2.4 Add editor tests for untitled text, multibyte text, growth beyond file size, save transitions, eviction, and saturating estimates.

## 3. Reactive Safe Eviction

- [ ] 3.1 Add one idempotent/coalesced window budget evaluator and route existing selection/load eviction triggers through it.
- [ ] 3.2 Snapshot scalar page facts, run the pure policy, and revalidate identity, generations, active state, modification, save/load state, path, and reloadability before each eviction.
- [ ] 3.3 Preserve active, modified, untitled, saving, loading, failed-load, and otherwise non-recoverable pages as ineligible and record a stable soft-budget outcome when they alone exceed the limit.
- [ ] 3.4 Ensure later size or eligibility changes re-arm evaluation without repeated no-progress loops or threshold churn.

## 4. Verification and Scale Evidence

- [ ] 4.1 Add window integration tests for unsaved growth crossing the budget, burst coalescing, active-tab changes, save races, and least-recent eviction to the lower watermark.
- [ ] 4.2 Add delayed/out-of-order session-restore tests proving stale load completions cannot evict active or modified pages.
- [ ] 4.3 Add scale/benchmark coverage for many clean tabs, many protected tabs, ASCII and Unicode estimates, and one-edit/one-coalesced-evaluation behavior.
- [ ] 4.4 Run formatting, Rust comments/architecture review, focused tests, `make check`, `make lint-advisory`, and `make pre-commit`; fix every issue found.
- [ ] 4.5 Run the learning workflow and update architecture or agent guidance only if implementation reveals a durable new contract.
