## Context

The `add-property-based-testing` change introduced a feature-gated property
target for pure deterministic LushText logic. Its first coverage slice focused
on inline footnote lowering, Replace All forward text transformation, path
rebasing, palette merge ordering, and encoding/sidecar identity helpers.

The next coverage gaps are still deterministic and cheap: EditorConfig
save-formatting idempotence, session/draft serialization round-trips, and the
successful Replace All -> Undo restore path. These are already covered by
examples, but property tests can exercise a broader input space without adding
GTK, watcher, file chooser, portal, or live session behavior.

## Goals / Non-Goals

**Goals:**

- Add bounded property coverage for save-formatting idempotence.
- Add bounded property coverage for `SessionData`, `SessionTab`,
  `DraftManifest`, and `DraftEntry` serialization round-trips.
- Add a bounded deterministic service property for Replace All followed by Undo
  restoring byte-identical content.
- Keep default nextest and default mutation runs separate from property tests.
- Update docs and agent rules so the expanded property scope is clear.

**Non-Goals:**

- Do not add fuzzing or hostile byte-ingestion coverage in this change.
- Do not add GTK widget, compositor, D-Bus, portal, file chooser, watcher, or
  live session property tests.
- Do not make property tests part of default mutation testing.
- Do not change user-facing save, session, draft, or Replace All behavior.

## Decisions

1. Keep the tests in the existing `lushtext-core` property target.

   The existing `property-tests` feature and `properties` target already solve
   the runtime boundary. Adding modules under `crates/lushtext-core/tests/properties/`
   keeps these invariants visible in the same lane and avoids a second
   generated-input test target.

2. Treat EditorConfig formatting as a pure idempotence property.

   `apply_save_formatting_overrides()` is pure string processing. The property
   should generate bounded text with spaces, tabs, LF, CRLF, CR, empty input,
   and mixed line endings, then assert that applying the same overrides twice is
   identical to applying them once.

3. Generate model-level session and draft values, not full app sessions.

   The useful invariant is serde stability for bounded persisted shapes:
   optional file paths, optional draft IDs, cursor/scroll values, pinned tabs,
   active-tab indices, draft mtimes, and saved timestamps. The test should avoid
   window construction and should not depend on the startup restore workflow.

4. Allow tiny deterministic tempdir-backed service properties.

   Replace All undo writes files by design: `undo_replacements()` validates that
   current bytes still match the replacement snapshot before restoring original
   bytes. A tempdir-backed property over one to a few tiny files exercises the
   real command/undo path while staying deterministic, bounded, and independent
   of GTK or live session state.

5. Keep generated inputs compact.

   The current property helpers cap strings, paths, vectors, cases, shrinking,
   and per-case timeouts. New generators should reuse those caps or add nearby
   bounds rather than increasing the global default case count.

## Risks / Trade-offs

- [Risk] Tempdir-backed Replace All properties become slow or flaky.
  -> Mitigation: cap file count, line count, text size, and replacement count;
  use only local tempdirs and avoid watcher/session/UI code.

- [Risk] Session/draft generators create invalid shapes that encode no useful
  product invariant.
  -> Mitigation: allow edge cases such as out-of-range active indices only when
  existing serialization examples already preserve them; keep assertions focused
  on round-trip stability, not semantic cleanup.

- [Risk] Docs drift and future agents put broad workflows into properties.
  -> Mitigation: update `docs/property-testing.md`, `.agents/rules/build.md`,
  and `gtk-testing` guidance with the new deterministic-tempdir boundary.

- [Risk] Property failures from intentionally lossy generators produce noisy
  regression seeds.
  -> Mitigation: tighten generators before committing `properties.txt` when a
  failure is a bad model rather than a product bug.
