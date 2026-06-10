# LushText Automation

LushText exposes a small automation spine for agents, smoke tests, and developer
tools that need to drive the app through real user workflows and then assert
bounded state. Mutating behavior stays on normal GTK/GIO actions. The
app-owned D-Bus object is read-only: it describes the action catalog, returns a
bounded snapshot, and waits for tracked workflows to settle.

The first automation interface is:

- D-Bus name: `dev.cominotti.lushtext`
- Object path: `/dev/cominotti/lushtext/Automation`
- Interface: `dev.cominotti.lushtext.Automation1`
- Reference: [`automation-reference.md`](automation-reference.md)

LushText still uses full filesystem access. The automation surface does not
move LushText to portals-only document access, and portal diagnostics remain
diagnostic only. Actions that open file dialogs, save files, rename files,
delete files, or inspect workspaces keep using the existing full-permission app
paths and safety checks.

## What Is Exposed

`GetActionCatalog` returns JSON rows for every known app, window, menu, and
widget-scoped action that LushText currently documents. Each row includes the
action id, label, parameter type, state type, enablement rule, owning module,
surfaces, external activation safety, exposure level, docs anchor, and coverage
lanes. Exported app/window actions can be activated through GTK's
`org.gtk.Actions` interface. Widget-scoped rows are documented because they are
real user operations, but they still depend on the widget-local context that
owns the action group.

`GetSnapshot` returns bounded JSON for the active window: tabs, modified/save
state, load state, visible shell surfaces, preview/search/focus states,
workspace scope and persistence state, command-palette mode and counters,
notes/bookmark availability, local-history availability, content-search and
Replace All counters, notification/progress summaries, and named visual
geometry anchors for screenshot invariant tooling. Geometry anchors include
surface rectangles, allocation sizes, absence reasons, scale factor, scroll
anchor state, native minimap slider diagnostics, and app-computed pixel-anchor
crop hints for rendered effects such as the minimap viewport edges and fill;
they do not include rendered text. Native minimap diagnostics include bounded
visible-rect, adjustment, document-height, the raw upstream-style slider
estimate, and a vertically source-map-fitted visible slider bounds row that
preserves the native horizontal CSS outset, so failures can explain source-map
frame drift without handing screenshot tools an off-surface crop. Those
snapshot hints are diagnostic and crop-bounding data: visual
pass/fail for rendered-only effects comes from screenshot-derived detectors in
the visual geometry lane, not from Automation1 rectangles alone. It intentionally
avoids dumping document text, draft identifiers, note bodies, bookmark labels,
sidecar contents, local-history contents, command-palette result bodies, or
search result bodies. File-backed tabs can expose their path because paths are
already visible in the editor, sidebar, status, and properties surfaces.
Free-form text fields such as paths, titles, queries, replacement text, and
status messages are capped to 4 KiB and marked with ` [truncated]` if shortened.

`GetReadinessPredicates` returns the named app-owned readiness predicates and
their blockers. `WaitForReady(predicate, timeout_msec)` is the preferred
synchronization point for agents after activating an action, opening a file,
changing search state, or waiting for preview/search/save/workspace work. Use
the narrowest predicate that matches the workflow under test, such as
`file-open-complete`, `search-complete`, `save-complete`, or
`workspace-refresh-complete`. Screenshot scenario helpers should use
`visual-geometry-settled` before capture so layout, adaptive shell state,
minimap refresh/debounce, and visual workflow blockers have settled.
`WaitForIdle(timeout_msec)` remains as a compatibility alias for waiting on the
broader `idle` predicate.

When a wait cannot report readiness, its status distinguishes
`predicate-timeout`, `workflow-failure`, `automation-unavailable`,
`unsupported-host-tooling`, and `unknown-predicate` cases.

`GetWorkflowEvents` returns the bounded state-change event log observed by the
automation adapter. Events use stable workflow IDs such as `file-load`, `save`,
`search`, `workspace-refresh`, `content-search`, `replace-preview`,
and `session-restore`, with `started` and `finished` phases. The current
implementation records equivalent readiness-state changes when automation
snapshots, event reads, or readiness waits observe those workflows; it does not
expose document contents or a private mutation channel. Recovery restore remains
a named readiness predicate until LushText has a dedicated recovery-specific
live event source.

## Quick Inspection

Run LushText in a D-Bus session, then use the reusable client for ordinary
inspection:

```sh
scripts/lushtext-automation.py catalog --json
scripts/lushtext-automation.py snapshot --field window.tab_count
scripts/lushtext-automation.py wait search-complete --timeout-ms 5000 --json
scripts/lushtext-automation.py events
```

The client keeps a stable result envelope for agents (`ok`, `status`,
`command`, `detail`, `data`), rejects unsupported or widget-scoped actions
based on the action catalog, and renders typed GVariant parameters for normal
`org.gtk.Actions` activation. Read commands normally return status `ok`;
readiness waits return `ready`, `predicate-timeout`, `unknown-predicate`, or
`workflow-failure`; action validation distinguishes `unknown-action`,
`unsupported-action`, and `parameter-mismatch`; artifact review uses
`artifact-error`, visual-specific failures such as
`visual-comparison-failed`, `state-mismatch`, and `warning-scan-failed`, or the
distinct successful `artifact-skipped` status:

```sh
scripts/lushtext-automation.py action win.set-search-query --string needle
scripts/lushtext-automation.py action win.set-sidebar-visible --bool true
scripts/lushtext-automation.py action win.select-tab --uint32 0
scripts/lushtext-automation.py action win.show-help-overlay
```

To review smoke output after a run:

```sh
scripts/lushtext-automation.py artifact-summary build/smoke/automation --json
```

The raw D-Bus calls remain useful for debugging the protocol directly. Inspect
the read-only automation object with:

```sh
gdbus introspect --session \
  --dest dev.cominotti.lushtext \
  --object-path /dev/cominotti/lushtext/Automation
```

Fetch the action catalog:

```sh
gdbus call --session \
  --dest dev.cominotti.lushtext \
  --object-path /dev/cominotti/lushtext/Automation \
  --method dev.cominotti.lushtext.Automation1.GetActionCatalog
```

Fetch the current bounded state:

```sh
gdbus call --session \
  --dest dev.cominotti.lushtext \
  --object-path /dev/cominotti/lushtext/Automation \
  --method dev.cominotti.lushtext.Automation1.GetSnapshot
```

Wait up to five seconds for search work to settle:

```sh
gdbus call --session \
  --dest dev.cominotti.lushtext \
  --object-path /dev/cominotti/lushtext/Automation \
  --method dev.cominotti.lushtext.Automation1.WaitForReady search-complete 5000
```

Or use the compatibility idle wait:

```sh
gdbus call --session \
  --dest dev.cominotti.lushtext \
  --object-path /dev/cominotti/lushtext/Automation \
  --method dev.cominotti.lushtext.Automation1.WaitForIdle 5000
```

Read recent workflow state-change events:

```sh
gdbus call --session \
  --dest dev.cominotti.lushtext \
  --object-path /dev/cominotti/lushtext/Automation \
  --method dev.cominotti.lushtext.Automation1.GetWorkflowEvents
```

For a real-process proof that does not require screenshots or AT-SPI, run:

```sh
make automation-smoke
```

The smoke lane launches the debug binary under an isolated `dbus-run-session`
and headless Mutter compositor, introspects the automation object, records
`org.gtk.Actions.List` and representative `Describe` outputs for the app/window
action groups, reads the action catalog, readiness predicate list, and
snapshots, waits on named readiness predicates, reads `workflow-events.json`,
activates target-state actions and verifies their state against snapshot
fields, runs `scripts/lushtext-automation.py` against the live app for catalog,
snapshot, field extraction, predicates, idle/search waits, events, and a safe
`win.set-search-query` activation, activates
`win.set-search-query("needle")` through `org.gtk.Actions`, then writes bounded
artifacts under `build/smoke/automation`. The final
`assertions/runtime-warning-scan.txt` artifact fails the lane on unexpected
GTK, GDK, Libadwaita, GIO, D-Bus, portal, AT-SPI, or filesystem warnings while
allowing known headless portal/accessibility and compositor-shutdown noise.
`scripts/lushtext-automation.py artifact-summary build/smoke/automation`
returns a compact pass/fail/skip summary over the same manifest and artifacts.

Automation and crash-recovery smoke runs write `scenario-manifest.json`; visual
smoke writes one `assertions/<capture>-manifest.json` per capture; visual
geometry smoke writes one case-level `scenario-manifest.json` plus a root
`summary.json`. The manifest is the review index for the run: it records
`schema_version`, the scenario id, launch mode or scenario type, helper
arguments, fixture setup, command/action steps, waits, `state_assertions`,
screenshot and AT-SPI assertion slots, protected and allowed-changing visual
regions, geometry snapshot rows, `dbus_summaries`, warning-scan status,
selected environment details, `bounded_artifact_policy`, and any skip or
failure reason. Large payloads stay in sibling artifacts such as snapshots,
logs, action lists, screenshots, comparison crops, and warning reports; the
manifest embeds only bounded text and relative artifact paths.

For screenshot-backed state, run:

```sh
make visual-smoke
```

The visual lane uses the same isolated headless Mutter helper, drives exported
window actions, waits through Automation1, saves `automation-snapshot.json` for
each capture, and scans logs for unexpected GTK/Adwaita/GDK/accessibility
warnings. Each capture also writes a bounded manifest that indexes its
screenshot, state assertions, AT-SPI excerpts, D-Bus summaries, warning scan,
environment file, and skip/failure reason. Current captures cover
search/minimap, normal document properties,
compact document properties, constrained document properties, normal Markdown
preview, constrained Markdown preview, zero-folder workspace, representative
workspace, dense/awkward workspace names and folders, constrained workspace,
workspace-refresh readiness, no-notes browser, few notes/bookmarks,
dense notes/bookmarks, constrained notes/bookmarks, command palette files,
commands, notes mode, no-results, dense files, dismissed state, short-layout
chrome, dark style, and recovery startup diagnostics.

For same-session pixel invariants, run:

```sh
make visual-geometry-smoke
```

The visual geometry lane launches each scenario in one isolated headless Mutter
session, waits for `visual-geometry-settled`, then also waits for scenario
specific final allocation predicates such as fully shown or hidden workspace
sidebar geometry before capturing screenshots. It captures before/after
screenshots and bounded `visual_geometry` snapshots from the same app process,
including native minimap visible-rect and adjustment diagnostics, compares
protected regions with exact PNG crops and declared masks, asserts
allowed-changing region relationships through geometry anchors, verifies
declared pixel anchors and relative anchor deltas, verifies those pixel anchors
again across the warmup and final frame for each capture step, scans runtime
warnings, and writes per-case manifests plus comparison reports under
`build/smoke/visual-geometry`. Reports include final sidebar/editor/minimap
geometry, screenshot-derived pixel rows, small crop paths, and app-vs-rendered
diagnostics when Automation1 anchors stay stable but rendered rows move.
Per-step `*-rendered-anchor-stability.json` files record warmup-vs-final row
stability so stale native frames fail before the before/after comparison. The
root summary records `verified_invariant_ids` plus
`pixel_verified_invariant_ids`; relevant pixel-sensitive visual diffs are not
considered covered unless the required invariant id is present in the
pixel-verified list and backed by per-case pixel evidence. It skips with an
explicit host-tooling reason when the compositor, PipeWire, D-Bus, GSettings, or
screenshot path is unavailable; skipped invariant coverage is not counted as
verified.

When a geometry bug only reproduces in a live user window, first capture the
bounded live state:

```sh
scripts/lushtext-automation.py visual-geometry-capture build/smoke/live-visual-geometry \
  --color-scheme force-light --word-wrap true --fixture-kind plain-lines
```

The command writes `live-snapshot.json`, `capture-manifest.json`, and a generated
scenario under `generated-scenarios/`, then records the exact
`scripts/visual-geometry-smoke.py --scenario-dir ...` replay command. If theme,
word wrap, fixture kind, direction, or viewport position cannot be inferred, it
exits with `missing-field` and asks for explicit overrides. Optional portal
screenshots are context-only; invariant proof is the Automation1 snapshot plus
the generated headless visual-geometry replay.
`make check-visual-proof-policy` is the fast companion gate: when local
visual-sensitive files have changed, it requires a passing
unfiltered `build/smoke/visual-geometry/summary.json` whose recorded
visual-sensitive diff fingerprint still matches the current worktree and whose
`pixel_verified_invariant_ids` cover any named pixel invariants required by the
changed files before `make check-policy` can pass.

The crash-recovery lane also uses Automation1 after relaunch: it waits for
`recovery-restore-complete`, writes `relaunch-automation-snapshot.json`, and
asserts restored file-backed and untitled tabs plus draft metadata and recovery
diagnostic evidence before accepting the relaunch screenshot. Its
`scenario-manifest.json` indexes the before/after metadata, relaunch snapshot,
AT-SPI recovery tree, warning scan, screenshot, and skip/failure reason.

For accessibility-backed anchors, run:

```sh
make accessibility-smoke
```

This lane keeps the accessibility bridge enabled, captures shell, command
palette, and notes-browser states through the same isolated headless Mutter
helper, and verifies stable AT-SPI names/roles plus the command-palette focus
target. It skips only with an explicit host-runtime reason when AT-SPI support
is unavailable, and action/D-Bus assertions do not count as accessibility
coverage on their own. The stable anchors are documented in
`docs/automation-reference.md` and checked by `make check-automation-docs`.

## Driving Actions

Mutation is intentionally routed through the same actions that menus,
shortcuts, buttons, and command-palette entries use. This keeps automation close
to the user contract and avoids a private widget-mutation back door.

Use the action catalog first. It tells you which actions are exported,
widget-scoped, diagnostic-only, or known gaps. Stable setup actions include:

- `win.set-search-query` with a string parameter
- `win.set-sidebar-visible` with a boolean parameter
- `win.set-properties-visible` with a boolean parameter
- `win.set-minimap-visible` with a boolean parameter
- `win.set-search-panel-visible` with a boolean parameter
- `win.set-focus-mode` with a boolean parameter
- `win.set-preview-pane-visible` with a boolean parameter
- `win.set-preview-mode` with a boolean parameter
- `win.select-tab` with a zero-based unsigned tab index
- `win.set-command-palette-mode` with `all`, `files`, `notes`, or `commands` while the command palette is visible
- `win.set-command-palette-query` with a string parameter while the command palette is visible
- `win.set-notes-browser-query` with a string parameter while Browse Notes is visible
- `win.select-notes-browser-row` with a zero-based unsigned visible row index
- `win.open-notes-browser-selection` to press the visible browser's `Open` action

Use target-state actions for scenario setup whenever possible. They are easier
to reason about than parity toggles because repeated calls converge on the same
state. After each mutation, call `WaitForReady` with the narrowest applicable
predicate and then assert `GetSnapshot`.

## Safety Rules

Automation must not bypass user safety flows. Save, close, discard, replace,
rename, delete, file-dialog, and workspace operations keep their existing
confirmation, durable-write, modified-buffer, and context rules. If an action is
disabled or requires a context that is not present, an agent should treat that
as a normal app state, not force the widget tree from the side.

Snapshots must stay bounded. They may report metadata that the UI already
surfaces or that is needed to wait on real workflows, such as paths, titles,
modified flags, counts, visibility, modes, selected workspace identity, and
background-save flags. The selected workspace ID is a documented stable
automation identity for the visible workspace selector; draft IDs, note IDs,
bookmark IDs, local-history snapshot IDs, and sidecar identity keys remain
private. Free-form text fields are byte-capped so repeated polling stays cheap
even after a user pastes a very large query. Snapshots must not include buffer
contents, complete search result text, note bodies, bookmark labels, draft
contents, local-history snapshots, or private persistence identifiers.

The automation object is not a remote-control security boundary. It is exposed
on the session bus of the user running LushText and is meant for same-user
agents, smoke tests, and developer tools.

## Troubleshooting

If the object is missing, confirm that LushText owns `dev.cominotti.lushtext`
in the same session bus where you are calling `gdbus`. A unique GTK app may hand
off to an existing process, so make sure the running process is the build you
intended to inspect.

Unsupported or unavailable calls fail with stable D-Bus error names documented
in the developer reference, including `Error.Unavailable`, `Error.Internal`,
and `Error.UnknownMethod`. Predicate waits return stable statuses, including
`predicate-timeout`, `workflow-failure`, `automation-unavailable`,
`unsupported-host-tooling`, and `unknown-predicate`. The client adds stable
wrapper statuses for everyday scripts, including `ok`, `ready`, `usage-error`,
`dbus-error`, `unknown-action`, `unsupported-action`, `parameter-mismatch`,
`artifact-error`, and `artifact-skipped`.

If `WaitForReady` returns `ok=false`, read its status and detail string. If
`WaitForIdle` returns `false`, read the detail string and the snapshot's
`idle_blocker`. Common blockers include app startup, file loading, save work,
draft autosave, workspace persistence, workspace filter animation, workspace
search, preview animation, editor search indexing, session restore,
command-palette index debounce, Replace All preview generation, and
close-safety work.

Headless GTK sessions may still print portal, AT-SPI, or compositor cleanup
warnings. Treat those as runtime diagnostics and correlate them with
`WaitForReady`, `WaitForIdle`, and `GetSnapshot`; do not infer that LushText has
migrated to a portals-only model.

Flatpak permission drift is guarded separately. `make check-flatpak-permissions`
parses `build-aux/dev.cominotti.lushtext.Flatpak.json` and fails if the
intentional `--filesystem=host` permission disappears.
`make portal-sandbox-smoke` also writes `permission-posture.txt`,
`portal-names.txt`, `session-bus-names.txt`, `flatpak-permissions.txt` when an
installed Flatpak exists, and `summary.txt` fields that keep
`permission_posture=full-filesystem` and `portals_only_migration=false`
explicit in portal/sandbox artifacts.

## Desktop Activation Metadata

LushText does not currently set `DBusActivatable=true` or advertise extra
desktop-file `Actions=` entries. The supported D-Bus automation surface starts
after the process is running: agents can inspect app/window `org.gtk.Actions`,
activate documented GTK actions, and read the read-only Automation1 snapshot.

Keep `Exec=lushtext %U` as the launch contract until a change proves the
freedesktop D-Bus activation path for every packaging lane. The Desktop Entry
Specification says launchers should ignore `Exec` when `DBusActivatable=true`
and instead call `org.freedesktop.Application.Activate`, `Open`, or
`ActivateAction` on the app's well-known bus name and object path. For LushText,
that proof must cover native installs, temporary development staging, Flatpak,
Snap, CLI opens, MIME opens, file-manager opens, multi-file forwarding,
startup-notification or activation-token data, duplicate-tab behavior, and
failed-placeholder activation recovery. Until that matrix is green, desktop
D-Bus activation metadata remains intentionally disabled and this app keeps the
existing `Exec`/MIME behavior unchanged.

## Maintenance

Every change that adds, removes, renames, or changes an exported action, action
parameter, action state, D-Bus method/property, snapshot field, readiness
predicate or blocker, workflow event field, scenario-helper flag, exposed state
meaning, documented privacy boundary, scenario manifest field, or scenario
artifact meaning must update this guide and
[`automation-reference.md`](automation-reference.md) in the same change.

Run the drift gate before review:

```sh
make check-automation-docs
make check-flatpak-permissions
make automation-smoke
```

`make check-policy`, `make pre-commit`, and `make check` include these gates so
documentation drift and Flatpak permission drift are caught with the normal fast
policy checks.
