## Context

LushText already follows `ui -> services -> model`, uses `mod.rs` as widget facade and `imp.rs` for template/state/setup, and prefers workflow modules over traits or manager objects. Three adapters are now structural outliers: `window/notes.rs` is roughly 3,600 lines and combines bookmark, note-editor, browser, palette, menu, migration, and preview flows; `window/imp.rs` is roughly 1,875 lines and contains a substantial plain adaptive-shell policy plus GObject lifecycle; `workspace_section/imp.rs` is roughly 1,870 lines and combines subclass state with row factory, context menus, gestures, and accessibility projection. This change is deliberately last so active behavior work lands before code moves.

## Goals / Non-Goals

**Goals:**

- Split each outlier along existing workflow and ownership seams.
- Keep public widget facades, template children, state ownership, actions, and services unchanged.
- Move pure adaptive calculations into a testable plain Rust module.
- Make row factory, context-menu, and accessibility wiring independently reviewable.
- Preserve all source, runtime, automation, accessibility, geometry, persistence, and data-safety behavior.

**Non-Goals:**

- Adding features, changing visuals, renaming public actions, or changing persistence formats.
- Introducing manager/controller/repository types, dependency injection, generic traits, or new crates.
- Moving GTK collections into services or GTK types into model/services.
- Combining draft and session workflows or undoing existing sibling modules.

## Decisions

### Keep the existing GObject facade and implementation boundary

`window/mod.rs` and `workspace_section/mod.rs` remain the public facades. Their `imp.rs` files retain `ObjectSubclass`/`ObjectImpl`/`WidgetImpl`, template children, owned widget state, disposal, and short setup calls. Extracted modules add private or `pub(super)` `impl` blocks/functions; they do not become new widgets or public APIs.

Alternatives considered:

- New controller/manager structs were rejected because state already has one clear widget owner.
- A new internal crate was rejected because all extracted logic is adapter-specific.
- Leaving files large was rejected because unrelated workflows currently collide in review and obscure the good existing boundaries.

### Turn window notes into a workflow folder

The `notes` module keeps its name but changes from `notes.rs` to:

- `notes/mod.rs`: shared window-facing facade, common private value types, editor callback routing, menu availability, and cross-workflow coordination;
- `notes/bookmarks.rs`: bookmark toggle/edit/navigation, persistence callbacks, bookmark browser/excerpt, and bookmark activation;
- `notes/editors.rs`: document-note and folder-note target selection, dialogs, rich note edit/preview/save lifecycle, and related sidecar resolution;
- `notes/browser.rs`: Browse Notes state, category/search/sidebar projection, async preview, command-palette note-source refresh, and palette target activation.

Rename/migration helpers stay beside the artifact they migrate; only genuinely shared orchestration remains in `mod.rs`. Existing `LushtextWindow` method names and visibilities remain stable unless made more private by the move.

### Extract only pure adaptive-shell policy

`window/adaptive_shell.rs` will own constants and plain value objects for requested/rendered surface intent, properties presentation, breakpoint thresholds, width/fraction math, compact-surface arbitration, and their unit tests. It has no template children, GSettings, `libadwaita::Breakpoint`, signal registration, focus manipulation, or widget mutation.

`window/imp.rs` continues to collect current widget/settings inputs and apply the returned decision. Breakpoint parsing/installation, split-view mutation, accessibility metadata, `size_allocate`, construction, and disposal remain in the adapter.

### Split workspace-section wiring by responsibility

`workspace_section/imp.rs` retains subclass state, template children, `constructed`/`dispose`, and calls into:

- `row_factory.rs`: setup/bind/unbind projection, binding/signal bags, DnD row handoff, inline-rename trigger, and recycled-row cleanup;
- `context_menus.rs`: file/header menu construction, pointer and keyboard targeting, popup lifecycle, action rows, and selection anchoring;
- `row_accessibility.rs`: row names/descriptions/position/expanded/disabled projection and expanded-state signal hooks.

Existing workflow modules such as `dnd`, `folders`, `peek`, `refresh`, `tree_loading`, and `watch` remain owners of their behavior. Extracted wiring delegates to them rather than duplicating their rules.

### Prove behavior neutrality with inventories and existing gates

Before moving code, implementation records inventories of window note methods/actions/callbacks, adaptive policy inputs/outputs, workspace-section factory phases/data keys/signal cleanup, context-menu entries/keyboard routes, and accessibility metadata. Tests move with their owning pure/helper code; no assertion is deleted merely to make the split compile.

Module-map documentation and nearest `AGENTS.md` contracts are updated in the same change. Automation docs change only if verification uncovers an accidental public-contract change; such a change is a defect to fix, not accepted scope.

## Risks / Trade-offs

- [Rust privacy changes can tempt broad `pub(crate)` exposure] → Prefer child modules under `notes` and `workspace_section`, use `pub(super)` narrowly, and keep private data near its owner.
- [Mechanical moves can drop signal cleanup or GTK object-data keys] → Inventory setup/bind/unbind/dispose symmetry and run open/close/rebind stress tests.
- [Notes split can create cyclic helper calls] → Keep shared types/routing in `notes/mod.rs` and preserve one-way calls from facade to workflow helpers.
- [Adaptive extraction can accidentally move widget policy into a pseudo-domain layer] → Keep the module inside `ui/window`, plain Rust, and limited to calculations from explicit inputs.
- [Large diff increases merge conflict risk] → Land this portfolio item last and split implementation commits by adaptive shell, workspace section, and notes.

## Migration Plan

1. Capture clean baseline results and the three behavior inventories.
2. Extract `adaptive_shell.rs`, move its unit tests, and verify no geometry output changes.
3. Extract workspace-section row accessibility, context menus, then row factory; verify each setup/bind/unbind/dispose step before continuing.
4. Convert `notes.rs` to `notes/` and move bookmark, editor, then browser/palette workflows with tests after each move.
5. Update root/crate/window/sidebar module maps and local agent guidance.
6. Run formatting, compile, unit/integration/property, widget, accessibility, automation-doc/client, visual smoke/geometry, lint, and strict OpenSpec gates.
7. Rollback is commit-scoped by extraction seam; no data migration exists.

## Open Questions

None. If an extraction appears to require a new abstraction or behavior change, it is out of scope and must be handled as a separate proposal.
