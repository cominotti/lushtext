## Why

The maintainer approved on 2026-11-09 moving LushText's development
environment and its shipped Flatpak to GNOME 51. Users of the Cominotti
Flatpak should run the toolkit LushText was tested and measured on. Today we
test on GTK 4.22, but the GNOME 51 runtime ships GTK 4.24.1, GtkSourceView
5.22.0, Pango 1.58.2, Libadwaita 1.10.0 and GLib 2.90.1, the same toolkit as
the new development toolbox `hypatia-f45` (Fedora 45). Every later
measurement in the long-line programme, and the GNOME nightly re-test of
upstream fixes queued after this change, should run on that toolkit.

The toolkit-update rule says `make gtk-axioms` runs first. It did, on `main`
`3dcbe682` inside `hypatia-f45` (GTK 4.24.1, Adw 1.10.0): 88 probes held and
three changed verdict:

- **A7 and A20** (`GtkListView` re-derives its value from a stray scroll
  anchor after a host `set_value`). On 4.24 the value stays put: at pages
  300/250/200/150 it reads 1360 each time, with alignment 0. On 4.22 it was
  1360/1139/918/697, with alignment −4.42. GTK 4.23.4 added "listbase:
  Sanity-check alignment values" (!9778), which matches this change.
  `ViewportSliceBin`'s write-back and re-announcement paths, and the Kani
  slice-loop envelope, rely on these two axioms.
- **A32** (detaching a `GtkSourceMap` with a pending tick logs CRITICALs).
  This is a defect axiom, and GtkSourceView 5.22.0 fixed the defect upstream
  (8d679ccd). The minimap's retire-and-rebuild path (A45) exists to work
  around it.

`make test` on Fedora 45 passes every test, but three new toolkit warning
classes fail the gate in every shard:

- W1: `GtkSourceMapSlider` snapshotted without an allocation (1,786 lines);
- W2: `GtkListView` measured below its minimum in the sidebar (830 lines);
- W3: Libadwaita 1.10's `AdwEntryRow placed into AdwExpanderRow` (20 lines).

One widget test is also `FLAKY`. The responsiveness lane's quick tier fails
too:

- typed text is drawn late in 100 typing phases. `baseline/small` p50 goes
  from 5.3 ms to 10.0 ms and its max from 16.4 ms to 60.1 ms, with 0 → 33
  withheld keys against GTK 4.22;
- typing misses the document in 36 HTML phases;
- `baseline/empty` fails.

So the change is more than a manifest bump. Three ledger entries change, and
the GNOME 51 SDK also changes the Rust compiler the Flatpak ships: its
`rust-stable//26.08` extension carries Rust 1.98.0, while `25.08` carries
1.98.1.

## What Changes

- **The shipped runtime moves to GNOME 51, but the source floor stays at
  GNOME 50.** `build-aux/dev.cominotti.lushtext.Flatpak.json` changes
  `runtime-version` 50 → 51; the SDK follows the runtime, and the Rust
  extension follows the SDK. The gtk-rs feature floor stays as it is:
  `gnome_50`, `v1_9`, `v5_18`, `v2_88` and `v1_57`. No `v4_24`-only API is
  needed, and the Snap (`core26`, GNOME 50 platform) and distribution builds
  still target GNOME 50.
- **The test toolkit is the shipped toolkit, and the floor stays verified.**
  - Development moves to the Fedora 45 toolbox.
  - CI's Fedora containers move `fedora:44` → `fedora:45`, and the Flatpak
    containers move `gnome-50` → `gnome-51`.
  - The `GTK Axioms` CI job runs on both `fedora:44` (the GNOME 50 floor) and
    `fedora:45` (the shipped toolkit).
  - `make gtk-axioms-runtimes` adds `org.gnome.Sdk//51` to its default SDKs.
- **Axioms A7, A20 and A32 become version-qualified.** Each ledger entry
  states the verdict for each supported toolkit: GTK < 4.24 or GtkSourceView
  < 5.22, and GTK ≥ 4.24 or GtkSourceView ≥ 5.22. Each probe and sample
  asserts the verdict for the toolkit it runs on, so the probes stay green on
  both and still trip on any further change. "Verified against" records both
  runs. Before any probe is edited, the change attributes the toolkit change
  (A7/A20 to GTK !9778, A32 to GtkSourceView 8d679ccd) and reviews every
  dependent design and envelope.
- **No LushText behaviour changes for A7/A20.** `ViewportSliceBin` keeps its
  write-back and re-announcement. They are still needed on the GNOME 50 floor
  and are no-ops on 4.24. The Kani slice-loop envelope already admits the
  4.24 behaviour, because a stray anchor's re-derivation is nondeterministic
  and includes "value unchanged". The citations are version-qualified, and
  the `widgets-geometry` shard is rerun.
- **The minimap keeps retiring its map (A45) instead of clearing its view.**
  The defect is fixed on 5.22, but every supported runtime older than 5.22
  (the GNOME 50 floor, the Snap, distributions) still has it. Retiring is also
  what releases the map's memory and validation work. This decision is
  recorded in the design.
- **The Rust pin follows the compiler the Flatpak ships.** Under the
  2026-10-29 maintainer decision, the binary we test must be built by the
  compiler we ship. The pinned toolchain therefore moves to whatever
  `org.freedesktop.Sdk.Extension.rust-stable//26.08` ships at implementation
  time (Rust 1.98.0 on 2026-11-09) in `rust-toolchain.toml`, every CI
  toolchain step, and the Snap bootstrap. The MSRV stays 1.98.0.
- **Toolkit regressions fixed at their cause, failing first:**
  - W1, the minimap slider;
  - W2, the slice-bin list measure;
  - W3, the excluded-names editor rows;
  - the `FLAKY` test;
  - the lane's late typed frames, misdirected typing and `baseline/empty`
    failure. Each is first attributed to GTK, Libadwaita, mutter 51 or
    LushText with an isolated probe.

  No warning filter is widened, and no lane rule is loosened.
- **Full local gate on Fedora 45:**
  - `make check`;
  - `make test` with 0 `FLAKY`;
  - `make gtk-axioms`;
  - the visual-geometry, accessibility and visual smokes;
  - the responsiveness lane's quick tier, interleaved `main` versus branch on
    the same toolkit.

  Any regression the toolkit bump causes is fixed in this change
  (pre-existing-blockers rule). GTK 4.22 (archived `main` records) versus
  4.24 deltas are recorded as context only, not as pass/fail.
- **Flatpak rebuilt and verified:** `make flatpak` and
  `make verify-flatpak-identity` on the 51 runtime. `cargo-sources.json`
  changes only if `Cargo.lock` does.
- **Docs:**
  - README and AGENTS tech stack (GNOME 51 runtime: GTK 4.24, GtkSourceView
    5.22, Libadwaita 1.10; GNOME 50 source floor);
  - `.agents/rules/build.md` (Flatpak, CI, toolchain, axiom runtimes);
  - `rust.md`;
  - the `gtk4-libadwaita-internals` skill's version lines;
  - the ledger;
  - `docs/upstream-reports.md`, for what the 51 runtime changes;
  - `docs/next/long-line-documents.md` and `docs/next/formal-verification.md`.

Not in scope:
- the GNOME nightly re-test of upstream fixes (A79, A54, A28, A52, A29,
  A102), which is the next queued item;
- moving the gtk-rs feature floor;
- dropping any workaround while a supported runtime still needs it;
- Snap activation, which is still gated on the GNOME 50 platform snap.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gtk-axiom-ledger`: version-qualified axioms. An entry whose verdict
  differs between supported toolkits states each verdict, and its probe
  asserts the one for the running toolkit. Changes A7, A20 and A32.
- `gtk-lush-axioms`: CI runs the probes on the floor and on the shipped
  toolkit, and the toolkit-update procedure covers the runtime the Flatpak
  ships.
- `editor-minimap`: the retire-instead-of-detach requirement is
  version-qualified. The detach CRITICAL is reproducible only on GtkSourceView
  < 5.22, and the retire path stays while any supported runtime predates 5.22.
- `rust-toolchain-adoption`: the pinned stable toolchain is the compiler the
  Flatpak SDK's Rust extension ships for the manifest's runtime.
- `gtk-lush-viewport-slice`: the bin never measures or allocates its list
  below the list's minimum on any supported toolkit (W2).
- `workspace-entry-visibility`: the excluded-names editor adds its rows the
  way Libadwaita 1.9 and 1.10 accept (W3).
- `responsiveness-measurement`: before a runtime move is accepted, the lane
  passes on the toolkit that runtime ships.
- `flatpak-sandbox-identity`: the Flatpak ships the GNOME 51 runtime, which
  is the toolkit the local and CI test lanes run.

## Impact

- Code:
  - `crates/gtk-lush/axioms` (probes and samples for A7, A20 and A32;
    version helpers);
  - `crates/gtk-lush/widgets/src/kani_proofs.rs` and `scroll_request.rs`
    (citations only);
  - minimap `retirement.rs` (documentation of the decision).

  No application behaviour change is expected unless the full gate finds a
  toolkit regression.
- Packaging: the Flatpak manifest, and `scripts/test-release.sh`'s manifest
  fixture.
- CI: about 28 `fedora:44` container references, 3 `gnome-50` Flatpak
  images and cache keys, and the `GTK Axioms` matrix. About 20
  `dtolnay/rust-toolchain@1.98.1` pins.
- Toolchain: `rust-toolchain.toml`, the CI pins and the Snap bootstrap.
- Docs: listed above, plus the axiom ledger.
