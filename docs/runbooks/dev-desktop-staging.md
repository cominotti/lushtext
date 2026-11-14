# Development desktop staging

What `make run` (`scripts/run-dev-app.sh`) does to the GNOME desktop, how
icon and MIME changes reach it, and how to clean up after a live run. The
rules (non-production staging ID, stale code never activated, MIME allowlist,
`clear-lushtext-xdg` never run speculatively) are in
[`.agents/rules/build.md`](../../.agents/rules/build.md), "Dev Builds".

## What the launcher does

On GNOME Shell, `make run` asks any already-running `dev.cominotti.lushtext`
owner to quit before staging a development-only desktop file plus matching
development `hicolor` icons and launching the freshly built debug binary. If
the existing owner refuses to close, the launcher fails instead of activating
stale code. It recognizes stale debug processes whose `/proc/$pid/exe` target
ends with ` (deleted)` after a rebuild, and falls back to the session bus owner
check when `gapplication list-apps` misses an already-owned
`dev.cominotti.lushtext` name.

The staged desktop entry uses a content-addressed absolute icon file path.
This avoids Shell holding onto a stale themed-icon cache entry when the app
icon bytes change between dev runs while keeping the icon file alive for as
long as a restored user-local desktop entry might reference it.

The launcher quarantines any stale same-ID non-Flatpak
`~/.local/share/applications/dev.cominotti.lushtext.desktop` shadow before
staging so the installed Flatpak export stays visible. Because the staged
desktop file also carries `MimeType` associations, the launcher refreshes the
applications desktop database after staging or restoring it so GNOME Settings
and `gio mime` see current handler metadata.

## Icons

Changing `data/icons/dev.cominotti.lushtext.svg` alone is not enough to refresh
every icon surface. The fixed-size PNG fallbacks must be regenerated, and an
already-running dev session still needs a fresh Shell app object. Run
`make refresh-dock-icon` after icon asset changes: it regenerates the PNG
fallbacks from the canonical SVG and then replaces the running dev instance so
the dock rebuilds from the fresh file-backed icon.

## File Types and MIME

For GNOME Settings File Types work, the desktop entry's explicit `MimeType=`
line is the allowlist source of truth. `gio mime <type>` can still list
LushText for source-like MIME types that inherit from `text/plain`, even when
LushText does not explicitly advertise those types.

## Installed-state verification

After Flatpak or desktop-entry work, run `make flatpak-install` immediately
before `make verify-flatpak-identity`; together they confirm the exported
desktop entry has `X-Flatpak=dev.cominotti.lushtext`, no same-ID dev shadow
exists, effective Flatpak permissions are reported, and required MIME handlers
remain registered.

## Cleaning up

`make clear-lushtext-xdg` removes stale development launchers and development
icon staging along with the app's XDG data when a live desktop run leaves
leftovers behind. It lists the paths it will delete, refuses to delete anything
when stdin is not a TTY unless `ASSUME_YES=1` (script flag `--yes`) is given,
and `--help` only prints usage. `DRY_RUN=1` previews.
