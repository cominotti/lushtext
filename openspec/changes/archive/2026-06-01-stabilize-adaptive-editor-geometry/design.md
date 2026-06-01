## Context

The reported bugs are different symptoms of one missing geometry contract in the main editor shell.

Runtime captures reproduced the short-height failure: at `1190x200` and `1190x180`, Libadwaita reported root content exceeding available height and the bottom status bar became clipped. A narrow capture at `837x902` reproduced the workspace-sidebar collapse issue: the sidebar remained visible as an overlay and covered the editor's left edge after a passive resize. The medium-width flicker is explained by code inspection: the document-properties breakpoint currently depends on `workspace_split_view.shows_sidebar()`, while compact arbitration can hide that same sidebar. With the corrected `280sp` properties-pane budget, the default `Comfy` loop sits from `933sp` through `1350sp`: hiding the workspace changes the guard back to the no-workspace value, which then reopens the pane and restores the workspace.

The minimap issue is adjacent but distinct. The active editor defaults to word wrap, while the minimap's `GtkSourceMap` is forced to `WrapMode::None`. Showing the workspace sidebar narrows the editor, which can reflow visible lines without a height change. Existing minimap marker work refreshes custom semantic markers, but it does not define or test the native viewport rectangle after sidebar-driven width changes.

GTK and Libadwaita documentation confirm the relevant constraints: widgets advertise minimum sizes, parents must not allocate them below those minima, `AdwBreakpoint` state should be driven declaratively from stable conditions, and `AdwMultiLayoutView` is intended to switch presentations without side-effect loops.

## Goals / Non-Goals

**Goals:**

- Make the shell's adaptive layout state deterministic for workspace sidebar and document properties.
- Preserve the normal status bar and quick editor-state controls at supported short heights.
- Prevent passive resize from leaving the editor's left edge hidden behind a compact workspace overlay or stale horizontal scroll.
- Keep the minimap viewport overlay aligned with the settled active editor viewport after width reflow.
- Add tests and headless runtime checks that fail on flicker, allocation warnings, clipped chrome, and viewport drift.

**Non-Goals:**

- Redesign the workspace sidebar, document properties content, or minimap feature set.
- Change saved workspace/session data formats.
- Replace Libadwaita adaptive widgets with custom layout containers.
- Implement a new minimap navigation model beyond correcting viewport projection and refresh timing.

## Decisions

1. Derive adaptive shell state in a pure compute step.

   Add a small layout-state model for the window shell that takes stable inputs: allocated width, requested workspace visibility, requested document-properties visibility, selected workspace preset, focus mode, and fixed content minimums. It returns the intended workspace presentation, properties presentation, split widths, and compact active surface. The apply step compares current widget state against that result and only mutates widgets when a value actually changes.

   Alternative considered: keep the existing incremental mutation flow and add more guards. That would be smaller, but it leaves the root cause intact because the breakpoint can still be computed from state changed by the same pass.

2. Compute document-properties guards from intent, not rendered visibility.

   The workspace-aware guard should use whether the workspace would consume width in the requested spacious layout, plus the selected preset's effective clamped width. It must not use temporary `shows_sidebar()` values created by compact mutual exclusion. This preserves the existing "workspace width affects properties guard" behavior while removing the feedback loop.

   The properties width budget should also match the rendered panel's actual minimum width. The current shell constant under-budgets the properties panel relative to its template width request.

3. Make compact mutual exclusion an arbitration result.

   In compact layouts, only one secondary surface can be active. When both surfaces are requested and document properties are the active compact surface, suppress the workspace sidebar without overwriting the user's desktop visibility intent. When the user explicitly opens the workspace sidebar in compact mode, make that an intentional active compact surface and close or suppress document properties according to the existing document-properties contract.

   Passive resizing across the workspace collapse threshold should not leave an overlay covering the editor. A compact overlay is acceptable only when it follows an explicit user action.

4. Advertise and enforce a normal-mode height budget.

   Define a normal-mode minimum height that includes header bar, tab strip, status bar, and a small editor viewport. Optional content must fit inside the remaining content area. Search results should clamp to the available content budget and must be allowed to shrink below their comfortable height instead of enforcing a hard floor that steals space from persistent chrome.

   Alternative considered: hide the status bar at very short heights. That conflicts with the current bottom-bar contract outside Focus Mode and would make quick encoding, line-ending, feedback, and workspace controls disappear unpredictably.

5. Treat editor left-edge visibility and horizontal adjustment as layout invariants.

   After passive width changes, the editor should keep the gutter and line starts visible unless the user has explicitly created horizontal-scroll intent. If GTK preserves or creates a right-biased horizontal adjustment during resize, clamp it back to the lower bound once layout settles. This covers both the overlay-obscured screenshot and the long-line stale-scroll risk found during review.

6. Define the minimap viewport source of truth.

   The minimap viewport overlay should be checked against the active editor's settled visible buffer range, not against stale pre-toggle adjustment geometry. Implementation can either align `GtkSourceMap` wrapping with the editor when appropriate or layer a custom viewport projection over the map. The chosen path must refresh after width-only allocation changes because wrapping and visible-line geometry can change even when height is unchanged.

   The existing semantic marker strip should remain separate but share the refreshed source-map geometry so marker fixes do not regress.

7. Verify with both widget tests and real GTK captures.

   Add pure tests around the adaptive-state derivation at the known guard boundaries, widget tests that watch for repeated layout/show/open notifications after settling, and headless Mutter captures for the problematic dimensions. The runtime warning gate should inspect raw app stderr, since helper summaries can miss Libadwaita warnings.

## Risks / Trade-offs

- Medium-width behavior changes can feel different for users who expect the workspace sidebar overlay to remain open after passive shrink. Mitigation: preserve explicit compact-sidebar toggles, but do not treat passive resize as a user overlay request.
- A stricter minimum height reduces how tiny the interactive window can become. Mitigation: make the minimum reflect the chrome LushText already promises to keep visible, and let optional surfaces scroll or compact inside that budget.
- Replacing or supplementing the native `GtkSourceMap` viewport overlay could increase minimap complexity. Mitigation: first prove whether matching wrap mode plus allocation refresh is enough; only add a custom overlay if the native rectangle cannot satisfy the new spec.
- Notification-counter tests for flicker can be timing-sensitive. Mitigation: drive them from deterministic widget state and use headless Mutter only as an end-to-end warning and screenshot sanity check.
