## ADDED Requirements

### Requirement: Public-API advisory tools install reproducibly on the CI image
The advisory lane SHALL install `cargo-semver-checks` and `cargo-public-api` at pinned versions with `--locked` on the CI image's system libraries. When a pinned tool's own lockfile cannot build there, the lane MAY change only the named dependencies to pinned versions, from a checksum-verified crate, with every pin in the workflow `env`.

#### Scenario: OpenSSL 4 image
- **WHEN** the advisory job runs on `fedora:45` with OpenSSL 4.0.3
- **THEN** `cargo-public-api` 0.52.0 installs with `openssl-sys` 0.9.118 instead of its locked 0.9.88
- **AND** the job generates a non-empty public-API snapshot for every GTK Lush crate

#### Scenario: Tampered or drifted source
- **WHEN** the downloaded crate's SHA-256 differs from the pinned checksum
- **OR** the lockfile update changes an entry other than the named override and its minimal resolver consequences
- **THEN** the install step fails before building

#### Scenario: Override retired
- **WHEN** a newer `cargo-public-api` release locks an `openssl-sys` that supports the image's OpenSSL
- **THEN** the pin is raised and the override and its env entries are removed in the same change
