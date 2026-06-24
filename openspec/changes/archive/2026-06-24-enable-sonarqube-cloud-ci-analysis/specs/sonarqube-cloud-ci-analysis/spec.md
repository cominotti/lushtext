## ADDED Requirements

### Requirement: CI uploads SonarQube Cloud analysis for trusted refs
The project SHALL run CI-based SonarQube Cloud analysis for the existing `cominotti_lushtext` project on trusted main-branch pushes and trusted pull-request updates when `SONAR_TOKEN` is available. The scanner workflow MUST authenticate with `SONAR_TOKEN`, use full git history for analysis relevance, and submit results to the `cominotti` SonarQube Cloud organization.

#### Scenario: Main branch upload
- **WHEN** GitHub Actions runs for a push to `main` with `SONAR_TOKEN` available
- **THEN** the SonarQube Cloud scanner runs against project key `cominotti_lushtext`
- **AND** the scanner uploads an analysis for the current commit

#### Scenario: Trusted pull request upload
- **WHEN** GitHub Actions runs for a same-repository pull request with `SONAR_TOKEN` available
- **THEN** the SonarQube Cloud scanner runs for the pull request context
- **AND** the result is visible as SonarQube Cloud analysis feedback for that ref

#### Scenario: Untrusted pull request skip
- **WHEN** GitHub Actions runs for a fork pull request or another context where `SONAR_TOKEN` is unavailable
- **THEN** the SonarQube Cloud scanner does not attempt an unauthenticated upload
- **AND** the workflow emits an explicit skip message instead of failing because the secret is absent

### Requirement: Rust analysis uses the repository's GTK-capable CI environment
The SonarQube Cloud scanner lane SHALL provide the Rust and GTK build prerequisites needed for Sonar's Clippy-based Rust analysis. The lane MUST install Cargo, Clippy, and the same GTK development libraries required by the existing Fedora lint job before invoking the scanner.

#### Scenario: Scanner can run Rust analysis
- **WHEN** the SonarQube Cloud scanner lane starts Rust analysis
- **THEN** `cargo` and `cargo clippy` are available on `PATH`
- **AND** GTK4, Libadwaita, GtkSourceView, compiler, gettext, and git dependencies needed by LushText's Rust workspace are installed

#### Scenario: Existing blocking Clippy remains authoritative
- **WHEN** the SonarQube Cloud scanner runs Clippy as part of analysis
- **THEN** the existing blocking Clippy command remains unchanged in the normal lint gate
- **AND** SonarQube Cloud analysis does not replace `cargo clippy --workspace --all-targets --all-features -- -D warnings`

### Requirement: Scanner configuration is code-focused and excludes generated artifacts
The project SHALL provide SonarQube Cloud scanner configuration in `sonar-project.properties`. The configuration MUST identify the `cominotti` organization and `cominotti_lushtext` project key, include source surfaces intended for analysis, and exclude generated output, build caches, local worktree caches, vendored or archived planning artifacts, and smoke/proof artifacts that are not maintained source.

#### Scenario: Project identity is configured
- **WHEN** the scanner reads repository configuration
- **THEN** it finds `sonar.organization=cominotti`
- **AND** it finds `sonar.projectKey=cominotti_lushtext`

#### Scenario: Generated artifacts are excluded
- **WHEN** the scanner resolves files for analysis
- **THEN** target directories, Flatpak/Meson build outputs, local agent worktree caches, smoke/proof output, and archived OpenSpec changes are excluded from maintained-source analysis
- **AND** source code, workflow files, scripts, and active build configuration remain eligible for analysis

#### Scenario: Automatic analysis configuration is not used as the primary path
- **WHEN** repository SonarQube Cloud configuration is reviewed
- **THEN** the primary scanner configuration is `sonar-project.properties`
- **AND** the change does not rely on `.sonarcloud.properties` or SonarQube Cloud automatic analysis for Rust

### Requirement: Quality gate and issue checks are locally inspectable
The project SHALL provide a local/API verification command that reports SonarQube Cloud quality gate status and unresolved issues for a selected branch. The command MUST write inspectable JSON reports, fail on quality gate errors, and fail when unresolved issues are present.

#### Scenario: Local check passes with clean uploaded analysis
- **WHEN** a developer runs the Sonar local/API verification command for a branch with uploaded analysis, an OK quality gate, and zero unresolved issues
- **THEN** the command exits successfully
- **AND** it writes quality gate and issue reports under the repository's ignored Sonar report directory

#### Scenario: Local check fails on unresolved issues
- **WHEN** SonarQube Cloud reports one or more unresolved issues for the selected branch
- **THEN** the local/API verification command exits unsuccessfully
- **AND** it prints the issue severity, rule, location, and message in a reviewable summary

#### Scenario: Local check fails when quality gate is unavailable
- **WHEN** the local/API verification command runs for a branch with uploaded analysis and no computed SonarQube Cloud quality gate status
- **THEN** the command exits unsuccessfully
- **AND** it reports that the quality gate has not been computed

#### Scenario: Main branch missing analysis is a blocker
- **WHEN** the local/API verification command runs for `main` after enablement and SonarQube Cloud has no analysis data for `main`
- **THEN** the command exits unsuccessfully
- **AND** it reports that the SonarQube Cloud project has not been populated

### Requirement: Sonar findings are fixed or explicitly justified
The project SHALL treat SonarQube Cloud findings as work items that must be fixed in code or explicitly accepted with a documented reason. Scanner configuration MUST NOT introduce blanket issue ignores before the first real LushText scan identifies concrete findings.

#### Scenario: First scan findings are triaged
- **WHEN** the first CI-based SonarQube Cloud analysis reports issues
- **THEN** each issue is either fixed in the implementation stream or documented as accepted/false-positive with a narrow rule and file scope
- **AND** the implementation does not leave unreviewed unresolved issues behind

#### Scenario: Suppressions stay narrow
- **WHEN** a SonarQube Cloud rule is intentionally ignored in scanner configuration
- **THEN** the ignore entry names the rule, resource scope, and project-specific rationale
- **AND** the ignore does not suppress unrelated files or future findings beyond the justified pattern
