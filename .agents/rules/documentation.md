---
description: Documentation maintenance rules
globs: "**/*.{rs,ui,css,toml}"
---

# Documentation Maintenance

## Mandatory Updates on Code Changes

Whenever code changes are made, evaluate whether the following files need updates:

1. **`README.md`** — features list, build instructions, EditorConfig docs, architecture overview, test count
2. **`AGENTS.md`** — module layout, key design decisions, architecture overview
3. **`.agents/rules/*.md`** — coding conventions, widget wiring patterns, UI rules, build rules
4. **`docs/accessibility.md`** — keyboard paths, screen-reader expectations, visual accessibility behavior, smoke coverage, release reference checks, and platform caveats when UI accessibility behavior changes
5. **`docs/accessibility-matrix.md`** — accessibility surface/state row ids, smoke/visual/manual proof mapping, stable anchors, and uncovered row status when UI accessibility behavior changes
6. **`docs/accessibility-orca-checklist.md`** — manual Orca validation fields, workflow matrix rows, privacy boundaries, and sample bounded release artifact when manual screen-reader expectations change
7. **`docs/workflow-readability-matrix.md`** — workflow `WFR-*` row ids, file sets, owned pure policy, test-seam counts, seam value objects, evidence surfaces, risk tiers, and migration status when workflow structure changes
8. **`docs/next/*.md`** — the repository's planned-work records. When a change advances, rescopes, defers, or completes a phase of a multi-change programme documented there (for example `docs/next/workflow-readability.md` or `docs/next/gtk-lush.md`), update that record's posture, baseline, remaining scope, and deferrals in the same change instead of leaving it describing a superseded plan
9. **`.agents/skills/*/references/*.md`** — testing patterns, async patterns, architecture references

**CRITICAL: `README.md` must always be kept in sync with the code.** It is the project's public-facing documentation and the first thing contributors and users see. When any of the following change, the README must be updated in the same commit or PR:
- Features added, removed, or significantly changed
- Build instructions or dependencies changed
- EditorConfig supported properties changed
- Architecture (crate structure, module layout) changed
- Test infrastructure or test commands changed
- Tech stack versions bumped (Rust MSRV, GTK, Libadwaita, etc.)

**Trigger conditions** (any of these means you must check):
- New widget or module added → update module layout in AGENTS.md and README.md, widget hierarchy in ui.md
- New user-visible feature → update Features section in README.md
- New exported action, action parameter/state, D-Bus automation method/property, snapshot field, workflow event field, readiness predicate/blocker, automation-client command/status/exit/result field, scenario-helper flag, scenario manifest field, or scenario artifact meaning → update `docs/automation.md`, `docs/automation-reference.md`, and run `make check-automation-docs` plus `make automation-client-self-test` when the client changed
- New or changed accessible name, role, description, state, relation, announcement, keyboard path, stable AT-SPI anchor, accessibility smoke scenario, visual accessibility caveat, or manual screen-reader expectation → update `docs/accessibility.md` and the relevant rows in `docs/accessibility-matrix.md`; update `docs/end-user-coverage.md` when release or smoke-lane expectations change; update `docs/automation.md` and `docs/automation-reference.md` when stable anchors, helper flags, snapshot fields, readiness predicates, or artifact fields change
- Automation, portal/sandbox, or Flatpak permission posture changes → update
  the user/developer automation docs and run `make check-flatpak-permissions`
- Workflow structure changed (module role added or renamed, pure policy relocated, seam value object introduced, evidence surface added or extended, test seam retired, or a workflow migrated to the readability convention) → update the workflow's row in `docs/workflow-readability-matrix.md`, advance the matching slot in `docs/next/workflow-readability.md` (status line, baseline, remaining-scope table, slot ledger), and run `make check-workflow-boundaries`
- A phase of a `docs/next/` planned-work programme advanced, was rescoped, or was deferred → update that record in the same change; a stale planned-work record is how a future session concludes finished work is still pending, or that outstanding work is done
- New pattern introduced (timer, signal, async) → document in the appropriate rules file
- New convention discovered or existing one refined → update rust.md or widget-wiring.md
- Testing pitfall encountered → add to gtk-testing skill references
- Build or dependency change → update build.md and README.md

Do not skip this step. Stale documentation causes repeated mistakes in future sessions.
