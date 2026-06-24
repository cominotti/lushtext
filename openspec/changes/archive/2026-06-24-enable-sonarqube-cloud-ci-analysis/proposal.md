## Why

LushText already has a public SonarQube Cloud project, but the project has no recorded analyses, so the apparent zero-issue state is only an unscanned state. Rust is not eligible for SonarQube Cloud automatic analysis, so the repository needs an explicit CI-based scanner lane before Sonar findings can become actionable.

## What Changes

- Add CI-based SonarQube Cloud analysis for the existing `cominotti_lushtext` project.
- Add repository scanner configuration for LushText's Rust workspace, scripts, workflows, and generated-artifact exclusions.
- Add a local/API verification path, modeled after `invowk`, that reports quality gate status and unresolved issues from uploaded SonarQube Cloud results.
- Document the operational contract for `SONAR_TOKEN`, fork pull-request handling, and first-scan issue triage.
- Do not introduce issue suppressions until a real LushText scan reports concrete findings that can be fixed or justified.

## Capabilities

### New Capabilities

- `sonarqube-cloud-ci-analysis`: CI and local verification contract for uploading LushText analysis to SonarQube Cloud and treating reported issues as fix-or-explicitly-accept work.

### Modified Capabilities

- None.

## Impact

- Affected files: GitHub Actions workflows, `sonar-project.properties`, Makefile targets/help text, local Sonar verification script, `.gitignore`, and relevant developer documentation or agent guidance.
- Affected systems: SonarQube Cloud project `cominotti_lushtext`, GitHub Actions, and repository branch-protection/status checks if enabled later.
- Required secret: `SONAR_TOKEN` with permission to submit analysis to the `cominotti` SonarQube Cloud organization.
- No app runtime behavior, Flatpak packaging behavior, or user-facing GTK behavior changes.
