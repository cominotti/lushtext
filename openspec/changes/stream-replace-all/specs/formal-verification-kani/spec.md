## ADDED Requirements

### Requirement: The Replace All per-file journal protocol is proved
Kani shard `core-replace-protocol` SHALL prove the I/O-free protocols of `services/content_search/replace/protocol.rs` over every outcome sequence and crash point, composed with an abstract durable write that assumes the existing write-protocol theorems. It SHALL prove: the target is always original or replaced; replaced implies an active entry whose body holds the original; no entry references a missing body; undo writes only over the recorded replaced digest.

#### Scenario: A reordered cleanup is caught
- **WHEN** a mutation makes cleanup delete a body before its entry
- **THEN** `core-replace-protocol` reports a counterexample in which an entry references a missing body

### Requirement: The chunked Replace All engine is proved split-independent
Kani shard `core-replace-stream` SHALL prove that, on inputs of at most 8 bytes, chunk sizes 1 to 4 and at most 2 edits, the streaming engine's output, outcome and digested input equal those of the whole-buffer reference. Both new shards SHALL be in the shard table with measured runner budgets. `services/filesystem/write_protocol.rs` and its shards SHALL be unchanged.

#### Scenario: A CRLF split across chunks
- **WHEN** a chunk boundary falls between the `\r` and `\n` of a targeted line
- **THEN** the proof covers it and the streamed result equals the reference
