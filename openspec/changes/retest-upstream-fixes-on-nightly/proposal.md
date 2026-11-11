## Why

Upstream maintainers asked us to re-test their fixes against the GNOME
nightly toolkit:

- On GNOME/gtk#8528 (A79: `gtk_text_view_get_iter_at_location()` below the
  layout walks the last line's hidden bytes), the maintainer asked whether
  GTK !10651 (merged on `main` 2026-11-05) helps, with Pango `main`.
- On GNOME/pango#582 (A28: x positions wrap negative past `i32::MAX` Pango
  units), the maintainer said Pango commit `f60f461` likely fixes it and asked
  for a re-test.
- GNOME/gtk#8529 (A54: `retrieve-surrounding` answers with the whole
  paragraph) was addressed by GTK !10646.
- GNOME/gtksourceview#432 (the first `set_language()` parses the `.lang`
  files on the main loop) got !465; the maintainer measured about 19/16/11 ms
  for html/markdown/json. The task text and `docs/upstream-reports.md` call
  this axiom A52. It is **A67**: A52 is the unparented-view layout axiom.
- The A29 context-engine guard (`aa6a36a3`) is on GtkSourceView `master`.

The programme record queues this re-test right after `adopt-gnome-51`
(`docs/next/long-line-documents.md`). The A102 GtkSourceView report also
waits on it. The maintainers need numbers from us, not assumptions. Our
workaround retirement notes need to say exactly what changes once the fixed
versions are the floor.

The spec stage measured a baseline (design, "Measured baseline"):

- **The nightly contains every fix.** The installed `org.gnome.Sdk//master`
  (build 2026-11-11 05:29 UTC) carries GTK `4.25.0-90-g1c64c1de` (4.25.1),
  Pango `1.58.2-51-g6bd640fd` (1.59.0) and GtkSourceView
  `5.22.0-64-g9f23f713` (5.23.0). By GitLab `merge_base`, it contains !10651
  `7c578d79`, !10646 `cb73e884`, Pango `f60f461`, GtkSourceView `aa6a36a3`,
  !463, and !465. !465 landed rebased as `4f966029`…`a6af0874`, not as its
  MR head `3e34734f`. None of them is in GNOME 51.
- **One run, not yet interleaved:**
  - A79 still walks the hidden bytes: 83/159/321 ms per lookup at 1/2/4 MiB
    on nightly, against 78/152/297 ms on GNOME 51.
  - A28 still overflows on Pango 1.59.0, at 261,252 characters with the
    SDK's font.
  - A54's view still answers with the whole 20,000-character paragraph.
  - A67's first attach drops to 12.7/12.8/8.5 ms on nightly, from
    32.4/33.1/21.2 ms on GNOME 51. That is under `B_SETTLE_US` (16 ms).
  - A102, A103 and A107 still reproduce.
- **The SDK runtimes lane fails before it can answer.** These are
  pre-existing blockers:
  - A29's sample ran for more than 600 s on nightly at 100% CPU and had to be
    stopped. Its fixture keeps doubling the line, bounded only by length.
  - A23 fails on both SDKs (a one-pixel bottom mismatch with the SDK's
    fonts).
  - A59–A61, A64 and A97 report `fixture-invalid` in every SDK sandbox,
    because the sandbox has no RemoteDesktop.
  - `make gtk-axioms-runtimes` cannot run only the affected samples, which
    the interleaved rounds need.
- **Nightly changes verdicts we did not ask about.** A4, A70, A74, A83, A85
  and A86 fail on GTK 4.25.1 / GtkSourceView 5.23.0, unattributed. Nightly is
  not a supported toolkit, so these are a preview for a future GNOME 52
  adoption, not ledger restatements.

## What Changes

- **Toolkit update.** Update `org.gnome.Sdk//master` from `gnome-nightly`
  (`flatpak update --user`, maintainer-approved). Record its build commit and
  date, and the GTK/Pango/GtkSourceView/Libadwaita/GLib source commits from
  the SDK's `/usr/manifest.json`. Record GitLab `merge_base` containment for
  each fix.
- **Make the runtimes lane answer:**
  - `scripts/run-gtk-axioms.sh runtimes` gains `--only <ids>`.
  - In an SDK sandbox, the RemoteDesktop samples report an explicit
    `skipped (needs RemoteDesktop): <id>`, counted like the slow skip. The
    host lane is unchanged and still fails them when its compositor lacks
    RemoteDesktop.
  - A per-SDK verdict summary is printed.
  - A29's fixture gets a wall-time bound next to its length cap.
  - A23's check is made structural against fractional line positions,
    attributed first.
  - Each fix comes with a self-test or a probe test that fails first.
- **Measure the costs the maintainers asked about:**
  - A54 records the cost of `retrieve-surrounding` on a 50,000-character
    paragraph, cold and after an edit, as a recorded value only. That is
    what !10646 changes.
  - Four interleaved rounds (nightly, GNOME 51) of A79, A28, A54, A67, A29,
    A102, A103 and A107 run at a 1-minute load below 4, with the load
    recorded.
  - The upstream reproducers in `~/Documents/Agentic/Reports/GTK/<folder>/`
    are rerun inside the nightly SDK.
- **Records:**
  - `docs/upstream-reports.md` gets each re-test result, fix containment, the
    corrected A67 id, and a precise retirement condition per workaround.
  - The axiom ledger rows A28, A29, A54, A67, A79, A102, A103 and A107 get a
    "Verified against" note for the nightly. The A67 row's probe name is
    corrected (`probe_a67`).
  - The programme record lists the nightly-only verdict changes for the next
    GNOME adoption.
- **Upstream text, never posted:**
  - Facts sheets for gtk#8528, pango#582 and gtksourceview#432: numbers,
    versions, commands and raw output, for the maintainer to write the reply.
    Ready-made replies are **not** drafted (design D1).
  - New report drafts for A102 (GtkSourceView) and A107 (Pango), and a
    refresh of the existing A103 draft, only if each still reproduces on the
    nightly. Each follows the `upstream-gtk-reports` skill with its
    AI-disclosure line, under `~/Documents/Agentic/Reports/GTK/<folder>/`.
- **No production code change**, because no re-test shows a LushText
  workaround wrong on GNOME 51. If one does, it is fixed failing-first in
  this change.

## Capabilities

### New Capabilities

- `upstream-reports`: how re-tests of upstream fixes are run, attributed and
  recorded. It covers what a retirement note must state, and what may be
  drafted for upstream: facts sheets for replies, and new reports only when
  the defect reproduces on the nightly, with disclosure.

### Modified Capabilities

- `gtk-axiom-ledger`: nightly re-test results are recorded as "Verified
  against" notes without restating an axiom. Nightly-only verdict changes are
  recorded as unattributed previews.
- `gtk-lush-axioms`: the SDK runtimes lane runs a subset and reports
  sandbox-unsupported samples explicitly. A time-limited fixture is also
  bounded in wall time.

## Impact

- **Code:**
  - `scripts/run-gtk-axioms.sh` (runtimes `--only`, sandbox skips,
    summary, self-test);
  - `crates/gtk-lush/axioms/src/probes/{a23,a29,a54}.rs` and their samples;
  - the RemoteDesktop sample helper;
  - `crates/gtk-lush/axioms/CHANGELOG.md`.
- **Docs:** `docs/upstream-reports.md`, the axiom ledger,
  `docs/next/long-line-documents.md`, `.agents/rules/build.md` (the runtimes
  lane paragraph), and `AGENTS.md`/`README.md` if the make target's usage
  changes.
- **Outside the repository:** `~/Documents/Agentic/Reports/GTK/` (facts
  sheets and drafts) and the user Flatpak installation (`org.gnome.Sdk//master`
  update only).
- **Not touched:** LushText production code, GSettings, persisted formats,
  Kani harnesses (no shard rerun needed; design D8), and the shipped runtime.
