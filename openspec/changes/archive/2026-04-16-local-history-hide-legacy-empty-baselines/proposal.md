## Why

LushText now captures new draft-restored history correctly, but older local
history timelines can still contain stale `Before edits · Empty file` rows left
behind by the previous behavior. Those entries remain on disk and continue to
appear in the browser even though they are not useful from the user's point of
view.

This should be fixed now because the product already decided not to create those
rows going forward. The remaining UX problem is legacy visibility: users should
not keep seeing noisy stale-disk baseline rows just because the old data still
exists on disk.

## What Changes

- Hide legacy empty baseline rows from the local-history browser when they match
  the known stale-disk draft-restore pattern.
- Preserve raw history on disk; this is a browser-filtering change, not a data
  migration or deletion pass.
- Keep legitimate empty snapshots visible when they do not match the legacy
  stale-baseline pattern.
- Update the local-history product note and living spec so the browser contract
  explicitly allows filtering legacy noisy rows from view.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `local-history`: The browser now filters legacy stale-disk empty baseline rows
  that are not meaningful user history, while keeping the underlying stored
  data intact.

## Impact

- Affected code: `crates/lushtext-core/src/ui/window/local_history.rs` and the
  widget tests covering local-history list behavior.
- Affected systems: local-history list presentation, legacy-history filtering,
  and browser-visible timeline semantics.
- Affected docs: `docs/next/session-time-travel.md` and
  `openspec/specs/local-history/spec.md` should describe the visibility filter
  explicitly.
- Dependencies and APIs: no new dependency is expected; the change stays inside
  the existing browser presentation layer.
