## Why

LushText currently gives users two extremes for recovering earlier document state: short-lived in-session undo and crash/session draft recovery. There is still no deliberate, user-visible way to browse prior versions of a saved document from the current editing session and recover one without reaching for external tools such as git.

This is the right time to add a narrow local-history MVP because the app already has robust async persistence, draft/session restore, large-file policy, and rename-aware sidecar patterns. The missing piece is a GTK-native recovery surface that makes those strengths visible and trustworthy without overreaching into a full compare or version-control workflow.

## What Changes

- Add an MVP local-history workflow for saved, file-backed documents that automatically records restore points and lets users browse and restore earlier text snapshots.
- Present local history in an adaptive, GTK-native dialog that uses a snapshot list plus a read-only preview, rather than adding another always-visible pane or a diff-heavy first release.
- Make local history easier to reach by adding a keyboard shortcut plus context-menu entry points in both the sidebar and the editor content surface for eligible saved files.
- Keep restore safe and reversible by taking a safety snapshot before applying a historical snapshot, marking the editor modified after restore, and surfacing an immediate undo path.
- Reuse stable canonical-path identity and in-app rename migration patterns so history follows sidebar renames but intentionally starts a new lineage after Save As.
- Reuse existing large-file degradation policy so history capture and preview stay conservative for very large files.
- Tighten the dialog layout so the preview surface has deliberate inner spacing, and codify that spacing rule as a permanent UI contract instead of a one-off local-history tweak.
- Document explicit follow-ups outside the MVP, including diff/compare UI, untitled-document history, workspace-wide history browsing, richer retention controls, and optional timeline metadata polish.

## Capabilities

### New Capabilities
- `local-history`: Automatic per-document snapshot capture and a GTK-native browse-and-restore workflow for saved files.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/model/sidecar_identity.rs`, new local-history model/service/UI modules, `crates/lushtext-core/src/ui/window/actions.rs`, `crates/lushtext-core/src/ui/window/documents.rs`, `crates/lushtext-core/src/ui/window/session_persistence.rs`, `crates/lushtext-core/src/ui/window/drafts.rs`, and related tests/resources.
- Affected systems: document recovery UX, background snapshot persistence, restore safety flow, sidebar rename migration, keyboard/context-menu discoverability, and large-file gating.
- Affected docs: `docs/next/session-time-travel.md` should be kept aligned with the MVP scope and the follow-up list captured by this change; `.agents/rules/ui.md` should capture the permanent text-surface spacing rule.
- Dependencies and APIs: builds on existing `spawn_blocking_then`, current draft/session restore flows, existing large-file limits, and the canonical-path sidecar identity pattern; no new external dependency is required for the MVP.
