---
description: Git conventions and commit rules
globs: *
---

# Git Rules

## Commit Messages

Use conventional commits matching invowk-rust style:

```
type(scope): short description

Optional longer explanation.

Co-Authored-By: Claude Opus 4.6 (1M context) <noreply@anthropic.com>
```

**Types:** `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `chore`
**Scopes (optional):** `ui`, `sidebar`, `editor`, `workspace`, `session`, `build`, `flatpak`

## Signing

All commits must be signed. SSH signing is configured globally (`commit.gpgsign=true`, `gpg.format=ssh`). Never skip signing with `--no-gpg-sign` or `-c commit.gpgsign=false`.

## Branch

Default branch is `main`. Do not use `master`.

## General

- Never use `--no-verify` to bypass hooks.
- Never force-push to `main`.
- Create new commits rather than amending, unless explicitly asked.
- Stage specific files by name, not `git add -A` or `git add .`.
- Repo-managed hooks live in `.githooks/`; install them for the current checkout with `make install-git-hooks`.
