## ADDED Requirements

### Requirement: GSettings backend behaviours the shell depends on are ledger axioms
The ledger SHALL record, with the next free ids, the GIO GSettings backend
behaviours that the window-construction and settings-writer designs depend
on: that a keyfile-backend write rewrites and fsyncs the settings file
synchronously on the calling thread once per write; that `delay()` and
`apply()` perform one backend write for many pending keys; whether a write on
one thread's settings instance blocks reads and writes on another thread's
instance sharing the default backend for the duration of the fsync; and when
`changed` is emitted on a second instance relative to the backend write. Each
entry SHALL be pinned by an isolated `gtk-lush-axioms` probe with a sample
that has an interactive and a `--check` mode, running against a private
keyfile directory, and the settings writer's flush path SHALL be chosen from
the recorded outcomes.

#### Scenario: Backend axioms are probed
- **WHEN** `make gtk-axioms` runs
- **THEN** each GSettings backend axiom's probe asserts its recorded behaviour in isolation and `make check-gtk-axioms` finds the ledger, catalogue, probes, and samples in agreement

#### Scenario: A GLib update changes backend behaviour
- **WHEN** a probe for a GSettings backend axiom fails after a GLib update
- **THEN** the change is reviewed against the settings writer's flush path before the update is accepted
