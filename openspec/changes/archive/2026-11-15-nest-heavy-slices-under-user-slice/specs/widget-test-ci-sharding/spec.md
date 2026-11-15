## MODIFIED Requirements

### Requirement: Parallel local shards are isolated from each other
In a parallel local run, each shard SHALL run in its own admitted memory scope
(`scripts/with-memory-cap.sh` inside the host-wide heavy slice, capped by
`WIDGET_SHARD_MEMORY_MAX`), its own temporary-file scope
(`scripts/with-temp-scope.sh --session-runtime`), its own D-Bus session, and
its own headless compositor, with a distinct `XDG_RUNTIME_DIR` and Wayland
socket. A shard's session SHALL refuse to start when its Wayland socket path
would exceed 100 bytes. Every shard's scope SHALL fail the run with
`TEMP-LEAK:` when its shard leaves a temporary entry behind, as the serial run
does. When memory admission cannot yet admit a shard, that shard SHALL wait and
the run SHALL say so, rather than start uncapped. When the kernel OOM-kills any
process inside a shard's memory scope, that shard SHALL fail with `OOM-KILL:`
even if a retry recovered the test.

#### Scenario: Shards do not share a session
- **WHEN** a parallel local run is in flight
- **THEN** each shard's compositor listens on a socket inside its own runtime directory, and no two shards share a runtime directory, bus, or memory scope

#### Scenario: Overlong socket path is refused
- **WHEN** the resolved temporary directory would make a shard's Wayland socket path longer than 100 bytes
- **THEN** each shard fails before its session starts, the message names the temporary directory as the cause, and the run fails

#### Scenario: A leak is attributed to its shard
- **WHEN** a test in one shard leaves a temporary entry in its scope
- **THEN** that shard's scope reports `TEMP-LEAK:`, the summary marks that shard failed, and the run fails

#### Scenario: An out-of-memory kill is never green
- **WHEN** a process in one shard's memory scope is OOM-killed and the harness's retry then passes the test
- **THEN** that shard reports `OOM-KILL:`, the summary marks it failed, and the run fails

#### Scenario: Interrupted run leaves nothing behind
- **WHEN** a developer interrupts a parallel run with Ctrl-C
- **THEN** every shard's session is stopped and its runtime directory and scope are removed

#### Scenario: Shard scopes nest under the user manager's user.slice
- **WHEN** a parallel local run admits a shard's memory scope on a host with a systemd user manager
- **THEN** the scope's cgroup path lies under `user.slice/user-lushtext.slice/user-lushtext-heavy.slice`
