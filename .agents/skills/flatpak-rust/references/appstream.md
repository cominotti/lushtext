# AppStream and desktop metadata

Use this reference when changing the metainfo, desktop entry, icons, or their Meson install rules.

## Repository-first workflow

1. Inspect `data/dev.cominotti.lushtext.metainfo.xml.in`, the matching desktop file, `data/meson.build`, installed icons, and current release automation.
2. Preserve identity agreement among component ID, desktop ID, icon name, executable, and exported Flatpak ID.
3. Edit the current file; never replace it with a template or synthesize release history.
4. Validate through the current Meson/Make targets. Use `appstreamcli` or `desktop-file-validate` directly only to diagnose a focused failure.

## Correctness checks

- Keep user-visible descriptions factual and synchronized with implemented behavior.
- Keep release versions, dates, and notes derived from the actual release workflow.
- Verify screenshot URLs resolve to maintained, representative images; never add placeholder paths.
- Preserve required licensing, developer, launchable, content-rating, URL, and release metadata according to current AppStream/Flathub validation.
- Review generated/translated output when changing `.in` sources or Meson merge rules.
- Avoid hard-coded claims about ideal dimensions or mandatory optional elements unless current upstream documentation confirms them.

For a release, the repository's release helper owns metainfo insertion and validation. Do not hand-edit a guessed release entry in parallel with that workflow.
