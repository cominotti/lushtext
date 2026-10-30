## ADDED Requirements

### Requirement: The lane attributes post-open settle by stage
The measurement build SHALL record one `post_open_*` span per post-open stage:
- the activation turn, editor construction, language catalog and language attach;
- install direct or slice, publish, highlight attach, and the first editor frame.

It SHALL also record a mark for each GtkSourceBuffer `highlight-updated` and for each change of the editor's vertical adjustment `upper`. Without the `responsiveness-probes` feature, each of these SHALL be an empty inline function.

The lane SHALL report a post-open settle table. For every case and repetition, it lists every dispatch from the activation turn until `document-settled`, at least 1 ms long, with the stage that contains it. A dispatch that no span covers SHALL be attributed to highlighting or validation when it ends at the matching mark, and otherwise reported as unattributed. The table SHALL be read by span, not by the `open`/`background` phase boundary, because `file-open-complete` can hold before the activation's opens run.

The background phase SHALL last until the app reports `document-settled`, capped. A cap hit SHALL be a failed phase. A `type-during-settle` phase SHALL inject one key every 50 ms, from load readiness until `document-settled`, and report each key's dispatch latency from its compositor event time to its handler.

#### Scenario: A post-open stall is attributed
- **WHEN** a post-open dispatch exceeds `B_SETTLE_US`
- **THEN** the report names the post-open stage, or the highlight or validation mark, that contains it, or states that it is unattributed

#### Scenario: Settle outlasts a fixed sleep
- **WHEN** background highlighting continues for longer than two seconds after open
- **THEN** the background phase continues until `document-settled` or its cap, and every dispatch until then is in the table

#### Scenario: A shipped build carries no post-open span
- **WHEN** LushText is built without the `responsiveness-probes` feature
- **THEN** no post-open span, mark, environment variable or frame-phase extension is compiled in

### Requirement: The post-open settle verdict is measured interleaved
The C6 verdict SHALL run under the measurement lock protocol and the reference precondition. It SHALL interleave a `main` build and the branch build, both measured by the branch's lane driver: `main`, branch, `main`, branch, 7 repetitions per side.

`baseline/small` SHALL run in the four construction profiles. The cases SHALL include:
- `baseline/empty`;
- `total-size` 16 kB, 1 MB and 5 MB as `.html` and `.txt`;
- `dense` 120, 500 and 710 with wrap on and off;
- a Markdown document with side-by-side preview;
- an EditorConfig tab width that differs from the setting;
- a three-document session restore;
- a two-draft recovery.

The verdict is met when, on the branch, in every profile and repetition:
- every post-open dispatch until `document-settled` is at most `B_SETTLE_US`;
- every `type-during-settle` key is dispatched within `B_INTERACT_US`;
- no post-open stage is slower than on `main`.

Input-to-frame SHALL be reported and not gated, unless the maintainer decides otherwise. The verdict, run ids, load, storage evidence and toolkit versions SHALL be recorded in `docs/next/long-line-documents.md`.

#### Scenario: The verdict is met
- **WHEN** every branch case meets the three conditions in every profile and repetition
- **THEN** the programme record marks C6's gate as met and cites the interleaved run ids

#### Scenario: A residual is accepted by the maintainer
- **WHEN** a dispatch over `B_SETTLE_US` remains and the maintainer accepts it
- **THEN** the record lists it with its stage, size, the maintainer decision with its date, and the upstream report
