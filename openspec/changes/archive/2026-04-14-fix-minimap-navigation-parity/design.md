## Context

The first minimap change successfully introduced `GtkSourceMap`, semantic markers, and a visible native viewport indicator. The initial follow-up then assumed plain `GtkSourceMap` interaction was sufficient to match GNOME Text Editor once LushText stopped layering extra gestures on top.

That assumption turned out to be incomplete. Upstream `GtkSourceMap` itself still centers click jumps and owns its own drag math, so removing LushText's extra gesture layer does not fully explain the remaining live mismatch. The new screenshot-based finding is more specific: LushText behaves reasonably through most of the document and only starts to drift near the last third of the minimap. GNOME Text Editor, by contrast, keeps extra blank space after the last line in both the editor and the minimap. Upstream Text Editor source confirms that it dynamically sets the source view bottom margin to `visible_rect.height * .75`, creating deliberate overscroll tail room that propagates into `GtkSourceMap` through the map's margin binding.

## Goals / Non-Goals

**Goals:**
- Add GNOME-style end-of-document overscroll so the editor and minimap keep usable tail room after the last line.
- Improve EOF minimap dragging and clicking by aligning the underlying geometry first rather than jumping straight to custom interaction logic.
- Keep the implementation narrowly scoped to editor-page geometry and minimap behavior near the bottom of the document.
- Add regression coverage that would catch the overscroll behavior disappearing or failing to propagate into the minimap.

**Non-Goals:**
- Rework the semantic marker strip or minimap availability policy.
- Replace `GtkSourceMap` with a custom minimap renderer.
- Ship a custom click or drag remapping layer in the same step unless overscroll alignment proves insufficient afterward.

## Decisions

### 1. Treat dynamic editor overscroll as the first parity mechanism

The next implementation step should mirror GNOME Text Editor's `EditorSourceView` approach by updating the editor view's bottom margin from the current visible rect after allocation. That added overscroll tail gives the last lines room to travel upward inside the editor viewport and, because `GtkSourceMap` binds top and bottom margins from the view, also extends the map's effective content runway near EOF.

Alternatives considered:
- Jump directly to custom minimap click and drag remapping: rejected as the first move because the new evidence points to missing overscroll tail room, not just bad pointer math.
- Keep the fixed `bottom-margin = 6` setup and only tune CSS: rejected because the screenshot points to a geometry and travel-range problem, not purely a visual one.

### 2. Let the minimap inherit the overscroll through upstream margin binding

Once the editor view gains dynamic bottom overscroll, LushText should avoid fighting `GtkSourceMap`'s own top and bottom margin binding. The minimap setup should stay as close as practical to the upstream wrapper contract so the map receives the same extra blank tail that GNOME Text Editor's map receives.

Alternatives considered:
- Add a separate minimap-only fake tail by overriding map margins directly: rejected because the editor and map need to stay geometrically consistent, and upstream already provides a shared propagation path through view margin binding.

### 3. Add regression tests around overscroll presence rather than only controller structure

Because the remaining bug appears only near EOF, the most useful regression coverage now is:
- widget or editor-page assertions that the source view bottom margin grows based on the visible rect after allocation, and
- focused minimap coverage that the constructed source map receives the resulting geometry instead of collapsing immediately at the last line.

This still does not replace full live drag replay, but it protects the geometry contract now believed to be responsible for the observed behavior.

Alternatives considered:
- Leave tests focused only on controller sets and fixed margin values: rejected because those checks no longer cover the real suspected failure mode.

## Risks / Trade-offs

- [Dynamic bottom overscroll could affect other editor behaviors such as save banners, search visibility, or cursor positioning near EOF] -> Keep the change tab-local to the editor page and rerun the focused editor and minimap widget coverage that already exercises those surfaces.
- [Overscroll may improve EOF behavior but not fully solve click targeting] -> Retest live after geometry alignment and only then decide whether custom click or drag remapping is still justified.
- [Static tests may still miss subtle live pointer behavior] -> Keep the regression tests focused on the new overscroll geometry contract and use `make run` as the final decision surface before escalating to custom interaction code.

## Migration Plan

No user-data migration is required. Ship the dynamic overscroll update inside the editor page, verify that the minimap receives the resulting tail room, and keep rollback simple by restoring the fixed bottom-margin behavior if the experiment causes regressions.

## Open Questions

If overscroll alignment still leaves meaningful click or drag mismatch afterward, decide whether the next step should be custom click positioning, custom drag anchoring, or both.
