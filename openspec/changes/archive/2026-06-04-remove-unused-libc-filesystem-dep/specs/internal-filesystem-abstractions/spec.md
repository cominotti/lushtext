## MODIFIED Requirements

### Requirement: No-leftovers audit is deterministic
The implementation SHALL provide deterministic audit commands that fail when disallowed direct filesystem access appears outside the approved filesystem implementation and test fixture boundary. The audit MUST cover production code, tests, benches, root and nested guidance, rules, and skills. The audit MUST also fail when a raw-backend crate that the boundary controls, such as `libc`, is declared in a crate manifest but has no matching backend usage in that crate's source, so a backend dependency cannot linger after the operations that needed it move to another backend.

#### Scenario: Audit fails on a direct std filesystem call
- **WHEN** a production service outside the approved filesystem modules contains a direct `std::fs` call
- **THEN** the no-leftovers audit reports the file and line
- **AND** the implementation is not considered complete

#### Scenario: Audit allows only explicit backend and fixture exceptions
- **WHEN** the audit encounters raw filesystem calls in approved backend or fixture modules
- **THEN** those occurrences are allowed only when documented by the audit allowlist
- **AND** every other occurrence is treated as a migration leftover

#### Scenario: Audit fails on a declared-but-unused raw backend dependency
- **WHEN** a crate manifest declares a controlled raw-backend dependency such as `libc` but no source file in that crate references it
- **THEN** the no-leftovers audit reports the unused backend dependency
- **AND** the implementation is not considered complete until the dependency is used or removed
