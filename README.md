# LushText

A fast, minimalist text editor for GNOME built with Rust, GTK4, and Libadwaita. Similar in spirit to GNOME Text Editor, but with a persistent workspace sidebar, an optional properties sidebar, and workspace support.

## Features

- **Document properties surface** -- persistent left workspace tree plus an adaptive document-properties surface that appears as a right pane on spacious windows and a bottom sheet on compact ones, keeping document metadata, file-health details, and formatting-source explanation out of the bottom bar
- **Focus Mode** -- `Ctrl+Shift+F11` enters a reversible fullscreen writing shell with chrome suppressed, readable editor/Markdown columns, a subtle source text-origin guide, optional typewriter scrolling, and `Alt+P` preview-only support
- **Adaptive workspace sidebar width** -- choose `Small`, `Comfy`, or `Large` in `Preferences > Workspace`; each preset stays comfortable on large displays by clamping to a bounded desktop width
- **Tab content transparency** -- adjust `Transparency` in `Preferences > Editor > Appearance` to soften editor and Markdown preview backgrounds while keeping the header, side panels, status/search chrome, and minimap opaque
- **Workspaces** -- named ordered folder sets with a shared current workspace scope, persisted across sessions
- **Workspace auto-refresh** -- external file and folder changes refresh the sidebar's currently materialized top-level folder rows and expanded directories automatically, with access-noise filtering plus in-place reconciliation for both subtree and manual folder refreshes to avoid visible flashing, and a per-section `Refresh` button for deterministic broader reloads
- **File peek** -- press `Space` on a selected sidebar file to inspect a bounded read-only preview in a floating card with the absolute file path, then `Enter` or `Open` to promote it into a real tab
- **Focus Folder** -- focus a workspace section on a deep subfolder so the sidebar can drill into nested trees without wasting width on clipped ancestors
- **Syntax highlighting** -- via GtkSourceView for common file types (Rust, Python, JSON, TOML, YAML, Markdown, and more)
- **EditorConfig support** -- per-file formatting overrides from `.editorconfig` files (`indent_style`, `tab_width`, `indent_size`); toggle in Preferences
- **Bookmarks and rich notes** -- saved-file bookmark gutter marks with labels and F2 navigation, plus markdown-capable document notes, folder notes, and a unified notes browser
- **Local history** -- saved-file snapshot browser with automatic baseline, periodic, and save-boundary restore points, an adaptive Adwaita browse/preview UI, restore safety snapshots, and one-click undo of a restore
- **Minimap** -- toggleable right-edge document overview with semantic markers for bookmarks, active in-tab search matches, modified-since-save regions, and long-line warnings on supported files
- **Session persistence** -- tabs, pinned state, cursor positions, and scroll offsets restored on restart
- **Draft recovery** -- unsaved changes auto-saved to disk and recovered after crash
- **Crash-safe saves** -- atomic temp-file-then-rename writes with safe temp permissions, metadata applied before the final temp sync, the full Linux fsync durability contract (data + parent directory), stable target coordination across saves and Replace All, symlink-backed saves that update the resolved target, and an explicit warning when a change reaches disk but its durability cannot be confirmed
- **Print** -- native GTK print dialog with syntax highlighting and editor settings preserved
- **Workspace content search** -- Ctrl+Shift+F parallel grep across the current workspace scope (`All workspaces` or one selected workspace) with streaming results, regex/literal/whole-word modes, .gitignore toggle, glob file filter, F4/Shift+F4 match navigation, progress reporting, search history with full state recall, and named saved searches
- **Multi-file Replace All** -- preview proposed changes with per-match checkboxes, atomic file writes, stable save/replace coordination, file and undo-memory caps, per-file durable undo journals, skip-modified-tabs safety, and full undo support within the active safety window
- **Find and replace** -- per-tab search bar with match highlighting
- **Command palette** -- Ctrl+P fuzzy search for files and commands, scoped to the current workspace selection unless `All workspaces` is active (SIMD-accelerated via nucleo)
- **Automation spine** -- same-user agents and smoke tools can discover the action catalog, activate normal exported GTK actions, wait for app-owned workflows to become idle, inspect bounded D-Bus snapshots, and summarize smoke artifacts without exposing document contents or changing LushText's full-filesystem permission model
- **Large file handling** -- graceful degradation: >1MB toast, >10MB disable syntax, >50MB disable undo, >500MB refuse
- **Buffer eviction** -- background tabs evicted when total memory exceeds 256MB, transparently reloaded on focus
- **Dark mode** -- automatic GtkSourceView scheme switching via Libadwaita StyleManager
- **Customizable font** -- system monospace or custom font, applied via CSS provider
- **Markdown support** -- editable source headings are visually emphasized, plus side-by-side or full-width native preview rendering for headings, emphasis, code, links, ordered and unordered lists with nested hanging indents, task lists, nested blockquote rails, GitHub alert callouts, reference-style and inline footnotes, and Markdown tables; use Main Menu > Markdown Preview or Alt+P for full-width preview
- **File monitoring** -- detects external changes and offers reload

## Installation and Running

LushText is packaged as a GNOME Flatpak and can also be run directly from a
source checkout for development. Public Flatpak releases are prepared for the
official Cominotti remote, with Flathub handoff kept as an optional secondary
path. An Ubuntu Snap is in preparation (see [Snap (preparation)](#snap-preparation)).

### Flatpak from this checkout

```sh
make flatpak-install
flatpak run dev.cominotti.lushtext
```

The Flatpak uses `org.gnome.Platform` 50 and requires the matching GNOME SDK.
`make flatpak-install` idempotently adds the user Flathub remote when needed
and installs missing runtime, SDK, and SDK-extension dependencies before
building. Use `make flatpak` when you only want to build the Flatpak without
installing it.
If dependencies change, regenerate the vendored Cargo sources before building:

```sh
make cargo-sources
```

### Cominotti Flatpak release preparation

Release automation follows the same Makefile style as the development and
packaging commands. Preview the next computed version without changing files:

```sh
make release-bump TYPE=patch DRY_RUN=1
```

Create a real release only from a clean `main` checkout, with release notes that
will be inserted into AppStream metadata:

```sh
make release-bump TYPE=minor RELEASE_NOTES_FILE=release-notes.md
make release VERSION=v0.2.0 RELEASE_NOTES_FILE=release-notes.md
```

The release helper updates the Cargo package versions, Meson project version,
`Cargo.lock`, AppStream release history, and Flatpak vendored Cargo sources,
then validates metadata and the Flatpak build before creating the release commit
and signed tag. `PRERELEASE=alpha|beta|rc` starts or continues a prerelease
stream, and `PROMOTE=1` is required before promoting a prerelease stream to a
stable tag.

The repository's local Flatpak manifest stays at
`build-aux/dev.cominotti.lushtext.Flatpak.json` and uses the current checkout.
For the Cominotti remote, generate signed repository artifacts from a tag and
commit:

```sh
make cominotti-flatpak-repo VERSION=v0.2.0 \
  COMINOTTI_FLATPAK_PUBLIC_KEY=public.gpg \
  COMINOTTI_FLATPAK_GPG_KEY=<key-id>
make verify-cominotti-flatpak-repo
```

The public URL layout is:

```text
https://flatpak.cominotti.dev/repo/
https://flatpak.cominotti.dev/cominotti.flatpakrepo
https://flatpak.cominotti.dev/lushtext.flatpakref
```

The default hosted backend is Cloudflare Pages because static asset requests are
free and unlimited when they do not invoke Pages Functions. Run
`make verify-cominotti-pages-limits` before publishing; it enforces the 25 MiB
per-asset limit and the configured file-count limit, then points maintainers to
Cloudflare R2 if the repository outgrows Pages.

Setup details live in
[`docs/next/cominotti-flatpak-hosting.md`](docs/next/cominotti-flatpak-hosting.md).

Once published, users can install the first app from the shared Cominotti
remote with:

```sh
flatpak install --user https://flatpak.cominotti.dev/lushtext.flatpakref
```

Do not publish user-facing `--no-gpg-verify` instructions; the `.flatpakrepo`
and `.flatpakref` include the Cominotti public GPG key.

For optional Flathub handoff, generate a tag-based manifest update artifact:

```sh
make flathub-manifest VERSION=v0.2.0
make verify-flathub-manifest
```

Flathub publication remains a reviewable pull request when configured, but it
is secondary to the Cominotti remote. The release workflow can open or update
that PR when `FLATHUB_TOKEN` and `FLATHUB_REPOSITORY` are configured, but it
does not enable Flathub automerge.

Flathub verification for `dev.cominotti.lushtext` is domain-based. A linked
GitHub account does not verify this custom-domain app ID. After Flathub provides
the app's verification token, publish it at:

```text
https://cominotti.dev/.well-known/org.flathub.VerifiedApps.txt
```

Then verify the endpoint locally:

```sh
make verify-flathub-domain FLATHUB_VERIFICATION_TOKEN=<token>
```

### Snap (preparation)

An Ubuntu Snap (`snap/snapcraft.yaml`) is scaffolded but **not yet buildable**.
LushText targets the GNOME 50 platform (GTK 4.22, Libadwaita 1.9), and the Snap
`gnome` extension currently provides only GTK 4.14 (`gnome-46-2404`, base
`core24`). A real build is gated on the `core26` / GNOME 50 platform snap
(`gnome-50-2604` or equivalent), which is not published yet — the `core26` base
itself already is. The Snap reuses the existing Meson → Cargo build via the
`meson` plugin, so no Rust changes are needed; a snap `layout:` bind-mounts the
baked `LUSHTEXT_PKGDATADIR` path into confinement.

Once the platform snap lands and `snap/snapcraft.yaml` is switched to `core26`:

```sh
make snap          # build (LXD backend)
make snap-smoke    # confined smoke test (skips cleanly until then)
```

Use the readiness helper to check the current external gates without mutating
Snap Store state. When `gh` is authenticated, it also checks the
`SNAP_PLATFORM_AVAILABLE` repository variable and `SNAPCRAFT_STORE_CREDENTIALS`
secret. The target exits nonzero while any gate is still pending, so a `make`
error here means the Snap is not release-ready yet:

```sh
make snap-store-readiness
```

It will be released **Unlisted on the `edge` channel** — omitted from store
search and installable only with an explicit command:

```sh
snap install lushtext --edge
```

Store registration and CI publishing still require authenticated operator
steps:

```sh
snapcraft login
snapcraft register lushtext
# Set Visibility to Unlisted in the Snap Store dashboard settings.
snapcraft export-login snapcraft-credentials.txt
gh secret set SNAPCRAFT_STORE_CREDENTIALS < snapcraft-credentials.txt
# After the GNOME 50 platform snap is visible and snap/snapcraft.yaml is updated:
gh variable set SNAP_PLATFORM_AVAILABLE --body true
```

### Development run

Prepare a full local development environment, including Flatpak runtime/SDK
dependencies plus helper tools used for live GTK input and screenshot
automation:

```sh
make dev-tools
```

`make dev-tools` runs `make flatpak-deps` first, then idempotently installs the
GTK debug helpers and UI generation tooling: headless
Mutter/PipeWire/WirePlumber/GStreamer screenshot tooling, portal screenshot
tools, system Python AT-SPI bindings, ydotool, isolated Xvfb fallback tooling,
the D-Bus/GSettings utilities used by the debug skills, and
`blueprint-compiler` for template regeneration/drift checks.
On Fedora Toolbx it uses `sudo dnf install`. It does not layer packages onto a
Silverblue host by default; set `LUSHTEXT_DEV_TOOLS_ALLOW_RPM_OSTREE=1` only
when host rpm-ostree mutation is intentional. If `/dev/uinput` is writable, the
target also starts a user `ydotoold` socket under `$XDG_RUNTIME_DIR` for
automated keyboard input during live GTK debugging.

```sh
make run
```

`make run` builds the debug binary, asks any already-running LushText instance
to quit, and temporarily stages a GNOME desktop entry and app icon so the fresh
development copy appears correctly in GNOME Shell. If the existing app refuses
to close, the launcher fails instead of activating stale code.

### Mutation testing

LushText uses `cargo-mutants` for deterministic model, service, and pure helper
coverage. It complements, rather than replaces, the normal gates: `cargo nextest`
proves the non-widget baseline, the GTK widget runner keeps Mutter and warning
behavior covered, benchmark compilation protects Criterion coverage, all-feature
Clippy and rustfmt protect code quality, and `cargo deny check advisories bans
sources licenses` remains the dependency-policy gate.

Install the local mutation tools once:

```sh
cargo install --locked cargo-mutants --version 27.0.0
cargo install --locked cargo-nextest --version 0.9.137
```

Then use the wrapper targets:

```sh
make mutants-smoke # bounded tooling and timeout check
make mutants-diff  # changed-code mutation against origin/main
make mutants-full  # configured deterministic model/service/helper scope
```

Generated `mutants.out` directories are ignored locally and uploaded from CI
when present. See [`docs/mutation-testing.md`](docs/mutation-testing.md) for
triage rules, CI behavior, sharding, and equivalent-mutant exclusion policy.

### End-user smoke coverage

The default test suite is intentionally deterministic. Live desktop, portal,
accessibility, and performance checks are exposed as separate lanes so they can
record host details and skip clearly when a machine lacks the required runtime:

```sh
make visual-smoke          # headless Mutter screenshot smoke with artifacts
make automation-smoke      # real-process D-Bus automation smoke with artifacts
make crash-recovery-smoke  # SIGKILL/relaunch recovery smoke with artifacts
make portal-sandbox-smoke  # available Flatpak/Snap confinement diagnostics
make accessibility-smoke   # AT-SPI-enabled smoke outside the widget harness
make performance-smoke     # lightweight Criterion timing smoke
make automation-client-self-test # reusable D-Bus client/parser self-test
make end-user-smoke        # run all host-supported smoke lanes
```

See [`docs/end-user-coverage.md`](docs/end-user-coverage.md) for the coverage
map and the expected pull-request, scheduled, release, and local validation
boundaries. See [`docs/recovery-reliability.md`](docs/recovery-reliability.md)
for recovery metadata, quarantine, migration-ledger, and crash-smoke triage.

## First Run

1. Open a file with `Ctrl+O`, from the header-bar open button, or by launching
   `lushtext PATH`.
2. Add a workspace folder from the left sidebar to browse a project directory.
3. Use the workspace selector to choose `All workspaces` or one specific workspace.
4. Open the command palette with `Ctrl+Shift+P` to search files and commands.
5. Open the main menu and choose **Keyboard Shortcuts** for the complete
   shortcut reference shipped with the app.

LushText restores open tabs, pinned tabs, cursor positions, scroll positions,
workspaces, search state, and recoverable drafts on restart.

## Preferences

Preferences are stored with GSettings under `dev.cominotti.lushtext`.

### Editor

- **Color Scheme** selects the base GtkSourceView style scheme; dark variants
  are chosen automatically when GNOME is in dark mode.
- **Use System Monospace Font** and **Custom Font** control editor and sidebar
  monospace text.
- **Transparency** adjusts editor and Markdown preview document backgrounds
  without making window chrome or side panels transparent.
- **Focus Mode** preferences set the target column width and optional typewriter
  scrolling.
- **Use EditorConfig**, **Word Wrap**, **Tab Width**, **Insert Spaces Instead of
  Tabs**, **Show Line Numbers**, **Highlight Current Line**, **Show Minimap**,
  and **Show Bookmark Gutter** control editing behavior and editor decorations.

### Workspace

- **Sidebar Width** chooses the `Small`, `Comfy`, or `Large` workspace sidebar
  preset.
- **Auto-Collapse Workspaces** collapses other workspace sections when focusing
  a folder.
- **Empty Folder Lookahead Cap** controls how many subdirectories LushText peeks
  into when deciding whether a folder should be marked `(Empty)`.

Advanced users can inspect or reset settings with:

```sh
gsettings list-recursively dev.cominotti.lushtext
gsettings reset-recursively dev.cominotti.lushtext
```

For Flatpak installs, run those commands inside the sandbox:

```sh
flatpak run --command=gsettings dev.cominotti.lushtext list-recursively dev.cominotti.lushtext
```

## Data, Privacy, and Reset

LushText keeps application state under `$XDG_DATA_HOME/lushtext` for source and
host installs. On typical systems this is `~/.local/share/lushtext`. Flatpak
installs keep the same app data inside the sandbox, normally under
`~/.var/app/dev.cominotti.lushtext/data/lushtext`.

Stored state can include document text:

| Path | Contains |
|------|----------|
| `session.json` | Open tabs, pinned state, cursor positions, and scroll offsets |
| `workspaces.json` | Saved workspace names and ordered folder sets |
| `drafts/` | Plain-text autosaved drafts for unsaved changes |
| `bookmarks/` | Saved-file bookmark metadata |
| `document-notes/` | Per-file document notes |
| `folder-notes/` | Per-folder note sidecars |
| `workspace-notes/` | Legacy-compatible folder-note sidecars from older releases |
| `local-history/` | Local-history snapshots for saved files |
| `migration-ledger.json` | Retryable sidecar and local-history migration work after in-app renames |
| `recovery-quarantine/` | Preserved malformed or unsupported app-owned recovery metadata |
| `search-history.json` | Recent workspace search queries and options |
| `saved-searches.json` | Named saved searches |
| `replace-backup-journal/` | Temporary per-file undo journal for multi-file Replace All |
| `replace-backup.json` | Legacy temporary Replace All undo file, cleared with stale journal state |

To fully reset LushText state, close the app and remove that app-data directory.
For Flatpak installs, also reset the sandboxed GSettings if you want preferences
back at defaults:

```sh
flatpak run --command=gsettings dev.cominotti.lushtext reset-recursively dev.cominotti.lushtext
```

## Flatpak Permissions

The Flatpak manifest grants host filesystem access because LushText is a local
workspace text editor that must open, save, search, rename, delete, and
event-monitor user-selected files and workspace folders across local paths, not
only under the home directory. It does not request network access.
Portal/sandbox diagnostics are intentionally observability-only for this
release line. `make check-flatpak-permissions` validates the source manifest,
and `make portal-sandbox-smoke` records `permission-posture.txt` plus runtime
permission artifacts without migrating LushText to portals-only access.

| Permission | Why it is used |
|------------|----------------|
| `--filesystem=host` | Open, save, search, rename, delete, and event-monitor user-selected local files and workspace folders across host paths |
| `--socket=wayland` | Native Wayland display support |
| `--socket=fallback-x11` and `--share=ipc` | X11 fallback support |
| `--device=dri` | GTK hardware-accelerated rendering |

The planned Snap uses **strict confinement plus xdg portals** instead — a
narrower posture than the Flatpak's `--filesystem=host`. It declares only the
`home` and `removable-media` interfaces (the `gnome` extension supplies Wayland,
X11 fallback, GPU, and portals). Workspace folders and files outside those
locations are reached through portals; the app surfaces an access error rather
than crashing or losing data when a path is out of scope.

## Common Shortcuts

The full shortcut list is available in **Main Menu > Keyboard Shortcuts**.

| Workflow | Shortcut |
|----------|----------|
| New file | `Ctrl+N` |
| Open file | `Ctrl+O` |
| Save / Save As | `Ctrl+S` / `Ctrl+Shift+S` |
| Close tab | `Ctrl+W` |
| Print | `Ctrl+P` |
| Find / Find and Replace | `Ctrl+F` / `Ctrl+H` |
| Next / previous find match | `Ctrl+G` / `Ctrl+Shift+G` |
| Command palette | `Ctrl+Shift+P` |
| Workspace search | `Ctrl+Shift+F` |
| Workspace search next / previous match | `F4` / `Shift+F4` |
| Toggle minimap | `Ctrl+Shift+M` |
| Cycle invisible characters | `Ctrl+Shift+I` |
| Document properties | `F9` |
| Fullscreen | `F11` |
| Focus Mode | `Ctrl+Shift+F11` |
| Markdown preview-only mode | `Alt+P` |
| Zoom in / out / reset | `Ctrl++` / `Ctrl+-` / `Ctrl+0` |

## Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust (Edition 2024, MSRV 1.96.0) |
| GUI | GTK4 0.11 + Libadwaita 0.9 + GtkSourceView 5 0.11 |
| Config | GSettings |
| Build | Cargo workspace + Makefile (dev), Meson (Flatpak/installed) |
| Packaging | Flatpak (org.gnome.Platform 50) |
| License | GPL-3.0-or-later |

## Building from Source

### Dependencies

- Rust 1.96.0+
- GTK4 development libraries
- Libadwaita development libraries
- GtkSourceView 5 development libraries
- GLib development tools (`glib-compile-schemas`)

On Fedora:

```sh
sudo dnf install gtk4-devel libadwaita-devel gtksourceview5-devel glib2-devel
```

On Ubuntu/Debian:

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev libglib2.0-dev
```

### Dev Builds

```sh
make build       # Release build
make build-debug # Debug build
make run         # Debug build + force a fresh run with temporary GNOME desktop staging
make refresh-dock-icon # Regenerate app icon assets + force a fresh GNOME Shell dock icon reload
make test        # All tests (unit + integration + widget)
make check       # fmt + all-feature Clippy + fast policy audits
make blueprint-generate # Regenerate generated .ui files from Blueprint sources
make check-blueprint    # Validate Blueprint drift and UI template contract
make check-automation-docs # Validate automation docs against exported action/D-Bus contracts
make automation-client-self-test # Validate reusable D-Bus automation CLI helper
make check-flatpak-permissions # Verify the Flatpak keeps intentional full filesystem access
make lint-blueprint     # Advisory grouped Blueprint lint triage
make lint-advisory # grouped advisory Rust lint discovery
make pre-commit  # repo pre-commit gate (fmt + all-feature Clippy + policy audits)
make install-git-hooks
```

LushText ships repo-managed Git hooks in `.githooks/`. Run `make install-git-hooks` once per checkout to configure `core.hooksPath`; after that, each commit runs the same rustfmt, all-targets/all-features Clippy, and fast policy-audit gate locally before Git creates the commit.

### UI Templates

UI templates are authored in Blueprint (`resources/ui/*.blp`). The generated
GtkBuilder XML files (`resources/ui/*.ui`) stay committed and remain the runtime
GResource inputs for direct Cargo, Meson, Flatpak, and Snap builds. Do not
hand-edit generated `.ui` files; edit the matching `.blp`, then run:

```sh
make blueprint-generate
make check-blueprint
```

Fedora/Toolbx and CI use the Fedora `blueprint-compiler` package (`sudo dnf
install blueprint-compiler`, currently 0.20.x). You can point at another
executable with `BLUEPRINT_COMPILER=/path/to/blueprint-compiler`. Missing
Blueprint tooling only blocks regeneration and drift checks; ordinary runtime
builds still consume the committed `.ui` resources.

`make check-blueprint` treats unknown compiler warnings as blocking. The only
accepted warnings are the documented GTK shortcuts deprecations in
`resources/ui/shortcuts.blp`. Run `make lint-blueprint` for curated advisory
lint triage; it groups diagnostics by rule and file, keeps promoted diagnostics
clean, and fails when accepted advisory findings exceed the documented policy.
See `docs/blueprint-validation.md` for the lint policy and the reusable visual
comparison workflow:

```sh
./scripts/compare-blueprint-visuals.sh --baseline-ref origin/main
```

The blocking Rust lint command is `cargo clippy --workspace --all-targets --all-features -- -D warnings`. Broad Clippy groups stay advisory instead of blanket-blocking; run `make lint-advisory` after Rust or Clippy updates to get grouped Clippy/rustc findings and fail on any new unclassified category. CI pins validation helpers in workflow env variables, including cargo-deny `0.19.8`, cargo-nextest `0.9.137`, cargo-fuzz `0.13.1`, and cargo-mutants `27.0.0`.

The Makefile auto-detects [cargo-nextest](https://nexte.st/) for parallel non-widget execution (optional), but it always runs widget tests explicitly through the shared `scripts/run-widget-tests.sh` runner so `make test` still means the full suite. Rust 1.90+ uses [rust-lld](https://blog.rust-lang.org/2025/09/01/rust-lld-on-1.90.0-stable/) as the default linker on Linux for fast linking.

On GNOME Shell, `make run` asks any already-running `dev.cominotti.lushtext`
owner to quit before it temporarily stages a user-local desktop entry plus
`hicolor` app icons and launches the freshly built debug binary. If the existing
owner refuses to close, the launcher fails instead of activating stale code. The
staged desktop entry points at a content-addressed absolute icon file so Shell
reloads icon changes reliably during development instead of reusing a stale
themed-icon cache entry. The launcher also repairs any stale user-local LushText
desktop entry whose absolute `Icon=` path no longer exists. If you changed the
app icon artwork, use `make refresh-dock-icon`: it regenerates the shipped PNG
fallbacks from `data/icons/dev.cominotti.lushtext.svg`, then restarts the
current dev instance against a fresh file-backed icon so the dock updates
immediately.

### Flatpak

```sh
make flatpak-deps    # Install Flatpak runtime/SDK deps into the user installation
make flatpak         # Build Flatpak (sets up missing runtime/SDK deps)
make flatpak-install # Build and install Flatpak into the user installation
make cargo-sources   # Regenerate cargo-sources.json after dependency changes
make cominotti-flatpak-repo VERSION=v0.2.0 # Generate Cominotti Flatpak repo artifacts
make verify-cominotti-flatpak-repo         # Check Cominotti Flatpak repo metadata
make test-cominotti-flatpak-repo           # Test Cominotti Flatpak repo tooling
make flathub-manifest VERSION=v0.2.0 # Generate Flathub tag-based manifest
make verify-flathub-manifest         # Check generated Flathub manifest invariants
make verify-flathub-domain           # Check cominotti.dev verification endpoint
make release-bump TYPE=patch DRY_RUN=1 # Preview the next release tag
```

## EditorConfig

LushText reads `.editorconfig` files from the directory tree and applies per-file formatting overrides. This is the same [EditorConfig](https://editorconfig.org/) standard supported by most editors.

### Supported properties

| Property | Maps to |
|----------|---------|
| `indent_style` | `insert-spaces-instead-of-tabs` |
| `tab_width` | `tab-width` (clamped 1-12) |
| `indent_size` | `indent-width` (clamped 1-12) |

### How it works

1. When a file is opened (or saved-as to a new path), the service walks from the file's parent directory upward, collecting `.editorconfig` files
2. Closer files take priority over farther ones
3. `root = true` stops the directory walk
4. Overrides are applied on the main thread; GSettings values are used as fallback for any property not specified in `.editorconfig`

The feature can be toggled in **Preferences > Use EditorConfig** (enabled by default). The status bar shows an "EditorConfig" indicator when overrides are active for the current tab.

### Deferred properties

`end_of_line`, `charset`, `trim_trailing_whitespace`, `insert_final_newline`, and `max_line_length` are not yet supported. See `docs/next/editorconfig-future.md` for details and implementation priorities.

## Bookmarks and Notes

LushText includes non-destructive notes for saved files and explicit workspace folders:

- **Bookmarks** live in the GtkSourceView gutter, can carry an optional label, and support next/previous navigation with `F2` / `Shift+F2`.
- **Document notes** store one markdown-capable note for a saved file as a whole.
- **Folder notes** store one markdown-capable note for each workspace folder.
- **Browse bookmarks** operates on the currently selected workspace scope, while **Browse notes** keeps workspace results scoped and adds an `Open Tabs` section for saved open files outside that scope.

### Shortcuts

| Workflow | Shortcut |
|----------|----------|
| Toggle bookmark | `Ctrl+F2` |
| Edit bookmark | `Ctrl+Shift+F2` |
| Next / previous bookmark | `F2` / `Shift+F2` |
| Browse bookmarks | `Ctrl+Alt+B` |
| Browse notes | `Ctrl+Alt+A` |

### Manual test checklist

Use this checklist to exercise the full shipped bookmark and rich-note flow:

1. Start the app with `make run`.
2. Add a workspace folder and open a saved text file from the sidebar.
3. Press `Ctrl+F2` on the current line.
   Expected: a bookmark appears in the gutter and the file content does not change.
4. Press `Ctrl+Shift+F2` on that bookmarked line, add a label, and change the line.
   Expected: the label saves, the gutter mark moves to the new line, and later bookmark browse surfaces show the updated label.
5. Add a second bookmark on another line, then use `F2` and `Shift+F2`.
   Expected: the cursor jumps forward and backward through bookmarks in the active file.
6. Press `Ctrl+Alt+B`.
   Expected: the bookmark browser opens for the current workspace scope, supports search, and clicking a row opens or focuses the bookmarked file and jumps to its line.
7. Press `Ctrl+Alt+A`.
   Expected: the unified notes browser opens for the current workspace scope, previews bookmarks, document notes, and folder notes, shows saved out-of-scope open-tab notes in `Open Tabs`, and clicking Open on a row routes to the right surface.
8. Open **Document Note…** for the active saved file.
    Expected: the file-level note opens, supports Edit/Render switching, and Save persists it without changing the file bytes.
9. Select one concrete workspace and open **Folder Note…**.
    Expected: the folder-level note opens for that workspace's concrete folder target; in `All workspaces`, the direct folder-note action stays disabled and the unified browser remains available.
10. Toggle **Preferences > Show Bookmark Gutter**.
    Expected: bookmark gutter indicators hide and reappear without losing stored bookmarks.
11. Close and reopen the file, then restart the app and open it again.
    Expected: bookmarks and document notes restore automatically; folder notes return when the same workspace folder is restored.
12. Rename the file from the LushText sidebar.
    Expected: reopening the renamed file keeps the same bookmarks and document note.
13. Use **Save As** to write the file to a new path.
    Expected: the new file opens without copied document notes, while the original file keeps its existing notes.
14. Try steps 3 and 8 in an untitled tab.
    Expected: LushText refuses to create bookmarks or document notes and shows clear feedback that a saved file is required.

### Persistence rules

- Bookmarks and document notes require a **saved file path**. Untitled tabs show feedback instead of creating note state.
- Folder notes require a **concrete workspace folder**. `All workspaces` keeps the browser available, but the direct folder-note action stays disabled until one workspace is selected.
- Sidecars live under `$XDG_DATA_HOME/lushtext/bookmarks/`, `$XDG_DATA_HOME/lushtext/document-notes/`, and `$XDG_DATA_HOME/lushtext/folder-notes/`; older `$XDG_DATA_HOME/lushtext/workspace-notes/` folder-note sidecars remain legacy-compatible.
- **Save As** creates a new file-backed note identity and does not copy the old file's bookmarks or document notes by default.
- **Sidebar rename inside LushText** migrates file-backed and folder-note sidecars to the new path automatically.

### First-release limitations

- Path-based identity does not automatically follow **external** filesystem moves or copies performed outside LushText.

## Local History

LushText includes a focused local-history MVP for saved documents.

- Open **Local History** from the main menu, the command palette, `Ctrl+Alt+L`, the sidebar file context menu, or the editor content context menu while a saved file is active.
- The browser opens in an adaptive Libadwaita dialog with newest-first snapshots and a read-only preview.
- On wide windows, the dialog expands into a large viewer-first surface that uses most of the parent window while staying parent-bounded, with an Adwaita snapshot rail beside the preview.
- Empty historical snapshots are explained explicitly in the browser so an empty file state does not look like a broken preview.
- Legacy stale-disk empty baseline rows from older history can be hidden from the browser while the stored history on disk remains unchanged.
- Restoring a snapshot writes it into the editor buffer, marks the document modified, and immediately offers **Undo Restore** without writing to disk.
- **Save As** starts a fresh history lineage for the new path, while sidebar renames inside LushText migrate the existing lineage to the renamed path.

### Shortcut

| Workflow | Shortcut |
|----------|----------|
| Local History | `Ctrl+Alt+L` |

### Capture policy

- A baseline snapshot is recorded when a clean saved document first becomes modified.
- If a file-backed draft is restored at open time, local history treats that restored working content as the baseline instead of adding a fresh row for stale on-disk file contents.
- Additional snapshots are captured no more than once every five minutes while the document stays modified.
- Every successful save records a save-boundary snapshot.
- Consecutive duplicate snapshot bodies are skipped so the browser stays readable.

### Large-file limits

- Up to `10 MB`: full capture cadence and browsing are available.
- Above `10 MB` and up to `50 MB`: local history captures only on save boundaries.
- Above `50 MB`: local history is unavailable.

## Preview and Sidebar Helpers

### Markdown preview

LushText can render Markdown files in a read-only preview pane instead of just
showing the raw source text.

- Markdown heading lines stay editable as source text but use a larger bold
  source style so document structure is visible while writing.
- **Main Menu > Markdown Preview** or `Alt+P` toggles **preview-only mode**,
  where the editor hides and the rendered Markdown takes the full content area.
- A separate side-by-side preview pane is also available through the existing
  preview action surfaces, giving you editor text on the left and rendered
  output on the right.
- The renderer uses native GTK styling and widgets for headings, emphasis,
  inline code, syntax-highlighted code blocks, activatable links, ordered and
  unordered lists, task lists, nested hanging list indentation, nested blockquote
  rails, GitHub alert callouts, reference-style and inline footnotes, Markdown
  tables, and local Markdown images with explicit fallback states for unsupported
  or unresolved image targets.
- Non-Markdown files show a placeholder instead of trying to render arbitrary
  text as Markdown.
- Canonical preview sample content lives under `samples/`. The file
  `samples/markdown-test.md` is the canonical showcase for the Markdown preview
  features LushText currently supports.

### Focus Folder

When deep directory nesting makes a folder hard to browse comfortably in the
workspace tree, the sidebar provides a **Focus Folder** action.

- Open the context menu on a directory in the sidebar and choose **Focus Folder**.
- The selected directory becomes the temporary focus for that workspace section,
  so the tree can drill into that area without wasting width on all of its
  ancestors.
- If **Auto-Collapse Workspaces** is enabled, focusing a folder can collapse
  other workspace sections to keep attention on the active subtree.
- Folders detected as effectively empty are marked `(Empty)` and do not offer
  the Focus Folder action.

### File peek

The sidebar includes a lightweight file peek flow for checking a file before
opening a real editor tab.

- Select a sidebar file row and press `Space` to open a bounded read-only
  preview popover.
- The preview shows the file name, absolute path, size, modified timestamp, and
  a short text sample or an explicit unsupported/error state.
- Pressing `Space` again on the same file, pressing `Escape`, clicking away, or
  moving selection to a non-file row closes the preview.
- Press `Enter` or use the **Open** button in the popover to promote the file
  through the normal open-tab flow.
- The preview is intentionally lightweight and does not create editor, draft,
  monitor, or undo state.

## Architecture

Cargo workspace:

- **`crates/lushtext-build-support`** -- build-script helper crate for the build-only filesystem boundary
- **`crates/lushtext-core`** -- all application logic: domain models, services, GTK widgets
- **`crates/lushtext`** -- thin binary entry point + integration tests
- **`crates/gtk-lush/`** -- governed `0.0.0` GTK Lush family crates for extracting reusable GTK4/Libadwaita patterns; functional in-tree APIs are not Phase 5 publication-ready
- **`crates/cargo-gtk-proof`** -- workspace visual proof tool outside the GTK Lush family
- **`workspace-hack`** -- generated cargo-hakari crate for unified dependency features

### Module layout

```
lushtext-core/src/
  app.rs             Application entry (AdwApplication subclass)
  config.rs          Compile-time constants
  lib.rs             Resource registration, CSS loading, and app bootstrap
  model/             Domain types (no GTK deps)
    action_catalog.rs  Automation action catalog value objects
    automation.rs    Bounded read-only automation snapshot value objects
    workspace.rs     Workspace persistence model
    session.rs       Tab session model
    palette.rs       Command palette types
    draft.rs         Draft persistence metadata
    note.rs          Shared note-body primitives
    bookmark.rs      Bookmark sidecar model
    document_note.rs Saved-file document-note model
    local_history.rs Local-history snapshot metadata
    migration_ledger.rs Retry state for post-rename sidecar/history migrations
    content_search.rs  Content search types (SearchMatch, SearchEvent, etc.)
    encoding.rs      Document encoding, line endings, file health, and invisible-character modes
    sidecar_identity.rs  Canonical-path sidecar identity helpers for notes and history
    folder_note.rs    Folder-note model
    formatting_overrides.rs   Per-file EditorConfig overrides
  services/          Business logic (GTK-free where possible)
    action_catalog/  Action catalog construction, audits, and developer-reference rows
    bookmark_service.rs  Bookmark sidecar load/save/move/list helpers
    bookmark_excerpt.rs  Bounded source excerpts for bookmark previews
    document_note_service.rs  Saved-file document-note load/save/move/list helpers
    local_history_service.rs  Local-history capture/list/load/prune/move helpers
    note_storage.rs  Shared sidecar identity/load/filter helpers for note workflows
    content_search/  Parallel workspace grep plus replace/undo helpers
    palette/         Command registry, SIMD fuzzy search, and file indexing
    durable_write.rs Private crash-durable write state machine over the filesystem backend
    editor_io.rs     Encoding-aware text file load/save helpers, health analysis, and mtimes
    editorconfig.rs  .editorconfig resolution
    file_peek.rs     Bounded read-only snapshots for sidebar file peek
    notifications.rs Window-scoped status and inline notification store
    file_tree.rs     Directory scanning
    draft_service.rs Draft autosave
    migration_ledger.rs   Durable retry ledger for sidecar/history migrations
    recovery_metadata.rs  Recovery-aware app-data metadata quarantine and diagnostics
    search_backup.rs Replace All per-file undo journal persistence for the active safety window
    search_history.rs  Search history persistence
    saved_searches.rs  Named saved search persistence
    session_service.rs  Session load/save
    folder_note_service.rs  Folder-note load/save/move/list helpers
    workspace_manager.rs  Workspace CRUD
    workspace_watch.rs  Materialized-scope filesystem watch service for sidebar auto-refresh
  ui/                GTK4/Libadwaita widgets
    automation.rs    App-owned read-only D-Bus automation adapter and snapshot collection
    window/          Main window shell plus actions, documents, drafts, encoding, Focus Mode, local-history, notes, search, preview, session persistence, tab management, transient-surface dismissal, print, and zoom wiring
    editor_page/     GtkSourceView tab plus Focus Mode presentation, local-history capture, minimap, overscroll, invisible-character rendering, bookmark projection, load/save, monitor, and in-tab search helpers
    sidebar/         Multi-workspace file tree, dialogs, callbacks, per-section async child-tree loading, and file peek
    properties_panel/ Right-side metadata + formatting controls
    search_panel/    Ctrl+Shift+F workspace content search plus history, list factory, replace, results, and runtime flows
    command_palette/ Ctrl+P fuzzy search
    search_bar/      Find/replace
    status_bar/      Bottom bar
    info_bar/        Contextual warnings
    preferences/     Settings dialog
```

Automation surfaces are documented in [`docs/automation.md`](docs/automation.md)
and [`docs/automation-reference.md`](docs/automation-reference.md). The reusable
developer/agent client is `scripts/lushtext-automation.py`; it wraps
Automation1 reads, readiness waits, catalog-checked `org.gtk.Actions`
activation, and smoke artifact summaries. Any change to exported actions, the
read-only D-Bus interface, snapshot JSON, readiness blockers, client
commands/statuses, or scenario-helper flags must update those docs and pass
`make check-automation-docs` plus `make automation-client-self-test`.
Automation and portal/sandbox work must also keep the Flatpak's intentional
full-filesystem posture documented and guarded by `make check-flatpak-permissions`.

### GTK Lush family

`crates/gtk-lush/` is the in-tree staging area for extracting LushText's
hardened GTK4/Libadwaita patterns into small Rust crates. The current family
members are functional in-tree `0.0.0` APIs for LushText and future adoption
testing: `gtk-lush-signals`, `gtk-lush-settle`, `gtk-lush-tasks`,
`gtk-lush-viewport`, `gtk-lush-widgets`, `gtk-lush-proof-harness`, and
`gtk-lush-proof-spine`. The separate `cargo-gtk-proof` workspace tool lives
outside the family so the leaf crates remain independently adoptable.
Governance lives in [`crates/gtk-lush/GOVERNANCE.md`](crates/gtk-lush/GOVERNANCE.md),
with the umbrella vision in [`docs/next/gtk-lush.md`](docs/next/gtk-lush.md).
The proof-tool schema, artifact, and privacy contract is documented in
[`docs/gtk-proof-schemas.md`](docs/gtk-proof-schemas.md).

Use the family-specific checks when touching that area:

```sh
make check-gtk-lush-policy
make gtk-lush-doctests
make gtk-lush-examples
make gtk-lush-msrv
make gtk-lush-api-advisory
```

## Testing

```sh
make test        # All tests
make test-unit   # Unit tests only
make test-int    # Integration tests only
make test-widget # Widget tests through the private headless runner
make test-widget-headless # Widget tests with the CI mutter/dbus setup
make automation-smoke # Real-process D-Bus automation smoke with artifacts
make visual-smoke # Headless Mutter screenshot smoke with artifacts
make crash-recovery-smoke # SIGKILL/relaunch recovery smoke with artifacts
make portal-sandbox-smoke # Confined Flatpak/Snap smoke diagnostics
make accessibility-smoke # AT-SPI-enabled accessibility smoke
make performance-smoke # Lightweight Criterion performance smoke
make check-automation-docs # Automation documentation drift check
make automation-client-self-test # Reusable D-Bus automation client self-test
make check-flatpak-permissions # Flatpak full-filesystem permission guard
make check-gtk-lush-policy # GTK Lush family scaffolding/dependency guard
make gtk-lush-doctests # GTK Lush family doctests
make gtk-lush-examples # GTK Lush standalone adoption examples
make gtk-lush-msrv # GTK Lush family MSRV check
make gtk-lush-api-advisory # Advisory semver/public-API checks
```

Widget tests require a display server. `make test`, `make test-widget`, and
`make test-widget-headless` use the private `mutter --headless` path for
deterministic full-suite runs. The runner defaults to GTK's Cairo renderer so
headless containers do not fail the warning gate while probing unavailable GPU
devices; set `GSK_RENDERER` explicitly when debugging a renderer-specific
issue.

GTK widget tests run through the custom harness in [`crates/lushtext/tests/widget.rs`](./crates/lushtext/tests/widget.rs), which executes each widget test in its own process so GTK objects stay on a real main thread and test state cannot leak across cases. Because that binary is not owned by nextest, the shared runner keeps the native and headless `cargo test --test widget` paths aligned in one place.

For end-user risks that the widget harness cannot honestly prove, use the
separate smoke lanes documented in
[`docs/end-user-coverage.md`](docs/end-user-coverage.md). Those lanes preserve
artifacts and record explicit skip reasons instead of treating missing desktop,
portal, accessibility, or packaging support as a pass.
Recovery metadata, quarantine, migration-ledger, and crash-smoke triage details
live in [`docs/recovery-reliability.md`](docs/recovery-reliability.md).

## Benchmarks

Performance-sensitive code (fuzzy search, file indexing, directory scanning) is benchmarked with Criterion.rs:

```sh
make bench              # Run benchmarks
make bench-baseline     # Save as baseline
make bench-compare      # Compare against baseline
make bench-report       # Generate markdown report
```

## License

LushText is licensed under the [GNU General Public License v3.0 or later](https://www.gnu.org/licenses/gpl-3.0.html).
