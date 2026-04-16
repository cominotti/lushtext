## Context

LushText now avoids creating new stale-disk empty baseline entries for
draft-restored files, but users who already accumulated those rows still see
them in the browser. The underlying history data is intentionally preserved, so
the remaining issue is presentation rather than capture or migration.

This follow-up is specifically about legacy visibility. We want the browser to
stop showing rows that are now recognized as noise, while keeping the on-disk
history intact and without hiding legitimate empty snapshots that still matter
outside the old draft-restore pattern.

## Goals / Non-Goals

**Goals:**
- Hide legacy empty baseline rows that match the stale-disk draft-restore
  pattern from the local-history browser.
- Keep raw local-history data on disk untouched.
- Preserve legitimate empty snapshots when they do not match the noisy legacy
  pattern.
- Add focused widget coverage for the filtered list behavior.

**Non-Goals:**
- Deleting or migrating existing local-history files.
- Changing current capture behavior for new snapshots.
- Hiding all empty snapshots universally.
- Introducing user-facing toggles or advanced filtering UI.

## Decisions

### 1. Filter legacy rows in the browser layer only

The browser should build a visible snapshot list from a filtered view of the
stored metadata instead of mutating the stored history document.

Rationale:
- The user explicitly chose the "hide, don't migrate" option.
- This is lower risk than rewriting stored history and keeps rollback simple.
- The noisy rows are a presentation problem now, not a persistence problem.

Alternatives considered:
- Prune matching rows on disk: rejected because it mutates historical data.
- Leave everything visible forever: rejected because the remaining rows are not
  useful to users.

### 2. Match a narrow legacy-noise pattern, not all empty baselines

The browser should hide only baseline snapshots that are empty and are part of
the known alternating stale-disk pattern left by the older draft-restore
behavior.

Implementation direction:
- Work from the newest-first metadata list already loaded for the browser.
- Suppress an empty baseline row only when nearby rows indicate the
  draft-restored stale-disk pattern rather than meaningful standalone history.
- Keep the rule conservative so uncertain cases remain visible instead of being
  hidden incorrectly.

Rationale:
- Some empty baselines are still legitimate history.
- The visible problem in the screenshot is the repetitive empty-baseline /
  non-empty-periodic alternation from the old workflow.
- A conservative filter is easier to trust than a broad "hide all empty
  baselines" rule.

Alternatives considered:
- Hide every empty baseline row: rejected because it would throw away valid
  document history from the browser.
- Ask the user every time: rejected because this is stale legacy noise, not an
  interactive decision.

### 3. Keep list and preview behavior aligned with the filtered view

If a legacy row is hidden from the list, selection and preview loading should
operate only on the visible filtered metadata sequence.

Rationale:
- The browser should not show gaps or load a hidden row by stale index.
- A single filtered backing vector keeps row indexing simple and avoids fragile
  "skip on render but not on selection" bugs.

Alternatives considered:
- Filter only row widgets while keeping the unfiltered metadata vector:
  rejected because selection and preview routing would drift.

### 4. Add widget coverage with legacy-pattern seed data

Widget tests should seed the old alternating pattern and assert that only the
useful rows remain visible.

Rationale:
- The bug is specifically about what users still see in the browser.
- A seeded legacy-pattern test makes the filter rule explicit and guards
  against accidental widening or removal later.

Alternatives considered:
- Service-only tests for the filter helper: useful but insufficient on their
  own because the regression is browser-visible.

## Risks / Trade-offs

- [The filter hides a row that some user would want] → keep the matching rule
  narrow and conservative so ambiguous cases remain visible.
- [The filter is too narrow and leaves some noisy rows visible] → acceptable for
  the first pass because false negatives are safer than false positives here.
- [List selection indexes drift after filtering] → use the filtered metadata as
  the single source of truth for both rows and preview loading.

## Migration Plan

1. Update the local-history browser requirement to allow hiding legacy noisy
   rows while preserving stored data.
2. Add the filtered browser metadata view in `ui/window/local_history.rs`.
3. Extend widget coverage with legacy-pattern seed data.
4. No data migration is required; rollback simply removes the visibility filter
   and the stored rows remain untouched throughout.

## Open Questions

- None blocking. The main implementation choice is the exact conservative match
  rule for "legacy stale-disk empty baseline noise," which can be finalized in
  code as long as it clearly targets the old draft-restore pattern rather than
  all empty snapshots.
