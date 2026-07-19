## ADDED Requirements

### Requirement: Bookmark-only browsing reuses the bounded Notes pipeline
The dedicated bookmark browser SHALL be a generation-scoped bookmark-only mode of the unified Browse Notes inventory, query, projection, and disposal workflow. It MUST preserve the existing Show Bookmarks action and bookmark-specific activation, scope, empty, truncation, recovery, keyboard, and accessibility behavior without retaining a separate uncapped loader or synchronous widget-rebuild path.

#### Scenario: Show Bookmarks opens
- **WHEN** the user activates the existing bookmark-browser action
- **THEN** the unified Notes workflow starts with a bookmark-only source filter and current workspace scope
- **AND** document notes and folder notes cannot appear in the result inventory

#### Scenario: Bookmark inventory is large
- **WHEN** bookmark sidecars contain more rows than one admitted inventory, query, or projection slice permits
- **THEN** source loading, matching, and GTK projection obey the same item, byte, active-plus-latest, and disposal bounds as Browse Notes
- **AND** GTK does not synchronously scan the full source or rebuild hundreds of row widget trees in one callback

#### Scenario: Bookmark query has no matches
- **WHEN** a bookmark-only query yields no accepted rows
- **THEN** the browser reaches the bookmark-specific empty state through bounded query completion
- **AND** the main loop remains responsive while prior rows retire

#### Scenario: A bookmark row is activated
- **WHEN** the user activates a current bookmark-only result
- **THEN** the existing bookmark navigation semantics open or focus the file and line
- **AND** a stale generation cannot activate a replaced row

#### Scenario: Bookmark metadata is malformed or truncated
- **WHEN** bounded inventory loading encounters recovery diagnostics or the configured source cap
- **THEN** the browser exposes the existing accessible recovery or truncation state
- **AND** valid admitted bookmarks remain usable

#### Scenario: Production bookmark inventory is requested
- **WHEN** an interactive caller constructs the bookmark-only inventory
- **THEN** it must supply the unified Notes source limit, byte budget, generation, and cancellation policy
- **AND** no unrestricted aggregate bookmark-vector API remains available to production UI code
