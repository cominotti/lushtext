## MODIFIED Requirements

### Requirement: Markdown preview rendering is bounded across planning and projection
The system SHALL render Markdown preview through a generation-owned GTK-free plan and bounded GTK projection slices. Automatic rendering MUST enforce deterministic source, event/node, embed, and per-slice work budgets, and MUST expose a limited or paused terminal state when an input exceeds a global budget.

A block larger than one projection slice MUST NOT stop planning. Planning SHALL cut a projection batch only where no inline state is open, MAY cut inside a block at such a checkpoint, and MUST emit an accessible omission marker and continue planning when the smallest unit delimited by those checkpoints still exceeds a per-slice budget. An omission MUST be scoped to that overflowing unit: a unit inside an open container MUST be replaced within that container while its sibling units still render, and only a top-level block with no interior checkpoint MAY be replaced whole. Every emitted batch MUST stay within the per-slice event and byte budgets. Cross-turn projection state SHALL be bounded by structural nesting depth plus at most one in-flight embedded-block buffer charged against a named retention ceiling for that block's kind, and MUST NOT grow with document size. A retention ceiling MUST NOT change which presentation a projection-side widget budget selects for an embedded block, so each ceiling MUST be expressed in the same unit as the widget budget governing that kind of block. Global source, event, retained-byte, embed-descriptor, structural-depth, and inline-footnote budgets MUST remain terminal stop conditions, and an omitted unit's parsed work MUST still be charged to them.

#### Scenario: Dense Markdown exceeds the global event budget
- **WHEN** a document is small enough for automatic preview but expands into more render events or GTK nodes than the global render-event budget
- **THEN** planning terminates at that global budget with an explicit limited preview state
- **AND** GTK does not build the unbounded remainder in one callback

#### Scenario: Accepted plan needs many GTK nodes
- **WHEN** a current render plan contains more nodes than one projection slice permits
- **THEN** GTK applies it over bounded main-loop turns
- **AND** input, repaint, and other completions can run between slices

#### Scenario: One block exceeds a projection slice
- **WHEN** a single top-level block needs more events or bytes than one projection slice permits
- **THEN** planning continues past that block instead of discarding the rest of the document
- **AND** every following block is planned and projected

#### Scenario: An oversized block has an inline-safe checkpoint
- **WHEN** an oversized block can be cut at a point where only block containers are open
- **THEN** the block is projected across several bounded turns
- **AND** each emitted batch stays within the per-slice event and byte budgets
- **AND** no batch boundary splits an open inline span

#### Scenario: One unit inside an open container overflows
- **WHEN** a single row, list item, code-block text run, quoted paragraph, or definition body exceeds a per-slice budget and has no interior checkpoint
- **THEN** only that unit is replaced by one accessible omission marker inside the still-open container
- **AND** the container's other units are still rendered in source order
- **AND** planning continues after the container closes

#### Scenario: A top-level block has no inline-safe checkpoint
- **WHEN** a top-level block such as one very dense paragraph or heading exceeds a per-slice budget and contains no point where only block containers are open
- **THEN** that block is replaced by one accessible omission marker
- **AND** planning continues with the following block
- **AND** the terminal state reports a complete preview with a count of omissions rather than a stopped preview

#### Scenario: An open embedded block crosses its retention ceiling
- **WHEN** an open embedded block would exceed the retention ceiling for its own kind — retained text bytes for a code block, or cell count for a table
- **THEN** planning stops retaining that block's remaining content at the crossing point and records the remainder as unretained for that block
- **AND** that record carries the unretained source byte and cell counts so projection-side widget budgets still evaluate the block's true total size
- **AND** it is treated as internal accounting rather than a user-visible omission: no marker is rendered for it, it does not count toward the reported omission total, and a plan whose only such records are these publishes the complete terminal
- **AND** batches already emitted for that block remain valid and unchanged
- **AND** planning continues after the block closes

#### Scenario: Continuation state crosses projection turns
- **WHEN** a projection turn ends inside a block that continues in a later turn
- **THEN** the retained continuation holds only open-container descriptors, scalar flow state, and at most one charged in-flight embedded-block buffer
- **AND** the following turn validates the continuation it holds against the continuation the next batch expects
- **AND** a mismatch resolves to an explicit terminal state instead of corrupted rendered content

#### Scenario: Preview generation changes during projection
- **WHEN** the document, preview mode, or page lifetime changes while a plan or projection session is active
- **THEN** all later work owned by the stale generation is discarded
- **AND** it cannot insert widgets, tags, placeholders, or terminal state into the newer preview
- **AND** any retained continuation buffer is released without freeing document-sized text on the GTK thread

#### Scenario: A global budget is exceeded
- **WHEN** source bytes, retained events, retained bytes, embed descriptors, structural depth, or inline-footnote expansion exceeds its global budget
- **THEN** planning stops at that budget with the existing limited or paused terminal state
- **AND** the reported reason names that global budget rather than a per-unit omission
