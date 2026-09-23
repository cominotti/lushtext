## ADDED Requirements

### Requirement: Automation can reveal a workspace path in the sidebar

LushText SHALL export a window action `win.reveal-workspace-path` taking one string parameter, an absolute path inside a configured workspace folder. The action SHALL be registered in the action catalog with scope `window`, parameter type `string`, surface `dbus-action`, exposure `exported`, and activation safety `diagnostic-only`, and SHALL be documented in `docs/automation.md` and `docs/automation-reference.md`. Activation SHALL expand every collapsed ancestor directory of the target through the sidebar's existing bounded, generation-guarded directory-scan path (never `autoexpand`, never a synchronous directory read on the GTK thread), then select the target row and scroll it into view through the file-tree list's own `scroll_to`, so that the workspace section's `ViewportSliceBin` forwards the request exactly as it does for user navigation. The action SHALL NOT change the workspace scope, the requested sidebar visibility, workspace persistence, file contents, or any open tab.

A reveal that cannot complete SHALL end in exactly one typed failure: `not-in-workspace` (the path is under no configured folder), `outside-scope` (the path's workspace is hidden by the current workspace scope), `not-found` (a path component does not exist), `hidden-by-visibility` (a component is excluded by the workspace entry visibility rule), `beyond-entry-cap` (a component lies past the per-directory 10,000-entry cap), `superseded` (a newer reveal request replaced it), or `unsettled` (the row did not come to rest inside the sidebar viewport within the bounded settle budget). A newer reveal request SHALL supersede an older one; at most one reveal SHALL be active per window.

#### Scenario: Revealing a row beyond the realized-row cap
- **WHEN** a workspace folder holds 400 files and automation activates `win.reveal-workspace-path` with the path of the last file
- **THEN** the file's row is selected, drawn wholly inside the sidebar viewport, and the reveal reports `revealed`

#### Scenario: Revealing a nested path expands its ancestors
- **WHEN** the target lies two collapsed directories below a workspace folder
- **THEN** both directories are expanded through bounded scans, the target row is selected and in view, and no other directory is expanded

#### Scenario: A path outside every workspace fails with a typed reason
- **WHEN** automation activates the action with a path under no configured workspace folder
- **THEN** the reveal reports `not-in-workspace`, the sidebar selection and scroll position are unchanged, and no scan is started

#### Scenario: A hidden path is not revealed
- **WHEN** the target has a dot-named component while `workspace-show-hidden-files` is off
- **THEN** the reveal reports `hidden-by-visibility` and the visibility setting is not changed

#### Scenario: A newer request supersedes an older one
- **WHEN** a second reveal is activated while the first is still expanding ancestors
- **THEN** the first reports `superseded`, its pending scan results do not select or scroll anything, and only the second target is revealed

### Requirement: Workspace path reveal has deterministic readiness and bounded evidence

The readiness blocker `workspace-path-reveal` SHALL be present from reveal activation until the reveal reaches `revealed` or a typed failure, including while an ancestor scan is pending, while the target row is not yet in the flattened tree model, and while the section's `ViewportSliceBin` reports a pending forwarded outer-scroll request. The readiness predicate `workspace-path-revealed` SHALL wait on `app-startup`, `workspace-tree-refresh`, `workspace-persist`, `workspace-filter-animation`, and `workspace-path-reveal`, and SHALL report `workflow-failure` when the latest reveal ended in a typed failure. The aggregate predicates `visual-geometry-settled`, `accessibility-settled`, and `idle` SHALL include the new blocker. A `workspace-path-reveal` workflow event SHALL be derived from the blocker like the existing workflow events.

The snapshot object `window.workspace` SHALL expose, projected from the sidebar's typed evidence surface, the latest reveal's status (`none`, `pending`, `revealed`, or one typed failure), its request generation, and the revealed row's depth. It SHALL NOT expose directory listings, file contents, or any private persistence identifier. The visual-geometry snapshot SHALL expose the named surfaces `workspace-sidebar-viewport`, `workspace-revealed-row`, and `workspace-revealed-section-header`, plus at most eight neighbouring mapped rows of the revealed section as unlabelled window-relative rectangles in list order; when no reveal is current these surfaces SHALL be reported absent with a reason.

#### Scenario: Waiting for a reveal
- **WHEN** automation activates a reveal and then calls `WaitForReady("workspace-path-revealed", timeout)`
- **THEN** the wait returns ready only after the target row is selected, in view, and its bin has no pending forwarded request, and the snapshot reports status `revealed`

#### Scenario: Waiting for a failed reveal
- **WHEN** the reveal ends in `not-found`
- **THEN** `WaitForReady("workspace-path-revealed", timeout)` reports `workflow-failure` and the snapshot reports status `not-found`

#### Scenario: Reveal geometry is bounded and unlabelled
- **WHEN** a reveal has completed in a section with 400 rendered-capable rows
- **THEN** the visual-geometry snapshot contains the three named reveal surfaces and no more than eight neighbour row rectangles, none of which carries a file name
