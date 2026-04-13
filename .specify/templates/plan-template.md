# Implementation Plan: [FEATURE]

**Branch**: `[###-feature-name]` | **Date**: [DATE] | **Spec**: [link]
**Input**: Feature specification from `/specs/[###-feature-name]/spec.md`

**Note**: This template is filled in by the `/speckit.plan` command. See
`.specify/templates/plan-template.md` for the execution workflow.

## Summary

[Extract from feature spec: primary requirement + technical approach from research]

## Technical Context

**Language/Version**: Rust 1.94.1 (Edition 2024)
**Primary Dependencies**: GTK4 0.11, Libadwaita 0.9, GtkSourceView 5 0.11, gio/glib/pango 0.22
**Storage**: Local files in workspace roots, XDG data-dir JSON state, GSettings
**Testing**: `make test`, targeted unit and integration coverage, `make test-widget-headless`, `make check`
**Target Platform**: Linux GNOME desktop and Flatpak
**Project Type**: GTK4/Libadwaita Rust desktop application
**Performance Goals**: Keep the GTK main thread responsive and avoid regressions in large-file, sidebar, search, and session flows
**Constraints**: No GTK/pixman warnings in affected flows; preserve documented UX contracts; keep blocking I/O off the main thread; maintain data-safety guarantees
**Scale/Scope**: Multi-workspace editor with deep file trees, large files, long-lived sessions, and persistent document state

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

- [ ] User-visible behavior is specified with concrete GTK or Libadwaita interaction, focus, sizing, animation, and enabled-state expectations.
- [ ] Data-safety impact is identified for save, close, draft, session, external-change, and search or replace flows touched by the feature.
- [ ] Architecture changes preserve `model/`, `services/`, and `ui/` boundaries, and move blocking or slow work off the GTK main thread.
- [ ] Verification lists the required automated coverage plus live `make run` validation whenever warnings, paned geometry, focus, or allocation behavior could regress.
- [ ] Documentation and delivery fallout is enumerated (`README.md`, `AGENTS.md`, `.agents/rules/*`, build or packaging metadata, `cargo-sources.json` when relevant).
- [ ] Any expected complexity exception or discovered pre-existing blocker is recorded here with the plan to resolve it in this work stream.

## Project Structure

### Documentation (this feature)

```text
specs/[###-feature]/
├── plan.md              # This file (/speckit.plan command output)
├── research.md          # Phase 0 output (/speckit.plan command)
├── data-model.md        # Phase 1 output (/speckit.plan command)
├── quickstart.md        # Phase 1 output (/speckit.plan command)
├── contracts/           # Phase 1 output (/speckit.plan command)
└── tasks.md             # Phase 2 output (/speckit.tasks command - NOT created by /speckit.plan)
```

### Source Code (repository root)

```text
crates/
├── lushtext/
│   └── tests/
│       ├── integration.rs
│       ├── widget.rs
│       └── widget_tests/
└── lushtext-core/
    ├── benches/
    └── src/
        ├── model/
        ├── services/
        ├── ui/
        ├── app.rs
        ├── config.rs
        └── lib.rs

resources/
data/
build-aux/
docs/
```

**Structure Decision**: [Document the selected structure and reference the real
directories captured above]

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| [e.g., new cross-layer helper] | [current need] | [why a smaller change was insufficient] |
| [e.g., temporary UX exception] | [specific problem] | [why the documented contract could not be preserved directly] |
