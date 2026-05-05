## Context

GNOME Settings has two separate concerns on an app details page:

- "Files & Links" comes from GLib `GAppInfo` supported content types and MIME cache data.
- The sandbox warning comes from whether Settings can resolve the app as a sandboxed package, primarily through Flatpak or Snap metadata.

LushText's packaged Flatpak export currently includes `X-Flatpak=dev.cominotti.lushtext`, but local development staging writes `~/.local/share/applications/dev.cominotti.lushtext.desktop` with the production desktop ID, a direct debug binary `Exec`, and no `X-Flatpak`. Because user-local desktop entries can outrank Flatpak exports in GLib app-info lookup, GNOME Settings can inspect the development entry and conclude that LushText is not sandboxed even though the Flatpak package is installed.

The current Flatpak manifest grants `--filesystem=home`. GNOME Text Editor's current Flatpak grants broader host/GVfs permissions, but copying that manifest would worsen LushText's permission posture. Flatpak's current guidance favors portals and the narrowest static filesystem permissions that still preserve expected behavior, so this change separates identity correctness from any future portal-first workspace redesign.

## Goals / Non-Goals

**Goals:**

- Make the installed Flatpak the app-info source GNOME Settings sees for `dev.cominotti.lushtext.desktop`.
- Ensure development desktop-entry staging cannot persistently shadow the production Flatpak export under the same desktop ID.
- Keep Plain Text, Markdown, and Empty document handler rows visible after the identity fix.
- Review LushText's static Flatpak permissions against its current workspace, search, watcher, note, and local-history workflows.
- Document the resulting permission rationale and the longer-term portal-first path.
- Add deterministic local verification so this does not depend on manually noticing a Settings banner.

**Non-Goals:**

- Do not implement a full portal-first workspace storage model in this change.
- Do not remove workspace roots, recursive search, file watches, sidecar notes, local history, or file operations.
- Do not copy GNOME Text Editor's broader `--filesystem=host` permission unless verification proves LushText needs it.
- Do not change user MIME defaults or write directly to `mimeapps.list`.
- Do not change document loading, saving, draft recovery, or editor buffer behavior.

## Decisions

### Decision 1: Treat desktop identity as the primary bug

The immediate GNOME Settings warning comes from desktop-entry resolution, not from `--filesystem=home` alone. The implementation should therefore first ensure that the active production desktop entry is the Flatpak-exported file with `X-Flatpak=dev.cominotti.lushtext`.

Alternatives considered:

- Remove `--filesystem=home` first. Rejected because Settings still shows an app as not sandboxed when it resolves a non-Flatpak desktop entry.
- Add `X-Flatpak` to the development desktop entry. Rejected because it would misrepresent a host debug binary as a Flatpak app and could hide real launch-path bugs.

### Decision 2: Development runs need a non-shadowing desktop surface

The development workflow still needs GNOME Shell association for dock icons, activation, and MIME testing. It must not leave a production-ID desktop entry in `~/.local/share/applications` after a dev run, and any intentionally persistent development entry must use a distinct development identity or another strategy that does not override the installed Flatpak app info.

Alternatives considered:

- Stop staging desktop files for development. Rejected because this would regress GNOME Shell app matching and local MIME-cache verification.
- Keep the production desktop ID permanently for dev runs. Rejected because it is the shadowing mechanism that caused the Settings warning.

### Decision 3: Verify app-info precedence, not only file contents

Inspecting `data/dev.cominotti.lushtext.desktop.in` is insufficient. Verification must check the actual desktop entry GLib and Settings will see: the user-local applications directory, the Flatpak export path, and the active `GAppInfo` metadata for the production desktop ID.

Useful checks include:

- No persistent same-ID non-Flatpak desktop entry exists in `~/.local/share/applications` after a normal dev run exits.
- The Flatpak export contains `X-Flatpak=dev.cominotti.lushtext`.
- The active production desktop entry used for `gtk-launch dev.cominotti.lushtext` is the Flatpak export when the Flatpak is installed.
- MIME checks still list `dev.cominotti.lushtext.desktop` as registered and recommended for the required types.

Alternatives considered:

- Rely on manual GNOME Settings inspection only. Rejected because it is slow, environment-specific, and easy to miss in CI or headless validation.

### Decision 4: Keep permission changes conservative for this spec

The manifest should be reviewed and documented, but this change should not blindly switch to either `--filesystem=host` or a portal-only manifest. LushText's current workspace features operate on persisted paths and directories, including background scanning, watches, search/replace, sidecar persistence, and local history. Removing broad filesystem access without a corresponding data-model and UX change would break expected workspace behavior after restart.

Alternatives considered:

- Match GNOME Text Editor exactly. Rejected because GNOME Text Editor currently grants broader filesystem and GVfs permissions than LushText, and matching it would not align with the "least permission that preserves behavior" goal.
- Remove all static filesystem access immediately. Deferred because persisted workspace roots and recursive workspace operations need a portal-first design before this is safe.

## Risks / Trade-offs

- Same-ID desktop entries can reappear from older dev runs -> Mitigation: verification must detect and report the shadowing file path explicitly.
- A distinct development app ID can split dev settings from production settings -> Mitigation: keep the normal `make run` path focused on transient staging unless the implementation intentionally introduces a documented devel identity.
- Keeping broad filesystem access preserves current workspace behavior but remains less sandboxed than a portal-only app -> Mitigation: document the rationale now and capture portal-first workspace access as follow-up work.
- GNOME Settings internals can change -> Mitigation: verify with stable inputs (`X-Flatpak`, Flatpak metadata, GLib app info, and MIME registration) instead of depending on one UI string.
- User MIME defaults can affect `gio mime` output -> Mitigation: acceptance checks require LushText to be registered and recommended, not automatically default.

## Migration Plan

1. Update development desktop-entry staging so normal runs clean up production-ID entries and persistent dev entries cannot shadow the Flatpak export.
2. Add verification for Flatpak export identity, desktop-entry precedence, and MIME handler visibility.
3. Review and document the current Flatpak manifest permission set, including why any broad filesystem permission remains.
4. Refresh `docs/next/flatpak-packaging.md` so it reflects GNOME 50 packaging, portals, and the current permission decision.
5. Rebuild or refresh the local Flatpak/exported desktop metadata and run the verification checks.

Rollback is safe: restore the previous dev-run staging behavior and manifest documentation. The only user-visible risk would be the old GNOME Settings warning returning when a same-ID development desktop entry shadows the Flatpak export again.

## Open Questions

- Should a follow-up proposal design portal-backed persisted workspace grants so the Flatpak can eventually drop broad `home` access?
