# Release Notes

## Table of Contents

- [Required Shape](#required-shape)
- [Poetic Opening](#poetic-opening)
- [Semantic Diff Analysis](#semantic-diff-analysis)
- [Tone](#tone)
- [Section Guidance](#section-guidance)
- [GitHub Release Body](#github-release-body)

Release notes should feel like a careful teammate explaining what changed. Use concrete language, avoid internal-only jargon, and explain why changes matter to users.

## Required Shape

Use these exact Markdown headers, in this order:

```markdown
## Poetic Opening

## What's Changed

## Manual Actions Needed

## Warnings and Deprecations

## Bug Fixes
```

Every section must contain real content. If a section has nothing to report, write a short, explicit sentence such as `No manual action is needed for this release.`

## Poetic Opening

Choose one short complete stanza or verse from one of these poets:

- Arthur Rimbaud
- Oscar Wilde
- Charles Baudelaire
- Edgar Allan Poe
- William Shakespeare
- Florbela Espanca

Rules:

- Never repeat a stanza across releases.
- Double-check both local history and GitHub Release bodies before using the stanza.
- Use the full stanza or full verse exactly as selected from the poem. Do not use a fragment, opening lines, an ellipsis, or a paraphrased excerpt.
- Verify the poem structure from a source before choosing the stanza. If you cannot tell whether the selected lines are complete, choose a different stanza.
- For Shakespeare sonnets, use a complete quatrain or the final complete couplet. For poems with irregular verse blocks, use one complete printed stanza/verse block.
- For Rimbaud, Baudelaire, and Florbela Espanca, include the full original-language stanza or verse and the full English rendering.
- For non-English originals, prefer a public-domain translation or provide an original working translation and label it clearly.
- Attribute the poem title and poet.
- Keep the complete stanza short enough that it supports the notes without overwhelming the release.
- Do not use a stanza if its source or prior-use status is uncertain.

Suggested format:

```markdown
## Poetic Opening

Source checked: <source or edition used to confirm the stanza boundary>
Selection: Complete original stanza and complete English stanza.

Original (complete stanza):
> [full line 1]
> [full line 2]
> [full line 3]
> [full line 4]

English (complete stanza):
> [full line 1]
> [full line 2]
> [full line 3]
> [full line 4]

Arthur Rimbaud, "Poem Title"
```

For English originals:

```markdown
## Poetic Opening

Source checked: <source or edition used to confirm the stanza boundary>
Selection: Complete stanza.

Complete stanza:
> [full line 1]
> [full line 2]
> [full line 3]
> [full line 4]

William Shakespeare, "Poem Title"
```

Before release:

```bash
.agents/skills/publish-release/scripts/validate-release-notes.py "$NOTES" --gh-repo cominotti/lushtext
```

Then manually spot-check at least one existing GitHub Release body if the script had to skip GitHub access.

## Semantic Diff Analysis

Build notes from the semantic diff, not from commit messages alone. Use conventional commits as hints, then inspect the diff and changed behavior.

Look for:

- new user-visible features and workflows;
- changed defaults, settings, shortcuts, permissions, file formats, metadata, or persistence behavior;
- fixes that users would recognize;
- packaging, Flatpak, AppStream, desktop, icon, and MIME behavior;
- data-loss, migration, compatibility, or rollback concerns;
- performance, responsiveness, large-file, search, and memory changes;
- accessibility, keyboard, focus, warning, or visual polish changes.

## Tone

- Write to users, not to the commit log.
- Prefer `You can now...`, `LushText now...`, and `This release fixes...` over implementation details.
- Name manual actions plainly.
- Put risks in `Warnings and Deprecations`, not hidden inside feature bullets.
- Keep the voice warm and collaborative without promising more than the release delivers.

## Section Guidance

`What's Changed` should cover features and meaningful behavior changes. Group related changes by user workflow when that reads better than a raw list.

`Manual Actions Needed` should say whether users or maintainers need to do anything after upgrading. Include Flathub/store actions only if they affect publication or users.

`Warnings and Deprecations` should include compatibility notes, known issues, removed behavior, risky migrations, or changed expectations.

`Bug Fixes` should focus on fixed symptoms and outcomes, not internal module names.

## GitHub Release Body

The GitHub Release must receive the authored notes. The release workflow may initially create generated notes, so update the release after the workflow if needed:

```bash
gh release edit "$VERSION" --title "LushText $VERSION" --notes-file "$NOTES"
```

If no release exists:

```bash
gh release create "$VERSION" --verify-tag --title "LushText $VERSION" --notes-file "$NOTES"
```
