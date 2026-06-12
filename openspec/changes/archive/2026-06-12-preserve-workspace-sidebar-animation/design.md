## Context

LushText renders the left workspace panel with `AdwOverlaySplitView`. The status-bar toggle drives `win.toggle-sidebar`, which persists requested intent and then asks the adaptive shell to decide how that intent is rendered. At the reproduced live size, the window was approximately `1100sp` wide with a `330sp` workspace sidebar. Toggling from hidden to shown changed state correctly, but the first sampled frame already reported the sidebar at its final visible position.

The current shell has two interacting adaptive systems:

- the outer workspace split view, which owns the workspace sidebar visibility and width;
- the inner document-properties layout, whose pane/sheet breakpoint is derived from whether the workspace sidebar consumes width.

At intermediate widths, a single workspace toggle can therefore also recalculate the document-properties breakpoint and switch presentation. That same toggle path also protects the active minimap and runs split-view width synchronization. The implementation must preserve the animation while keeping those adaptive contracts intact.

## Goals / Non-Goals

**Goals:**

- Preserve a visible workspace sidebar show/hide animation at narrow/collapsed, reproduced intermediate, and wide desktop widths.
- Keep requested sidebar visibility, rendered sidebar visibility, compact secondary-surface arbitration, and persisted preferences consistent.
- Keep document-properties behavior and minimap protection correct while avoiding same-frame layout work that collapses the sidebar transition.
- Add proof that observes intermediate animation frames, not only final settled screenshots.
- Keep UI responsive and free of unexpected GTK/Libadwaita warnings during the transition.

**Non-Goals:**

- Replacing `AdwOverlaySplitView` with a custom animation system.
- Changing workspace folder, workspace scope, or sidebar width-preset behavior.
- Changing document-properties user-facing behavior except where coordination is needed to avoid eating the workspace animation.
- Adding new automation privacy surface beyond bounded geometry/timing fields already needed by visual proof.

## Decisions

### Use Libadwaita's sidebar animation as the primary animation

`AdwOverlaySplitView` already animates `show-sidebar` through its own transition progress. The fix should preserve that path and remove app-side coordination that causes the animation to become visually instantaneous.

Alternative considered: reintroduce a custom paned or snapshot animation. This would fight the current Adwaita-native shell and bring back a class of geometry bugs this codebase has been moving away from.

### Separate user intent from transition coordination

The toggle action should continue to update requested visibility immediately, but layout work that depends on the post-toggle steady state should not force the transition endpoint into the same visible frame. In practice, implementation should inspect the current `sync_secondary_surface_layout()` path and isolate the operations that must happen before `set_show_sidebar()` from operations that can run after the first animation frame or after final settle.

Candidate shape:

- compute the target adaptive layout once from the requested state;
- start the workspace `show-sidebar` transition without repeatedly rewriting unrelated split-view fractions or breakpoint conditions;
- defer or coalesce document-properties breakpoint/presentation reconciliation when it is not immediately visible or when it would cause endpoint snapping;
- keep final layout reconciliation and action-state sync at animation completion or final visual settle.

Alternative considered: keep all current sync work but rely on faster sampling or final readiness. That would not address the visible snap and would miss the defect in proof.

### Cover the intermediate width explicitly

The reproduced class is around `1100sp`: wider than the workspace collapsed breakpoint, but still close enough to the properties breakpoint that showing the workspace can change the inner properties presentation. Tests must include this middle case because narrow and wide cases alone do not exercise the competing adaptive decision.

Alternative considered: only test canonical 720p/1080p/wide cases. Those can all pass while the exact threshold class still snaps.

### Prove animation with stream evidence

Final settled geometry is necessary but not sufficient. Visual proof must include an action-triggered frame stream with timestamp-correlated automation samples and at least one mapped intermediate frame. The report must distinguish passing final settle from passing animation-frame proof.

Alternative considered: widget tests that assert requested/rendered state changes. Those are useful but cannot prove the human-visible animation.

## Risks / Trade-offs

- Layout deferral could leave properties presentation stale briefly -> Limit deferral to transition-safe work and always run final reconciliation after the sidebar animation settles.
- Automation may not expose enough phase/timing detail for the new proof -> Extend bounded geometry diagnostics rather than using private widget inspection or fixed sleeps.
- Minimap protection could mask the sidebar issue or add frame cost -> Keep minimap freeze/settle behavior in the transition matrix and require warning-scan plus frame evidence with the minimap visible.
- Intermediate-width proof may be host-sensitive -> Use headless same-session visual geometry where possible and report explicit unsupported-host status when stream capture is unavailable.

## Migration Plan

No data migration is required. The change affects runtime layout coordination and test/proof artifacts only. Existing GSettings values for workspace sidebar visibility, width preset, and document-properties visibility remain valid.

Rollback is a code rollback to the previous adaptive sync path. The proposal adds no persisted schema that would require cleanup.

## Open Questions

- Does Libadwaita expose a reliable public animation-complete signal for the exact sidebar transition in the Rust binding, or should LushText use bounded geometry stabilization through existing visual-readiness helpers?
- Should the transition coordination live in `window/imp.rs` or be extracted into a small window workflow module if the implementation grows beyond the current sync helpers?
