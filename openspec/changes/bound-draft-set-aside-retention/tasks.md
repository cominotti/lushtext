## 1. Failing-first tests

- [ ] 1.1 Add a `set_aside.rs` unit test seeding 300 bodies whose newest stamps sort last in directory order. Assert that the listing shows the newest 256 and reports a total of 300 and "incomplete". Record that it FAILS on the current tree.
- [ ] 1.2 Add widget tests in `crates/lushtext/tests/widget/preferences.rs` for: summary row count/size/over-bound text; "Showing the newest 256 of N" truncation; the Delete All confirmation states the exact count and total size; cancelling it deletes nothing; confirming it removes exactly the confirmed set; a body set aside while the dialog is open survives. Record each failing on the current tree.
- [ ] 1.3 Add a widget test that seeds an over-bound set-aside area, completes startup restore, and asserts exactly one status warning with no deletion. Then activate `app.review-preserved-drafts`, and assert Preferences opens on the Data page with the group visible, that a later placement without material growth publishes no further notice in the same process, and that no file is written under the app data directory by the notice or the action. Record the failures.

## 2. Pure retention core and proofs

- [ ] 2.1 Create `crates/lushtext-core/src/services/draft_service/set_aside_retention.rs` (GTK-free, I/O-free) with `bound_status`, `notice_due`, `deletion_plan`, `UserDecision`, `ListedBody`, and fingerprint types, plus characterization unit tests. Verify: `make test-unit`.
- [ ] 2.2 Add a proptest for R1–R3 (design D4). Verify: it passes, and a deliberately broken `deletion_plan` (planning a listed body outside `confirmed`) fails it; revert the break.
- [ ] 2.3 Add `#[cfg(kani)] mod kani_proofs` with harnesses for R1–R4 over 4 bodies, and a `should_panic` harness showing that a plan deleting a body outside the confirmed set breaks R2. Assign them in `scripts/kani-shards.py`. Verify: `cargo kani -p lushtext-core` with those harnesses PROVED and the `should_panic` harness passing, and `make check-kani-shards` passes. Record times.

## 3. Service and persistence

- [ ] 3.1 Replace `set_aside::list` with the bounded single-traversal `SetAsideListing` (design D6), and update `draft_service::list_set_aside_drafts`. Verify: 1.1 passes, and existing set-aside tests pass.
- [ ] 3.2 Add `set_aside::delete_confirmed` (revalidate fingerprint, `ensure_inside`, one directory sync per batch, partial-failure count). Update the `set_aside.rs` module doc's ownership rule. Verify with unit tests, including a mismatched-fingerprint case that deletes nothing.

## 4. UI, action, and notice

- [ ] 4.1 Render the summary row and the "Delete All Preserved Drafts…" bulk action with its count-and-size confirmation in `ui/preferences/data_page.rs`, following `.agents/rules/ui.md` grouped-row and accessibility-metadata rules (via `ui/accessibility` helpers, with named labels on every button). Per-row Open and Delete stay as they are. Verify: the 1.2 tests pass.
- [ ] 4.2 Add `app.review-preserved-drafts` in `app.rs` and its `services/action_catalog` entry (command palette visible). Add the process-once over-bound evaluation after startup restore and after placements, with the in-memory rate limit (last notified totals, review-done flag) fed to `notice_due`, publishing through the active window's notification bus. Verify: the 1.3 test passes and `make check` (catalog audits) is green.
- [ ] 4.3 Run the state-extreme checks: empty, 1 body, many bodies, truncated, over bound, and constrained dialog geometry. Commands stay reachable, and only the details region scrolls.

## 5. Data-safety audit

- [ ] 5.1 Run the explicit `data-safety` skill audit over the diff. Confirm by test or code trace that no startup, bound, notice, or review-action path deletes a set-aside body; that every deletion goes through `deletion_plan` or the per-row confirmed Delete; that no new persisted file is written; and that no body text reaches logs, announcements, notifications, or automation. Fix every confirmed finding failing-first.

## 6. Documentation sync

- [ ] 6.1 Update `docs/accessibility.md` and `docs/accessibility-matrix.md` (row `A11Y-PREFERENCES-DATA-SET-ASIDE` for the summary row and the Delete All confirmation, plus a new row for the over-bound notice and the review action), and `docs/accessibility-orca-checklist.md` if the manual expectations change.
- [ ] 6.2 Update `docs/automation.md` and `docs/automation-reference.md` for `app.review-preserved-drafts`. Run `make check-automation-docs` (and `make automation-client-self-test` if the client changed).
- [ ] 6.3 Update `docs/next/formal-verification.md` (a step 8a record with the R1–R4 results, and the deferral-inventory "no size bound" item now resolved as "soft bound, no automatic deletion"), and `docs/next/formal-verification-next.md` N10.
- [ ] 6.4 Update the README features, AGENTS.md / `.claude/CLAUDE.md` (the draft persistence and set-aside notes and the module layout for `set_aside_retention.rs`), and the `WFR-DRAFT-RECOVERY` row in `docs/workflow-readability-matrix.md` if called modules changed. Run `make check-workflow-boundaries`.
- [ ] 6.5 Run the full gates: `make check`, `make test`, `make test-widget`, and the Kani harnesses from 2.3. Re-run the new widget tests 5 times in isolation.
- [ ] 6.6 Run `openspec validate bound-draft-set-aside-retention --strict` and confirm it is valid.
