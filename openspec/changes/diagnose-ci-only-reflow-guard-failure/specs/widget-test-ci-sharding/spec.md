## ADDED Requirements

### Requirement: A widget failure seen only on CI is reproduced in the CI image first
Before a widget test failure that passes in the toolbox is treated as CI-only, it SHALL be run in the CI widget job's own image (read from `.github/workflows/ci.yml`, with that job's packages) under the runner's CPU budget, through a tracked repository script that runs one test or one shard N times under the shared heavy-work lock. A failure that reproduces there SHALL be diagnosed and fixed against that reproduction, failing first, and proved by 20 consecutive green runs in it.

#### Scenario: Failure reproduced under the CPU limit
- **WHEN** a widget test fails on every CI run and passes in the toolbox
- **AND** it fails in the CI image limited to four CPUs
- **THEN** its diagnosis and failing-first test use that container run
- **AND** the fix holds on 20 consecutive container runs at that limit before CI is asked again

#### Scenario: Failure seen only inside a full shard
- **WHEN** a test failed on CI inside its full shard and passes isolated
- **THEN** the whole shard is run in the CI image under the CPU limit at least five times before the test is declared not reproducible

#### Scenario: Lane self-test
- **WHEN** `make check-agent-helpers` runs
- **THEN** the lane script's self-test checks its image and package extraction from `ci.yml` and its argument handling without starting a container

### Requirement: A shard's CI proof counts only the branch's final code
A widget shard's CI regression proof SHALL count only runs of the branch's final code for that shard, one per fresh job attempt, each recorded with its run id and attempt number; runs of an earlier branch or of earlier code are recorded as history and not counted.

#### Scenario: Inherited runs
- **WHEN** a follow-up change inherits clean runs from the change that handed it the work
- **THEN** those runs are listed as history and the proof restarts at zero on the follow-up's final code

#### Scenario: Reruns as proof
- **WHEN** a widget job of the branch's pull-request run is rerun
- **THEN** each fresh attempt counts as one run of that shard, if it ran the final code and printed no `FLAKY` line for the watched test
