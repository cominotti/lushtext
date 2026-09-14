## ADDED Requirements

### Requirement: Repository agent guidance is reachable from every supported entry point
The repository SHALL keep each agent entry point resolving to the real guidance
tree. Where an entry point is a symbolic link, that link MUST resolve to an
existing file in the checkout. A dangling entry-point link is a silent total
failure: the agent loads nothing and reports nothing, so the guidance appears to
exist while no request has ever seen it.

Entry points MUST NOT be verified by reading the link text. Verification SHALL
follow the link, because a link whose text looks plausible and whose target does
not exist is exactly the state this requirement forbids.

#### Scenario: Entry-point link resolves to a real file
- **WHEN** the repository exposes agent guidance through a symbolic link such as
  `.claude/CLAUDE.md`
- **THEN** following that link reaches an existing file in the checkout
- **AND** a change that moves or renames the link's target repoints the link in
  the same change

#### Scenario: Dangling entry point is not evidence of loaded guidance
- **WHEN** an entry-point link names a path that does not exist
- **THEN** the guidance it claims to provide is treated as not loaded at all
- **AND** the defect is fixed rather than recorded as accepted debt

### Requirement: Rule files declare their scope in the format the agent honours
Every file under `.agents/rules/` SHALL declare its loading scope in the
frontmatter format the consuming agent actually parses. For Claude Code that key
is `paths:`, holding a YAML list of glob patterns; a file with no `paths:` key is
loaded unconditionally for every request.

A scoping key the agent does not recognise — notably Cursor's `globs:` — SHALL NOT
be used. Such a key does not narrow anything: the file loads globally while
reading as if it were scoped, so every reviewer and every future change is misled
about which requests the rule reaches. A rule intended to load for every request
SHALL say so explicitly, either by carrying no `paths:` key or by carrying the
repository's documented always-load marker, rather than by relying on a key the
agent ignored.

#### Scenario: Path-scoped rule uses the honoured key
- **WHEN** a rule file applies only to part of the tree
- **THEN** its frontmatter carries a `paths:` list of glob patterns covering that
  part
- **AND** it carries no unrecognised scoping key

#### Scenario: Global rule is explicit rather than accidental
- **WHEN** a rule governs every change regardless of the files touched
- **THEN** it omits `paths:` or carries the documented always-load marker
- **AND** its global scope is a stated decision rather than the side effect of an
  ignored key

#### Scenario: Unrecognised scoping key is a finding
- **WHEN** a rule file carries a `globs:` key
- **THEN** the guidance check fails and names the file
- **AND** the fix converts the key to an equivalent `paths:` list or removes it in
  favour of explicit global scope

### Requirement: The guidance check enforces the scoping contract mechanically
`make check-agent-docs` SHALL verify, for every `.agents/rules/*.md` file, that the
file either declares a non-empty `paths:` list of strings or is explicitly marked
global, and that no file carries an unrecognised scoping key. The check SHALL name
the offending file and key.

The check SHALL also reject a rule file that declares **both** a `paths:` list and
the always-load marker. Such a file states its scope twice and the two statements
disagree, so neither can be read as authoritative; the fix removes one of them
rather than deciding which one a reader should believe.

The check SHALL also reject a `paths:` glob that matches no tracked file. A glob
selecting nothing cannot fail, so a rename that strands one silently narrows the
rule to nothing while the check keeps passing — the inclusion-side blind spot of
scoping by glob. Where the tracked-file set cannot be determined, that half SHALL
be skipped rather than guessed, and the findings that do not depend on it SHALL
still be reported.

The check SHALL carry self-tests that prove each finding fires on a fixture that
violates it and that a conforming fixture passes, because a frontmatter check that
matches nothing does not fail — it passes while enforcing nothing.

#### Scenario: Check fails on an unscoped, unmarked rule
- **WHEN** a rule file has neither a `paths:` list nor the always-load marker
- **THEN** the check fails and names that file

#### Scenario: Check fails on a malformed paths list
- **WHEN** a rule file's `paths:` value is not a non-empty list of strings
- **THEN** the check fails and names that file

#### Scenario: Check fails on a scope stated twice
- **WHEN** a rule file carries a `paths:` list and the always-load marker together
- **THEN** the check fails and names that file
- **AND** either key alone on a conforming file produces no such finding

#### Scenario: Check fails on a dead paths glob
- **WHEN** a rule file's `paths:` list contains a glob matching no tracked file
- **THEN** the check fails and names the file and the glob
- **AND** a glob matching at least one tracked file produces no finding

#### Scenario: Unknown tracked set skips only the dead-scope half
- **WHEN** the tracked-file set cannot be determined
- **THEN** no dead-scope finding is reported
- **AND** the scope-unstated, unrecognised-key, malformed-list, and
  scope-stated-twice findings are still reported

#### Scenario: Self-test proves both directions
- **WHEN** the guidance check's self-test runs
- **THEN** it asserts that a violating fixture produces the finding
- **AND** it asserts that a conforming fixture produces none
