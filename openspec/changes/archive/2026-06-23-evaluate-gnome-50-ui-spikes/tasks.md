## 1. Spike Setup

- [x] 1.1 Re-read `openspec/changes/evaluate-gnome-50-ui-spikes/proposal.md`, `design.md`, and both spec files before starting implementation work.
- [x] 1.2 Create spike evidence notes under `openspec/changes/evaluate-gnome-50-ui-spikes/` for sidebar/view-switcher findings and builder-diagnostics findings.
- [x] 1.3 Record the current GTK, Libadwaita, Blueprint compiler, and `gtk4-builder-tool` versions or the exact reason any version cannot be queried.

## 2. AdwSidebar / ViewSwitcher Spike

- [x] 2.1 Inventory current `AdwSidebar` uses in Notes and Local History, linking each to existing specs, code locations, and coverage.
- [x] 2.2 Inventory candidate surfaces for `AdwSidebar` or `AdwViewSwitcherSidebar`, including document properties, file health, notes, history, encoding, workspace sidebar, and any Document Activity or Inspector concept.
- [x] 2.3 Classify each candidate as `AdwSidebar` fit, `AdwViewSwitcherSidebar` fit, already covered, defer, or reject, with rationale tied to data shape and workflow ownership.
- [x] 2.4 Explicitly record why the primary workspace file tree remains on `GtkListView`, `GtkTreeListModel`, and `GtkTreeExpander`.
- [x] 2.5 For any `AdwViewSwitcherSidebar` candidate, define the stable `AdwViewStack` pages it would need and reject the candidate if stable pages cannot be named.
- [x] 2.6 Evaluate no-context, representative, dense or awkward, and constrained-geometry states for each viable sidebar/view-switcher candidate.
- [x] 2.7 Run the smallest existing widget, smoke, screenshot, or documentation-backed probe needed to support the sidebar/view-switcher recommendation, or record why no runtime probe is needed.
- [x] 2.8 Produce a final sidebar/view-switcher recommendation for each candidate: adopt in a separate proposal, defer, reject, or already covered.

## 3. GTK Builder Diagnostics Spike

- [x] 3.1 Run or reference `make check-blueprint`, `make lint-blueprint`, and the template-contract check as the existing Blueprint validation baseline.
- [x] 3.2 Probe `gtk4-builder-tool validate` against GTK-only and Libadwaita/app-composite generated templates, recording which templates validate standalone and which fail because the standalone tool lacks initialized types.
- [x] 3.3 Run an initialized LushText runtime, widget, smoke, or equivalent harness with `GTK_DEBUG=builder,builder-objects`, capturing stdout, stderr, command line, runtime/tool versions, and covered surfaces.
- [x] 3.4 Instantiate no-context startup, a representative open document, and any intentionally selected lazy surfaces needed to cover generated templates.
- [x] 3.5 List template-backed dialogs, popovers, or secondary surfaces not instantiated by the diagnostics run as uncovered or covered by separate commands.
- [x] 3.6 Classify each builder diagnostic line as actionable defect, known standalone limitation, benign diagnostic noise, unsupported-host blocker, or candidate for future advisory or blocking enforcement.
- [x] 3.7 For each actionable diagnostic, record the affected template or surface, exact diagnostic text, likely owning source, and recommended follow-up path.
- [x] 3.8 Recommend whether builder diagnostics should remain a manual recipe, become an advisory target, join a widget or smoke mode, or become a future blocking check.

## 4. Documentation And Closeout

- [x] 4.1 Update `docs/next/gnome-50-api-opportunities.md` if the sidebar/view-switcher spike changes or supersedes its current follow-up note.
- [x] 4.2 Update `docs/blueprint-validation.md` if the builder diagnostics spike establishes a useful manual or advisory workflow.
- [x] 4.3 Confirm no Cargo dependency, Flatpak permission, schema, app-data format, automation API, or user-visible UI behavior changes were introduced.
- [x] 4.4 Run `openspec validate evaluate-gnome-50-ui-spikes --strict`.
- [x] 4.5 Run `openspec validate --changes --strict`.
- [x] 4.6 Run `git diff --check`.
- [x] 4.7 Summarize both spike outcomes, evidence artifacts, uncovered areas, and follow-up proposal recommendations.
