## Why

The workspace sidebar toggle currently changes state at some live window sizes without a visible show/hide animation. This makes the shell feel broken even though the requested visibility state changes correctly, and it is especially easy to miss with final-settle-only verification.

## What Changes

- Preserve a visible workspace-sidebar show/hide animation whenever the user toggles the sidebar, regardless of whether the current width renders the sidebar as consuming layout width or as an overlay.
- Keep adaptive secondary-surface arbitration, document-properties presentation changes, minimap protection, and split-view width synchronization from collapsing the sidebar transition into an immediate jump.
- Add regression coverage for narrow/collapsed, intermediate desktop, and wide desktop widths, including the reproduced `1100sp` class.
- Require animation-frame visual evidence for the sidebar transition so a passing final screenshot cannot mask a missing animation.
- No breaking changes.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `workspace-sidebar-shell`: Define the visible animation contract for workspace-sidebar show/hide across adaptive window sizes.
- `visual-geometry-invariants`: Require sidebar-animation scenarios to prove intermediate frames, including the reproduced intermediate-width class.
- `dbus-automation-spine`: Ensure automation geometry/readiness continues to expose enough state to distinguish hidden, shown, and intermediate sidebar transition phases.
- `automation-client-tools`: Ensure live capture/replay summaries can prove or reject workspace-sidebar animation-frame evidence.

## Impact

- Affected UI code: `crates/lushtext-core/src/ui/window/actions.rs`, `crates/lushtext-core/src/ui/window/imp.rs`, and any focused helper modules extracted for adaptive sidebar animation coordination.
- Affected verification: window/widget tests for requested vs rendered sidebar state, visual geometry stream scenarios, automation readiness/snapshot assertions, and proof-policy summary checks.
- Affected docs if automation contracts or visual scenario commands change: `docs/automation.md`, `docs/automation-reference.md`, and visual proof documentation.
