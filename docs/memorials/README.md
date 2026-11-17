# Memorials

A memorial is dated history that explains why a rule exists: the incident, the
measurement, or the approach that was tried and retired. It is not guidance.
Every rule a memorial explains lives in `AGENTS.md` or `.agents/rules/`, which
link here where an agent may need the reason.

Memorials are append-only. Correct a factual error in place; otherwise add a
new dated entry rather than rewriting an old one. History that belongs to a
planned-work programme goes to its record under `docs/next/` instead, and
history of a crate goes to its `CHANGELOG.md`.

| Memorial | Explains |
| --- | --- |
| [shared-machine-lock-incidents.md](shared-machine-lock-incidents.md) | the heavy/measure/wait helpers and the lock protocol |
| [2026-10-20-test-data-incidents.md](2026-10-20-test-data-incidents.md) | test isolation as a property of the process, and the temporary-file rule |
| [gtkpaned-shell-animation.md](gtkpaned-shell-animation.md) | the retired `GtkPaned` shell and the allocation-frame churn rules |
| [tree-expander-activation.md](tree-expander-activation.md) | why file rows disable `GtkTreeExpander`'s click gesture |
| [axiom-lane-warning-scan.md](axiom-lane-warning-scan.md) | why the GTK axiom lane fails on any warning line, and the sample flag guard |
| [release-terminology-guard.md](release-terminology-guard.md) | why `make check-terminology` runs in the local gate |
| [2026-11-17-compiler-front-end-threads.md](2026-11-17-compiler-front-end-threads.md) | `heavy.sh`'s contention-sized `CARGO_BUILD_JOBS` and the parallel front-end wrapper, with the measurements |
