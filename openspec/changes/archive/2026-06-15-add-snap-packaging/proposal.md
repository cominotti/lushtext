## Why

LushText currently ships only as a Flatpak. We also want an Ubuntu Snap so the
app reaches Snap Store users, but published quietly: discoverable only by people
who already know the command, not by store search. The blocker is that LushText
targets the GNOME 50 platform (GTK 4.22, Libadwaita 1.9, GtkSourceView `v5_18`),
and the Snap ecosystem cannot satisfy that floor today — the only GNOME platform
snap that will (the `core26` / GNOME 50 stack) is not yet published. We therefore
build everything that does **not** depend on that platform snap now, and gate the
single platform-dependent lever so the Snap ships the day the platform lands,
with zero throwaway work.

## What Changes

- Add a Snap packaging definition (`snapcraft.yaml`) that reuses the existing
  Meson → `cargo.sh` build and the compile-time `LUSHTEXT_PKGDATADIR` seam, so
  the confined binary loads its GResource bundle and GSettings schema with **no
  Rust source changes**.
- Use a snap `layout:` to bind-mount the baked `LUSHTEXT_PKGDATADIR` absolute
  path to its real location under `$SNAP`, satisfying strict-confinement path
  resolution without touching `config.rs`.
- Adopt **strict confinement + xdg portals** (not classic). This is a more
  principled posture than the Flatpak's current `--filesystem=host` and aligns
  with the portal-first follow-up already documented in
  `flatpak-sandbox-identity`. Strict confinement narrows host access compared to
  the Flatpak; behavior for workspace roots outside `$HOME` must be defined.
- Register and release the Snap as **Unlisted + edge-only**: omitted from store
  search, and `snap install lushtext` fails by default so testers must run
  `snap install lushtext --edge`. Promotion to discoverable/stable is a later
  store action, not a rebuild.
- Add CI that validates and (when the platform snap exists) builds + publishes
  the Snap, split into an always-on validation path and a platform-gated build
  path that does not fail the pipeline while `gnome-50-2604` is unavailable.
- Add a local, repeatable smoke test for the **confined** artifact (install the
  built `.snap`, launch headless, assert it starts and reads a file, and check
  for AppArmor denials), because native/Flatpak tests cannot catch
  confinement-only regressions.
- Update `README.md`, `AGENTS.md`, and `.agents/rules/build.md` to document Snap
  packaging, the platform-availability gate, and the unlisted+edge release flow.

## Capabilities

### New Capabilities
- `snap-packaging`: The Snap build contract — `snapcraft.yaml` structure, base
  and GNOME-platform dependency, Meson/Cargo part, the `LUSHTEXT_PKGDATADIR` +
  `layout:` mechanism for confined resource/schema loading, and the explicit gate
  on `core26` / GNOME 50 platform-snap availability.
- `snap-sandbox-identity`: The Snap's desktop identity (registered snap name vs
  the `dev.cominotti.lushtext` AppStream/common-id), the strict-confinement +
  portal permission posture with per-plug rationale, the Unlisted + edge-only
  release/visibility contract, and deterministic identity/permission verification
  — mirroring the existing `flatpak-sandbox-identity` capability.
- `snap-ci-and-testing`: The CI build/validate/publish contract gated on platform
  availability, plus the local confined smoke-test contract (install, headless
  launch, file-read, AppArmor-denial check).

### Modified Capabilities
<!-- No existing spec requirements change. The Flatpak packaging and sandbox
     identity behavior is unaffected; the Snap capabilities are additive and
     cross-reference flatpak-sandbox-identity rather than modify it. -->

## Impact

- **New files**: `snap/snapcraft.yaml`, a local smoke-test script under
  `scripts/`, a `.github/workflows/snap.yml` CI workflow, and `Makefile` targets
  for building/testing the Snap locally.
- **No expected Rust changes**: the `LUSHTEXT_PKGDATADIR` seam (`config.rs:12`,
  `lib.rs::register_resources` / `init_schema_dir`) is reused as-is; this must be
  verified, not assumed.
- **External dependency (gating)**: the Snap build cannot produce a GTK 4.22
  artifact until Canonical publishes the `core26` / GNOME 50 platform snap
  (`gnome-50-2604` or equivalent) and the `gnome` extension gains a `core26`
  target. The `core26` base snap itself is already published.
- **External actions (out of repo)**: registering the `lushtext` snap name in the
  Snap Store, setting Unlisted visibility, and configuring the
  `SNAPCRAFT_STORE_CREDENTIALS` CI secret.
- **Docs**: `README.md`, `AGENTS.md`, `.agents/rules/build.md` per the project's
  documentation-maintenance rule.
