# Delta for gtk-accessibility-spine

## MODIFIED Requirements

### Requirement: Accessibility guardrails prove current tree and current evidence
LushText SHALL provide guardrails that detect accessibility drift in the current codebase, documentation, smoke matrix, stable anchors, and release evidence, enforcing smoke freshness against `--base-ref` on committed trees while keeping widget tests under static policy only.

#### Scenario: Current-tree policy detects helper and row regressions
- **WHEN** accessibility policy checks run in strict mode
- **THEN** they inspect the current UI tree for direct helper bypasses, unallowlisted direct GTK accessibility calls, list factories without row apply/clear logic, icon-only controls without names, hover-only affordances without accessible fallback evidence, and transient surfaces without matrix coverage
- **AND** failures include clear file/line remediation guidance

#### Scenario: Matrix, docs, and smoke anchors stay synchronized
- **WHEN** stable AT-SPI anchors, smoke helper flags, accessibility smoke cases, accessibility matrix rows, or user-facing accessibility expectations change
- **THEN** documentation drift checks fail unless `docs/accessibility.md`, `docs/accessibility-matrix.md` or equivalent, `docs/automation.md`, `docs/automation-reference.md`, and relevant agent rules are synchronized
- **AND** stale fixture-only anchors are clearly marked as fixture-only rather than public product anchors

#### Scenario: Release proof uses fresh unfiltered summaries
- **WHEN** a release-grade accessibility claim is made for a commit
- **THEN** the accessibility summary is passed, unfiltered unless a scoped release note explains the filter, not skipped, has no unexpected warnings, includes required matrix coverage, and is fresh for the relevant current tree or exact release commit
- **AND** visual and manual evidence required by the matrix is reviewed with the same freshness rule

#### Scenario: Widget test edits keep static policy without invalidating smokes
- **WHEN** a change edits only files under `crates/lushtext/tests/widget/`
- **THEN** `scripts/check-accessibility-policy.py` enforces static accessibility rules on those lines
- **AND** `scripts/accessibility_source_fingerprint.py` does not require re-running `accessibility-smoke` or `visual-smoke`

#### Scenario: Committed accessibility-relevant branch changes require fresh smoke proof
- **WHEN** `scripts/check-accessibility-policy.py --strict-current-tree --base-ref origin/main` runs on a clean worktree whose `origin/main...HEAD` diff touches accessibility-relevant files
- **THEN** missing or stale smoke summaries fail the check even though the working tree is clean
