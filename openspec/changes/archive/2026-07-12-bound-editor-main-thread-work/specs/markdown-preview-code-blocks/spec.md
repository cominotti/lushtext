## ADDED Requirements

### Requirement: Code-block width repair skips unchanged embed sets
The system SHALL preserve all deferred Markdown code-block geometry repair passes while avoiding repeated embed traversal when the effective text-column width and rendered-embed generation are unchanged. Render, clear, placeholder, and embed membership changes MUST invalidate the cached decision.

#### Scenario: Deferred passes see unchanged layout
- **WHEN** immediate, idle, and timed repair passes observe the same valid text-column width and embed generation
- **THEN** code-block widgets are traversed only for the first required pass
- **AND** the final timed pass still releases waiting visual-readiness callbacks

#### Scenario: Preview rerenders at the same width
- **WHEN** the document rerenders a different set of code blocks while the preview width stays constant
- **THEN** the embed-generation change invalidates the fast path
- **AND** every new nested and root code block receives its correct width request

#### Scenario: Nested context width changes
- **WHEN** a layout change alters a code block's effective nested column width
- **THEN** the changed valid width triggers a complete code-block refresh
- **AND** the existing nested overflow and no-false-scrollbar requirements remain satisfied
