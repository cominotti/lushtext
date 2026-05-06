# Portal-First Sandbox Migration

## Status

Exploration note, not an official OpenSpec change.

This document captures the May 2026 exploration of whether LushText should keep
its current Flatpak `--filesystem=home` permission or move toward a
portal-first model. It intentionally documents the future spec slices and
learnings without creating OpenSpec artifacts yet.

## Recommendation

Keep `--filesystem=home` as the temporary shipping baseline, but make
portal-first access the long-term direction.

The current permission is defensible because LushText's shipped workspace model
persists root paths and uses them for tree loading, search, replace, notes,
local history, file monitoring, and session restore. It should not be treated as
the app's permanent identity. A text editor with workspaces should eventually
use user-selected file and folder grants instead of broad default home access.

The migration should be staged:

```text
Current release
    |
    | keep --filesystem=home
    v
Portal-backed grants
    |
    | prove workspace behavior on portal paths
    v
Portal-compatible workspace features
    |
    | only after parity gates pass
    v
Remove broad filesystem access
```

Do not switch the manifest first. A safer-looking Flatpak that quietly weakens
workspace reliability would be worse than the current honest permission.

## Exploration Snapshot

The local environment used for this exploration reported:

- Flatpak 1.17.6
- xdg-desktop-portal 1.21.1
- GTK 4.22.3
- FileChooser portal version 4
- Documents portal version 5

Relevant current LushText behavior:

- The manifest grants `--filesystem=home`.
- The exported Flatpak desktop entry uses `--file-forwarding`, so file-manager
  launches can pass files through the document portal.
- `gtk4::FileDialog` is already used for open, save, and folder selection.
- Workspaces are currently persisted as plain filesystem paths.

Portal behavior verified during exploration:

- A file outside the sandbox, `/etc/hosts`, could be forwarded into the
  sandbox with `--nofilesystem=home` and read through a document-portal path.
- A temporary directory exported to `dev.cominotti.lushtext` with read, write,
  and delete permissions could be listed recursively inside the sandbox with
  `--nofilesystem=home`.
- Through that exported directory path, the sandbox could create, rename, and
  delete child files, and the changes appeared on the host.
- `gio monitor` produced events on a normal host directory, but produced no
  events for portal-exported file or directory paths in the sandbox during the
  probe. Portal-backed workspace roots should therefore assume polling or manual
  refresh fallback unless a later implementation proves a better watcher path.

## Capability Verdict

Portal-first is implementable today, but not as a perfect drop-in replacement
for broad home access.

| Capability | Portal-first feasibility | Notes |
| --- | --- | --- |
| Open a user-selected file anywhere readable | Yes | FileChooser and Flatpak file forwarding cover this. |
| Save As to a user-selected location | Yes | FileChooser `SaveFile` covers this. |
| Add a folder as a workspace | Yes | FileChooser directory selection and Documents directory export are available on the tested stack. |
| Restore a workspace after restart | Yes, with redesign | Persist document IDs/grants plus display metadata, not only plain paths. |
| Sidebar tree loading | Yes | Directory FUSE paths supported recursive listing in the probe. |
| Create, rename, delete inside a workspace | Yes, with granted permissions | The probe verified basic child operations through an exported directory. |
| Workspace search and replace | Likely yes | Depends on performance and durable write behavior over document-portal FUSE paths. |
| Local history, notes, and bookmarks | Yes, with identity redesign | Host path hints can help display, but stable identity must tolerate portal paths. |
| External change auto-refresh | Degraded unless redesigned | Event watches did not fire in the probe; polling/reconcile is the likely fallback. |
| GVfs and remote URI locations | Separate follow-up | Do not silently add GVfs permissions unless a concrete workflow requires them. |

## Future Spec 1: Portal-Backed File And Workspace Grants

Purpose:

Define the durable access model that lets LushText represent user-selected files
and workspace folders without broad static filesystem access.

Core problem:

LushText currently treats external resources as ordinary `PathBuf`s. In a
portal-first Flatpak, the app may receive a document-portal FUSE path and a
document ID, while the user-facing host path is only metadata or an extended
attribute. The model needs to represent all of that explicitly.

Likely scope:

- Add an external-resource model for file and directory grants.
- Persist document IDs, sandbox-accessible paths, host-path hints, display
  names, grant permissions, and last validation state.
- Preserve current path-based behavior for host/dev builds and migrated
  existing workspace entries.
- Add reauthorization states for missing, revoked, stale, or inaccessible
  grants.
- Define how Save As, Open File, file-manager launch, and Add Workspace Root
  create or update grants.
- Define how sidecar identity should use canonical host-path hints when
  available, while still working on portal paths when host paths cannot be
  resolved.

Acceptance shape:

- A user can open a supported file outside sandbox-visible locations without
  `--filesystem=home`.
- A user can add a folder workspace outside sandbox-visible locations without
  `--filesystem=home`.
- The persisted workspace state can distinguish a normal host path from a
  portal-backed grant.
- LushText can show a clear reauthorization state instead of silently dropping
  a workspace when a grant is unavailable.
- Existing `workspaces.json` entries migrate without data loss.

Go/no-go gate:

Do not proceed to broad behavior migration until the model can round-trip a
portal-backed file and directory grant across restart on the target runtime.

## Future Spec 2: Portal-Compatible Workspace Behavior

Purpose:

Adapt the shipped workspace and document workflows so portal-backed roots are
first-class, reliable, and honest about their limitations.

Core problem:

The product behavior is larger than opening files. LushText's workspace roots
drive the sidebar tree, command palette indexing, workspace search and replace,
file peek, notes, local history, document restore, file monitoring, and in-app
filesystem operations.

Likely scope:

- Make sidebar tree loading operate on portal-backed directory paths.
- Add a portal-root refresh policy that does not depend only on filesystem
  watch events.
- Add manual refresh and bounded polling/reconcile behavior for portal roots.
- Benchmark directory scanning, file peek, and search over document-portal FUSE
  paths.
- Ensure create, rename, delete, and Save As flows behave correctly with grant
  permissions.
- Ensure session restore and draft recovery handle portal paths and
  reauthorization states.
- Ensure document notes, workspace notes, annotations, bookmarks, and local
  history keep stable identities through in-app rename and Save As flows.
- Decide how symlinks that leave the granted tree should behave.
- Keep unsupported or degraded behavior visible in the UI rather than hidden in
  logs.

Acceptance shape:

- A portal-backed workspace can be restored, listed, searched, edited, and
  refreshed without broad home access.
- Search and replace can operate inside a granted workspace and can preserve the
  existing undo/backup safety contract.
- Notes, bookmarks, annotations, and local history still attach to the expected
  documents after restart.
- External changes under a portal-backed root are eventually reflected, even if
  the implementation uses polling instead of event watches.
- The UI tells the user when a grant needs reauthorization or when automatic
  refresh is degraded.

Go/no-go gate:

Do not remove `--filesystem=home` until this spec proves the main workspace
loops on portal-backed roots with acceptable performance and no data-loss
regression.

## Future Spec 3: Remove Broad Flatpak Filesystem Access

Purpose:

Tighten the Flatpak manifest only after portal-backed workspaces are proven.

Core problem:

Removing `--filesystem=home` is the visible safety win, but it is the final
packaging step, not the foundation. If done too early, it breaks restored
workspaces and path-based features.

Likely scope:

- Remove `--filesystem=home` from the manifest.
- Keep Wayland, fallback X11, IPC, and DRI permissions as required GTK desktop
  surface permissions.
- Verify the exported desktop entry continues to use Flatpak file forwarding.
- Update README and Flatpak packaging documentation with the new permission
  posture and portal-backed workspace behavior.
- Add deterministic verification for effective permissions and file-forwarding
  behavior.
- Add migration notes for users with existing path-backed workspaces.
- Document any remaining degraded behavior, especially delayed refresh for
  portal-backed roots.

Acceptance shape:

- `flatpak info --show-permissions dev.cominotti.lushtext` no longer reports
  `filesystems=home`.
- Opening supported files from the file manager still works through
  file-forwarding.
- Open File, Save As, and Add Workspace Root still work through native dialogs.
- Previously saved workspaces either restore or present a reauthorization flow.
- GNOME Software/Flathub no longer flags LushText because of broad filesystem
  access, assuming no other static permission triggers the same rating.

Go/no-go gate:

Only ship this after Specs 1 and 2 are complete and verified against the actual
Flatpak runtime target. If portal-backed workspace behavior remains meaningfully
less reliable, keep the current permission and document why.

## Capability Losses To Accept Or Mitigate

The migration should not promise exact equivalence with unrestricted host-path
access. These are the important tradeoffs to either accept explicitly or design
around:

- External-change detection may become delayed for portal-backed roots.
- File monitor warnings may need polling or focus-time stat checks.
- Workspace startup may need reauthorization prompts.
- `flatpak run dev.cominotti.lushtext /path/to/file` is not the same as
  file-forwarding; direct command-line paths should be documented as a developer
  or advanced-user edge unless launched with Flatpak's forwarding syntax.
- Some symlink, mount, permission, and host-path canonicalization behavior may
  differ through the document portal.
- Remote and GVfs locations should not be smuggled into the same change unless
  there is a concrete, tested workflow.

## Source References

- Flatpak sandbox permissions:
  <https://docs.flatpak.org/en/latest/sandbox-permissions.html>
- Flatpak file forwarding:
  <https://docs.flatpak.org/en/latest/flatpak-command-reference.html>
- XDG FileChooser portal:
  <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html>
- XDG Documents portal:
  <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Documents.html>
- GNOME Software safety metadata:
  <https://gnome.pages.gitlab.gnome.org/gnome-software/help/C/software-metadata.html>

## Open Questions For Proposal Time

- Should portal-backed roots be opt-in first while `--filesystem=home` remains
  available, or should the app internally use portal grants everywhere as soon
  as the model exists?
- Should LushText request delete permission for workspace roots by default, or
  split destructive in-app operations behind an additional confirmation and
  reauthorization step?
- What polling cadence is acceptable for portal-backed roots without hurting
  battery life or large-workspace performance?
- Should existing path-backed workspace entries be migrated eagerly on first
  launch, lazily when opened, or only when the user chooses to tighten sandbox
  access?
- Which portal and Flatpak versions should be the minimum supported target for
  removing `--filesystem=home`?
