## Context

LushText already presents document properties as a right-side pane on spacious layouts and a bottom sheet on compact layouts. The existing code accomplishes that by manually removing `properties_panel` from one container and inserting it into another when the dynamic editor-width guard changes.

Libadwaita's adaptive layout tools provide a better fit for this problem: one logical secondary surface can be placed into named slots across multiple layouts. This change should therefore reduce custom widget reparenting while preserving the exact visible contract already described by `document-properties-pane`.

The architecture constraint is important: this is UI shell orchestration, not domain behavior. The work belongs in the GTK driving adapter around `LushtextWindow`, with no new model, service, persistence, or preference contract.

## Goals / Non-Goals

**Goals:**

- Preserve the current user-facing behavior of document properties across wide layouts, compact layouts, workspace-sidebar mutual exclusion, Focus Mode suppression, and `F9`.
- Replace manual document-properties panel rehosting with a slot-based adaptive layout that treats pane and sheet as presentations of one logical surface.
- Keep the implementation shaped by `rust-hex-arch`: use GTK widgets as natural adapter ports, keep domain and services untouched, avoid gratuitous traits, and name layout/state concepts where they remove boolean ambiguity.
- Add robust widget tests for pane/sheet transitions, requested-state preservation, focus restoration, and dynamic breakpoint behavior.

**Non-Goals:**

- No visual redesign of document properties.
- No new document properties rows, language controls, preferences, or GSettings keys.
- No GTK, Libadwaita, gtk-rs, sourceview, Rust, or Flatpak runtime baseline change.
- No new service layer, domain model, storage abstraction, or trait wrapper around Libadwaita widgets.

## Decisions

### Use `AdwMultiLayoutView` as the presentation switch

The window template should introduce a document-properties layout view with two layouts:

```text
workspace OverlaySplitView
├─ sidebar: workspace sidebar
└─ content: document-properties MultiLayoutView
   ├─ slot "primary": editor/search/preview shell
   ├─ slot "properties": LushtextPropertiesPanel
   ├─ layout "pane": OverlaySplitView
   │  ├─ content: slot "primary"
   │  └─ sidebar: slot "properties"
   └─ layout "sheet": BottomSheet
      ├─ content: slot "primary"
      └─ sheet: slot "properties"
```

Rationale: this matches Libadwaita's intended adaptive-layout model and removes the custom `set_sidebar(None)` / `set_sheet(None)` reparenting path. The properties content remains one logical surface in the template, while Libadwaita handles placement.

Alternative considered: keep the current manual rehosting and only rename helpers. That would preserve the immediate behavior but leave the hardest-to-test part intact.

### Keep adaptive policy in the window driving adapter

The dynamic editor-width guard, compact mutual exclusion, and Focus Mode suppression are shell-presentation rules. They should remain in `ui/window`, either in the existing window implementation or in a focused sibling module if extraction improves navigation.

Rationale: moving this into `model/` would leak GTK layout concerns into the domain. Moving it into `services/` would create an artificial application-service boundary for behavior that cannot run without GTK widgets.

Alternative considered: create a service or trait for adaptive layout. That adds ceremony without a second implementation or a meaningful mock boundary.

### Name the presentation mode instead of relying on collapsed-state booleans

The code should use an explicit UI-layer concept such as `PropertiesPresentation::Pane` / `PropertiesPresentation::Sheet` or equivalent named helpers. Query-shaped helpers should answer the current presentation and requested/rendered state without mutating widgets. Command-shaped helpers should apply layout state and return `()`.

Rationale: this follows Command-Query Separation and avoids making future code remember whether `properties_split_view.is_collapsed()`, bottom-sheet openness, or layout name is the authoritative answer.

Alternative considered: keep checking `properties_split_view.is_collapsed()` directly. That couples app logic and tests to an implementation detail that should become secondary after `AdwMultiLayoutView` owns layout selection.

### Preserve existing breakpoint math

The change should keep the current dynamic editor-width guard, including the way it accounts for whether the workspace sidebar consumes width and which width preset is active. The breakpoint action should switch the document-properties layout presentation, not replace the guard with a fixed width.

Rationale: this protects the current ergonomics: a large workspace sidebar forces document properties into sheet mode earlier, while hiding the workspace sidebar relaxes that threshold.

Alternative considered: use a single static Libadwaita breakpoint. That would be simpler but would regress the existing workspace-aware layout behavior.

### Test semantic outcomes, not incidental widget plumbing

Widget tests should assert whether document properties are rendered as a pane or sheet, whether requested state survives transitions, whether the workspace sidebar arbitration still works, and whether focus restoration remains presentation-independent. Test helpers should expose semantic questions such as "is the properties surface visible?" and "which presentation is active?" instead of making every test know the internal widget path.

Rationale: this keeps tests strong while making the implementation free to use Libadwaita layout slots instead of manual rehosting.

Alternative considered: update the smallest number of assertions to match new widget names. That would compile faster initially but would miss the regression risks introduced by changing adaptive layout mechanics.

## Risks / Trade-offs

- **Layout slot IDs or template binding errors** -> Keep the template changes small, add template-child coverage through existing window construction tests, and run the widget suite under the headless Mutter harness.
- **Breakpoint transitions leave pane and sheet state out of sync** -> Centralize presentation queries and commands so there is one place that maps requested/rendered state onto `AdwMultiLayoutView`, `AdwOverlaySplitView`, and `AdwBottomSheet`.
- **Focus is lost when the active properties presentation changes** -> Keep the existing focus-restoration path and add tests that close or suppress properties from both pane and sheet presentations.
- **Tests become brittle against Libadwaita internals** -> Add semantic test helpers and avoid requiring each test to inspect low-level widget relationships.
- **`imp.rs` grows harder to navigate** -> If the implementation touches several adaptive-surface helpers, extract a workflow module under `ui/window/` rather than adding a generic utility module or service abstraction.

## Migration Plan

1. Update the window template to introduce the adaptive layout view and named slots while keeping the existing child widgets and IDs where practical.
2. Rewire window state synchronization to set the properties presentation through the layout view and to open only the relevant pane or bottom sheet for that presentation.
3. Remove manual properties-panel rehosting after the slot-based path owns both presentations.
4. Update and expand widget tests around existing document-properties behavior and the new continuity requirement.
5. Run the full relevant verification gates before considering the change complete.

Rollback is straightforward because this is local UI orchestration: revert the template and window synchronization changes, then restore the previous manual rehosting tests if needed.

## Open Questions

- None. The desired user-visible behavior is unchanged; the open work is implementation and regression coverage.
