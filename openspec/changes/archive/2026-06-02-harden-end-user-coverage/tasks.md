## 1. Coverage Map And Harness Setup

- [x] 1.1 Document the end-user coverage map, listing which lane owns unit, integration, property, fuzz replay, fuzz smoke, widget, visual smoke, portal/sandbox smoke, accessibility smoke, performance smoke, and mutation coverage
- [x] 1.2 Add or update Make targets for the new smoke lanes with clear names, dependency checks, and skip behavior when host support is unavailable
- [x] 1.3 Add shared helpers for smoke scripts to create isolated XDG state, temporary fixtures, log directories, and artifact output paths
- [x] 1.4 Add documentation that explains which new lanes are expected on pull requests, scheduled/manual runs, release validation, and local investigation

## 2. External File Monitor Coverage

- [x] 2.1 Add a widget or integration-plus-widget test that opens a temp file-backed document, modifies the file externally, and waits for the changed-on-disk inline alert through the live monitor path
- [x] 2.2 Add coverage proving unsaved local edits remain visible after an external backing-file modification
- [x] 2.3 Add coverage for discard-and-reload restoring the current disk bytes and clearing the warning
- [x] 2.4 Add coverage for dismissing the external-change warning without reloading or mutating buffer content
- [x] 2.5 Add coverage proving LushText's own successful save does not create a false changed-on-disk warning
- [x] 2.6 Add coverage proving a failed save keeps the document modified and surfaces failure feedback

## 3. Unsaved Close Safety Coverage

- [x] 3.1 Add window-level tab close tests for modified file-backed documents covering Cancel, Save success, Save failure, and Discard
- [x] 3.2 Add window-level close-request tests for modified file-backed documents using the same close path users trigger from the window
- [x] 3.3 Add close-flow tests for modified untitled documents proving Save requires a successful Save As destination
- [x] 3.4 Add close-flow tests for untitled Cancel and Discard, including draft preservation or cleanup
- [x] 3.5 Add multi-tab close-request tests for Cancel, selected-row Save, unchecked-row Discard, and draft cleanup
- [x] 3.6 Add close-flow coverage proving in-flight saves inhibit close completion until the save result is known
- [x] 3.7 Add coverage proving confirmed window close persists session and draft cleanup state consistently

## 4. Desktop Open Activation Coverage

- [x] 4.1 Add an application-level test for `ApplicationImpl::open` that opens one supported file into a tab
- [x] 4.2 Add activation coverage for multiple files, duplicate canonical paths, and active-tab selection
- [x] 4.3 Add coverage proving activation reuses an existing application window when possible
- [x] 4.4 Add activation coverage for missing and inaccessible paths, including feedback and open-path bookkeeping
- [x] 4.5 Extend desktop metadata verification so static `Exec` document forwarding is tied to a real activation smoke or focused test
- [x] 4.6 Add coverage proving explicit CLI or desktop file arguments take priority over restored session active-tab selection

## 5. Menu Workflow Coverage

- [x] 5.1 Add user-action coverage for Zoom In, Zoom Out, Reset Zoom, and zoom control enabled or disabled state
- [x] 5.2 Add coverage proving zoom behavior is scoped according to the implementation contract across tab switches
- [x] 5.3 Add coverage for theme or style selection updating the current window and active editor style
- [x] 5.4 Add coverage proving newly opened editors inherit the selected theme or style behavior
- [x] 5.5 Add coverage for invalid or missing style scheme fallback without crashing or unreadable editor colors
- [x] 5.6 Add coverage for cycling invisible-character modes through the user-visible action path
- [x] 5.7 Add coverage proving invisible-character preference persistence applies to newly opened tabs
- [x] 5.8 Add a testable print operation path or test double that verifies Print prepares active-document content without requiring a physical printer
- [x] 5.9 Add print cancel and print failure coverage proving document state remains unchanged and feedback is shown

## 6. Desktop Visual Smoke Coverage

- [x] 6.1 Build a visual smoke script that launches LushText in an isolated headless desktop session and captures screenshot artifacts
- [x] 6.2 Add state setup for a normal main editor shell with a representative document
- [x] 6.3 Add state setup and captures for a narrow or compact layout, a short-window layout, search with minimap markers, Markdown preview, and document properties or dialog geometry
- [x] 6.4 Add pre-capture verification through stable actions, accessible names, or a narrow read-only inspection surface instead of fixed sleeps or coordinate guesses
- [x] 6.5 Add coarse screenshot assertions for nonblank output, expected bounds, persistent chrome visibility, and artifact preservation
- [x] 6.6 Add log scanning that fails on unexpected GTK, Libadwaita, GDK, renderer, or accessibility warnings
- [x] 6.7 Add extended visual smoke coverage for at least one alternate environment dimension such as dark style, high scale factor, or non-Cairo renderer when supported

## 7. Portal And Sandbox Workflow Coverage

- [x] 7.1 Add file chooser smoke coverage for Open File accepting a selected document through native or portal-backed chooser paths
- [x] 7.2 Add file chooser smoke coverage for Save As adopting the selected destination only after a successful write
- [x] 7.3 Add chooser cancellation coverage for Open File, Save As, and Add Workspace Folder preserving document, workspace, modified, and draft state
- [x] 7.4 Add confined Flatpak smoke coverage that launches the app, verifies GResource and GSettings loading, and opens a file from an accessible path
- [x] 7.5 Add confined Snap smoke coverage or a clearly gated task that activates after the Snap platform dependency can build the app
- [x] 7.6 Add inaccessible-path confined smoke coverage proving graceful access error or supported grant behavior without crash or data loss
- [x] 7.7 Capture package type, runtime version, portal implementation, permissions, denials, and relevant environment details as artifacts
- [x] 7.8 Ensure missing Flatpak, Snap, portal, or runtime dependencies produce explicit skips rather than false passes

## 8. Accessibility And Keyboard Coverage

- [x] 8.1 Add or stabilize accessible names and roles for the in-tab search entry and search controls
- [x] 8.2 Add accessibility metadata coverage for workspace toggle, document-properties toggle, tab controls, primary menu controls, status controls, and editor action buttons
- [x] 8.3 Add accessibility metadata coverage for save-changes, local-history, notes, preferences, and file-related dialog controls
- [x] 8.4 Add keyboard-only workflow tests for opening search, typing a query, navigating matches, closing search, and restoring editor focus
- [x] 8.5 Add keyboard-only workflow tests for command palette, workspace/sidebar visibility, document-properties visibility, and return-to-editor focus
- [x] 8.6 Add keyboard-only save-changes dialog coverage for Save, Discard, Cancel, and multi-document selection controls
- [x] 8.7 Add an accessibility-enabled smoke lane that does not set `NO_AT_BRIDGE=1` and queries the app through AT-SPI or the host accessibility API
- [x] 8.8 Preserve accessibility smoke artifacts including queried tree subset, focus path, warnings, and clear skip reasons when accessibility services are unavailable

## 9. Performance And Large-File Coverage

- [x] 9.1 Add a lightweight performance smoke command distinct from full Criterion benchmark reports
- [x] 9.2 Record environment, build profile, toolkit versions, fixture sizes, thresholds, and measured timings in performance smoke artifacts
- [x] 9.3 Define coarse documented thresholds for startup or first-window readiness and representative file-open latency
- [x] 9.4 Add performance smoke coverage for workspace indexing, command-palette file search, and workspace-wide content search
- [x] 9.5 Add performance smoke coverage for representative save, Save As, Replace All, and undo workflows using disposable fixtures
- [x] 9.6 Add UI-observable tests for large-file syntax-disable, undo-disable, and refuse-to-load thresholds
- [x] 9.7 Add coverage for very large save snapshot consistency, duplicate save blocking, and read-only protection while save is pending
- [x] 9.8 Add memory-pressure coverage for background tab eviction and reload without user data loss or open-path corruption
- [x] 9.9 Add a scheduled/manual or release validation path for deeper benchmark reports with artifact upload

## 10. CI And Documentation Wiring

- [x] 10.1 Wire cheap deterministic/widget coverage into the appropriate default or pull-request CI jobs
- [x] 10.2 Wire visual, portal/sandbox, accessibility, and deeper performance checks as scheduled, manual, release, or opt-in CI lanes with artifacts
- [x] 10.3 Update CI comments and Makefile help text so maintainers know why each lane is gated where it is
- [x] 10.4 Update `.agents/rules/build.md`, `AGENTS.md`, and testing documentation with the new commands and lane boundaries
- [x] 10.5 Update any skill or agent guidance needed so future test work chooses the correct harness for live desktop, portal, accessibility, and performance behavior

## 11. Validation

- [x] 11.1 Run `openspec validate harden-end-user-coverage --strict`
- [x] 11.2 Run focused tests for each newly added widget or integration coverage group
- [x] 11.3 Run smoke scripts locally where host dependencies are available and verify skip messages where they are not
- [x] 11.4 Run `make test-unit`, `make test-int`, `make test-widget-headless`, and any newly added fast coverage targets
- [x] 11.5 Run documentation or lint checks affected by Makefile, CI, script, and docs changes
- [x] 11.6 Record any host-dependent validation that could not run locally, with the exact missing dependency and the CI or manual lane that will cover it
  - Local host-dependent note: Snap smoke skipped with `SKIP: snapcraft is not installed.` The scheduled/manual `end-user-smoke` portal-sandbox lane preserves that explicit skip until a Snap-capable host is available.
