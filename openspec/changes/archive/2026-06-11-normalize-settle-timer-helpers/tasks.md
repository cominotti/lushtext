## 1. Audit And Scope

- [x] 1.1 Inventory GLib one-shot timers, recurring timers, timeout futures, idle deferrals, generation counters, and `SourceId` cancellation sites under `crates/lushtext-core/src/ui` plus directly related service helpers.
- [x] 1.2 Classify each candidate as debounce, delayed settle/repair, superseding one-shot, chunked yield, heartbeat/polling, stale async freshness, pure model/domain generation, or out of scope.
- [x] 1.3 Record the audit result in the least noisy reviewable location and name the planned treatment for each candidate.
- [x] 1.4 Confirm that public GTK Lush crate APIs and family crate dependencies remain out of scope for this Phase 0 follow-up.

## 2. Private Helper Foundation

- [x] 2.1 Add a private in-tree helper module for debounce, settle-burst, and superseding one-shot scheduling.
- [x] 2.2 Split deterministic generation, staleness, and pending-state decisions from GLib scheduling where practical.
- [x] 2.3 Add unit or property tests for latest-generation wins, stale callback rejection, pending-state transitions, and dropped-target no-op behavior.
- [x] 2.4 Keep recurring pollers and lifecycle-owned `SourceId` sources explicit unless the audit documents a safe helper fit.

## 3. Low-Risk Debounce Conversions

- [x] 3.1 Convert command palette query and index-update debounce sites while preserving empty-query and pending-index behavior.
- [x] 3.2 Convert search panel query/glob debounce sites while preserving history-restore guards and immediate empty-query behavior.
- [x] 3.3 Convert notes browser and bookmark/search debounce sites while preserving live row rebuild and preview behavior.
- [x] 3.4 Add or update focused tests that prove stale input does not publish stale results.

## 4. Persistence And Refresh Conversions

- [x] 4.1 Convert session and workspace persistence debounces while preserving latest-state-wins and dirty/inflight behavior.
- [x] 4.2 Convert workspace refresh and external file monitor debounce sites while preserving selection, expansion, and reload behavior.
- [x] 4.3 Convert draft, bookmark, and local-history scheduling only where the audit proves the helper matches the existing save/capture semantics.
- [x] 4.4 Add or update tests for rapid mutation ordering, stale save rejection, and refresh coalescing.

## 5. Visual And Readiness-Sensitive Conversions

- [x] 5.1 Convert status pulse cleanup, focus-mode affordance hide, search progress visibility delay, and similar superseding one-shots.
- [x] 5.2 Convert preview render/layout settle and Markdown code-block repair only after preserving pending/readiness behavior.
- [x] 5.3 Convert minimap refresh, reflow settle, and reveal-delay scheduling while preserving `minimap-refresh` and visual readiness blockers.
- [x] 5.4 Add or update widget tests for converted visual/timed surfaces, including no-context, representative, dense or awkward, and constrained-geometry states where applicable.
- [x] 5.5 Run visual-geometry proof when converted code can affect rendered minimap, preview, adaptive layout, or status/focus pulse pixels.

## 6. Documentation And Governance

- [x] 6.1 Update `docs/next/gtk-lush.md` to reserve `normalize-settle-timer-helpers` as the missing Phase 0.3 follow-up before extraction.
- [x] 6.2 Update `.agents/rules/widget-wiring.md` and any related local guidance to describe the proven private helper pattern and exception classes.
- [x] 6.3 Keep guidance clear that `gtk-lush-settle` public functional API remains deferred to `extract-gtk-lush-signals-and-settle`.
- [x] 6.4 Update automation docs only if readiness predicates, blockers, snapshot fields, or automation-client behavior change.

## 7. Verification

- [x] 7.1 Run `openspec validate normalize-settle-timer-helpers --strict`.
- [x] 7.2 Run `openspec validate --changes --strict`.
- [x] 7.3 Run `make check`.
- [x] 7.4 Run `make check-agent-docs` after rule or guidance updates.
- [x] 7.5 Run `make test-widget-headless`.
- [x] 7.6 Run `make check-gtk-lush-policy`.
- [x] 7.7 Run `make visual-geometry-smoke` and `make check-visual-proof-policy` if visual-sensitive files changed.
- [x] 7.8 Run `make check-automation-docs` and `make automation-client-self-test` if automation contracts changed.
- [x] 7.9 Run `git diff --check`.
