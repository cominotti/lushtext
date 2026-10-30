## ADDED Requirements

### Requirement: Protected View is announced by one bounded inline alert
A document in Protected View SHALL show one inline alert. The alert SHALL state that the document is view-only because it contains very long lines, give the folded paragraph count, and offer an Inspect Line action. For a recovered draft it SHALL also keep the existing Discard and Save actions of the restored-draft alert, merged into the same single alert. Its title, body and accessible text SHALL be bounded and SHALL NOT contain document text. Invoking a disabled Protected View action SHALL re-surface this alert or publish one rate-limited status message instead of stacking alerts. The alert SHALL replace C1's reduced-features alert for the same document, so that one document never shows both.

#### Scenario: Single alert across repeated blocked actions
- **WHEN** the user presses Ctrl+V ten times in a protected document
- **THEN** exactly one Protected View inline alert is visible, and at most one status message is published within the rate-limit interval
