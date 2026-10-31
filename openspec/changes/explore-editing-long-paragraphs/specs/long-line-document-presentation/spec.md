## MODIFIED Requirements

### Requirement: One monotone document scale policy governs feature availability
The system SHALL derive every size- and length-dependent feature decision from one GTK-free `DocumentScale` policy computed from the profile's total bytes, longest paragraph, and batch density. That policy SHALL replace the byte-only size classification.

The policy outputs are:
- open admission;
- undo;
- the live-memory buffer multiplier;
- local-history availability;
- syntax highlighting;
- bracket matching;
- minimap availability;
- invisible-character drawing;
- automatic search highlighting;
- word wrapping (`wrap_allowed`);
- the `protected_view` flag.

Each threshold MUST be a named policy constant. `L_syntax`, `L_minimap`, `L_invisibles`, `L_search_highlight`, `L_protect`, `N_visible`, `V_batch`, the batch row count and the syntax total-size cutoff MUST take the values that the `probe-long-line-toolkit-costs` measurement records; a threshold that measurement does not derive MUST say in its documentation which recorded value it defaults to. The total-size open, undo and local-history tiers and the minimap total-size tier MUST keep their current values (500 MB refuse, 50 MB undo off and history unavailable, 10 MB save-only history, 10 MB minimap). `L_wrap_edit` MUST take the value the responsiveness lane's keystroke group derives (the largest paragraph length whose wrapped keystroke meets B_interact by the lane's rule); `wrap_allowed` MUST be false exactly when the longest paragraph exceeds `L_wrap_edit`. `protected_view` MUST hold exactly when the longest paragraph exceeds `L_protect` or the batch density exceeds `V_batch`.

The policy's outputs are of two kinds. The open, undo, buffer-multiplier and local-history outputs depend on total bytes alone and describe the document's latest loaded or saved bytes. The other outputs are presentation features.

The policy MUST be monotone: if one profile is componentwise no larger than another, the larger profile MUST NOT enable any feature that the smaller one disables, MUST NOT grant better local-history availability, MUST NOT clear `protected_view`, and MUST NOT raise the buffer multiplier. Callers that know only a byte count MUST use an explicit total-size-only constructor.

#### Scenario: Short-line document under the syntax size keeps every feature
- **WHEN** a 2 MB document whose longest paragraph and batch density are below every threshold is loaded
- **THEN** syntax highlighting, bracket matching, the minimap, invisibles and search highlighting remain available

#### Scenario: One giant paragraph disables syntax although the file is small
- **WHEN** a 5 MB document whose longest paragraph exceeds `L_syntax` is loaded
- **THEN** the language is none and syntax highlighting is off before the buffer is installed into a view that could highlight it
- **AND** no GtkSourceView context-engine timeout warning is emitted

#### Scenario: Threshold edges
- **WHEN** a profile component equals a threshold and when it exceeds that threshold by one
- **THEN** the feature stays enabled at the threshold and is disabled one above it

#### Scenario: Monotonicity is machine-checked
- **WHEN** the Kani lane runs the document-scale harnesses
- **THEN** it proves, for every pair of profiles over every `u64` component where the first is componentwise no larger than the second, that every presentation feature enabled for the second is enabled for the first, that the second has no better local-history availability and no larger buffer multiplier, and that `protected_view` of the first implies `protected_view` of the second
- **AND** it proves the meet laws of the session join on the presentation features (commutative, associative, idempotent, never enabling a feature either argument disables), that the join's byte-only outputs are its second argument's, and that the presentation features of the componentwise maximum of two profiles equal the join of their two policies
- **AND** it proves agreement of the open, undo, multiplier and local-history outputs with the retired byte tiers for every byte count

#### Scenario: Replacing the policy on save is pinned as wrong
- **WHEN** a `should_panic` Kani harness replaces the effective policy with the total-size-only policy of the saved bytes instead of joining it
- **THEN** Kani finds a presentation feature that comes back on, and the harness passes

#### Scenario: Byte-only callers keep their tiers
- **WHEN** file peek, a bookmark excerpt, draft availability or the local-history journal classifies a file it knows only by byte size
- **THEN** it uses the total-size-only constructor and gets the same open and history decisions as before this change

#### Scenario: A paragraph over the wrap threshold disables wrapping only
- **WHEN** a document whose longest paragraph exceeds `L_wrap_edit` but not `L_protect` is loaded
- **THEN** `wrap_allowed` is false and the document stays editable
- **AND** the monotonicity and meet-law harnesses cover `wrap_allowed` like every other presentation feature
