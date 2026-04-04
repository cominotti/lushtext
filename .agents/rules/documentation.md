---
description: Documentation maintenance rules
globs: "**/*.{rs,ui,css,toml}"
---

# Documentation Maintenance

## Mandatory Updates on Code Changes

Whenever code changes are made, evaluate whether the following files need updates:

1. **`README.md`** — features list, build instructions, EditorConfig docs, architecture overview, test count
2. **`.claude/CLAUDE.md`** — module layout, key design decisions, architecture overview
3. **`.claude/rules/*.md`** — coding conventions, widget wiring patterns, UI rules, build rules
4. **`.claude/skills/*/references/*.md`** — testing patterns, async patterns, architecture references

**CRITICAL: `README.md` must always be kept in sync with the code.** It is the project's public-facing documentation and the first thing contributors and users see. When any of the following change, the README must be updated in the same commit or PR:
- Features added, removed, or significantly changed
- Build instructions or dependencies changed
- EditorConfig supported properties changed
- Architecture (crate structure, module layout) changed
- Test infrastructure or test commands changed
- Tech stack versions bumped (Rust MSRV, GTK, Libadwaita, etc.)

**Trigger conditions** (any of these means you must check):
- New widget or module added → update module layout in CLAUDE.md and README.md, widget hierarchy in ui.md
- New user-visible feature → update Features section in README.md
- New pattern introduced (timer, signal, async) → document in the appropriate rules file
- New convention discovered or existing one refined → update rust.md or widget-wiring.md
- Testing pitfall encountered → add to gtk-testing skill references
- Build or dependency change → update build.md and README.md

Do not skip this step. Stale documentation causes repeated mistakes in future sessions.
