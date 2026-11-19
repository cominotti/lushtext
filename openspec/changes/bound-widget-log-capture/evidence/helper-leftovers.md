# Helper self-tests leave nothing (task 3.3, spec R5)

`scripts/heavy.sh --mem none -- env TMPDIR=<private base> make check-agent-helpers`,
the base a fresh `mktemp -d /tmp/bwlc-helpers-base.XXXXXX`, then
`find <base> -mindepth 1`:

| Tree | When | Result | Base afterwards |
| --- | --- | --- | --- |
| main (unchanged helpers) | 2026-11-18 21:13 | exit 0 | empty |
| branch (lt-heavy-, lt-envelope-, one with-memory-cap self-test root) | 2026-11-19 00:56 | exit 0, every helper self-test ok (envelope cases exercised under the user manager) | empty |

The six `with-memory-cap-marker.*` / `with-memory-cap-envelope-test.*`
entries of 2026-11-17 in shared `/tmp` did not come back on main in this
run, so they were left by interrupted self-tests (no trap runs on SIGKILL);
with the branch every such entry lives under the self-test root, removed by
its trap, and a late marker touch into a removed root fails instead of
recreating the file.
