## ADDED Requirements

### Requirement: A lane run through scripts/measure.sh runs on and records the reference CPU set
When `LUSHTEXT_MEASURE_CPUS` is set, the responsiveness lane SHALL run on exactly that set, and SHALL fail before any case when its inherited affinity differs from it or it exceeds the 16-CPU cap, never trimming it. The lane SHALL record the set and its source in `environment.txt`, `run.json` and `report.md`, and SHALL label unpinned runs, overridden sets and merges of parts on different sets as not comparable with reference runs.

#### Scenario: A reference run is recorded
- **WHEN** the lane runs under `scripts/measure.sh` with the default reference 8-23
- **THEN** `environment.txt` has `affinity=8-23`, `measure_cpus=8-23` and `measure_cpus_source=default`, `run.json` has `reference_run: true`, and `report.md` names 8-23 as the reference CPU set

#### Scenario: An unpinned run is labelled
- **WHEN** the lane runs without `LUSHTEXT_MEASURE_CPUS`
- **THEN** it takes at most 16 CPUs from its inherited affinity as before, `run.json` has `reference_run: false`, and `report.md` says the run is not pinned to the reference CPU set and is trend evidence

#### Scenario: An overridden reference is labelled
- **WHEN** the lane runs under `scripts/measure.sh` with `LUSHTEXT_REFERENCE_CPUS=12-23`
- **THEN** `run.json` has `measure_cpus_source: "override"` and `reference_run: false`, and `report.md` says the run is not comparable with reference runs

#### Scenario: A reference over the cap is refused
- **WHEN** `LUSHTEXT_MEASURE_CPUS` names more than 16 CPUs
- **THEN** the lane fails before any case runs and the message names the set and the cap

#### Scenario: A mismatched inherited affinity is refused
- **WHEN** `LUSHTEXT_MEASURE_CPUS` is set and the lane's inherited affinity differs from it
- **THEN** the lane fails before any case runs and the message names both sets

#### Scenario: Parts on different CPU sets merge as mixed
- **WHEN** `merge-runs` joins parts whose `affinity`, `measure_cpus` or `measure_cpus_source` differ
- **THEN** the merged `run.json` has `affinity: "mixed"`, each part's set in `merged_from` and `reference_run: false`, and `report.md` says the parts are not comparable
