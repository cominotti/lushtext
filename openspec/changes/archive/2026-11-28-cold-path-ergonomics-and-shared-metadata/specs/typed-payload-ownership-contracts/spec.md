## ADDED Requirements

### Requirement: Shared Arc values never hold unguarded document-sized payloads or enter policy proofs
Production code SHALL NOT store document-sized payloads in naked `Arc<String>` or `Arc<Vec<u8>>`; shared document-sized values SHALL be wrapped in `Arc<DisposalOwned<T>>` so terminal drops occur off the GTK main thread, and `Arc` SHALL NOT appear inside `ui/**/policy.rs` or Kani-checked proof cores.

#### Scenario: Folded note body uses Arc<DisposalOwned<String>>
- **WHEN** `ui/window/notes/install_execution.rs` shares a note body larger than `L_PROTECT_EDIT` across background analysis and UI callbacks
- **THEN** it wraps the body in `Arc<DisposalOwned<String>>` rather than naked `Arc<String>`, preventing main-thread document deallocation

#### Scenario: Adding naked Arc<String> or Arc in policy.rs fails policy check
- **WHEN** a production module introduces naked `Arc<String>` or imports `Arc` inside `ui/**/policy.rs` or a Kani-verified module
- **THEN** `make check-policy` fails naming the file and violation
