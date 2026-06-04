## 1. Inventory and Scope Confirmation

- [x] 1.1 Inventory every `file_facts(...).is_ok()` and `file_facts(...).is_err()` status probe across production code, tests, benches, and guidance.
- [x] 1.2 Classify each rich-facts probe as either existence-only or facts-required, with facts-required callers left in place.
- [x] 1.3 Inventory repeated sidecar filesystem mechanics in bookmark, document-note, workspace-note, and local-history services.
- [x] 1.4 Decide whether a tiny shared sidecar helper improves the current code or whether workflow-specific helpers remain clearer, and record that decision in code comments or completion notes as appropriate.

## 2. Status Probe Cleanup

- [x] 2.1 Replace existence-only test and benchmark `file_facts()` probes with `metadata::exists` or `metadata::path_status`.
- [x] 2.2 Keep rich `file_facts()` assertions only where returned facts such as canonical path, byte size, mtime, or kind are inspected.
- [x] 2.3 Add or adjust focused tests for `metadata::exists` and `path_status` if the cleanup exposes missing coverage.

## 3. Sidecar Helper Cleanup

- [x] 3.1 If sharing is justified, extract a small helper for repeated sidecar filesystem mechanics only, with active callers in at least two sidecar workflows.
- [x] 3.2 If sharing is not justified, leave workflow-specific helpers in place and avoid introducing any new helper surface.
- [x] 3.3 Preserve all domain-specific identity rebasing, workspace-root filtering, merge, retention, and empty-document behavior in the owning services.
- [x] 3.4 Run or add targeted sidecar tests for bookmarks, document notes, workspace notes, and local history affected by the cleanup.

## 4. Audit and Guidance

- [x] 4.1 Extend `scripts/check-filesystem-boundary.sh` to catch status-only `file_facts()` probes in tests and benchmarks without flagging callers that inspect returned facts.
- [x] 4.2 Ensure the audit or final search evidence catches any unused sidecar helper module, export, or function introduced by this cleanup.
- [x] 4.3 Update root/nested guidance, rules, or filesystem-sensitive skills only where they need narrow clarification for test status helpers or sidecar helper leftovers.
- [x] 4.4 Confirm the audit still allows documented content-search engine adapter behavior and GTK/GIO toolkit integration points.

## 5. Validation and Closure

- [x] 5.1 Run `scripts/check-filesystem-boundary.sh`.
- [x] 5.2 Run `cargo fmt --check`.
- [x] 5.3 Run targeted Rust tests for filesystem metadata helpers and affected sidecar services.
- [x] 5.4 Run the broader relevant Rust validation for filesystem-sensitive changes, at minimum `cargo check -p lushtext-core --all-targets`.
- [x] 5.5 Run `openspec validate cleanup-filesystem-boundary-polish --strict`.
- [x] 5.6 Run `openspec validate --changes --strict`, `openspec validate --specs --strict`, and `openspec validate --all --strict`.
- [x] 5.7 Run final no-leftovers searches proving status-only rich probes and unused sidecar helper surfaces do not remain.
