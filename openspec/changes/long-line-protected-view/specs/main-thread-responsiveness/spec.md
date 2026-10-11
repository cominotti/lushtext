## ADDED Requirements

### Requirement: Protected View interactions and settles stay within named budgets
For documents in Protected View, every interaction dispatch SHALL stay within `B_interact`. Interaction dispatches are scroll, key, click, tab switch, toggles, find navigation, inspector navigation and fold-marker activation. Input-to-presented-frame latency SHALL stay within two frames. Every settle dispatch SHALL stay within `B_settle`. Settle dispatches are install, restore, eviction reload, external reload, source capture and find-batch publication. The one declared exception is the existing single-turn installation of one paragraph larger than the slice budget ("Large file installation yields in bounded GTK slices"): until `sliced-protected-install` (C3) is adopted, that turn is a declared envelope whose measured cost C0's lane records, not a `B_settle` guarantee. Both budgets SHALL be read from the responsiveness-budget constants that `probe-long-line-toolkit-costs` (C0) defines once in its GTK-free model module; Protected View SHALL NOT restate their values. Their working values are `B_interact = 8 ms` and `B_settle = 16 ms`, and C0 records the confirmed values. PR-gated widget tests SHALL assert the preconditions that make the expensive toolkit paths unreachable:
- the visible-character bound per paragraph;
- the density bound;
- `editable=false` and `cursor-visible=false`;
- the `move-cursor` intercept;
- no search context, language or minimap attached;
- invisibles off.

C0's responsiveness lane SHALL measure the wall-clock budgets.

#### Scenario: Precondition gates on a protected document
- **WHEN** the widget suite opens each synthetic protected fixture (2k/10k/50k/250k/1.9M-character paragraphs, the dense wrap-off fixture, CR-only, U+2029-separated)
- **THEN** every precondition holds after install, after tab switch and after eviction reload

#### Scenario: Responsiveness lane run
- **WHEN** `make responsiveness-smoke` runs the protected scenarios (open, scroll, find, inspector navigation, tab switch, restore of three protected documents)
- **THEN** the watchdog's largest interaction dispatch is at most `B_interact`, the largest settle dispatch other than a declared single-paragraph install turn is at most `B_settle`, the single-paragraph install turns are recorded against the envelope, and the input-to-frame latency is at most two frames

### Requirement: Protected View bounded work has declared chunk sizes
LushText-owned Protected View work SHALL stay within declared chunk sizes:
- the immutable source capture uses the existing chunked buffer snapshot slices;
- find delivers at most a declared number of results and bytes per GTK turn under a total result cap;
- the inspector reads at most `N_inspect` characters per window;
- notice and accessible text stay within the inline-alert bounds.

None of this work SHALL copy a whole paragraph or the whole buffer on the GTK thread.

#### Scenario: Source capture on first find
- **WHEN** the user opens find on a 100 MB protected document for the first time
- **THEN** the source is captured in chunked snapshot turns, each within `B_settle`, with progress shown, and later finds reuse the captured source
