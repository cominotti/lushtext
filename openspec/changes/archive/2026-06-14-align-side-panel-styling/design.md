## Context

LushText currently applies the same `shell-chrome-opaque` styling role to both the workspace sidebar and document properties panel. That keeps side surfaces opaque while tab-content transparency is active, but it also ties the two panes to the generic window background tone.

The document properties panel is an inspector surface made from `Adw.PreferencesGroup` rows. In the screenshot-driven comparison, GNOME Text Editor's right Info panel reads more clearly because the side surface and grouped rows have a stronger tonal relationship. The workspace sidebar is different: it is a dense navigation rail with a fixed selector row, workspace headers, virtualized tree rows, drag handles, and constrained-width rules.

## Goals / Non-Goals

**Goals:**

- Give the workspace sidebar and document properties panel a coordinated side-rail background tone.
- Preserve each surface's content idiom: workspace stays navigation-tree-like; document properties becomes a clearer inspector.
- Keep tab-content transparency isolated to document/preview content while side rails remain opaque.
- Cover light and dark style schemes, no-document states, populated document metadata, dynamic health rows, compact bottom sheet, many workspace rows, long names, and constrained widths.

**Non-Goals:**

- Do not redesign the workspace model, row hierarchy, drag-and-drop, file tree behavior, or width presets.
- Do not add new document properties rows such as `Document Type`, language selection, or duplicate encoding/line-ending controls.
- Do not introduce a global theme system, new dependency, persisted preference, or user-facing style setting.
- Do not change automation APIs, services, models, persistence, or filesystem behavior.

## Decisions

### Use semantic side-surface styling instead of changing `shell-chrome-opaque` globally

Create or repurpose a side-surface CSS role for the workspace sidebar and document properties panel, leaving unrelated opaque surfaces such as empty states, placeholders, search bars, and preview placeholders on their current background role.

Rationale: `shell-chrome-opaque` currently groups multiple surfaces that are opaque for different reasons. A side-rail-specific role lets the implementation tune the left and right side panels without accidentally changing the empty editor state or other chrome.

Alternative considered: change `shell-chrome-opaque` directly. That is simpler, but it would broaden the visual change beyond the two side panels and make regressions harder to understand.

### Share the side-rail tone, but not the content treatment

Both side panels should use the same side-rail tone so the app reads as one balanced shell. The document properties panel should then use grouped inspector rows with enough contrast from that rail. The workspace sidebar should keep its existing selector, section headers, and `navigation-sidebar` file rows rather than becoming a card/list inspector.

Rationale: visually coordinating the side rails solves the "left old, right new" risk, while preserving the sidebar's navigation affordances avoids fighting its dense tree contract.

Alternative considered: fix only the document properties panel. That would improve the immediate pain point but could make the two side surfaces feel unrelated when both are open.

### Prefer Adwaita-derived colors and existing widget styles

The implementation should prefer GTK/Libadwaita named colors, existing `.sidebar` and `.navigation-sidebar` behavior, and `Adw.PreferencesGroup` row styling before introducing custom hard-coded colors. If a small custom CSS role is needed, keep it semantic and local to the side-surface relationship.

Rationale: the goal is GNOME-like readability that tracks light/dark schemes and future Adwaita changes, not a one-off palette.

Alternative considered: hard-code the screenshot's exact grays. That may match one theme snapshot but is brittle across light mode, dark mode, high contrast, and Adwaita updates.

### Treat visual proof as part of the acceptance surface

Because this is a visual-sensitive shell change, implementation should refresh Blueprint-generated UI and run focused widget/visual proof. The proof should include both side panels visible, properties-only compact sheet, no-document properties state, populated document metadata, health rows, zero/one/many workspace folders, long folder names, and constrained widths.

Rationale: the likely failure modes are not compiler errors; they are washed-out hierarchy, accidental scrollbar changes, clipped controls, and compact-layout regressions.

Alternative considered: rely only on code review. That would miss the specific readability and geometry risks this change exists to fix.

## Risks / Trade-offs

- Side-rail colors could make selected navigation rows too subtle -> verify selected/hover/focus states in light and dark schemes and adjust only semantic container roles before restyling rows.
- Properties group rows could lose contrast inside the compact bottom sheet -> verify the bottom-sheet presentation separately from the right-pane presentation.
- Changing dynamic transparency CSS could accidentally make side rails translucent -> keep side-rail classes explicitly opaque and verify with tab transparency enabled.
- Sidebar dense states could gain padding or horizontal overflow if inspector styling leaks into the tree -> keep `navigation-sidebar` rows and constrained-width rules intact, then test long names and many folders.
- Visual smoke artifacts may need refresh after intentional styling changes -> rerun the visual proof lane and commit updated expected artifacts only when the visual difference matches this design.
