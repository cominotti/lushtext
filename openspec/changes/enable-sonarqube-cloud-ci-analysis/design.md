## Context

LushText has a public SonarQube Cloud project at `cominotti_lushtext`, but the project currently has no recorded analyses and no Rust quality profile attached. SonarQube Cloud automatic analysis cannot analyze Rust projects, so the existing project shell will remain empty until GitHub Actions runs a CI-based scanner.

The repository's primary CI jobs already run in Fedora 44 containers because the GTK4/Libadwaita/GtkSourceView stack required by `gtk4-rs` is newer than Ubuntu LTS packages. SonarQube Cloud's Rust analyzer integrates with Clippy, so the scanner lane must have Cargo, Clippy, and the same GTK development dependencies available before analysis starts.

`invowk` provides a useful local/API verification precedent: a `sonar-project.properties` file, a `make sonar-local` target, and a script that fails on quality gate errors or unresolved issues. LushText should reuse that operational pattern, but not the automatic-analysis premise behind `invowk`'s setup.

## Goals / Non-Goals

**Goals:**

- Upload real LushText analysis to SonarQube Cloud for the existing `cominotti_lushtext` project.
- Keep the scanner lane deterministic enough for pull requests and main-branch pushes.
- Preserve the existing Fedora/Rust/GTK dependency assumptions needed for Clippy-based Rust analysis.
- Add a local/API check that reports quality gate status and unresolved Sonar issues after analysis exists.
- Make first-scan triage explicit: findings are fixed in code or deliberately accepted/ignored with a documented reason.

**Non-Goals:**

- Do not enable SonarQube Cloud automatic analysis for LushText.
- Do not import Rust coverage initially; LushText does not currently produce LCOV or Cobertura in CI.
- Do not add blanket issue suppressions before the first real scan.
- Do not make Sonar a replacement for rustfmt, Clippy `-D warnings`, cargo-deny, widget tests, Flatpak validation, or existing policy gates.
- Do not change app runtime behavior.

## Decisions

### 1. Use CI-based analysis, not automatic analysis

Rust is excluded from SonarQube Cloud automatic analysis, so LushText will use a GitHub Actions scanner workflow. The workflow will authenticate with `SONAR_TOKEN`, check out with full history (`fetch-depth: 0`), and submit to `sonar.organization=cominotti` / `sonar.projectKey=cominotti_lushtext`.

Alternative considered: add `.sonarcloud.properties` and rely on the GitHub App. Rejected because that configuration only refines automatic analysis, and automatic analysis cannot process Rust for this project.

### 2. Run Sonar in a dedicated CI job or workflow

Sonar analysis should be isolated from the main lint job so scanner availability, SonarCloud processing, or missing secrets do not obscure the existing Rust/GTK lint diagnostics. The job should still use Fedora and install the GTK/Rust prerequisites required for Clippy-based analysis.

The implementation may place the job in `.github/workflows/ci.yml` or a dedicated `sonarqube.yml`, but it must keep a bounded timeout and participate in normal push/PR feedback when secrets are available.

Alternative considered: append the scan to the existing `Lint` job. Rejected because that job is already the fast local-equivalent lint contract, while Sonar adds external service, token, and quality-gate timing concerns.

### 3. Use the official SonarQube scan action, pinned intentionally

Use the current official `SonarSource/sonarqube-scan-action` rather than the deprecated SonarCloud action. The action release/tag or SHA should be chosen during implementation from the then-current official release and recorded in the workflow. The action runs the SonarScanner CLI and accepts additional scanner arguments, including quality-gate waiting.

The scanner job must account for the action's container/self-hosted prerequisites, especially `gpg` and `dirmngr` for scanner signature verification when running inside Fedora.

Alternative considered: manually download and invoke SonarScanner CLI. Keep this as fallback if the action cannot operate cleanly in the Fedora job, but prefer the official action for maintenance and alignment with SonarQube Cloud docs.

Implementation note from the first trusted scan: the scanner successfully uploads LushText analysis, but SonarQube Cloud currently reports the project quality gate as not computed through the public quality-gate API while the scanner's built-in quality-gate wait exits as failed. CI therefore lets the scanner upload and then runs the repository's API verifier, which waits for the pushed commit SHA to appear in SonarQube Cloud and fails on real unresolved issues or a real quality-gate `ERROR` when Sonar exposes one.

### 4. Let Sonar run Clippy automatically for the initial scan

The first implementation should let the Sonar Rust analyzer run Clippy automatically. This avoids duplicate Clippy issue import and keeps the scanner config small. The repository's existing `cargo clippy --workspace --all-targets --all-features -- -D warnings` remains the blocking Rust lint gate.

Alternative considered: generate a Clippy JSON report and set `sonar.rust.clippyReport.reportPaths`. Defer this until there is a proven need, because Sonar warns that combining automatic Clippy with imported Clippy reports can duplicate issues.

### 5. Keep source scope code-focused and generated artifacts excluded

`sonar-project.properties` should include repository code/configuration surfaces that Sonar can analyze, while excluding generated artifacts, build outputs, vendored/cache directories, smoke artifacts, archived OpenSpec changes, and target directories. The first scan should not hide product code or CI scripts merely to get an empty dashboard.

The implementation should start without `sonar.issue.ignore.multicriteria`. If the first scan produces false positives or intentional-pattern findings, each suppression must name the rule, resource scope, and reason.

### 6. Add an API verification target modeled after invowk

Add a local `make sonar-local` target and script that query SonarQube Cloud for the selected branch, write reports under `.sonar/reports`, and fail on quality gate `ERROR` or unresolved issues. For public projects it may operate unauthenticated; when `SONAR_TOKEN` is present it should use the token.

Unlike `invowk`, the script documentation must say it verifies uploaded CI-based analysis results. It must treat `main` with no Sonar branch/analysis as a failure after enablement, while non-main branches with no analysis may report a clear no-data state.

## Risks / Trade-offs

- [Risk] `SONAR_TOKEN` is missing or unavailable on fork pull requests -> Mitigation: gate the scanner step/job so trusted pushes and same-repository PRs run analysis, while fork PRs skip with an explicit notice instead of failing for secret absence.
- [Risk] Sonar's automatic Clippy invocation fails because GTK development dependencies are missing -> Mitigation: run analysis in the Fedora environment and install the same GTK/Rust prerequisites used by existing CI lint lanes.
- [Risk] First real scan reports many issues -> Mitigation: keep initial suppressions empty, triage every reported issue, then either fix or add narrow documented ignores in the same work stream.
- [Risk] Quality-gate waiting increases CI duration or flakes on SonarCloud processing delays -> Mitigation: keep the job isolated from the core Rust lint/test diagnostics, and use the bounded `make sonar-local` verifier to wait for the uploaded revision and check issues/gate state after scanner upload.
- [Risk] Broad source scope analyzes generated or archived files -> Mitigation: maintain explicit exclusions and verify the first scan's component list before declaring the setup complete.
