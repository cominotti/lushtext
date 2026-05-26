## 1. Parser Contract

- [x] 1.1 Add parser event-shape tests for pulldown-cmark colon-style definition lists, including simple terms, multiple definitions, inline markup, nested blocks, and unsupported `~` marker syntax.
- [x] 1.2 Enable `Options::ENABLE_DEFINITION_LIST` in the Markdown preview render options after event-shape expectations are covered.

## 2. Renderer Tags and State

- [x] 2.1 Add definition-list text tags for terms and definitions in the Markdown preview tag setup, including dark/light update behavior.
- [x] 2.2 Add dedicated streaming-renderer state for `DefinitionList`, `DefinitionListTitle`, and `DefinitionListDefinition` without routing them through ordinary list markers.
- [x] 2.3 Render simple definition lists in source order without showing raw colon markers.
- [x] 2.4 Preserve supported inline formatting inside definition-list terms and definition bodies through the existing inline tag stack.

## 3. Nested Content

- [x] 3.1 Preserve multiple paragraphs inside a definition body without duplicated blank rows.
- [x] 3.2 Preserve ordinary ordered and unordered lists nested inside a definition body.
- [x] 3.3 Preserve generic blockquotes nested inside a definition body with existing blockquote styling.
- [x] 3.4 Preserve fenced and indented code blocks nested inside a definition body, including the existing no-false-horizontal-overflow behavior for short code lines.

## 4. Tests, Docs, and Validation

- [x] 4.1 Add Markdown preview widget tests for simple definition lists, multiple definitions, inline markup, nested paragraphs, nested blockquotes, nested lists, nested code blocks, and unsupported `~` syntax.
- [x] 4.2 Update Markdown preview follow-up documentation so definition lists are no longer described as future work after implementation.
- [x] 4.3 Run the focused Markdown preview widget tests.
- [x] 4.4 Run `cargo fmt --all -- --check`.
- [x] 4.5 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 4.6 Run `openspec validate support-markdown-definition-lists --strict`.
