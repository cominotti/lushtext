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

### Requirement: The Replace All stream's UTF-8 seams are proved split-independent
Kani shard `core-replace-stream` SHALL prove that, fed in chunks of 1 to 4 bytes, the stream's incremental UTF-8 carry rejects exactly the sources of at most 8 arbitrary bytes that one validation of the whole source rejects, and that the byte-loop validation the harness runs on equals `core::str::from_utf8` on every input of at most 4 bytes. Both new shards SHALL have measured runner budgets in the shard table, and `write_protocol.rs` and its shards SHALL be unchanged.

#### Scenario: A character split across chunks
- **WHEN** a chunk boundary falls inside a multi-byte character
- **THEN** the proof shows the UTF-8 verdict is the one-shot verdict

### Requirement: The Replace All engine's line logic is checked exhaustively over a bounded domain
Because no Kani form of the engine that was tried fitted the shard margin (design D11, "Kani memory"), a unit test SHALL check the engine at every point of bounded domains. On a valid UTF-8 source, the chunked feed's outcome SHALL equal the whole feed's, and a replaced file's output, count and digests SHALL equal the whole feed's and the reference builder's. A source that is not valid UTF-8 SHALL never be replaced, at any chunk size.

#### Scenario: The bounded domain
- **WHEN** the check runs
- **THEN** it covers every source of at most 4 characters over `a`, `b`, `\r`, `\n` and `é`, every edit set of at most two edits over small line and replacement tables that include a recorded line holding `é`, every chunk size, and an unlimited and a binding output limit
- **AND** each case the retired engine harness covered (two edits applied, the limit stopping a file, a stale line, an unterminated final line replaced across a seam, a cut character inside a replaced targeted line) is reached at least once

#### Scenario: A CRLF split across chunks
- **WHEN** a chunk boundary falls between the `\r` and `\n` of a targeted line
- **THEN** the bounded check covers it and the streamed result equals the reference

#### Scenario: A mishandled seam is caught
- **WHEN** a mutant of the engine drops the earlier part of a targeted line split across chunks
- **THEN** the bounded check fails on the chunked feed's outcome

#### Scenario: An invalid source is never replaced
- **WHEN** the check feeds every source of at most 3 symbols over the valid alphabet plus the bytes `0x80`, `0xC3` and `0xFF`, with the same edit sets, chunk sizes and limits
- **THEN** no source that is not valid UTF-8 is `Replaced` at any chunk size, it ends `NotUtf8`, `StaleLine` or `OutputLimitExceeded`, and with no edit and no limit it ends `NotUtf8`
- **AND** a mutant whose end of stream skips the UTF-8 carry's final check fails the check
