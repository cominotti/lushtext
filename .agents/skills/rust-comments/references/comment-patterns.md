# Comment Patterns

Use these patterns as shapes, not templates to paste mechanically. Name the
actual invariant in the changed code.

## Table of Contents

- [Module boundaries](#module-boundaries)
- [Public contracts](#public-contracts)
- [Safety and threading](#safety-and-threading)
- [Ordering and lifecycle](#ordering-and-lifecycle)
- [Policy values](#policy-values)
- [Noise and stale comments](#noise-and-stale-comments)

## Module boundaries

Useful when the path does not fully explain ownership:

```rust
//! Coordinates session restoration after draft recovery has identified the
//! recoverable tabs. Filesystem work stays in services; this module sequences
//! results on the GTK main thread.
```

Avoid a name-only module comment:

```rust
//! Session restoration helpers.
```

## Public contracts

Document effects the signature cannot express:

```rust
/// Saves the captured editor generation with a durable atomic replacement.
///
/// The callback runs on the GTK main thread. A newer editor generation remains
/// modified even when this older snapshot reaches disk successfully.
```

Skip rustdoc on an obvious private getter. Rename it if its meaning is unclear.

## Safety and threading

State the proof obligation, not merely that code is unsafe:

```rust
// SAFETY: initialization runs before the GTK main loop starts and before any
// worker thread can read or mutate the process environment.
unsafe { std::env::set_var("GSETTINGS_SCHEMA_DIR", schema_dir) };
```

For a background result:

```rust
// Recheck the generation on the main thread because edits may have arrived
// while the filesystem worker owned this snapshot.
```

Do not comment every `move` closure or `RefCell`; explain only the ownership or
threading consequence that is easy to violate.

## Ordering and lifecycle

Good comments identify what breaks if reordered:

```rust
// Disconnect before clearing the model: the callback closes over row state
// that becomes invalid once the list factory releases the item.
```

```rust
// Sync the parent after rename so a reported success also makes the new
// directory entry durable across power loss.
```

Avoid narration such as `// Now clear the model`.

## Policy values

Name ownership and the tradeoff:

```rust
/// Bounds automatic recovery memory while preserving a lazy marker for drafts
/// that can be loaded later through the serialized restore queue.
const RECOVERY_PRELOAD_LIMIT: u64 = 64 * 1024 * 1024;
```

Do not claim measurements, hardware guarantees, or upstream behavior without a
current source. When evidence lives in a benchmark or specification, reference
that stable artifact rather than copying a volatile result.

## Noise and stale comments

Remove comments that:

- restate an immediately visible operation;
- describe a type or function only by repeating its name;
- cite an old pull request instead of the current invariant;
- duplicate a numeric value that the code already names;
- explain standard Rust syntax;
- promise behavior the implementation no longer provides.

Before deleting a stale-looking comment, inspect the surrounding workflow and
tests. The implementation may be wrong and the comment may describe the real
contract.
