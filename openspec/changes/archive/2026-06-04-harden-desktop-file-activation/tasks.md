## 1. Editor Load State And Duplicate Ownership

- [x] 1.1 Add an explicit editor load-state model that distinguishes untitled, loading, loaded, and failed tabs without relying on titles, inline-alert text, or file-size side effects.
- [x] 1.2 Update `load_file_async` and load-result application so new file loads enter loading state, successful results enter loaded state, cancelled/stale results do not corrupt newer state, and failures enter failed state before window-level callbacks run.
- [x] 1.3 Ensure failed-load handling removes provisional path and canonical keys from `open_paths` even when the failed tab preserves modified buffer content for user safety.
- [x] 1.4 Audit save-as, rename, close, evict/reload, retry, and canonical duplicate reconciliation paths so they preserve or update the new load state consistently.

## 2. Activation-Aware Open Flow

- [x] 2.1 Add an activation-aware document-open entry point or option used by `ApplicationImpl::open` while keeping ordinary in-app `open_document` behavior available for sidebar, command palette, file chooser, and session restore.
- [x] 2.2 Implement duplicate selection so activation focuses existing loaded or loading owners, but bypasses failed placeholders and creates a new selected tab for the explicitly requested path.
- [x] 2.3 Preserve canonical duplicate detection for successfully loaded documents, including symlink/canonical-path reconciliation after async load completion.
- [x] 2.4 Confirm startup session restore cannot steal focus from explicit activation when restored tabs are added later or when restored load failures settle later.

## 3. Non-Path URI Robustness

- [x] 3.1 Update `ApplicationImpl::open` to handle every incoming `gio::File`, including inputs whose `path()` is `None`.
- [x] 3.2 Publish visible user feedback for unsupported URI/non-path activation inputs, including the URI when available, without creating a fake saved document tab.
- [x] 3.3 Continue processing remaining local files in the same activation after reporting unsupported URI inputs.
- [x] 3.4 Review `FileDialog` open/save callbacks for the same silent-drop pattern and either route unsupported selections to visible feedback or document why chooser APIs cannot produce non-path selections in the supported flow.

## 4. Widget Regression Tests

- [x] 4.1 Add a widget test that seeds or constructs a restored failed placeholder for a path, then activates the same path after it becomes readable and verifies the old failed tab remains while the new file tab is selected with matching content.
- [x] 4.2 Add a widget test that drives a normal missing-file activation failure, verifies `open_paths` cleanup, creates the file, then verifies a later activation succeeds.
- [x] 4.3 Add a widget test for a modified failed-load placeholder that remains recoverable but does not block an explicit activation for the same path.
- [x] 4.4 Add a widget test where explicit activation remains selected after session restore adds another tab whose load later fails.
- [x] 4.5 Add a widget test that repeated activation of an already loaded file focuses the existing tab without duplication.
- [x] 4.6 Add a widget test that canonical/symlink duplicate activation still deduplicates successfully loaded documents.
- [x] 4.7 Add a widget test that a non-path URI `gio::File` activation publishes visible feedback and creates no bogus path-backed tab.
- [x] 4.8 Add a widget test that one activation containing an unsupported URI and a valid local file reports the URI problem while still opening and focusing the valid file.
- [x] 4.9 Add a widget test that an existing window receives unsupported URI activation feedback and remains responsive.

## 5. Portal And Sandbox Diagnostics

- [x] 5.1 Identify the existing portal/sandbox smoke lane or add a scoped diagnostic hook that can record unsupported URI activation behavior when host support is available.
- [x] 5.2 Ensure the smoke/diagnostic path records URI form, runtime identity, portal implementation, granted permissions, and relevant GIO/portal/access-denial logs.
- [x] 5.3 Ensure the smoke/diagnostic path clearly skips unsupported URI validation when the host lacks portal, confined runtime, or URI activation support.
- [x] 5.4 Keep accessible local-file smoke coverage separate from unsupported URI diagnostics so URI failures cannot mask local-file open regressions.

## 6. Verification

- [x] 6.1 Run the focused widget tests for application open activation and window duplicate bookkeeping.
- [x] 6.2 Run the broader widget test shard that includes `crates/lushtext/tests/widget/app.rs` and relevant window tests.
- [x] 6.3 Run `cargo test -p lushtext --test widget` or the repository's current widget-test command if the harness requires a wrapper.
- [x] 6.4 Run `cargo test --workspace` or the repository's current full Rust test command if environment support allows it.
- [x] 6.5 Run `openspec validate --change harden-desktop-file-activation --strict`.
- [x] 6.6 Run formatting/linting commands required by `.agents/rules/build.md` for Rust and OpenSpec changes.
