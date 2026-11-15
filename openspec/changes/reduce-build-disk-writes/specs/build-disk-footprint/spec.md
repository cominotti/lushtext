## ADDED Requirements

### Requirement: A single helper resolves the build cache root
The repository SHALL compute its build cache root in one helper. The root SHALL be `LUSHTEXT_BUILD_CACHE` when that is set, otherwise `lushtext-build` under `XDG_CACHE_HOME`, otherwise `lushtext-build` under `$HOME/.cache`. No repository script or Makefile target SHALL hardcode a home-relative cache path or compute the root itself.

#### Scenario: The cache root follows the environment
- **WHEN** `LUSHTEXT_BUILD_CACHE` is unset and `XDG_CACHE_HOME` is set
- **THEN** the root is `lushtext-build` under `XDG_CACHE_HOME`
- **AND** with `LUSHTEXT_BUILD_CACHE` set, the root is exactly that path

### Requirement: Each checkout gets its own recorded cache directory
The helper SHALL give each checkout a directory under the root, named after the checkout's basename and a hash of its real path. The directory SHALL record that real path and SHALL carry a `CACHEDIR.TAG`.

#### Scenario: Two checkouts get distinct directories
- **WHEN** the helper is asked for the checkout directory of two worktrees of the repository
- **THEN** it returns two different directories under the same root, each recording its own checkout's real path

### Requirement: Prune removes only orphaned checkout directories
A prune command SHALL remove the checkout directories whose recorded checkout path no longer exists, and SHALL remove nothing else.

#### Scenario: A removed worktree's directory is pruned
- **WHEN** prune runs while one recorded checkout has been removed and another still exists
- **THEN** only the removed checkout's directory is deleted, and the mutation slots and the live checkout's directory are kept

### Requirement: Local mutation runs use exclusive mirror slots in place
The local developer mutation commands SHALL run cargo-mutants in place in mirror slots under the build cache root. Each slot SHALL have its own fixed target and build directories. A slot SHALL be used by at most one run at a time, under an exclusive lock taken without waiting. A run that finds no free slot SHALL exit with status 75 and name each holder. The CI mutation modes SHALL keep their existing behaviour.

#### Scenario: A local run leaves no build directory behind
- **WHEN** a developer runs a local mutation command on a host whose Cargo configuration places build directories by workspace path
- **THEN** after the run no new build directory exists for any scratch copy, and the only build state the run used is inside its slots

#### Scenario: A warm slot does not rebuild dependencies
- **WHEN** a second local run uses a slot that a previous run already built, with no dependency change
- **THEN** no dependency crate is recompiled, and only the workspace members whose files differ are rebuilt

#### Scenario: No free slot fails fast
- **WHEN** every slot in the pool is held by another run
- **THEN** the run exits with status 75 without waiting, naming each holder's process and checkout

### Requirement: A mirror slot holds the checkout's current files
Before a run, a slot SHALL receive the checkout's tracked and untracked, non-ignored files, uncommitted edits included, and never the VCS directory. A file SHALL be rewritten only when its bytes differ from the checkout's. Files absent from the checkout SHALL be removed from the slot. Mutants SHALL be applied only in the slot.

#### Scenario: Uncommitted edits are tested without being mutated in the checkout
- **WHEN** the checkout has uncommitted edits and a local mutation run starts
- **THEN** the slot contains those edits, the mutants are applied in the slot, and the checkout's files are never modified

#### Scenario: An interrupted run heals on the next sync
- **WHEN** a run is interrupted while a mutated file is in place in a slot
- **THEN** the next sync of that slot restores the file to the checkout's bytes

### Requirement: Parallel local mutation runs shard across slots and merge one report
A local run on more than one slot SHALL give each slot a disjoint shard of the mutants. It SHALL merge the slots' reports into the checkout's `mutants.out`, with the same files a single run produces. Its exit status SHALL follow cargo-mutants' own precedence.

#### Scenario: Parallel slots produce one report
- **WHEN** a local run uses three slots
- **THEN** each slot tests a disjoint shard, and the checkout's `mutants.out` lists every mutant's outcome exactly once

### Requirement: Lanes that own their build directory disable incremental compilation
The local mutation slots, both proof-strength arms and the Kani lane SHALL build with incremental compilation disabled. A lane whose recorded measurement shows its median per-unit wall time regressing by more than 25% SHALL instead leave the setting unset. No lane SHALL export `CARGO_INCREMENTAL=1`.

#### Scenario: A mutation slot writes no incremental cache
- **WHEN** a local mutation run builds a mutant
- **THEN** no incremental compilation directory is written under the slot, unless the recorded decision kept incremental for that lane

### Requirement: Shared build directories keep their incremental setting
A lane that builds in a build directory shared with ordinary builds SHALL NOT change `CARGO_INCREMENTAL`.

#### Scenario: The ordinary lanes do not toggle incremental
- **WHEN** the widget tests, Clippy or the ordinary test lane build in the checkout's build directory
- **THEN** none of them changes `CARGO_INCREMENTAL`, so none forces a recompilation of workspace members

### Requirement: Mutated workspace crates bypass the compiler cache
The local mutation slots and the proof-strength tests arm SHALL compile workspace members without the compiler cache. Dependencies MAY still use it.

#### Scenario: Mutated crates do not enter the compiler cache
- **WHEN** a local mutation run builds a mutant on a host whose rustc wrapper uses sccache
- **THEN** the mutated workspace crate is compiled directly, and the sccache cache does not grow by that crate's artifacts

### Requirement: Proof-strength keeps its worktree in the cache root
The proof-strength driver's default disposable worktree, and the Kani build directory it pins inside it, SHALL be under the checkout's directory in the build cache root. Per-mutant results SHALL stay in the checkout's `target/proof-strength/results`. The tests arm SHALL run cargo-mutants in place inside that clean worktree, with its build and target directories pinned inside it.

#### Scenario: Proof strength writes no build tree into the checkout
- **WHEN** a maintainer runs the proof-strength lane for one module
- **THEN** its worktree and every build directory it uses are under the build cache root, its results are under `target/proof-strength/results`, and no new build directory is left elsewhere

### Requirement: Flatpak builder state lives in the cache root
Every repository `flatpak-builder` invocation SHALL pass a state directory under the checkout's directory in the build cache root. The local and release Flatpak app directories SHALL be on the same filesystem subvolume as that state directory.

#### Scenario: A Flatpak build keeps no state in the checkout
- **WHEN** a maintainer runs `make flatpak`
- **THEN** no `.flatpak-builder` directory is created or grown inside the checkout, and the build succeeds without cross-device link errors
