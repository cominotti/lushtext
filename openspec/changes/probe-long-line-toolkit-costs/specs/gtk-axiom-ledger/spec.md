## ADDED Requirements

### Requirement: Long-line text behaviours are ledger axioms
Every GtkTextView, GtkTextBuffer, Pango, input-method, accessibility and GtkSourceView behaviour that the long-line designs rely on SHALL be a ledger entry with a new stable id, starting at A23. Existing ids SHALL NOT be reused. The ledger SHALL cover at least the following behaviours:

| Id | Behaviour |
|---|---|
| A23 | One PangoLayout per paragraph, over its visible text, with no partial layout |
| A24 | Invisible chunks are left out of the layout text |
| A25 | Toggle gravity: text inserted at a tagged run's start or end boundary is untagged, while text inserted strictly inside the run is tagged |
| A26 | Log attrs are computed over the whole logical paragraph, hidden text included, and are invalidated by any character change |
| A27 | Tag apply and remove emit no `changed`, leave `modified` unchanged, and record no undo history |
| A28 | Pango's glyph-width int32 overflow |
| A29 | The GtkSourceView context engine analyses a line atomically, stops at a 2000 ms cut-off, then disables highlighting, and reads hidden text |
| A30 | Input-method spot and surrounding-text work grows with the cursor's paragraph and is armed by caret moves, keys, clicks and scroll |
| A31 | Validation runs in 2000 px batches |
| A32 | `gtk_source_map_set_view(NULL)` with a pending update tick causes a CRITICAL (a defect axiom) |
| A33 | `GtkSourceSearchContext`: the non-regex path sees hidden text, and the regex path sees visible text only |
| A34 | `move-cursor` with an invisible cursor moves only the viewport, unless `gtk-keynav-use-caret` is set |
| A35 | AT-SPI text contents and offsets over invisible text, and `TextChanged` payloads on insert |
| A36 | The btree copies both strings when it merges adjacent character segments |

Each entry's statement SHALL be written from what its probe or measurement observes, not from the source reading that first suggested it. Each entry SHALL list the long-line changes that depend on it.

#### Scenario: A long-line design cites a toolkit behaviour
- **WHEN** the document-scale, protected-view or sliced-install change relies on one of these behaviours
- **THEN** it cites the ledger id, and the ledger row lists that change among its dependent designs

#### Scenario: An entry cannot be pinned structurally
- **WHEN** an axiom has no structural observation in an isolated fixture, as with the AT-SPI behaviour that needs an accessibility bus or the cost-only segment merge
- **THEN** its row records why it cannot be pinned, and names the responsiveness-lane evidence that stands in for a probe

#### Scenario: A defect axiom stops holding
- **WHEN** a toolkit update fixes the behaviour that a defect axiom such as A32 records, and its probe reports violated
- **THEN** the change is reviewed as an axiom change against every dependent design before the probe is edited
