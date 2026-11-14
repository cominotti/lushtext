# Profiling

Running `perf` and `gdb` on the maintainer's workstation. The rule that a
profile is attribution evidence, never a verdict, is in
[`.agents/rules/verification-runs.md`](../../.agents/rules/verification-runs.md),
"Shared-machine agent work".

On the maintainer's workstation `perf` and `gdb` are installed in the toolbox,
and Fedora's debuginfod fetches GTK, Pango and GLib symbols on demand. Prefer
them to hand-written timing shims. The host keeps `perf_event_paranoid = 2` and
`ptrace_scope = 1`, so:

- profile user space only and launch the target as a child; do not attach to a
  running process:
  `perf record -e cycles:u --call-graph dwarf --user-callchains -F 999 -o <file> -- <command>`
  (without `:u` and `--user-callchains`, `perf record` fails with "Failure to
  open any events for recording");
- `gdb --args <command>` works on a child; `gdb -p`/`eu-stack -p` on another
  process does not;
- a profiled measurement still goes through `scripts/measure.sh`
  (`scripts/measure.sh --label <l> -- perf record … -- <command>`).
