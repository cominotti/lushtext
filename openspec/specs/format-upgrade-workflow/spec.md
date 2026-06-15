# format-upgrade-workflow Specification

## Purpose
Define the user-facing, app-owned metadata format upgrade workflow so supported older data is discovered before normal startup consumers run, converted only through sealed upgrade code, and preserved safely before replacement.

## Requirements
### Requirement: Startup preflight inventories app-owned metadata before restore
The system SHALL run a bounded app-owned metadata format preflight before workspace loading, session restore, draft restore, sidecar browse recovery, or any other normal startup consumer can default, quarantine, rewrite, or delete upgradeable metadata. The preflight SHALL classify known metadata paths and bounded sidecar directories as current, missing, upgradeable, future-version, unsupported-old, damaged, or unsafe-to-replace.

#### Scenario: Current metadata continues startup without a gate
- **WHEN** every discovered app-owned metadata file is missing or already uses the latest supported format
- **THEN** startup proceeds to the normal workspace, session, draft, sidecar, and autosave flows without showing the compatibility dialog
- **AND** the preflight does not write metadata or alter recovery state

#### Scenario: Upgradeable critical metadata gates startup before restore
- **WHEN** preflight finds supported older workspace, session, draft-manifest, sidecar, local-history, search, migration-ledger, or Replace All undo metadata that affects startup or recovery behavior
- **THEN** normal metadata consumers remain paused
- **AND** the user is shown a compatibility decision before any affected metadata is defaulted, quarantined as ordinary corruption, or rewritten

#### Scenario: Preflight is bounded and app-data scoped
- **WHEN** the app data directory contains many sidecars, history lineages, backup entries, or unrelated files
- **THEN** preflight scans only documented app-owned metadata locations with documented bounds
- **AND** the GTK main thread remains responsive while blocking filesystem work runs off the main thread

### Requirement: Upgrade decisions use safest user-facing defaults
The system SHALL present user choices based on the preflight plan. Supported older metadata SHALL offer Convert as the primary action. Future or newer metadata SHALL NOT offer Convert and SHALL default to Quit as the safest action. Start Fresh SHALL preserve affected data before writing replacement latest-format state.

#### Scenario: Supported older metadata offers conversion
- **WHEN** the preflight plan contains one or more upgradeable items and no future-version blocker
- **THEN** the startup dialog offers Convert, Start Fresh, and Quit
- **AND** Convert is the primary action

#### Scenario: Future metadata cannot be converted by an older app
- **WHEN** the preflight plan contains metadata created by a newer LushText format than this binary supports
- **THEN** the startup dialog offers Quit and Start Fresh
- **AND** it does not offer Convert or any downgrade action

#### Scenario: Start Fresh preserves data first
- **WHEN** the user chooses Start Fresh for upgradeable or future metadata
- **THEN** the system preserves the affected metadata through a backup or quarantine record before any latest-format default is written
- **AND** startup continues only after preservation succeeds or the user chooses to quit

#### Scenario: Unsupported old data stays in recovery
- **WHEN** preflight finds older, pre-public, or wrong-shape metadata without a tested converter path
- **THEN** the startup format gate does not offer Convert or Start Fresh for that metadata
- **AND** existing recovery/quarantine/default handling remains responsible for preserving it

#### Scenario: Failed conversion remains retryable
- **WHEN** the user chooses Convert and at least one required upgrade item fails before completion
- **THEN** the original or backed-up metadata remains available
- **AND** the user can retry conversion or quit without silently continuing on empty/defaulted state

### Requirement: Old-format intelligence is sealed outside normal runtime readers
The system SHALL keep legacy payload structs, old-version parsers, and old-to-latest conversion logic inside a dedicated GTK-free format-upgrade service. Ordinary latest-format model types, recovery loaders, workspace/session/draft services, and UI adapters MUST NOT carry permanent old-format deserialization branches.

#### Scenario: Normal reader rejects unconverted older metadata
- **WHEN** an ordinary latest-format metadata reader encounters an older supported version that was not converted by the format-upgrade workflow
- **THEN** it reports an unsupported-version recovery diagnostic
- **AND** it does not deserialize the older payload through a compatibility branch

#### Scenario: Converter chain is compartmentalized
- **WHEN** a future LushText release introduces a v2 or later metadata format
- **THEN** older payload structs and converter functions are added only under the format-upgrade service's legacy area
- **AND** normal domain models and service readers continue to represent only the latest format

#### Scenario: Upgrade planning is query-shaped
- **WHEN** the UI requests a format scan or upgrade plan
- **THEN** the service returns plain Rust inventory and plan values without mutating app data
- **AND** filesystem writes occur only in the explicit apply command

### Requirement: Format upgrades preserve previous bytes before writing latest state
The system SHALL preserve affected metadata before applying an upgrade or Start Fresh action. Preservation MUST record enough information to identify the original app-data-relative path, metadata kind, previous version or classification, LushText version performing the action, timestamp, and per-item result. Replacement writes MUST use the durable filesystem boundary.

#### Scenario: Conversion backs up before first replacement write
- **WHEN** the user confirms Convert
- **THEN** each affected metadata item is backed up or quarantined before the first latest-format replacement for that item is written
- **AND** a backup failure prevents replacement of that item

#### Scenario: Latest writes use durable envelopes
- **WHEN** conversion writes a latest-format JSON metadata file
- **THEN** the written file uses the public latest JSON envelope for its document kind
- **AND** the write goes through the durable filesystem boundary with temp-file, flush/sync, rename, and parent-directory durability semantics

#### Scenario: Dependent upgrades avoid unsafe partial state
- **WHEN** an upgrade plan contains dependent items such as session entries and matching draft manifest entries
- **THEN** the plan applies those dependent items as one guarded group or fails before writing a misleading partial result
- **AND** the outcome reports which items remain unchanged and retryable

### Requirement: Preferences exposes a Data page for status and retry
The system SHALL add a `Preferences > Data` page that lets the user inspect current app-owned metadata format status, manually rescan, retry applicable upgrades, and see concise recovery/backup state. The page SHALL render correctly for empty/current state, representative upgradeable state, many or awkward items, failed conversion state, future-version state, active verification state, and constrained dialog geometry. Empty or irrelevant action groups MUST NOT be shown, and a completed current-state scan SHALL provide an explicit verified-current visual affordance alongside the manual rescan control.

#### Scenario: Current data shows a quiet status
- **WHEN** the user opens `Preferences > Data` with no app data or only latest-format metadata
- **THEN** the page reports that the data format is current
- **AND** no destructive, irrelevant, or empty action section is presented
- **AND** the Data Format row shows a verified-current affordance near the manual rescan control

#### Scenario: Manual rescan gives visible verification feedback
- **WHEN** the user activates the manual Data Format rescan control
- **THEN** the Data Format row reports that app data formats are being verified
- **AND** the rescan control is disabled while verification is active
- **AND** a fast no-op scan remains visibly in the verification state for at least one short perceptible dwell interval before returning to the completed state
- **AND** a completed current scan restores the quiet current status with the verified-current affordance visible

#### Scenario: Upgradeable data can be converted from Preferences
- **WHEN** the manual scan finds supported older metadata after startup
- **THEN** the page shows a grouped summary and a Convert action
- **AND** activating Convert uses the same format-upgrade service path as startup conversion

#### Scenario: Future data has no Convert control
- **WHEN** the manual scan finds metadata from a newer LushText format
- **THEN** the page explains that this app cannot convert it
- **AND** no Convert action is visible or enabled for that item
- **AND** no verified-current affordance is shown for the non-current state

#### Scenario: Many details scroll without hiding commands
- **WHEN** the page displays many metadata items or long app-data-relative paths
- **THEN** the summary and primary actions remain reachable
- **AND** only the item detail region scrolls without horizontal scrolling, fake rows, or clipped commands

#### Scenario: Failure status remains actionable
- **WHEN** a previous conversion attempt failed
- **THEN** the page reports a concise failure status with retry or quit/start-fresh guidance as appropriate
- **AND** detailed paths remain available through logs, backup manifests, or an explicit details area rather than overwhelming the main page
- **AND** no verified-current affordance is shown for the failed state

### Requirement: Format-upgrade behavior has layered deterministic coverage
The project SHALL cover the format-upgrade workflow with deterministic tests at the lowest practical level, plus widget coverage for startup and Preferences behavior. Test coverage MUST include no-op current v1 behavior, supported older conversion fixtures once a newer format exists, future-version refusal, backup-before-write ordering, failure retry, generated malformed input classification, and constrained UI states.

#### Scenario: Service tests cover inventory and planning
- **WHEN** service tests run against missing, current, upgradeable, future-version, unsupported-old, damaged, and unsafe metadata fixtures
- **THEN** each fixture produces the expected inventory classification and plan action
- **AND** scan and plan tests prove no app-data writes occur

#### Scenario: Conversion tests prove preservation ordering
- **WHEN** deterministic failure seams simulate backup failure, write failure, or dependent-item failure
- **THEN** tests prove original data remains available
- **AND** latest replacement files are written only after preservation succeeds

#### Scenario: Widget tests cover startup gate decisions
- **WHEN** the widget harness launches with seeded upgradeable, future-version, and failed-conversion app data
- **THEN** the startup dialog shows the correct actions before normal restore consumes affected metadata
- **AND** successful conversion continues startup while failed conversion remains retryable

#### Scenario: Widget tests cover Preferences Data state extremes
- **WHEN** widget tests exercise `Preferences > Data`
- **THEN** they cover empty/current state, representative upgradeable state, many or awkward items, failed conversion state, future-version state, and constrained geometry
- **AND** command reachability, readable empty states, item-region-only scrolling, and preserved dialog actions are asserted

#### Scenario: Generated inputs cannot crash classification
- **WHEN** bounded generated JSON values or malformed bytes are passed to format inventory classification
- **THEN** the classifier returns a current, upgradeable, future-version, unsupported, damaged, or unsafe result
- **AND** it does not panic or write app data
