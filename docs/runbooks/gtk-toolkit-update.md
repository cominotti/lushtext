# GTK toolkit update

The GTK axiom steps of a GTK, Libadwaita, GtkSourceView, Pango or GLib version
change: the CI Fedora container image, the shipped GNOME runtime in the
Flatpak manifest, the GNOME 50 source floor, the Snap `core26` platform. The
rules (run `make gtk-axioms` first, never adjust a probe to the new behaviour
first, version-qualify rather than rewrite) are in
[`.agents/rules/build.md`](../../.agents/rules/build.md), "GTK Axioms and
Toolkit Updates".

1. **Probe first.** Run `make gtk-axioms` before any other work. A failing
   probe is an axiom change under the ledger's rule: revisit the ledger entry
   (`.agents/skills/gtk4-libadwaita-internals/references/gtk-axiom-ledger.md`),
   its dependent designs, and every verification envelope that cites the id
   (the Kani `kani::assume` clauses) before editing any probe.
2. **Update "Verified against".** Copy the printed `gtk`/`adw` versions into
   the rows' "Verified against", with where they ran.
3. **Check the next floor.** Before a new GNOME runtime becomes the floor, run
   `make gtk-axioms-runtimes GTK_AXIOMS_RUNTIMES="org.gnome.Sdk//<next>"`
   against it; it builds inside each SDK (the gtk-rs feature flags bind a
   binary to its SDK), installs nothing, and skips an SDK that is not
   installed. The runtimes lane is local only.
4. **Version-qualify an axiom** whose verdict differs between the shipped
   runtime and the floor, rather than rewriting it: the catalogue entry names
   the first release with the new verdict (`version_bound:
   Some(VersionBound::gtk(4, 24, 0))` or `::sourceview(..)`), the probe asserts
   the verdict of the toolkit it runs on through `version::catalogued_bound`
   and records `asserted_branch`, and the ledger row states both ranges
   (`GTK < 4.24` / `GTK ≥ 4.24`), which `make check-gtk-axioms` enforces.
   "Verified against" then lists a run on each side (Fedora 45 host or CI, and
   `make gtk-axioms-runtimes` against `org.gnome.Sdk//50` or the
   `GTK Axioms (GNOME 50 floor)` job).
5. **Re-test a subset.** `make gtk-axioms-runtimes GTK_AXIOMS_ONLY=A28,A79`
   (`scripts/run-gtk-axioms.sh runtimes --only A28,A79 <sdk-ref>…`) builds and
   checks only the named samples, for interleaved re-test rounds; run them
   through `scripts/measure.sh`, alternating the SDK order. An unknown id
   fails the run.
6. **Expect the sandbox's RemoteDesktop skips.** The SDK sandbox has no session
   bus, so it cannot reach the private compositor's RemoteDesktop: the
   runtimes mode sets `GTK_LUSH_AXIOMS_SANDBOX=1`, and the samples whose
   fixture drives real input (the `REMOTE_INPUT_AXIOMS` list, which a unit
   test keeps equal to the probes that start a `RemoteInput`) print
   `skipped (needs RemoteDesktop): <id>` and exit 0 there. The host lane and
   CI never set the variable, so a host compositor without RemoteDesktop still
   fails them as `fixture-invalid`.
7. **Read the per-SDK summary.** After each SDK the lane prints each sample's
   verdict or skip reason, the toolkit versions the samples reported, and the
   SDK's `pkg-config` module versions.
8. **Update the pins** a gtk-rs bump moves: the Fedora version in `ci.yml` and
   `release-benchmark.yml`, and the GNOME tag in `flatpak.yml` and the Flatpak
   manifest (build.md, "CI").

A nightly verdict is a preview of the next adoption, never a ledger
restatement on its own.
