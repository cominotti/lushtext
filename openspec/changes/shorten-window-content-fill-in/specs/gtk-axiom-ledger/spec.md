## ADDED Requirements

### Requirement: The realize frame and the frame clock's pacing after an unpresented frame are a ledger axiom
The ledger SHALL record, with the next free id (A51 at the time of writing; A42–A45 belong to
`document-scale-policy` and A46–A50 to `first-frame-window-shell`), that:
- realizing a toplevel schedules a frame on its frame clock whose paint idle runs ahead of
  `G_PRIORITY_DEFAULT_IDLE` sources;
- when the surface is not yet mapped, that frame runs its layout phase and emits `after-paint`
  but never receives presentation timings;
- the frame clock then starts the next frame no earlier than one `FRAME_INTERVAL` (16,667 µs)
  after that frame, because without presentation timings its refresh interval falls back to
  `FRAME_INTERVAL`;
- realizing and presenting in one dispatch runs no frame on the unmapped surface.

The entry SHALL be pinned by an isolated `gtk-lush-axioms` probe, with a sample that has an
interactive and a `--check` mode, recording each `after-paint`'s mapped state, frame counter,
frame time, and presentation time. The window-composition design SHALL cite the id.

#### Scenario: The realize-frame axiom is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the probe observes an unmapped `after-paint` before map when realize and present run in separate default-idle dispatches, and none when they run in one dispatch
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe, and sample in agreement

#### Scenario: A toolkit update stops running the realize frame
- **WHEN** the probe reports no unmapped `after-paint` with realize and present in separate dispatches after a GTK update
- **THEN** the composition's merged realize-and-present dispatch is reviewed before the probe is edited, and the ledger entry is revised
