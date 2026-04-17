## Context

LushText already persists a broad range of state under `$XDG_DATA_HOME/lushtext`, but that storage map is currently only partially reflected in OpenSpec. Some living specs already exist for storage-backed features such as local history, bookmarks, annotations, and tab transparency, but several of them still reflect the narrower change that introduced the feature rather than the complete shipped contract. Other important persistence surfaces such as drafts, session restore, workspace state, search history, saved searches, and Replace All undo backup are currently represented only in code and tests.

The codebase now stores or derives the following app-data surfaces:

```text
$XDG_DATA_HOME/lushtext/
├─ session.json
├─ workspaces.json
├─ search-history.json
├─ saved-searches.json
├─ replace-backup.json
├─ drafts/
│  ├─ manifest.json
│  └─ *.draft
├─ local-history/
│  └─ <sidecar-id>/
│     ├─ index.json
│     └─ *.txt
├─ bookmarks/
│  └─ <sidecar-id>.json
├─ annotations/
│  └─ <sidecar-id>.json
└─ style-schemes/
   └─ lushtext-opacity-*.xml
```

The exhaustive capability mapping for those surfaces is:

| App-data surface | Capability |
| --- | --- |
| `session.json` | `draft-session-recovery` |
| `drafts/manifest.json` and `drafts/*.draft` | `draft-session-recovery` |
| `workspaces.json` | `workspace-state-persistence` |
| `search-history.json` | `search-history-and-saved-searches` |
| `saved-searches.json` | `search-history-and-saved-searches` |
| `replace-backup.json` | `search-replace-safety` |
| `local-history/<sidecar-id>/index.json` and `*.txt` snapshots | `local-history` |
| `bookmarks/<sidecar-id>.json` | `line-bookmarks` |
| `annotations/<sidecar-id>.json` | `sidecar-annotations` |
| `style-schemes/lushtext-opacity-*.xml` | `tab-content-transparency` |

This change is documentation-heavy rather than runtime-heavy, but it is still cross-cutting because the storage contract spans `model`, `services`, `ui/window`, `ui/editor_page`, `ui/sidebar`, and `ui/search_panel`. The design therefore needs to define clean capability boundaries so the exhaustive pass stays readable and future changes know where to land.

## Goals / Non-Goals

**Goals:**
- Create an exhaustive OpenSpec view of every persistence surface that writes under `$XDG_DATA_HOME/lushtext`.
- Add missing capabilities for drafts and session recovery, document save and close safety, workspace state persistence, search history and saved searches, and search replace safety.
- Refresh existing living specs so they match the full shipped behavior rather than only the original change delta.
- Make storage location, lifetime, identity, and cleanup guarantees explicit enough that future reliability work has a stable contract to extend.
- Keep spec boundaries narrow enough that future changes can update one capability without editing a monolithic persistence spec.

**Non-Goals:**
- Change runtime behavior in this change.
- Introduce new persistence formats, migrations, or app-data directories.
- Specify unrelated persistence systems such as GSettings, external `.editorconfig` files, or version-control data outside `$XDG_DATA_HOME/lushtext`.
- Reorganize the current runtime modules purely to match the spec layout.

## Decisions

### 1. Use one umbrella change with multiple capability specs, not one giant persistence spec

This pass will use one OpenSpec change with multiple capability specs. Each capability will own either one app-data file or one closely related subtree, while the design document will explain the full storage map and the boundaries between capabilities.

Rationale:
- The user asked for an exhaustive pass, but a single monolithic living spec would be hard to review and hard to maintain.
- The code already separates drafts and session restore, document save safety, local history, sidecars, workspace persistence, and search persistence into distinct service and UI workflows.
- The umbrella change gives one place to review the whole storage contract while preserving future editability at the capability level.

Alternatives considered:
- One global `data-home-persistence` spec: rejected because it would be too broad and create frequent cross-cutting churn.
- Separate OpenSpec changes for each storage area: rejected because the point of this pass is to establish one coherent storage map now.

### 2. Group the app-data contract by persistence role, not by UI entry point

Capability boundaries will follow storage role and lifecycle:
- `draft-session-recovery` for drafts and session restore
- `document-save-safety` for file-backed load/save/discard/close safety
- `workspace-state-persistence` for `workspaces.json`
- `search-history-and-saved-searches` for recent and user-managed query memory
- `search-replace-safety` for persisted Replace All undo backup and rollback semantics
- refreshed specs for `local-history`, `line-bookmarks`, `sidecar-annotations`, `draft-restore-validation`, and `tab-content-transparency`

`draft-restore-validation` remains a focused behavioral refinement on file-backed draft restore and cleanup, but it does not own a separate app-data surface from `draft-session-recovery`.

Rationale:
- Users experience persistence by outcome: “my tabs came back,” “my draft was restored,” “my notes survived rename,” “undo replace is still available,” not by which widget triggered the write.
- Storage role creates the cleanest mapping from code to spec without forcing every spec to repeat the same directory tree.
- This boundary keeps `draft-restore-validation` narrow and lets it remain a focused requirement slice inside the broader draft/session story.

Alternatives considered:
- Grouping by feature surface such as sidebar, editor page, search panel, or preferences: rejected because storage and safety rules would then be split across multiple specs that touch the same data.
- Folding stale-draft validation into the broader draft capability and deleting the focused spec: rejected because the file-backed freshness rule is already a useful focused capability and should stay reviewable on its own.

### 3. Treat app-data entries as one of four durability classes

The storage map will be documented using four durability classes:
- **primary user state**: session, drafts, workspaces, bookmarks, annotations, local history, saved searches
- **bounded convenience state**: search history
- **bounded safety state**: Replace All undo backup
- **derived cache state**: transparency-derived style-scheme XML files

Rationale:
- Not everything under app data has the same recovery promise.
- Search history is intentionally helpful but losable recent memory, not a safety or recovery mechanism.
- Replace All undo backup is intentionally temporary and must be discarded at panel-close or startup boundaries rather than treated like durable user-authored history.
- Derived style schemes are recreatable cache artifacts whose absence should degrade into regeneration rather than user-visible data loss.

Alternatives considered:
- Treating every entry as equally durable state: rejected because it would over-promise recovery guarantees for replace-backup and style-schemes.
- Treating all app-data files as cache-like implementation detail: rejected because drafts, session restore, sidecars, and local history are core user-trust contracts.

### 4. Let capability specs include storage location and lifecycle guarantees when those details define the contract

This pass will keep living specs behavior-first, but it will explicitly include storage location, identity, cleanup, and regeneration rules whenever those details are central to the feature contract.

Examples:
- local history keyed by canonical-path identity under `local-history/`
- bookmark and annotation sidecars stored outside the source file under app data
- stale file-backed drafts deleted after confirmed mismatch
- derived transparency schemes recreated when cache files are missing

Rationale:
- For this change, the storage map itself is the subject of the documentation.
- Omitting storage and cleanup semantics would leave the exhaustive pass incomplete.
- The current repo already treats identity and storage lifetime as important design choices, especially for drafts, local history, and note sidecars.

Alternatives considered:
- Keeping living specs purely UI-facing and putting all storage details only in design: rejected because it would weaken the durability contract for the exact features the user wants formalized.

### 5. Refresh existing specs where current runtime behavior materially exceeds the archived delta

Existing living specs will be updated when the current shipped behavior is broader or more precise than the current living contract.

This applies to:
- `draft-restore-validation`, which still has archive-placeholder purpose text and now sits inside a broader draft/session recovery model
- `local-history`, whose shipped storage and restore model is richer than the original archived baseline
- `line-bookmarks` and `sidecar-annotations`, which should explicitly reflect canonical-path sidecar identity and storage-backed rename and Save As behavior
- `tab-content-transparency`, which now also persists derived style-scheme cache files under app data

Rationale:
- The goal is not just to add missing specs, but to make the existing spec set accurate as a whole.
- Several current living specs are already good foundations; refreshing them is lower-risk and more readable than replacing them with new overlapping capability names.

Alternatives considered:
- Leaving existing specs unchanged and documenting gaps only in design: rejected because the living spec set would remain uneven.
- Replacing existing capabilities with all-new names: rejected because stable capability names that already match the shipped feature should be preserved when possible.

### 6. This change is spec-only and requires no runtime migration

The change will not alter the app's on-disk formats or behavior. It introduces OpenSpec artifacts only.

Rationale:
- The request is to capture current behavior, not to implement new persistence logic.
- A spec-only change minimizes rollout risk while still improving future correctness.

Alternatives considered:
- Bundling opportunistic runtime cleanup with the spec pass: rejected because it would mix capture and implementation.

## Risks / Trade-offs

- [Capability overlap makes future edits ambiguous] -> Use the design document to define boundaries explicitly and keep the most focused existing capability names where they already add value.
- [Exhaustive coverage drifts into implementation trivia] -> Only include storage details that materially affect durability, identity, cleanup, or regeneration guarantees.
- [Some existing living specs still read like archived deltas] -> Refresh them in this change instead of leaving placeholder purpose or partial behavior in place.
- [Temporary and derived app-data entries are over-specified] -> Classify them separately as bounded safety state or derived cache state so the specs do not over-promise durability.

## Migration Plan

1. Create the umbrella OpenSpec change and proposal.
2. Add or refresh the capability specs that correspond to each current app-data surface.
3. Review the resulting set for coverage gaps against the concrete `$XDG_DATA_HOME/lushtext` tree and the relevant tests.
4. Because this is a spec-only change, rollout requires no runtime migration and rollback is simply reverting the documentation artifacts.

## Open Questions

- None blocking. The capability boundaries for this pass are now explicit enough to write the specs directly.
