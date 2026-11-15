## ADDED Requirements

### Requirement: A single helper resolves the build cache root
The repository SHALL compute its build cache root in one helper. The root SHALL be `LUSHTEXT_BUILD_CACHE` when that is set, otherwise `lushtext-build` under `XDG_CACHE_HOME`, otherwise `lushtext-build` under `$HOME/.cache`. No repository script or Makefile target other than the helper SHALL compute the build cache root or name a build-cache location under the home directory. A policy audit SHALL fail when one does.

#### Scenario: The cache root follows the environment
- **WHEN** `LUSHTEXT_BUILD_CACHE` is unset and `XDG_CACHE_HOME` is set
- **THEN** the root is `lushtext-build` under `XDG_CACHE_HOME`
- **AND** with `LUSHTEXT_BUILD_CACHE` set, the root is exactly that path

#### Scenario: Resolving a location creates nothing
- **WHEN** a caller asks the helper for a checkout's directory path without asking it to create the directory
- **THEN** the path is printed and no directory or file is created under the root

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
The local developer mutation commands SHALL run cargo-mutants in place in mirror slots under the build cache root. Each slot SHALL have its own fixed target and build directories. A slot SHALL be used by at most one run at a time, under an exclusive lock taken without waiting. A run that finds no free slot SHALL exit with status 75 and name each process that holds a slot's lock. The CI mutation modes SHALL keep their existing behaviour.

#### Scenario: A local run leaves no build directory behind
- **WHEN** a developer runs a local mutation command on a host whose Cargo configuration places build directories by workspace path
- **THEN** after the run no new build directory exists for any scratch copy, and the only build state the run used is inside its slots

#### Scenario: A warm slot does not rebuild dependencies
- **WHEN** a second local run uses a slot that a previous run already built, with no dependency change
- **THEN** no dependency crate is recompiled, and only the workspace members whose files differ are rebuilt

#### Scenario: No free slot fails fast
- **WHEN** every slot in the pool is held by another run
- **THEN** the run exits with status 75 without waiting, naming each holder's process and checkout

### Requirement: A slot lock is held only by its run
A slot's lock SHALL NOT be inherited by the processes the run starts, so it is released when the run exits.

#### Scenario: A daemon left by a build does not keep a slot
- **WHEN** a local run exits while a process it started, such as a compiler-cache server, keeps running
- **THEN** that slot's lock is free for the next run

### Requirement: Mirror slots stay within a size bound
A slot whose build and target directories together exceed the configured bound SHALL be emptied before it is used.

#### Scenario: An oversized slot is emptied before use
- **WHEN** a run acquires a slot whose build and target directories exceed the bound
- **THEN** the run empties them before the sync and says so

### Requirement: A mirror slot holds the checkout's current files
Before a run, a slot SHALL receive the checkout's tracked and untracked, non-ignored files, uncommitted edits included, and never the VCS directory. A file SHALL be rewritten only when its bytes differ from the checkout's. Files absent from the checkout SHALL be removed from the slot. Mutants SHALL be applied only in the slot.

#### Scenario: Uncommitted edits are tested without being mutated in the checkout
- **WHEN** the checkout has uncommitted edits and a local mutation run starts
- **THEN** the slot contains those edits, the mutants are applied in the slot, and the checkout's files are never modified

#### Scenario: An interrupted run heals on the next sync
- **WHEN** a run is interrupted while a mutated file is in place in a slot
- **THEN** the next sync of that slot restores the file to the checkout's bytes

### Requirement: A mirror slot keeps the checkout's file kinds
A slot file's executable bit SHALL match the checkout's. A symlink SHALL be mirrored as a symlink with the same target, never followed, and a symlink whose target resolves outside the slot SHALL be refused.

#### Scenario: Symlinks and executable files keep their kind
- **WHEN** the checkout tracks a directory symlink and an executable script
- **THEN** the slot holds a symlink with the same target and an executable file, and nothing is written through the symlink

### Requirement: Parallel local mutation runs shard across slots and merge one report
A local run on more than one slot SHALL give each slot a disjoint shard of the mutants. The run SHALL merge the slots' reports into the checkout's `mutants.out`, with the same files a single run produces, keeping every log and diff of every slot under a name no other slot can produce. Its exit status SHALL follow cargo-mutants' own precedence.

#### Scenario: Parallel slots produce one report
- **WHEN** a local run uses three slots
- **THEN** each slot tests a disjoint shard, and the checkout's `mutants.out` lists every mutant's outcome exactly once
- **AND** every outcome's log path names a file present in the merged report, with one baseline log per slot

### Requirement: Slot runs are sharded unambiguously
A mirror-mode run SHALL NOT pass a jobs count to cargo-mutants. A user shard request combined with more than one slot SHALL be refused. A slot whose shard holds no mutants SHALL contribute nothing and SHALL NOT fail the merge.

#### Scenario: A user shard with several slots is refused
- **WHEN** a developer sets a shard identity and the run would use more than one slot
- **THEN** the run exits with an error before acquiring a slot or building anything

#### Scenario: An empty shard does not fail the merge
- **WHEN** a slot's shard holds no mutants
- **THEN** that slot writes no outcomes, and the merged report and exit status come from the other slots

### Requirement: Lanes that own their build directory disable incremental compilation
The local mutation slots and both proof-strength arms SHALL build with incremental compilation disabled. The Kani lane SHALL do so when its recorded measurement shows that it writes an incremental cache into a build tree no ordinary build uses. A lane whose recorded measurement shows its median per-unit wall time regressing by more than 25% SHALL instead leave the setting unset. No lane SHALL export `CARGO_INCREMENTAL=1`, and a policy audit SHALL fail when one does.

#### Scenario: A mutation slot writes no incremental cache
- **WHEN** a local mutation run builds a mutant
- **THEN** no incremental compilation directory is written under the slot, unless the recorded decision kept incremental for that lane

### Requirement: Shared build directories keep their incremental setting
A lane whose compilation units coincide with those of ordinary builds in a shared build directory SHALL NOT change `CARGO_INCREMENTAL`. A policy audit SHALL fail when the widget, test, Clippy or dev-build targets set it.

#### Scenario: The ordinary lanes do not toggle incremental
- **WHEN** the widget tests, Clippy or the ordinary test lane build in the checkout's build directory
- **THEN** none of them changes `CARGO_INCREMENTAL`, so none forces a recompilation of workspace members

### Requirement: Mutated workspace crates bypass the compiler cache
The local mutation slots and the proof-strength tests arm SHALL compile workspace members without the compiler cache. Dependencies MAY still use it.

#### Scenario: Mutated crates do not enter the compiler cache
- **WHEN** a local mutation run builds a mutant on a host whose rustc wrapper uses sccache
- **THEN** the mutated workspace crate is compiled directly, and the sccache cache does not grow by that crate's artifacts

### Requirement: Proof-strength keeps its worktree in the cache root
The proof-strength driver's default disposable worktree, and the Kani build directory it pins inside it, SHALL be under the checkout's directory in the build cache root. Per-mutant results SHALL stay in the checkout's `target/proof-strength/results`. The tests arm SHALL run cargo-mutants in place inside that clean worktree, without a jobs count, with its build and target directories pinned inside it. Importing or self-testing the driver SHALL create nothing in the build cache root.

#### Scenario: Proof strength writes no build tree into the checkout
- **WHEN** a maintainer runs the proof-strength lane for one module
- **THEN** its worktree and every build directory it uses are under the build cache root, its results are under `target/proof-strength/results`, and no new build directory is left elsewhere

### Requirement: Flatpak builder state lives in the cache root
Every repository `flatpak-builder` invocation SHALL pass an explicit state directory on the same mount and subvolume as its app directory. The Makefile's Flatpak targets and the release validation build SHALL place both under the checkout's directory in the build cache root. The Cominotti repository generator SHALL place its state directory beside its app directory, under its output directory.

#### Scenario: A Flatpak build keeps no state in the checkout
- **WHEN** a maintainer runs `make flatpak`
- **THEN** no `.flatpak-builder` directory is created or grown inside the checkout, and the build succeeds without cross-device link errors

#### Scenario: The repository generator keeps its state beside its app dir
- **WHEN** the Cominotti repository generator runs with an output directory
- **THEN** `flatpak-builder` receives a state directory under that output directory, next to the app directory
