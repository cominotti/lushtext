## Context

LushText already exposes meaningful accessibility metadata on several compact controls, ships widget tests for roles, and has an AT-SPI-enabled `make accessibility-smoke` lane. The current lane passed during exploration, but it verifies only a small set of shell, command-palette, and notes-empty anchors. The app does not yet have a comprehensive product contract for accessible relations, dynamic states, announcements, editor text exposure, visual accessibility, or keyboard parity across every major workflow.

The implementation must fit the existing GTK4/Libadwaita/GtkSourceView architecture. UI behavior lives under `crates/lushtext-core/src/ui/**`, app-owned automation is bounded through normal GTK/GIO actions plus the read-only D-Bus Automation1 surface, and host-sensitive proof lives in scheduled/manual smoke lanes with preserved artifacts. Widget tests intentionally disable the accessibility bridge, so real AT-SPI proof must stay in the smoke lane.

```
                       ┌───────────────────────────────┐
                       │ gtk-accessibility-spine spec   │
                       │ product accessibility contract │
                       └───────────────┬───────────────┘
                                       │
        ┌──────────────────────────────┼──────────────────────────────┐
        │                              │                              │
        ▼                              ▼                              ▼
┌─────────────────┐          ┌──────────────────┐          ┌──────────────────┐
│ UI metadata     │          │ workflow state   │          │ visual proof     │
│ roles/names/    │          │ focus, keyboard, │          │ contrast, scale, │
│ relations       │          │ announcements    │          │ focus rings      │
└────────┬────────┘          └────────┬─────────┘          └────────┬─────────┘
         │                            │                             │
         ▼                            ▼                             ▼
┌─────────────────┐          ┌──────────────────┐          ┌──────────────────┐
│ widget/static   │          │ AT-SPI smoke     │          │ visual geometry  │
│ policy checks   │          │ + automation     │          │ + visual smoke   │
└─────────────────┘          └──────────────────┘          └──────────────────┘
```

## Goals / Non-Goals

**Goals:**

- Make accessibility a first-class product contract, not a collection of local labels.
- Keep the implementation GTK-native by using `GtkAccessible` roles, properties, relations, states, announcements, focus behavior, and existing Libadwaita widgets wherever possible.
- Cover every major LushText workflow at state extremes: no required context, representative populated data, dense or awkward data, and constrained geometry.
- Provide layered proof: cheap widget/static checks for metadata, real AT-SPI smoke for accessibility-tree behavior, and visual smoke/geometry for focus visibility, scaling, contrast, and layout.
- Preserve automation privacy: accessibility scenarios may use bounded paths, names, counts, roles, and states, but must not expose unbounded document text, note bodies, draft bodies, or complete search results.
- Make the work maintainable through helpers, documentation, and policy checks that future UI changes naturally use.

**Non-Goals:**

- Reimplement GtkSourceView, GTK text accessibility, Libadwaita widgets, AT-SPI, Orca, or platform screen-reader behavior.
- Promise identical accessibility behavior across non-GTK platforms or unsupported host sessions.
- Add a new runtime accessibility service, background daemon, or external dependency beyond documented host-sensitive smoke tooling.
- Make `make accessibility-smoke` a required pull-request gate while it remains host-sensitive.
- Guarantee WCAG conformance as a legal certification. The goal is a strong GTK desktop accessibility contract with testable evidence.

## Decisions

### Decision: Create an accessibility spine helper layer

Add a small internal accessibility helper module under the UI layer, for example `ui/accessibility.rs` or closely scoped sibling helpers, to centralize common operations:

- set accessible label and description together;
- set roles for semantic groups, alerts, lists, tabs, rows, and custom containers;
- set relations such as labelled-by, described-by, controls, and controlled-by where GTK exposes them through gtk4-rs;
- set states such as expanded, selected, pressed, busy, invalid, current, checked, disabled, and readonly when a widget's semantic state differs from GTK defaults;
- announce important workflow changes with an explicit priority and throttling policy;
- derive accessible shortcut labels from GTK accelerator helpers when the active GTK version supports it;
- provide test-only audit helpers for widget tests.

Rationale: LushText already repeats local `update_property` blocks. A helper layer reduces drift, makes policy checks easier, and gives custom widgets one obvious path for accessible metadata.

Alternative considered: keep adding per-widget calls inline. That is simple for one control but makes consistency, state updates, and future audits fragile.

### Decision: Keep product semantics separate from automation mechanics

Accessible names, roles, descriptions, relations, and states are product semantics first. Automation helpers may target them, but names must remain meaningful to assistive technology users, not optimized for scripts.

Rationale: The current automation reference already treats stable anchors as public accessibility metadata. This change strengthens that boundary and prevents automation-only wording from leaking into the user experience.

Alternative considered: expose hidden automation-only widgets or IDs. Rejected because it weakens the actual screen-reader contract and bypasses visible UI behavior.

### Decision: Use a surface inventory and acceptance matrix

Create a reviewable surface inventory that includes at least:

- shell/header/tab/status controls;
- main editor and source-view states;
- Markdown preview and embedded code/image/table widgets;
- workspace sidebar, file tree, row actions, context menus, inline rename, DnD reorder, file peek, and Focus Folder;
- Open popover;
- command palette;
- in-tab search and workspace search;
- document properties and encoding/file-health controls;
- notes, bookmarks, folder notes, and document notes;
- local history;
- preferences;
- save/close/destructive dialogs;
- focus mode, preview mode, minimap, transparency, and compact layout variants.

Each surface gets acceptance rows for no context, representative data, dense or awkward data, and constrained geometry where applicable.

Rationale: Accessibility bugs cluster at state extremes: empty states without useful names, long rows whose controls vanish, compact layouts that drop close buttons, and recycled list rows with stale metadata.

Alternative considered: audit only visible default shell controls first. Rejected because the app's complex workflows live in transient surfaces and list factories.

### Decision: Explicitly prove the editor text surface

Keep GtkSourceView as the editor implementation, but add LushText-owned metadata and proof around it:

- stable editor region name and description that includes active document identity class without unbounded contents;
- editable/read-only state when load/save/large-file policy changes editability;
- focus restoration proof after transient surfaces close;
- AT-SPI proof for text exposure, caret/focus, selection, and visible content boundaries where the platform exposes them;
- fallback documentation for any GtkSourceView or AT-SPI limitation discovered during implementation.

Rationale: The editor is the core product surface. Relying on GtkSourceView defaults without proof is not enough for a state-of-the-art claim.

Alternative considered: build a custom accessible text wrapper over GtkSourceView. Rejected unless discovery proves a GTK/platform gap that cannot be solved with metadata and upstream interfaces.

### Decision: Announcements are workflow events, not every state mutation

Use announcements for user-meaningful events:

- inline alerts and blocking errors;
- save/load success or durability warnings when not otherwise obvious;
- search result count changes after debounced completion;
- replace-all completion and undo availability;
- content-search start, progress milestones, completion, and cancellation;
- recovery restore or quarantine summaries;
- destructive confirmation outcomes;
- workspace refresh/index completion when user-initiated;
- mode changes such as focus mode, preview mode, and document properties visibility.

Do not announce every keystroke, transient hover, background heartbeat, or rapidly repeated status update.

Rationale: Screen-reader noise is a usability bug. LushText already has notification buses and debounce helpers that can route polite/high-priority announcements without flooding users.

Alternative considered: make every status-bar message an announcement. Rejected because progress heartbeats and routine typing feedback would become disruptive.

### Decision: Layer tests by what each lane can honestly prove

- Widget tests: roles, labels, descriptions, state helper calls, relation helper calls where available, focus restoration, row recycling, keyboard paths, and static policy checks.
- AT-SPI smoke: real accessibility tree, focus path, editable text, dynamic announcements where observable, and stable anchors in a live headless session.
- Automation smoke: drive normal actions and expose bounded readiness/snapshot fields that accessibility smoke can use before querying AT-SPI.
- Visual smoke/geometry: focus rings, high contrast, large text, reduced motion, color-not-only states, opacity/readability, constrained geometry, and internal scrolling.
- Documentation drift checks: stable anchors, helper flags, scenario manifests, and coverage lists.

Rationale: No single lane proves accessibility. Widget tests are cheap but disable the bridge; AT-SPI is real but host-sensitive; screenshots prove visual affordances; automation proves state without relying on coordinates.

Alternative considered: put all coverage into `make accessibility-smoke`. Rejected because it would be slow, fragile, and unable to catch cheap metadata regressions early.

### Decision: Keep hover-only features reachable by keyboard and context menu

Any affordance revealed by hover, overlay, DnD, or pointer-only interaction must have a keyboard path and/or context-menu path, and visible focus styling must identify the reachable target.

Rationale: LushText already has this principle in sidebar guidance for hover row actions. This change promotes it into a cross-app accessibility rule.

Alternative considered: document hover-only affordances as mouse conveniences. Rejected for state-of-the-art accessibility.

### Decision: Make visual accessibility explicit

Visual proof must include:

- focus indicator visibility in normal, dark, high-contrast, compact, and constrained states;
- large text / text scale behavior where host tooling supports it;
- reduced-motion behavior for transitions or documented GTK-owned limitations;
- color-not-only communication for alerts, disabled states, search matches, file health, local history, and destructive states;
- background opacity/readability guardrails for editor and preview surfaces;
- no unintended horizontal scrollbars or clipped primary controls in dense states.

Rationale: Accessible names are not enough. Keyboard and low-vision users need visible focus, readable contrast, stable layout, and non-color semantic cues.

Alternative considered: treat visual accessibility as a manual QA checklist. Rejected because LushText already has visual smoke/geometry lanes that can preserve reviewable evidence.

## Risks / Trade-offs

- Accessibility smoke may become slow or flaky on host-sensitive infrastructure -> keep broad coverage scheduled/manual, add narrow focused local commands, preserve skip reasons, and keep cheap widget/static checks in default policy.
- AT-SPI may expose GTK widgets differently than GTK accessible properties suggest -> make smoke assertions target user-meaningful names/roles, document platform caveats, and prefer multiple proof signals over brittle depth/order checks.
- Announcements may become noisy -> route them through explicit workflow events with debounce/throttle helpers and test no-announcement paths for high-frequency updates.
- Custom list factories may leak stale accessible metadata through row recycling -> add bind/unbind helper patterns and widget tests for recycled rows.
- High contrast, text scale, and reduced motion may vary by desktop environment -> record environment metadata and make unsupported-host skips explicit without counting skipped coverage as verified.
- Adding relations/states everywhere may overfit implementation details -> require relations/states only when they improve assistive technology semantics beyond GTK defaults.
- Developer policy checks can become annoying if too broad -> start with high-signal checks for icon-only buttons, custom rows, transient surfaces, hover-only actions, and new smoke anchors.

## Migration Plan

1. Add the accessibility helper layer and documentation skeleton without changing user-visible behavior.
2. Build the surface inventory and categorize each surface by metadata, state, keyboard, AT-SPI, and visual proof needs.
3. Update foundational shell/editor/search/open-popover surfaces first, including editor text proof.
4. Expand transient and list-heavy surfaces: sidebar/file tree, workspace search, command palette, notes/bookmarks, local history, properties, and preferences.
5. Add dynamic announcements and throttling after metadata/state surfaces are stable.
6. Expand `make accessibility-smoke` and scenario artifacts in slices so each new scenario is reviewable.
7. Add visual accessibility scenarios and proof-policy hooks.
8. Update docs, rules, and drift checks.
9. Run the full verification ladder and fix any pre-existing blockers in the same work stream.

Rollback is mostly additive: if a new smoke scenario is unstable, keep the product metadata fix and mark the scenario as diagnostic until stabilized; do not remove user-facing accessibility improvements merely because a host-sensitive proof lane needs tuning.

## Open Questions

- Which screen-reader manual check should be the release-level reference: Orca on GNOME in a normal session, the existing private AT-SPI headless helper, or both?
- Can the current GtkSourceView/AT-SPI stack expose enough text, caret, and selection detail for automated proof, or do we need documented manual checks for some editor behaviors?
- Should the broad accessibility smoke matrix be one command with filters or several focused commands that roll up into `make accessibility-smoke`?
- Which visual accessibility variants are stable enough for scheduled CI on the current host image: high contrast, large text, reduced motion, or a smaller subset with manual instructions for the rest?
