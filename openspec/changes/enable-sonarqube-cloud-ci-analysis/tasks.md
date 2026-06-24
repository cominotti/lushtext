## 1. Scanner Configuration

- [x] 1.1 Add `sonar-project.properties` for `sonar.organization=cominotti` and `sonar.projectKey=cominotti_lushtext`.
- [x] 1.2 Define a code-focused Sonar source scope for Rust crates, scripts, workflow files, and active build/configuration files.
- [x] 1.3 Add exclusions for generated artifacts, target directories, Flatpak/Meson build outputs, local agent worktree caches, smoke/proof outputs, and archived OpenSpec changes.
- [x] 1.4 Leave `sonar.issue.ignore.multicriteria` absent unless the first real scan produces a specific finding that is fixed or narrowly justified.
- [x] 1.5 Add `.sonar/` report output to `.gitignore`.

## 2. GitHub Actions Analysis

- [x] 2.1 Add a bounded SonarQube Cloud CI job or workflow for trusted `main` pushes and trusted pull requests.
- [x] 2.2 Configure checkout with `fetch-depth: 0` for scanner relevance.
- [x] 2.3 Install Fedora Rust/GTK scanner prerequisites, including Cargo, Clippy, GTK4, Libadwaita, GtkSourceView, compiler, gettext, git, `gpg`, and `dirmngr`.
- [x] 2.4 Use the current official `SonarSource/sonarqube-scan-action` release or pinned SHA, not the deprecated SonarCloud action.
- [x] 2.5 Pass `SONAR_TOKEN` from GitHub secrets and skip scanner upload with an explicit notice when the secret is unavailable.
- [x] 2.6 Enable bounded post-scan Sonar verification so uploaded-analysis issues or a real quality gate `ERROR` fail the Sonar job without hiding core lint/test diagnostics.
- [x] 2.7 Preserve the existing blocking Clippy lint command unchanged in the normal lint lane.
- [x] 2.8 Update workflow path filters or timeout policy inputs if a dedicated Sonar workflow is added.

## 3. Local/API Verification

- [x] 3.1 Add `scripts/sonar-local.sh` adapted from the `invowk` pattern for `cominotti_lushtext` and CI-based analysis results.
- [x] 3.2 Make the script query SonarQube Cloud branch existence, quality gate status, unresolved issues, and analysis absence.
- [x] 3.3 Make `main` with no analysis data fail after enablement, while non-main branches with no analysis report an explicit no-data state.
- [x] 3.4 Write inspectable JSON reports under `.sonar/reports`.
- [x] 3.5 Print a reviewable unresolved-issue table with severity, type, rule, file/line, and message.
- [x] 3.6 Add `make sonar-local` plus Makefile help/environment documentation for `SONAR_TOKEN`, `SONAR_HOST_URL`, `SONAR_PROJECT_KEY`, `SONAR_BRANCH`, and `SONAR_PAGE_SIZE`.

## 4. First Scan Triage

- [x] 4.1 Run or trigger the first trusted SonarQube Cloud analysis and confirm the `cominotti_lushtext` project records an analysis.
- [x] 4.2 Fetch the unresolved issue list and quality gate state for the analyzed ref.
- [x] 4.3 Fix each actionable finding in code or configuration.
- [x] 4.4 For any false-positive or intentional-pattern finding, add a narrow documented suppression or record the accepted/false-positive state in SonarQube Cloud.
- [x] 4.5 Re-run analysis or verification until SonarQube Cloud records the target ref with zero unreviewed unresolved issues and no real quality gate `ERROR`.

## 5. Validation and Documentation

- [x] 5.1 Run `openspec validate enable-sonarqube-cloud-ci-analysis --strict`.
- [x] 5.2 Run `openspec validate --changes --strict`.
- [x] 5.3 Run `make check-workflow-timeouts` after workflow changes.
- [x] 5.4 Run `make check-agent-docs` if agent guidance, Makefile target lists, or policy docs change.
- [x] 5.5 Run `shellcheck scripts/sonar-local.sh` if available, or document why shellcheck was unavailable. (`shellcheck` was unavailable; `bash -n scripts/sonar-local.sh` passed.)
- [x] 5.6 Run `make sonar-local` against the analyzed branch after SonarQube Cloud has data.
- [x] 5.7 Verify GitHub Actions reports the Sonar job as success, skipped with an explicit no-secret reason, or failed for a real quality issue.
