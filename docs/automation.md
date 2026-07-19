# LushText Automation

LushText exposes a small automation spine for agents, smoke tests, and developer
tools that need to drive the app through real user workflows and then assert
bounded state. Mutating behavior stays on normal GTK/GIO actions. The
app-owned D-Bus object is read-only: it describes the action catalog, returns a
bounded snapshot, and waits for tracked workflows to settle.

Internally, Automation1 now projects readiness and workflow observations through
the `gtk-lush-proof-spine` value objects used by the extracted proof toolchain.
That backing layer does not define a new D-Bus contract: the object path,
interface name, method signatures, status strings, predicate names, and snapshot
JSON documented here remain the Automation1 surface.

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
Replace All counters, notification/progress summaries, accessibility readiness
diagnostics, and named visual geometry anchors for screenshot invariant tooling.
Accessibility readiness mirrors the `accessibility-settled` predicate as a
bounded ready flag plus first blocker string; it does not expose document, note,
history, or result bodies. Geometry anchors include
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
`workspace-refresh-complete`. Workspace refresh readiness includes watcher
lifecycle work, one bounded coalescing mailbox notice, targeted/full-refresh
debounce state, active directory scans and batched row application, persistence,
filter animation, command-palette indexing, and bounded note-source refreshes. Current command-palette query
work is tracked by `search-complete`, `visual-geometry-settled`,
`accessibility-settled`, and broad `idle` readiness. Palette queries retain at
most one active worker plus one latest request, and cancelled obsolete
generations stop blocking readiness once their bounded completion releases
active ownership. A watcher generation that ends in a reported startup or
disconnect failure is settled as unavailable rather than remaining a
permanently pending lifecycle operation.
The compatibility `preview-animation` blocker remains pending through the
current Markdown planner/projection/image generation, one deferred latest
render, off-main plain-payload retirement, bounded detached GTK retirement,
layout switching, and embedded-widget repair. Replace Preview readiness likewise
includes worker-side checked-identity selection and retirement of rejected or
stale plain payloads before the generation is terminal.
Workspace-search snapshots expose only whether replacement text is present;
they never serialize the replacement template or expanded replacement content.
Screenshot scenario helpers should use
`visual-geometry-settled` before capture so layout, adaptive shell state,
workspace sidebar transitions, minimap refresh/debounce, and visual workflow
blockers have settled. Accessibility smoke captures use
`accessibility-settled` before querying AT-SPI so focus targets, recycled rows,
search or preview rendering, and announcement-sensitive workflow state have
settled, then add narrower scenario waits such as `search-complete` when a
specific workflow needs its own proof.
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

For Rust-backed visual proof roots, `artifact-summary` preserves the same
result envelope while surfacing proof engine metadata, schema version,
scenario source, parity report, environment report, and missing host
capabilities. Python oracle or diagnostic artifacts remain labeled through the
same `engine` and `parity` fields rather than being treated as default Rust
proof.

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
environment file, and skip/failure reason.

The helper discovers its checkout through Git and resolves bundled AT-SPI tools
relative to its own location. Renamed or relocated development fixtures can
override the repository root, binary, application ID/object path, Automation1
interface, and GSettings schema/directory through documented command flags or
the matching `LUSHTEXT_DEBUG_*` environment variables; the production defaults
remain unchanged.

Current captures cover
search/minimap, normal document properties,
compact document properties, constrained document properties, preview-only
Markdown preview, constrained preview-only Markdown preview, side-by-side
Markdown preview, constrained side-by-side Markdown preview, zero-folder workspace, representative
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
specific final allocation predicates such as fully shown, fully hidden, or
compact-overlay workspace sidebar geometry before capturing screenshots. It captures before/after
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
The lane also includes Open popover cases for empty, single-row,
representative, all-closed, all-open, dense, awkward-label, and 720p-height
recent-document states. The Open popover cases also assert the header's
`header-open-menu-button` surface remains before `header-new-tab-button` while
the popover is active.
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

The command writes `live-snapshot.json`, `capture-manifest.json`, and a
generated scenario under `generated-scenarios/`, then records the exact
`cargo run -q -p cargo-gtk-proof -- run --scenario-dir ...` replay command. If
theme, word wrap, fixture kind, direction, or viewport position cannot be
inferred, it exits with `missing-field` and asks for explicit overrides.
Optional portal screenshots are context-only; invariant proof is the
Automation1 snapshot plus the generated headless visual-geometry replay.
`make check-visual-proof-policy` is the fast companion gate: when local
visual-sensitive files have changed, it requires a passing
unfiltered `build/smoke/visual-geometry/summary.json` whose recorded
visual-sensitive diff fingerprint still matches the current worktree and whose
`pixel_verified_invariant_ids` cover any named pixel invariants required by the
changed files before `make check-policy` can pass. The Make target now runs
`cargo gtk-proof policy`; `scripts/check-visual-proof-policy.py` remains a
compatibility shim for command callers and an importable metadata helper for the
Python live runner.

For parity investigation or a diagnostic cross-check, run the legacy visual
runner through Rust supervision explicitly:

```sh
cargo gtk-proof run --oracle python --artifact-dir build/smoke/visual-geometry
```

That command records bounded Rust supervision metadata and the
`python-visual-oracle` engine. It is diagnostic/oracle output, not default Rust
proof, and skipped oracle summaries still do not satisfy visual-sensitive proof
policy.

The crash-recovery lane also uses Automation1 after relaunch: it waits for
`recovery-restore-complete`, writes `relaunch-automation-snapshot.json`, and
asserts restored file-backed and untitled tabs plus draft metadata and recovery
diagnostic evidence before accepting the relaunch screenshot. The fixture also
distinguishes an accepted generation from a newer pre-debounce edit terminated
by `SIGKILL`, and crosses the 64 MiB aggregate eager cap so one valid draft must
settle through lazy restore. Its
`scenario-manifest.json` indexes the before/after metadata, relaunch snapshot,
AT-SPI recovery tree, warning scan, screenshot, and skip/failure reason.

For accessibility-backed anchors, run:

```sh
make accessibility-smoke
```

This lane keeps the accessibility bridge enabled, captures shell, editor,
search, Open popover, command-palette, workspace, properties, preferences,
Markdown preview, notes-browser, and local-history states through the same isolated headless
Mutter helper, and verifies stable AT-SPI names/roles plus focus and text
interface evidence where the host exposes it. Each capture writes a bounded
`assertions/<scenario>-manifest.json`, the run writes
`assertions/accessibility-assertions.jsonl`, and the root `summary.json`
records scenario manifests, matrix row coverage, focused-run filters,
readiness waits, screenshots, warning status, assertion artifacts, environment
metadata, and unsupported-host reasons. Each manifest distinguishes stable
public anchors from seeded fixture-only anchors and declares the bounded
artifact/privacy boundary for the captured text. It skips only with an explicit
host-runtime reason when AT-SPI support is unavailable, and action or D-Bus
assertions do not count as accessibility coverage on their own. The stable
anchors are documented in
`docs/automation-reference.md` and checked by
`make check-automation-docs`.

For focused debugging, list scenario names and run one surface or a glob:

```sh
scripts/run-accessibility-smoke.sh --list-cases
scripts/run-accessibility-smoke.sh --case open-popover-*
```

Scenarios that need interleaved action/readiness/action sequences, such as
workspace Replace All preview followed by confirmation, use the capture helper's
ordered `--step KIND:VALUE` flag instead of relying on grouped legacy action
flags. Ordered steps may drive normal app/window actions, wait for Automation1
or AT-SPI state, click visible AT-SPI buttons, or set the named editor text
through AT-SPI's editable-text interface for modified-buffer close-safety proof.

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
- `win.set-search-panel-query` with a string parameter while workspace search is visible
- `win.set-focus-mode` with a boolean parameter
- `win.set-preview-pane-visible` with a boolean parameter
- `win.set-preview-mode` with a boolean parameter
- `win.select-tab` with a zero-based unsigned tab index
- `win.set-command-palette-mode` with `all`, `files`, `notes`, or `commands` while the command palette is visible
- `win.set-command-palette-query` with a string parameter while the command palette is visible
- `win.set-open-popover-query` with a string parameter while the recent-document Open popover is visible
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
draft persistence or recovery resolution, workspace persistence, workspace filter animation,
workspace search or detached-result retirement, Markdown planning/projection/image work,
deferred Markdown work, plain-payload worker retirement, or detached-render retirement,
preview layout/code-block repair, editor search indexing, session
restore, command-palette index debounce, Replace All preview generation, and
close-safety work.

Workspace persistence remains a blocker from the first requested mutation
until the newest generation is durably saved. This includes debounce waiting,
an active write, a newer snapshot waiting behind an older write, bounded retry
backoff, and a failed generation awaiting an explicit retry or later mutation.
Window close bypasses debounce and waits for that newest durable terminal; a
write failure cancels close and leaves the same retryable blocker visible.

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
