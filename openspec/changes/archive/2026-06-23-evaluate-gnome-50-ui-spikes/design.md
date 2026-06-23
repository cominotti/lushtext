## Context

LushText now targets the GNOME 50 stack (`gtk4` 0.11 with `gnome_50`, `libadwaita` 0.9 with `v1_9`, and GtkSourceView on the current GNOME 50 runtime family). The dependency-refresh change deliberately kept runtime behavior unchanged and deferred GNOME 50 feature adoption to separate proposals.

Two deferred areas are ready for bounded spikes:

- Libadwaita 1.9 sidebar widgets: `AdwSidebar` is already used for shallow Notes and Local History browse rails, while `AdwViewSwitcherSidebar` only fits surfaces that are modeled as stable `AdwViewStack` pages.
- GTK builder diagnostics: `GTK_DEBUG=builder,builder-objects` can expose runtime GtkBuilder issues after Libadwaita and app composite widget types are registered, but it is diagnostic output rather than a stable user-facing configuration API.

The current workspace file tree is intentionally not a candidate for `AdwSidebar`: it owns persisted workspace folder sets, `All workspaces` scope, `GtkTreeListModel` expansion, file operations, file peek, async scanning, deep-folder focus, and watcher reconciliation. The current Blueprint pipeline already compiles, checks drift, enforces a template contract, and runs advisory lint; the builder spike must complement those gates instead of replacing them.

## Goals / Non-Goals

**Goals:**

- Produce two implementation-ready spike contracts: one for `AdwSidebar` / `AdwViewSwitcherSidebar`, and one for `GTK_DEBUG=builder`.
- Decide whether `AdwViewSwitcherSidebar` is worth a follow-up product proposal for a Document Activity or Inspector surface.
- Confirm which existing `AdwSidebar` uses need documentation, tests, or no further action.
- Define a runtime builder diagnostics lane that can load Libadwaita templates and app composite widgets.
- Preserve evidence: commands, logs, screenshots or snapshots where useful, limitations, and final recommendations.

**Non-Goals:**

- Do not rewrite the primary workspace file tree around `AdwSidebar`.
- Do not implement a new Document Activity, Inspector, document-properties replacement, or workspace navigation shell in this spike.
- Do not make `GTK_DEBUG` an end-user setting or a required runtime environment.
- Do not replace `make check-blueprint`, `make lint-blueprint`, `resources/ui/template-contract.json`, widget tests, or visual smoke.
- Do not change Cargo dependencies, Flatpak permissions, GSettings schemas, automation APIs, persistence formats, or user-visible UI behavior unless a later proposal explicitly scopes that work.

## Decisions

### Keep the Two Spikes Separate

Create two capability specs in one OpenSpec change because the work shares a GNOME 50 platform-adoption theme but has different evaluation surfaces and acceptance evidence.

Alternative considered: one broad "GNOME 50 UI features" spec. Rejected because sidebar adoption is a UX architecture question, while builder diagnostics is a validation/tooling question.

### Treat Sidebar Adoption as a Fit Matrix

The `AdwSidebar` / `AdwViewSwitcherSidebar` spike should inventory candidate surfaces, score each against widget fit, state extremes, and existing ownership, then recommend adopt, defer, reject, or split.

`AdwSidebar` remains appropriate for shallow, sectioned browse rails such as Notes and Local History. `AdwViewSwitcherSidebar` is only considered when the content is a small set of stable, page-like destinations in an `AdwViewStack`. The primary workspace file tree remains out of scope for replacement.

Alternative considered: immediately prototype `AdwViewSwitcherSidebar` in the document-properties area. Rejected because the current properties shell is already an adaptive secondary surface, and the spike first needs to prove whether a stable page model improves the product instead of creating navigation churn.

### Evaluate State Extremes Before Recommending UI Work

Any sidebar/view-switcher recommendation must cover no-context, representative, dense or awkward, and constrained geometry states. The spike must check command reachability, empty-state readability, item-region scrolling, header/back/close visibility, focus restoration, accessible names, and absence of unintended horizontal scrolling or fake rows.

Alternative considered: evaluate only a happy-path mock. Rejected because most LushText sidebar regressions historically appear in empty, dense, constrained, or transition-heavy states.

### Add Builder Diagnostics as a Runtime Lane

Standalone `gtk4-builder-tool validate` remains useful for GTK-only generated templates, but it cannot load every LushText template because many use Libadwaita types and app-owned composite widget classes. The spike should run a runtime harness with `GTK_DEBUG=builder,builder-objects` after GTK, Libadwaita, GResource, and LushText widget types are initialized.

Alternative considered: rely only on `gtk4-builder-tool validate`. Rejected because it fails before app initialization for templates such as `window.ui` that depend on `AdwApplicationWindow`.

### Classify Diagnostic Output Before Making It Blocking

The builder diagnostics spike should produce a classification table: actionable finding, known limitation, noisy-but-benign line, unsupported-host blocker, or candidate for a future blocking check. Only a later proposal should promote any diagnostic lane into `make check`.

Alternative considered: immediately fail CI on all builder debug output. Rejected because `GTK_DEBUG` output is diagnostic and may include version- or host-sensitive lines that need triage first.

## Risks / Trade-offs

- A spike can drift into implementation -> Keep tasks focused on inventory, prototypes only when needed for proof, notes, and recommendation artifacts.
- `AdwViewSwitcherSidebar` can look attractive but add a second navigation model -> Require a stable page model and state-extreme proof before recommending adoption.
- Builder diagnostics can produce noisy host-specific logs -> Capture tool/runtime versions and classify output before making it normative.
- Runtime harnesses can miss lazily opened dialogs -> Explicitly list which templates/surfaces were instantiated and which remain uncovered.
- Existing dependency-refresh artifacts are still unarchived -> Keep this change's edits isolated to `openspec/changes/evaluate-gnome-50-ui-spikes/`.

## Migration Plan

1. Run the sidebar/view-switcher inventory against current repo guidance, existing specs, and code surfaces.
2. Run focused runtime or widget proof only when needed to validate a candidate fit or state extreme.
3. Run the builder diagnostics probe with the smallest runtime harness that initializes Libadwaita and app composite widgets.
4. Record command lines, versions, logs, unsupported limitations, and recommendations in spike notes.
5. Produce follow-up recommendations: adopt now via separate proposal, defer, reject, or document as already covered.
6. Roll back by removing the spike notes or this OpenSpec change; no product code should be required to revert.

## Open Questions

- Should a future Document Activity or Inspector surface replace any existing document-properties groups, or sit beside them as a new workflow?
- Should builder diagnostics eventually become part of `make check`, a non-blocking advisory target, or only a manual debugging recipe?
- Which runtime harness is the best long-term home for builder diagnostics: widget tests, visual smoke, automation smoke, or a dedicated script?
