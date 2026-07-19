## ADDED Requirements

### Requirement: Materialized directory scans are active-plus-latest per store
Each materialized workspace child store SHALL own at most one active directory scan and one replaceable latest compact scan request. Superseding a scan MUST cancel the active generation without retaining another strong store or full mirror snapshot in the generic worker backlog. Strong store ownership and the current mirror MUST be captured only when the request receives worker admission, and every completion MUST revalidate section lifetime, store identity, target generation, and scan generation before reconciliation.

#### Scenario: Slow filesystem receives repeated refresh requests
- **WHEN** watcher notices and manual Refresh repeatedly target one materialized directory while its scan is still active
- **THEN** intermediate requests collapse into one latest compact request
- **AND** queued ownership contains no strong store and no cloned full mirror for each superseded request

#### Scenario: Admitted scan is superseded
- **WHEN** a newer request replaces an active scan before filesystem traversal completes
- **THEN** the active scan stops at a bounded cancellation checkpoint
- **AND** only the latest request may capture a fresh mirror and publish reconciliation state

#### Scenario: Store disappears before admission or completion
- **WHEN** a workspace filter, section rebuild, folder removal, or lifetime end destroys the target store
- **THEN** weak queued ownership fails closed and admitted completion releases its result
- **AND** stale work does not block refresh readiness or recreate removed tree state

#### Scenario: Empty-folder evidence arrives out of order
- **WHEN** an older slow emptiness probe completes after a newer scan or probe has observed different folder contents
- **THEN** generation and folder identity checks reject the older result
- **AND** expansion affordance, `(Empty)` state, and Focus Folder availability reflect only current accepted evidence

#### Scenario: Latest scan reaches terminal readiness
- **WHEN** the current scan and any resulting bounded reconciliation complete with no newer pending request
- **THEN** the store publishes cache, watcher-target, selection, and readiness finalization exactly once
- **AND** cancelled generations no longer contribute readiness blockers
