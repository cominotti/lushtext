## MODIFIED Requirements

### Requirement: A committed inventory gates every integer constant
`docs/thresholds.md` SHALL inventory every integer or zero-cost unit (`Bytes`, `Sp`, `Micros`, `CharCount`) `const` and `static` in production `src/` trees (excluding `#[cfg(test)]`, `kani_proofs.rs`, and `gtk-lush-axioms` probe parameters). `make check-thresholds` SHALL fail on missing/stale rows, mismatched values, invalid non-powers-of-two, lowered presentation caps, or missing/disagreeing `Kind:` rustdoc, proved by `--self-test`.

#### Scenario: A new threshold must be classified
- **WHEN** a change adds `const MAX_THINGS: usize = 1000;` to a production module without an inventory row
- **THEN** `make check-thresholds` fails, naming the constant

#### Scenario: A non-power-of-two cap is refused
- **WHEN** an inventory row classifies a constant of value 10,000 as a cap
- **THEN** the check fails until the value is 8,192 or the row is a recorded exception with its reason

#### Scenario: A presentation cap rounded down is refused
- **WHEN** an inventory row classifies a constant of value 8,192 as a presentation cap whose recorded former value is 10,000
- **THEN** the check fails until the value is 16,384 or the row is a recorded exception at 10,000

#### Scenario: Same-named constants in different modules are distinct rows
- **WHEN** two inline modules of one file each declare `const JSON: u64` with different values
- **THEN** the inventory has one row per module-qualified name, and a value change to either fails the check against its own row

#### Scenario: An out-of-scope constant needs only its class
- **WHEN** a constant is UI geometry, a percentage, a GLib priority or a fixture size
- **THEN** its row names that class and the check accepts any value

#### Scenario: A typed unit constant is verified identically to a bare integer
- **WHEN** a module declares `pub const CAP: Bytes = Bytes::kib(64);` or `pub const MIN_W: Sp = Sp::new(280);`
- **THEN** `make check-thresholds` extracts its underlying integer value (`65536` / `280`) and enforces the same `docs/thresholds.md` and `Kind:` rules
