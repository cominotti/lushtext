## ADDED Requirements

### Requirement: The Replace All per-file journal protocol is proved
Kani shard `core-replace-protocol` SHALL prove the I/O-free protocols of `services/content_search/replace/protocol.rs` over every outcome sequence and crash point, composed with an abstract durable write that assumes the existing write-protocol theorems.

#### Scenario: A crash at any point
- **WHEN** the process stops at any step of an apply, an Undo or a cleanup
- **THEN** the proof shows the target holds its original or its replaced bytes, and replaced implies an active entry whose body holds the original

#### Scenario: Undo over a diverged file
- **WHEN** the current file's digest equals neither the original nor the replaced digest
- **THEN** the proof shows Undo writes nothing to it

#### Scenario: A reordered cleanup is caught
- **WHEN** a mutation makes cleanup delete a body before its entry
- **THEN** `core-replace-protocol` reports a counterexample in which an entry references a missing body

### Requirement: The chunked Replace All engine is proved split-independent
Kani shard `core-replace-stream` SHALL prove that, on inputs of at most 8 bytes, chunk sizes 1 to 4 and at most 2 edits, the streaming engine's output, outcome and digested input equal the whole-buffer reference's, observed through a recording sink and the byte-loop UTF-8 path. Both new shards SHALL have measured runner budgets in the shard table, and `write_protocol.rs` and its shards SHALL be unchanged.

#### Scenario: A CRLF split across chunks
- **WHEN** a chunk boundary falls between the `\r` and `\n` of a targeted line
- **THEN** the proof covers it and the streamed result equals the reference
