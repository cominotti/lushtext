# Automation no-widening proof (task 8.5)

**Proven by diff, not asserted.** `make automation-smoke` was run on a baseline
tree at `origin/main` and on the changed tree, each under isolated headless Mutter
and a private D-Bus session with the same fixtures, and every JSON artifact was
compared.

## Method

```
$ git worktree add /tmp/lt-base origin/main
$ (cd /tmp/lt-base && make automation-smoke)     # before
$ make automation-smoke                          # after
```

Then all 22 `assertions/*.json` artifacts were compared under two relations: a
**shape** comparison (key structure and value types) and a **value** comparison
with a declared normalization applied.

## Result: 22 of 22 identical, zero normalization actually needed

| Artifact | Result |
| --- | --- |
| `snapshot-initial.json`, `snapshot-after-search.json` | **identical** |
| `client-snapshot.json`, `client-snapshot-tab-count.json` | **identical** |
| `readiness-predicates.json`, `client-predicates.json` | **identical** |
| `action-catalog.json`, `client-catalog.json` | **identical** |
| all 6 `action-state-*.json` | **identical** |
| `workflow-events.json`, `client-events.json` | **identical** |
| `client-wait-idle.json`, `client-wait-search-complete-client.json` | **identical** |
| `window-action-set-search-query.json`, `client-action-set-search-query.json` | **identical** |
| `client-sanity-summary.json` | **identical** |

No artifact was only-in-before or only-in-after, and **no key changed shape**.

**A normalization list was prepared and then not needed.** Fixture-dependent keys
(`path`, `title`, `file_size`, `app_version`, `build_profile`, workspace roots,
recent documents, query text) were going to be masked, on the grounds that they
describe the *fixture* rather than the *contract*. In the event the raw values
matched too, because both runs use the same seeded fixtures under the same
isolated XDG state. **Recording the list anyway**, because a future run on a
differently-seeded fixture will need it, and because an unstated normalization is
how a "zero differences" claim quietly stops meaning anything.

## The fields task 8.1 names, confirmed specifically

| Field | Before | After |
| --- | --- | --- |
| `local_history.browse_available` | `true` | `true` |
| `local_history.automatic_capture_available` | `true` | `true` |
| `local_history.availability` | `"full"` | `"full"` |
| `local_history.active_document_file_backed` | `true` | `true` |
| `tabs[].draft_present` | `true` | `true` |
| every readiness predicate, including `session-restore-complete` and `recovery-restore-complete` | — | **identical object** |
| `idle_blocker` | `null` | `null` |

The `local_history` object is the one that **changed its source**: it now projects
from `LocalHistoryEvidence` instead of re-deriving from widgets. Its values are
unchanged, which is the whole point.

## The drift gate had to grow, and the growth was proved

`local_history` is a **third** surface projecting into the documented map, so it
was registered in `scripts/check-automation-docs.py`'s `EVIDENCE_PROJECTIONS`
alongside `SearchPanelEvidence`, `CommandPaletteEvidence`, `SaveEvidence`, and
`LoadEvidence`.

**The extension was verified by rejection, not by assertion.** With the row
documented but the projection unregistered, `make check-automation-docs` passed
while `LocalHistoryEvidence::browse_available` was renamed to `browse_reachable` —
a real drift the gate should catch. After registering the projection, the same
rename produced:

```
evidence projection map: missing 2 item(s)
  - window.local_history: Evidence Projection Map documents evidence field
    `LocalHistoryEvidence.browse_available` -> snapshot field
    `local_history.browse_available`, but that evidence field no longer exists
  - window.local_history: evidence field `LocalHistoryEvidence.browse_reachable`
    is projected but the Evidence Projection Map documents no snapshot field for it
```

The rename was then reverted. **The gate now rejects both halves of a real
rename** — a vanished documented field and an undocumented projected one.

## One contingency did not fire, and that is a finding

Task 8.3 anticipated `tabs[].draft_present` making **three** surfaces project into
one `tabs` object, which would have required extending the per-binding attribution
3b added. **It does not.** `draft_present` is a per-tab **document-identity** fact,
read through the editor page's existing `draft_id()` operation, while the draft
workflow's evidence surface is **window-level** and carries no per-tab field.

Fabricating a projection row for it would make the map claim something the
projection function does not do, which the gate's own stated authority — "the
authority for *is this field projected* is the Rust snapshot function" — forbids.
So `tabs` still has exactly two projecting surfaces, and the attribution mechanism
needed no change. Recorded rather than forced.

## Redaction confirmed

Task 8.4's requirement that no draft body, session content, or local-history
content can reach the schema is unchanged and still enforced by the existing
redaction tests (`draft_body`, `local_history_contents`, `draft_id`). Every field
the four new evidence surfaces added is **internal**: generations, tickets,
retained weights, continuation offsets, tombstone and queue counts, admission and
disposal state. Two deliberate choices keep it that way:

- `DraftEvidence` reports the manifest as an **entry count**, never the manifest,
  which would carry original file paths;
- `LocalHistoryEvidence` and `DraftEvidence` report retained bodies as
  **booleans and counts**, never text.

## Lane status, stated honestly

- **Changed tree: PASS.** `PASS: automation D-Bus smoke completed`, including the
  final runtime-warning scan (`PASS: no unexpected
  GTK/GDK/Libadwaita/GIO/D-Bus/portal/AT-SPI/filesystem warnings`).
- **Baseline tree: the lane's final warning scan failed, reproducibly, for an
  environmental reason unrelated to either tree.** Its private D-Bus session could
  not activate `org.a11y.atspi.Registry`
  (`GDBus.Error:...NameHasNoOwner: ... unit failed`), which produced three
  `xdg-desktop-portal-gtk` AT-SPI warnings and one `Gtk-CRITICAL` about
  application registration. The host has **no failed user units**, and the same
  lane on the same host passed on the changed tree minutes earlier, so this is a
  second-isolated-session artifact rather than a property of `origin/main`.

  It does **not** weaken this proof: the baseline run still produced all 35
  assertion artifacts, including every snapshot, predicate, catalog, and
  action-state file, because the warning scan is the lane's *last* step. The diff
  above is over those artifacts. Recorded rather than hidden, and recorded as an
  environment finding rather than as a code finding, because attributing it to
  either tree would be wrong.

  Also recorded: the **first** baseline attempt failed differently, with
  `libmutter-ERROR: Failed to create socket`, because the comparison worktree was
  created under the deep session scratch path. Mutter's Wayland socket path is
  length-limited; re-creating the worktree at `/tmp/lt-base` fixed it. That lesson
  is now in the programme record's friction section.
