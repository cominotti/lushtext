## 1. Renderer Structure

- [x] 1.1 Add a Markdown code-block buffer state that captures `Tag::CodeBlock` kind and accumulates literal text until `TagEnd::CodeBlock`
- [x] 1.2 Replace the current code-block text-tag insertion path with one embedded code-block widget inserted into the parent preview flow
- [x] 1.3 Ensure embedded code-block widgets are registered in the existing rendered-embed cleanup list
- [x] 1.4 Preserve inline code span rendering on the existing inline `TAG_CODE` path

## 2. Code Block Widget

- [x] 2.1 Build a read-only `sourceview5::Buffer` and `sourceview5::View` for each rendered code block
- [x] 2.2 Apply the active LushText GtkSourceView style scheme to embedded code buffers
- [x] 2.3 Resolve fenced code info strings through a local language helper with common aliases such as `js`
- [x] 2.4 Fall back to plain monospaced rendering when a fenced language cannot be resolved
- [x] 2.5 Wrap the source view in a padded GTK container that renders as one continuous block
- [x] 2.6 Keep vertical scrolling owned by the parent preview while allowing horizontal overflow inside wide code blocks

## 3. Styling

- [x] 3.1 Add CSS classes for Markdown code-block container padding, background, border, and radius
- [x] 3.2 Add CSS or style-scheme adjustment only if GtkSourceView paints a conflicting inner background
- [x] 3.3 Verify the code block remains readable in light and dark styles

## 4. Tests

- [x] 4.1 Add widget coverage for a fenced code block with a blank line rendering as one embedded code-block widget
- [x] 4.2 Add widget coverage for a supported fenced language applying a source language to the embedded buffer
- [x] 4.3 Add widget coverage for unsupported fenced language fallback rendering
- [x] 4.4 Add regression coverage that inline code remains inline text-buffer content
- [x] 4.5 Add widget geometry coverage that code text has a nonzero inset from the code-block container edges

## 5. Verification

- [x] 5.1 Run targeted Markdown preview widget tests
- [x] 5.2 Run `cargo fmt --all --check`
- [x] 5.3 Run `cargo clippy --workspace --all-targets -- -D warnings`
- [x] 5.4 Run `openspec validate improve-markdown-code-block-preview --strict`

## 6. Width and Background Amendments

- [x] 6.1 Add a helper that computes the available Markdown preview text-column width from the text view allocation minus current left and right margins
- [x] 6.2 Apply the computed text-column width to embedded code-block containers after render so ordinary code blocks do not allocate as narrow natural-width boxes
- [x] 6.3 Refresh embedded code-block width when the preview allocation or readable-column margins change
- [x] 6.4 Ensure code-block horizontal scrolling appears only when code content is wider than the computed text-column width
- [x] 6.5 Use one shared background color for the code-block container and the embedded GtkSourceView text area

## 7. Amendment Tests

- [x] 7.1 Add widget coverage proving a short code block in a wide preview has no horizontal overflow
- [x] 7.2 Add widget coverage proving a long code line in a narrow preview can still use horizontal overflow
- [x] 7.3 Add widget coverage proving rendered code-block width tracks preview text-column width after layout changes
- [x] 7.4 Add widget or style coverage proving the outer code-block surface and inner source text area use a matching background

## 8. Amendment Verification

- [x] 8.1 Run targeted Markdown preview widget tests
- [x] 8.2 Run `cargo fmt --all --check`
- [x] 8.3 Run `cargo clippy --workspace --all-targets -- -D warnings`
- [x] 8.4 Run `openspec validate improve-markdown-code-block-preview --strict`
