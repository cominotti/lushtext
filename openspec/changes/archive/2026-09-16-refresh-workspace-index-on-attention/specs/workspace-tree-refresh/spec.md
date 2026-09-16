## ADDED Requirements

### Requirement: Collapsed directory emptiness hints become correct again
The sidebar SHALL bring the empty-or-not state of collapsed directory rows back into agreement with disk at the attention moments defined by the workspace attention refresh capability, by using its existing automatic full-refresh path. Revalidation MUST NOT expand any row, MUST NOT introduce recursive watching, and MUST preserve scroll position, selection, and expansion state.

#### Scenario: A collapsed empty directory that gained content stops claiming to be empty
- **WHEN** a directory row is shown collapsed with its empty state, and a file is created inside it by another process
- **AND** the user returns to the window
- **THEN** the row stops showing the empty state
- **AND** its expander affordance becomes available
- **AND** its focus-folder action becomes enabled

#### Scenario: A collapsed directory emptied on disk gains the empty state
- **WHEN** a collapsed directory row that previously had children has every visible entry removed by another process
- **AND** the user returns to the window
- **THEN** the row shows the empty state
- **AND** its expander affordance is hidden

#### Scenario: Revalidation does not expand or disturb the tree
- **WHEN** an attention-triggered refresh settles
- **THEN** no collapsed row becomes expanded
- **AND** scroll position, selection, and expansion state are preserved

#### Scenario: Only hidden or excluded new entries leave the state unchanged
- **WHEN** the only entries created inside a collapsed directory are hidden or excluded under the current workspace entry visibility rule
- **THEN** the row's empty state is unchanged

#### Scenario: Affordance changes are silent and do not strand focus
- **WHEN** a focused collapsed directory row's expander or focus-folder affordance changes as a result of the refresh
- **THEN** no screen-reader announcement is emitted for the affordance change
- **AND** focus and selection remain on the same path
- **AND** the row exposes no stale position or expanded-state metadata after the update

## MODIFIED Requirements

### Requirement: Workspace sections refresh automatically for external filesystem changes
The system SHALL keep each workspace section's visible folder trees aligned with files and directories inside the sidebar's currently materialized scope when those paths are created, removed, renamed, or moved outside the LushText sidebar workflow. Automatic watching MUST prefer the visible top-level workspace folder rows and expanded directories needed to keep the rendered tree current, rather than recursively watching every descendant under every broad configured folder at startup. Watch-driven refresh covers changes to the entry sets of those materialized directories; changes to the entry set of a collapsed child directory are not observable through its parent's non-recursive watch, and are covered instead by the attention-moment refresh that keeps collapsed rows' emptiness state correct.

#### Scenario: External file creation appears in the tree
- **WHEN** a new file is created on disk under a workspace folder that is currently visible in the sidebar
- **THEN** the corresponding workspace section shows the new file without requiring the user to remove and re-add the folder or reopen the workspace

#### Scenario: External removal clears stale rows
- **WHEN** a file or directory that is currently shown in a workspace section is removed on disk outside LushText
- **THEN** the workspace section removes the stale row after refresh processing settles
- **AND** the tree no longer exposes actions for the removed path

#### Scenario: External rename updates the visible tree
- **WHEN** a visible file or directory inside a workspace folder is renamed outside LushText
- **THEN** the workspace section stops showing the old path
- **AND** the workspace section shows the renamed path in the correct sorted position

#### Scenario: Broad folder with unreadable deep descendants does not block startup
- **WHEN** a workspace folder points at a broad directory such as the user's home folder and some deep descendant paths are unreadable to the watcher backend
- **THEN** the workspace section still renders its visible tree without waiting for a recursive watch across every descendant
- **AND** automatic refresh covers the currently materialized folder rows and expanded directories
- **AND** the user can still use the manual `Refresh` control for broader reloads

#### Scenario: Zero-folder workspace starts no folder watchers
- **WHEN** a workspace section contains zero folders
- **THEN** automatic refresh does not attempt to watch a fake folder
- **AND** the workspace section remains usable for adding folders
