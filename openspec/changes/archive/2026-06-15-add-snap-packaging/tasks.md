## 1. Snap Build Scaffold (ready now)

- [x] 1.1 Create `snap/snapcraft.yaml` with name (`lushtext`), `base: core24` placeholder, `confinement: strict`, `grade`, summary/description, and `adopt-info` from the AppStream metainfo
- [x] 1.2 Define the LushText part using the `meson` plugin so it drives the existing `meson.build` → `cargo.sh` build, with no separate cargo invocation
- [x] 1.3 Add the `gnome` extension to the app entry and set `command`, `desktop`, and `common-id: dev.cominotti.lushtext`
- [x] 1.4 Add a `layout:` entry that bind-mounts the baked `LUSHTEXT_PKGDATADIR` path to its `$SNAP` location
- [x] 1.5 Ensure the part installs `lushtext.gresource` and `dev.cominotti.lushtext.gschema.xml` to the directory the layout maps, and compiles schemas
- [x] 1.6 Confirm no `.rs` files require changes; if any do, stop and reconsider the layout/PKGDATADIR approach

## 2. Confinement & Permissions (ready now)

- [x] 2.1 Declare `home` and `removable-media` plugs and confirm the `gnome` extension supplies Wayland/X11/GPU/portal access
- [x] 2.2 Document the rationale for each declared plug beyond the extension defaults
- [x] 2.3 Define and document behavior for workspace roots / paths outside confined-accessible locations (portal-mediated open or graceful access error, never crash or silent data loss)

## 3. Local Confined Smoke Test (ready now, runs gated)

- [x] 3.1 Add `scripts/run-snap-smoke.sh` that builds the snap (LXD backend), installs it with `--dangerous`, and launches it headlessly
- [x] 3.2 Assert the confined app starts, loads its GResource and GSettings schema, and opens a file in an accessible directory
- [x] 3.3 Capture AppArmor/seccomp denials (`snappy-debug` and/or journal) and fail the smoke test if any are present
- [x] 3.4 Make the smoke test skip cleanly with a clear message when the GNOME 50 platform snap is unavailable

## 4. Make Targets & Docs (ready now)

- [x] 4.1 Add `Makefile` targets to build the snap and run the smoke test, consistent with the `flatpak` / `verify-flatpak-identity` targets
- [x] 4.2 Update `README.md` with Snap build/install instructions and the platform-availability gate
- [x] 4.3 Update `.agents/rules/build.md` with the Snap section (Meson reuse, layout/PKGDATADIR, platform gate, online-build note on `cargo-sources.json`)
- [x] 4.4 Update `AGENTS.md` module/architecture overview to mention Snap packaging alongside Flatpak

## 5. CI Validation (ready now)

- [x] 5.1 Add `.github/workflows/snap.yml` with an always-on job that validates `snap/snapcraft.yaml` and fails on malformed definitions
- [x] 5.2 Add a build/publish job using `snapcore/action-build` + `snapcore/action-publish`, gated on platform availability and `SNAPCRAFT_STORE_CREDENTIALS`, configured as skipped/non-failing while the platform snap is missing
- [x] 5.3 Configure publish to release only to the `edge` channel

## 6. Identity & Permission Verification (ready now)

- [x] 6.1 Add a verification script (or extend smoke test) that reports confinement type and effective plug connection state
- [x] 6.2 Assert `common-id` is `dev.cominotti.lushtext` and the desktop entry is present in the built snap
- [x] 6.3 Record the configured snap name for scaffold verification by reading it from `snap/snapcraft.yaml` (`lushtext`); final Store registration confirmation is deferred below

## Deferred Snap Activation Ledger

The scaffold is complete and intentionally archived. These external/platform
activation items are preserved for a future change instead of remaining as
active acceptance tasks here.

### Store Registration & Release

- Resolve and register the snap name in the Snap Store, using a fallback if `lushtext` is taken.
- Set store visibility to Unlisted.
- Export store credentials and add the `SNAPCRAFT_STORE_CREDENTIALS` secret to the CI repository.

### Platform-Gated Activation

- When the `core26` / GNOME 50 platform snap publishes, switch `base:` to `core26` and confirm the `gnome` extension targets it, or wire the content plug manually if the extension lags.
- Build the strict-confined snap against GTK 4.22 / Libadwaita 1.9 and run the local smoke test against the real artifact.
- Enable the gated CI build/publish-to-edge job and confirm a green build produces an edge revision.
- Verify the end-to-end install flow: `snap install lushtext` fails by default, `snap install lushtext --edge` succeeds, and the snap is absent from `snap find`.

### Acceptance Verification

- Run the confined smoke test and confirm no AppArmor/seccomp denials.
- Confirm GResource and GSettings load inside confinement and a HOME-rooted workspace operates normally.
- Confirm an out-of-scope path is handled gracefully (portal or access error, no crash/data loss).
- Confirm documentation (`README.md`, `AGENTS.md`, `.agents/rules/build.md`) matches the shipped Snap behavior and the platform gate.
