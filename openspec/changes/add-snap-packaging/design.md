## Context

LushText ships as a Flatpak built via Meson, which wraps Cargo through
`build-aux/cargo.sh`. The binary resolves its installed data through the
compile-time constant `LUSHTEXT_PKGDATADIR` (`config.rs:12`, an
`option_env!`): when set, `register_resources()` loads `lushtext.gresource`
from that directory and `init_schema_dir()` defers to the system GSettings
schema directory; when unset (dev builds), both fall back to embedded/source
paths. The app's hard floor is the GNOME 50 platform (GTK 4.22, Libadwaita 1.9,
GtkSourceView `v5_18`), which is why CI runs Fedora 44 containers rather than
`ubuntu-latest` (Ubuntu LTS ships older GTK).

Snap faces the same floor, more strictly. Investigation of the live Snap Store
API and Canonical's `ubuntu/gnome-sdk` repository established (May 2026):

- The `gnome` extension's platform snap tops out at `gnome-46-2404` (GTK 4.14,
  `base: core24`). There is no `gnome-48`/`gnome-49` platform snap; Canonical
  ships one GNOME platform per LTS base.
- The `core26` base snap (Ubuntu 26.04, GNOME 50, GTK 4.22) is **published**
  (released 2026-04-29), but the matching GNOME platform/SDK snap is **not**:
  `gnome-50-2604` returns HTTP 404, and `gnome-core26-sdk` exists only as a
  work-in-progress git branch. The `gnome` extension is documented for
  `core22`/`core24` only.

Therefore a GTK 4.22 Snap cannot be built today. The chosen strategy (Path 1)
is to build everything independent of the platform snap now and gate the single
platform-dependent lever.

## Goals / Non-Goals

**Goals:**
- A `snapcraft.yaml` and supporting tooling that reuse the existing Meson/Cargo
  build and the `LUSHTEXT_PKGDATADIR` seam with no Rust source changes.
- Correct GResource and GSettings loading under **strict confinement** via a
  snap `layout:` bind-mount, not code changes.
- A documented, deterministic permission posture: strict confinement + xdg
  portals, with a rationale for every plug, narrower than the Flatpak's
  `--filesystem=host`.
- An Unlisted + edge-only release contract: hidden from search, installable only
  via an explicit `snap install lushtext --edge`.
- CI that always validates the Snap definition and builds/publishes only when
  the platform snap is available, never failing the pipeline on the missing
  dependency.
- A local smoke test that exercises the **confined** artifact and surfaces
  AppArmor denials.

**Non-Goals:**
- Shipping a GTK 4.22 Snap before `gnome-50-2604` (or equivalent) is published.
- Classic confinement or a `system-files`/`personal-files` escape hatch.
- Changing the Flatpak build, its `--filesystem=host` posture, or
  `flatpak-sandbox-identity`.
- Lowering LushText's GNOME 50 / GTK 4.22 feature floor.
- Building the GNOME platform stack from source inside the Snap (rejected; see
  Decisions).
- Promoting the Snap to public/stable visibility (a later, separate decision).

## Decisions

### D1: Reuse `LUSHTEXT_PKGDATADIR` + a snap `layout:` instead of changing code
The Meson build already installs the GResource bundle and GSettings schema under
`pkgdatadir` and bakes that path into the binary. Under strict confinement the
baked absolute path (e.g. `/usr/share/lushtext`) does not resolve, because the
snap's `/usr` is the base snap's. A `layout:` entry bind-mounts the baked path to
`$SNAP/...` inside the snap's mount namespace, so the existing code resolves
correctly.

- **Alternative — bake `$SNAP/...` directly into `LUSHTEXT_PKGDATADIR`**: viable
  because the snap name is fixed and `current` is a stable symlink, but couples
  the Rust build to the snap layout and is less relocation-safe than a `layout:`.
  Rejected as the default.
- **Alternative — add a Snap-specific runtime path branch in `lib.rs`**:
  unnecessary; the `option_env!` seam already covers it. Rejected.

### D2: Strict confinement + xdg portals (not classic)
Strict confinement keeps the Snap auto-listable, keeps store review light (a
prerequisite for a quiet unlisted release), and matches the portal-first
direction already written into `flatpak-sandbox-identity`. The `gnome` extension
auto-wires Wayland/X11/GPU/portals; the app adds `home` (auto-connected) and
`removable-media` (manual-connect) for file access.

- **Alternative — classic confinement** to mirror `--filesystem=host` exactly:
  triggers heavyweight manual store review, conflicts with a quiet unlisted
  release, and is not how GNOME editors ship. Rejected.
- **Trade-off**: strict confinement is *narrower* than the Flatpak. Workspace
  roots outside `$HOME` (e.g. `/opt`, `/etc`, dotfile paths) and arbitrary CLI
  paths (`lushtext /etc/hosts`) are blocked unless reached via portal or an
  explicitly connected plug. This divergence is specified, not hidden.

### D3: Gate the platform dependency; scaffold everything else now
The base (`core26`) exists but the GNOME 50 platform snap does not. The Snap
definition targets the platform via a single lever (`base:` + `extensions:`/
content plug). CI's build/publish job is conditioned on platform availability so
the missing dependency never reds the pipeline.

- **Alternative — vendor GTK 4.22 from source as snap parts**: rebuilds exactly
  what `gnome-core26-sdk` will provide, producing large/slow/fragile builds that
  are obsolete the day the platform snap ships. Rejected.
- **Alternative — Path 1.5, manual content-plug wiring against `gnome-50-2604`
  at edge before the extension supports `core26`**: kept as a documented
  fast-follow once the platform snap reaches the store's edge channel, not the
  initial target.

### D4: Unlisted + edge-only release
Register the snap name, set visibility Unlisted (omitted from search, installable
by name), and publish revisions only to the `edge` channel so the default
`snap install lushtext` fails and testers must use `--edge`. Promotion is a store
action (`snapcraft release ... stable` + set Public), not a rebuild.

- **Alternative — private visibility**: excludes any tester not explicitly
  invited by email. Too restrictive for the intended limited testing. Rejected.

### D5: Snap identity vs AppStream identity
The registered snap name lives in Snap's flat global namespace (target:
`lushtext`, fallback if taken). The desktop/AppStream identity stays
`dev.cominotti.lushtext` via the app's `common-id`, linking the snapped desktop
entry to the existing metainfo. Verification asserts both: the registered name
and the `common-id` linkage.

## Risks / Trade-offs

- **[Platform snap never lands on the expected timeline]** → Scaffold has
  standalone value (validation, registration, CI structure); the gated build path
  simply stays inactive. Path 1.5 (edge content-plug wiring) is the escape hatch
  if waiting for extension support is too slow.
- **[Strict confinement breaks workspace roots outside `$HOME`]** → Specify the
  behavior explicitly (portal-mediated open, `removable-media` for media mounts)
  and cover it in the smoke test rather than discovering it post-release.
- **[Baked absolute path drifts from the `layout:` target]** → The smoke test
  must assert the app actually loads its GResource/schema inside the snap, not
  just that it builds, so a path mismatch fails loudly.
- **[CI cannot build a real artifact while the platform is missing]** → Split CI
  into always-on validation and a platform-gated build/publish job; document the
  gate so a green pipeline is not mistaken for a shipping Snap.
- **[`cargo-sources.json` divergence]** → Snap builds are online by default, so
  the Flatpak vendoring artifact is not required for Snap; decide explicitly
  whether to vendor for reproducibility rather than coupling the two pipelines.
- **[Snap name `lushtext` already registered by someone else]** → Resolve the
  registered name before wiring CI publish; verification keys off the actual
  registered name.

## Migration Plan

This is additive; there is nothing to roll back in the app. Sequencing:

1. Land the scaffold (snapcraft.yaml, layout, local smoke test, CI validation,
   docs) with the build/publish job gated off.
2. Register the snap name in the Store and set Unlisted visibility (external).
3. When `gnome-50-2604` (or the `core26` GNOME platform) publishes, flip the
   platform lever, enable the gated CI build/publish-to-edge job, and verify the
   confined artifact via the smoke test.
4. Optional fast-follow: Path 1.5 manual content-plug wiring if the platform snap
   reaches edge before the `gnome` extension gains a `core26` target.
5. Future, out of scope: promote to stable/public when ready.

## Open Questions

- Exact registered snap name (`lushtext` vs a fallback) — depends on store
  availability.
- Whether to vendor Cargo dependencies for Snap reproducibility or rely on
  online fetch during the build.
- Which arches to target (`amd64` only initially, or also `arm64`).
- Precise published name of the core26 GNOME platform/SDK snap and whether the
  `gnome` extension will expose a `core26` target or require manual content-plug
  wiring at first.
