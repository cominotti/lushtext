# blueprint-validation-hardening Specification

## Purpose
Define how Blueprint validation, proof artifacts, warnings, advisory lint, visual comparison, and headless capture diagnostics are handled without weakening committed source or runtime packaging.

## Requirements

### Requirement: Blueprint proof artifacts SHALL stay out of source control

Generated validation, screenshot, pixel-diff, and smoke-proof artifacts for Blueprint template work SHALL be ignored through targeted paths that do not hide committed source or helper files.

#### Scenario: Targeted artifact paths are ignored

- **WHEN** Blueprint validation, visual comparison, or visual-smoke workflows emit logs, screenshots, or generated proof output under their configured `build/` artifact directories
- **THEN** those generated artifacts are excluded from ordinary Git status
- **AND** existing tracked smoke helper artifacts such as the local smoke launcher remain visible to Git

#### Scenario: Reviewable proof stays textual and bounded

- **WHEN** a Blueprint change requires before/after visual proof
- **THEN** the workflow records a bounded text summary containing the baseline ref, compiler version, commands, state matrix, artifact directory, and pixel-diff result
- **AND** large screenshots, raw logs, and pixel-diff images remain disposable artifacts rather than committed source files

### Requirement: Blueprint compile validation SHALL classify warnings

The Blueprint compile gate SHALL keep generated `.ui` drift and template-contract checks blocking, while allowing only documented known compiler warnings.

#### Scenario: Deprecated GtkShortcuts warnings fail the gate

- **WHEN** the compile gate processes any Blueprint template
- **THEN** warnings for deprecated `GtkShortcuts*` widgets are not accepted as
  known-good output
- **AND** `resources/ui/shortcuts.blp` uses the maintained Libadwaita shortcuts
  dialog widgets instead of the deprecated GTK shortcuts widget family

#### Scenario: Unknown compile warnings fail the gate

- **WHEN** `blueprint-compiler compile` emits a warning outside the documented known-warning policy
- **THEN** `make check-blueprint` fails
- **AND** the failure output identifies the file and warning text that must be fixed or classified

#### Scenario: Compiler version and template coverage are reported

- **WHEN** `make check-blueprint` runs
- **THEN** the output includes the `blueprint-compiler` version used for validation
- **AND** the output identifies the templates covered by the compile, drift, and contract checks

### Requirement: Blueprint lint SHALL remain advisory until triage is explicit

Blueprint lint diagnostics SHALL be grouped and classified before any full lint gate becomes blocking.

#### Scenario: Advisory lint summarizes diagnostics

- **WHEN** the advisory Blueprint lint workflow runs
- **THEN** diagnostics are summarized by rule and file
- **AND** current diagnostic families such as scroll-parent structure, Adwaita container suggestions, translation text, Unicode text, descriptive text, adjustment property order, and all-caps labels are either fixed or classified

#### Scenario: Safe lint fixes preserve generated UI contracts

- **WHEN** a lint fix changes a `.blp` template
- **THEN** the corresponding `.ui` is regenerated
- **AND** Blueprint compile, drift, and UI-template contract checks pass before the fix is accepted

#### Scenario: Geometry-sensitive lint suggestions require proof

- **WHEN** a lint suggestion changes container structure, scrolling behavior, layout ownership, or widget geometry
- **THEN** it is not treated as a safe cleanup
- **AND** the change is accepted only after relevant widget or visual proof shows the generated UI contract remains intact

### Requirement: Visual comparison SHALL be reusable for Blueprint template changes

Blueprint template reviews SHALL have a reusable before/after visual comparison workflow instead of one-off build artifacts.

#### Scenario: Baseline and current captures use the same inputs

- **WHEN** the visual comparison script is run with a baseline ref, current checkout, and artifact directory
- **THEN** it captures both revisions with the same fixture data, helper scripts, viewport matrix, and state matrix
- **AND** it writes a concise comparison summary with the resulting pixel-diff metrics

#### Scenario: State matrix covers Blueprint-sensitive surfaces

- **WHEN** the visual comparison workflow captures Blueprint template states
- **THEN** it includes representative populated states, empty or no-required-context states where relevant, constrained geometry, and secondary surfaces such as menus, dialogs, popovers, editor alerts, search, properties, preview, and sidebar states touched by the template change
- **AND** the captured surfaces remain readable, reachable, and free of unintended scrollbars, fake rows, clipped actions, and unrelated context dependencies

#### Scenario: Visual differences are explained

- **WHEN** the visual comparison reports non-zero differences
- **THEN** each difference is either tied to an intentional template change or treated as a validation failure requiring investigation
- **AND** accepted differences are recorded in the bounded text summary rather than by committing raw screenshots

### Requirement: Headless capture SHALL preserve diagnostics with short runtime paths

The headless Mutter capture helper SHALL avoid PipeWire socket path-length failures while keeping useful diagnostics for both success and failure runs.

#### Scenario: Runtime directory path stays short

- **WHEN** the capture helper launches a headless session from a deeply nested workspace or artifact path
- **THEN** it sets `XDG_RUNTIME_DIR` to a short temporary path suitable for PipeWire sockets
- **AND** the artifact output records the runtime directory path or cleanup status

#### Scenario: Failure artifacts explain runtime cleanup

- **WHEN** a capture run fails
- **THEN** logs and runtime-dir diagnostics are preserved
- **AND** `runtime-dir.txt` does not point to a deleted directory unless a companion marker explains that cleanup already occurred

#### Scenario: Successful runs clean temporary runtime state

- **WHEN** a capture run succeeds
- **THEN** temporary runtime directories created by the helper are cleaned up
- **AND** the artifact output records that cleanup so later debugging does not mistake success cleanup for missing failure evidence

### Requirement: Blueprint validation guidance SHALL stay synchronized

Contributor and agent guidance SHALL describe how Blueprint source edits, generated `.ui` files, warnings, lint diagnostics, visual proof, and generated artifacts are handled.

#### Scenario: Guidance explains the source and artifact contract

- **WHEN** a contributor edits Blueprint templates
- **THEN** project guidance identifies `.blp` as the editable source, generated `.ui` as committed compatibility output, and generated validation artifacts as ignored disposable output
- **AND** the guidance names the commands needed to regenerate and validate templates

#### Scenario: Guidance explains warning and lint policy

- **WHEN** a contributor sees Blueprint compiler warnings or lint diagnostics
- **THEN** project guidance distinguishes blocking unknown compile warnings from advisory lint diagnostics
- **AND** it explains that new warning classes must be fixed or explicitly classified before publication

#### Scenario: Local and CI expectations remain aligned

- **WHEN** Blueprint validation guidance is updated
- **THEN** local Makefile targets, scripts, and any CI wiring describe the same required checks
- **AND** no end-user runtime dependency is introduced by validation-only tooling
