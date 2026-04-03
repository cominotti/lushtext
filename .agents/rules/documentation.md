---
description: Documentation maintenance rules
globs: "**/*.{rs,ui,css,toml}"
---

# Documentation Maintenance

## Mandatory Updates on Code Changes

Whenever code changes are made, evaluate whether the following files need updates:

1. **`.claude/CLAUDE.md`** — module layout, key design decisions, architecture overview
2. **`.claude/rules/*.md`** — coding conventions, widget wiring patterns, UI rules, build rules
3. **`.claude/skills/*/references/*.md`** — testing patterns, async patterns, architecture references

**Trigger conditions** (any of these means you must check):
- New widget or module added → update module layout in CLAUDE.md, widget hierarchy in ui.md
- New pattern introduced (timer, signal, async) → document in the appropriate rules file
- New convention discovered or existing one refined → update rust.md or widget-wiring.md
- Testing pitfall encountered → add to gtk-testing skill references
- Build or dependency change → update build.md

Do not skip this step. Stale documentation causes repeated mistakes in future sessions.
