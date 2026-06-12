## 1. Baseline And Diagnosis

- [x] 1.1 Capture the current workspace-sidebar toggle behavior at hidden-to-shown and shown-to-hidden endpoints for narrow/collapsed, `1100sp` intermediate, and wide desktop widths.
- [x] 1.2 Record the live `1100sp` reproduction evidence: initial requested/rendered state, sidebar width, geometry samples, minimap state, and absence or presence of runtime warnings.
- [x] 1.3 Trace the `win.toggle-sidebar` path through requested state, `sync_secondary_surface_layout()`, split-view width sync, properties breakpoint sync, and minimap protection to identify which same-frame operation collapses the visible animation.

## 2. Workspace Sidebar Animation Coordination

- [x] 2.1 Refactor the workspace-sidebar visibility path so user intent is persisted immediately while transition-sensitive layout reconciliation can be staged safely.
- [x] 2.2 Preserve Libadwaita `AdwOverlaySplitView:show-sidebar` as the primary animation mechanism and avoid introducing a custom sidebar animation surface.
- [x] 2.3 Prevent document-properties breakpoint or presentation reconciliation from forcing the workspace sidebar directly to its endpoint during the first visible transition frame.
- [x] 2.4 Ensure final reconciliation runs after the transition settles so requested visibility, rendered visibility, toggle state, compact surface arbitration, document-properties presentation, and GSettings remain consistent.
- [x] 2.5 Keep minimap protection and final refresh behavior correct when the minimap is visible during sidebar show/hide transitions.

## 3. Automation And Visual Proof

- [x] 3.1 Extend bounded Automation1 visual geometry diagnostics as needed to distinguish fully hidden, fully visible, and intermediate workspace-sidebar transition states without exposing private document data.
- [x] 3.2 Add or update visual geometry stream scenarios for workspace-sidebar show and hide at collapsed overlay, `1100sp` intermediate, and wide desktop widths.
- [x] 3.3 Ensure visual proof summaries report animation-frame evidence separately from final-settle evidence, including intermediate frame counts, timing/skew metadata, final-settle status, and warning-scan status.
- [x] 3.4 Ensure proof policy rejects workspace-sidebar animation-sensitive artifacts that contain only final-settle screenshots or stale/incomplete frame-to-geometry mappings.
- [x] 3.5 Update automation and visual proof documentation if snapshot fields, readiness behavior, client commands, or summary fields change.

## 4. Test And Verification

- [x] 4.1 Add focused widget/unit coverage for requested vs rendered sidebar state and adaptive layout reconciliation at narrow, `1100sp`, and wide widths.
- [x] 4.2 Verify state extremes: no workspaces, one representative workspace, many or awkward workspaces/folders, constrained geometry, minimap visible, and document-properties requested/hidden combinations.
- [x] 4.3 Run the targeted visual geometry stream scenarios and confirm at least one mapped intermediate frame exists for each show/hide case.
- [x] 4.4 Run automation contract checks affected by any diagnostics or client changes, including `make check-automation-docs` and `make automation-client-self-test` when relevant.
- [x] 4.5 Run `make test-widget-headless` for GTK/widget coverage and treat any `FLAKY:` output as blocking.
- [x] 4.6 Run OpenSpec validation with `openspec validate preserve-workspace-sidebar-animation --strict`, then the broader validation set required before archive.
- [x] 4.7 Run `git diff --check` before handing off the completed implementation.
