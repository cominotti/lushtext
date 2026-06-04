## Why

The rustix filesystem boundary is now adopted, but a few small polish items still make the codebase look slightly less settled than the contract it now enforces. This change cleans up those leftovers so tests, sidecar workflows, audit coverage, and guidance all teach the same boundary shape.

## What Changes

- Replace remaining test-only full-facts existence assertions with lightweight filesystem status helpers where no canonical path, size, or mtime facts are needed.
- Review the repeated sidecar scan/delete/migration scaffolding in bookmark, document-note, workspace-note, and local-history workflows, then either extract a tiny shared helper where it improves cohesion or explicitly keep the current workflow-specific helpers if that remains clearer.
- Tighten deterministic audit coverage so status-probe drift in tests and any new sidecar helper surface cannot linger unnoticed.
- Refresh guidance only where it materially clarifies the final boundary polish, without re-opening the completed rustix backend migration.
- Preserve all existing runtime behavior for saves, Replace All, notes, bookmarks, local history, drafts, JSON persistence, workspace scanning, and content search.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `internal-filesystem-abstractions`: Tighten the cleanup contract so tests prefer lightweight status helpers when they only assert existence, sidecar helper reuse/removal remains intentional, and audit tooling catches those polish-level leftovers.

## Impact

- Affected code: filesystem-boundary tests and assertions, note/bookmark/local-history sidecar services, `services::filesystem` helper surface if a small shared helper is justified, and `scripts/check-filesystem-boundary.sh`.
- Affected guidance: root or nested guidance and filesystem-sensitive skills only if they need narrow updates for the cleanup rules.
- Dependencies: none expected.
- Behavior: no user-facing behavior changes are intended; this is an architecture-polish and no-leftovers follow-up.
