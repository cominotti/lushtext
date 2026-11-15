# GTK axioms through the refreshed bindings (task 7.3)

Toolkit (printed by every probe): GTK 4.24.1, Libadwaita 1.10.0, Pango
1.58.2, GtkSourceView 5.22.0 (Fedora 45 toolbox). No C library changed in this
change, so no ledger "Verified against" cell changes and no axiom is added.

Bindings (`Cargo.lock`): gtk4 0.11.5, glib 0.22.10, gio 0.22.10, pango 0.22.9,
libadwaita 0.9.2, sourceview5 0.11.3.

Every run: `scripts/heavy.sh --mem none -- make gtk-axioms`, CPUs 8-23, with
`GTK_LUSH_AXIOMS_INCLUDE_SLOW=1` (local default), on a host shared with other
agents' builds, mutation runs and widget shards.

| Run | Load (1 min) | Result | First attempts that failed, then held on retry |
| --- | --- | --- | --- |
| 1, before the probe hardening, beside this change's own `lint-advisory` and rustdoc builds | 11–13 | exit 0, probe binary passed, 100 sample checks hold | A70 (violated), A76 (fixture-invalid), A79 (violated) |
| 2, after a first A76 attempt (rounds), since reverted | 13–17 | exit 1 | A76 failed every attempt: rounds of the 8 MiB run are not independent (design finding 9); A29's 60 s budget stopped the doubling under load (fixture-invalid, both attempts); A85, A91, A98 held on retry |
| 3, the final probes | 13 → 4.5 | exit 0, probe binary passed, 100 sample checks hold | A86 (violated: "a per-widget class swap does not grow with the unrelated widgets") |
| 4, after merging `origin/main` (`a5ddb117`, which adds A112) | 9–16 | exit 0, probe binary passed, 101 sample checks hold | A110 (held on retry) |

In isolation (`cargo test -p gtk-lush-adoption-lab --test axiom_probes --
a70_ a76_ a79_`, three rounds, load 11–15), A70, A76 and A79 held 9 of 9
before the hardening; in run 3 all three held at the first attempt.

A29, A85, A86, A91, A98 and A110 are timing probes that this change does not touch:
each failed one first attempt once, under load, and held on retry. They are
recorded, not fixed here (see the PR's open decisions).

## Stage 4 review: branch against `main`

The six probes that held only on retry (A29, A85, A86, A91, A98, A110) were run
in isolation, interleaved branch → `main` (detached `origin/main` `cfee0c48`,
whose only difference from the branch is this change), each round
`scripts/heavy.sh --mem 6G -- cargo test -p gtk-lush-adoption-lab --test
axiom_probes -- a29_ a85_ a86_ a91_ a98_ a110_` with
`GTK_LUSH_AXIOMS_INCLUDE_SLOW=1`:

| Round | Branch: load (1 min) before → after, first-attempt failures | `main`: load before → after, first-attempt failures |
| --- | --- | --- |
| 1 | 18.20 → 12.50, none | 11.05 → 7.65, none |
| 2 | 7.65 → 5.11, none | 5.11 → 6.39, none |
| 3 | 6.39 → 6.34, none | 6.34 → 4.54, none |
| 4 | 4.58 → 4.88, none | 4.88 → 6.56, none |
| 5 | 6.68 → 5.91, none | 5.91 → 5.69, none |

Neither side failed a first attempt (0 FLAKY lines in 10 rounds), so the
binding refresh is not shown to change these probes. A fifth full lane on the
review head (`scripts/heavy.sh --mem none -- make gtk-axioms`, load 5.7 → 6.2)
exited 0 with 0 FLAKY lines: the probe binary passed and 101 sample checks
held. The lane cannot exit 0 with a probe that never passed: run 2's A29 and
A76, which failed both attempts, made it exit 1 (`test result: FAILED. 2
failed`). The rounds and the lane ran at load 5–12, below runs 1–4's 9–17, so
load sensitivity of these timing probes on a busier host is not excluded by
this evidence; sibling branches on `main`'s lockfile ran the lane at similar
load with no FLAKY line.
