# Implementation Plan: File Peek

**Branch**: `[001-file-peek]` | **Date**: 2026-04-13 | **Spec**: `/var/home/danilo/Workspace/github/cominotti/lushtext/specs/001-file-peek/spec.md`
**Input**: Feature specification from `/var/home/danilo/Workspace/github/cominotti/lushtext/specs/001-file-peek/spec.md`

## Summary

Add a transient, read-only file peek card for sidebar-selected files. `Space`
opens or closes the peek for the current file, Up and Down keep sidebar
navigation active while the preview updates in place, and promotion still flows
through the window's existing `open_document()` path so duplicate-tab reuse,
editor focus, and session behavior stay authoritative in one place. The peek
will be implemented as a `workspace_section`-owned overlay backed by a bounded
background snapshot service so it never resizes split panes or creates editor
state.

## Technical Context

**Language/Version**: Rust 1.94.1 (Edition 2024)  
**Primary Dependencies**: GTK4 0.11, Libadwaita 0.9, GtkSourceView 5 0.11, gio/glib/pango 0.22, existing `spawn_blocking_then` background executor  
**Storage**: Local workspace files for read-only snapshot reads; transient in-memory peek state only; no new XDG, draft, session, or GSettings persistence  
**Testing**: `make test-unit`, targeted `lushtext-core` unit tests, `make test-widget-headless`, targeted widget coverage under `/var/home/danilo/Workspace/github/cominotti/lushtext/crates/lushtext/tests/widget/`, `make check`, live `make run` validation  
**Target Platform**: Linux GNOME desktop and Flatpak  
**Project Type**: GTK4/Libadwaita Rust desktop application  
**Performance Goals**: Eligible local text files should show preview content or an explicit fallback within 0.25 seconds in normal cases; bounded reads must keep the GTK main thread responsive while users hold Up and Down through large trees  
**Constraints**: Peek remains strictly read-only; it must not create drafts, undo history, file monitors, or session entries; the surface must stay anchored beside the selected row without consuming pane width; stale async completions must be dropped cleanly; unsupported, unreadable, binary, and too-large files need explicit user-facing fallback states  
**Scale/Scope**: v1 covers sidebar-selected files only; it must behave across Small, Comfy, and Large sidebar presets, deep drill-down roots, workspace filtering, row recycling, and long-lived sessions without changing the existing split-view shell

## Constitution Check

*GATE: Pass before Phase 0 research. Re-check after Phase 1 design.*

- [x] User-visible behavior is specified with concrete GTK or Libadwaita interaction, focus, sizing, animation, and enabled-state expectations.
  The spec fixes `Space`, `Escape`, `Enter`, click-away, overlay anchoring,
  focus return, and the "no pane resize" contract.
- [x] Data-safety impact is identified for save, close, draft, session, external-change, and search or replace flows touched by the feature.
  Peek is explicitly read-only and must not touch draft, session, undo,
  monitor, or close-save flows.
- [x] Architecture changes preserve `model/`, `services/`, and `ui/` boundaries, and move blocking or slow work off the GTK main thread.
  GTK interaction stays in `ui/sidebar/workspace_section/`; bounded file I/O
  moves into a GTK-free service and runs through `spawn_blocking_then`;
  promotion stays in the existing window document workflow.
- [x] Verification lists the required automated coverage plus live `make run` validation whenever warnings, paned geometry, focus, or allocation behavior could regress.
  The plan requires unit coverage for snapshot classification and truncation,
  widget coverage for keyboard and focus flows, and live runtime validation for
  overlay positioning and warning-free behavior.
- [x] Documentation and delivery fallout is enumerated (`README.md`, `AGENTS.md`, `.agents/rules/*`, build or packaging metadata, `cargo-sources.json` when relevant).
  `docs/next/file-peek.md` must stay aligned with the shipped behavior. If new
  module files are added under `services/` or `ui/sidebar/workspace_section/`,
  update the root `AGENTS.md` and `README.md` module maps in the same change.
  No packaging or `cargo-sources.json` work is expected.
- [x] Any expected complexity exception or discovered pre-existing blocker is recorded here with the plan to resolve it in this work stream.
  No justified exceptions are needed at planning time. Any focus leak, popover
  clipping, or GTK warning found during runtime validation becomes same-stream
  blocker work.

## Phase 0 Research Summary

Research is captured in `/var/home/danilo/Workspace/github/cominotti/lushtext/specs/001-file-peek/research.md`.
The chosen design keeps row anchoring local to `workspace_section`, uses a new
bounded snapshot service instead of the full editor load path, and treats live
focus and overlay validation as mandatory acceptance work.

## Project Structure

### Documentation (this feature)

```text
/var/home/danilo/Workspace/github/cominotti/lushtext/specs/001-file-peek/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── file-peek-ui.md
└── tasks.md
```

### Source Code (planned touch points)

```text
/var/home/danilo/Workspace/github/cominotti/lushtext/crates/lushtext-core/src/
├── services/
│   ├── file_limits.rs
│   └── file_peek.rs                    # New bounded snapshot loading + classification
└── ui/
    ├── sidebar/
    │   ├── mod.rs
    │   ├── callbacks.rs
    │   └── workspace_section/
    │       ├── mod.rs
    │       ├── imp.rs
    │       ├── roots.rs
    │       ├── tree_loading.rs
    │       ├── actions.rs
    │       └── peek.rs                 # New section-owned peek workflow
    └── window/
        ├── imp.rs
        └── documents.rs                # Existing open_document() remains the promotion path

/var/home/danilo/Workspace/github/cominotti/lushtext/crates/lushtext/tests/widget/
├── common.rs
├── workspace_section.rs
├── sidebar.rs
└── window.rs

/var/home/danilo/Workspace/github/cominotti/lushtext/docs/next/file-peek.md
/var/home/danilo/Workspace/github/cominotti/lushtext/README.md
/var/home/danilo/Workspace/github/cominotti/lushtext/AGENTS.md
```

**Structure Decision**: Add one GTK-free `services/file_peek.rs` module for
bounded snapshot reads and one `peek.rs` sibling workflow inside
`ui/sidebar/workspace_section/`. Do not create a top-level `ui/file_peek/`
subtree and do not reuse the existing tab-oriented markdown preview pane. Row
selection, row recycling, popover anchoring, and dismissal wiring are already
section-local, while sidebar-to-window promotion plumbing stays in the existing
sidebar stack and duplicate-tab reuse already belongs to the window shell.

## Phase 1 Design Summary

### Data model

- Use a request-oriented design with `PeekTarget`, `PeekSnapshot`, and
  `PeekSession` concepts documented in `data-model.md`.
- Keep the persisted model untouched. Peek state stays transient and section-owned.
- Make preview eligibility and fallback state explicit so the UI does not infer
  behavior ad hoc from I/O errors.

### UI contract

- `Space` toggles peek for the selected file row.
- `Escape`, click-away, invalid selection, workspace filter hide, or row
  invalidation close the peek.
- Up and Down keep focus in the sidebar list while the current peek updates in place.
- `Enter` or the popover's `Open` button promote the previewed file through the
  normal open-document path, then dismiss the peek.
- The surface is an anchored floating card beside the selected row and must not
  resize the left, center, or right panes.

### Verification design

- Unit coverage in `lushtext-core` for bounded snapshot reads, preview state
  classification, truncation metadata, and stale-result suppression helpers.
- Widget coverage in `crates/lushtext/tests/widget/` for keyboard toggling,
  repeated-`Space`, `Escape`, and click-away dismissal, focus return to the
  selected sidebar row after non-promotion close, promotion reuse, and
  invalidation caused by selection changes or section rebuilds.
- Live `make run` validation across Small, Comfy, and Large sidebar presets to
  confirm warning-free popover positioning, no pane resize, and no focus strand.
- Acceptance timing validation for `SC-002` using the quickstart flow: record
  20 eligible local text-file peek attempts and confirm at least 19 render
  preview content or an explicit fallback within 0.25 seconds without freezing
  sidebar navigation.

## Post-Design Constitution Check

- [x] The design still honors the GNOME UX contract with explicit trigger,
  focus, sizing, and overlay behavior.
- [x] The design remains read-only and does not introduce draft, session, or
  close-flow risk.
- [x] The design keeps slow I/O in a service and keeps GTK wiring in the
  sidebar section adapter.
- [x] The design has proportional verification at unit, widget, and live
  runtime layers.
- [x] Documentation fallout remains bounded to UX and module-map updates; no
  packaging or build metadata work is currently required.

## Complexity Tracking

No justified constitution exceptions are required for this feature.
