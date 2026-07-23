## ADDED Requirements

### Requirement: File-load transient ownership spans decoded-body disposal
Byte-weighted file-load admission SHALL be paired with a measured future-disposal reservation for every successful decoded body that may cross onto GTK. Transient load permits MUST remain exact through planning and installation, while decoded-body disposal ownership MUST remain attached independently until accepted baseline transfer or off-GTK terminal retirement. When disposal capacity is unavailable, the runtime MUST retain only bounded compact load intent and MUST NOT publish an unreserved document-sized body to GTK.

#### Scenario: Successful large decode awaits GTK installation
- **WHEN** worker-side decoding produces a large supported UTF-8 body
- **THEN** the result reaches GTK only after its future disposal is reserved and shrunk to the measured body weight
- **AND** transient load accounting still charges the admitted operation until installation reaches a terminal outcome

#### Scenario: Disposal capacity is unavailable at load admission
- **WHEN** ordinary disposal ownership cannot reserve the conservative decoded-body bound
- **THEN** the load coordinator retains no unreserved decoded body on GTK
- **AND** later progress retries only bounded compact request state under the existing load concurrency policy

#### Scenario: Direct installation does not retain a baseline
- **WHEN** a current decoded body is installed directly but local-history policy does not keep its clean text
- **THEN** load admission completes exactly once
- **AND** the guarded source retires on a disposal worker after GTK no longer needs it

#### Scenario: Repeated tab churn releases both ownership domains
- **WHEN** large tabs repeatedly load, cancel, reload, and close
- **THEN** transient byte permits and disposal job/byte reservations each return to their pre-run levels
- **AND** neither domain relies on the other's counter as a substitute for exact release
