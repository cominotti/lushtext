## 1. Renderer

- [x] 1.1 Add quote-rail constants, helper functions, and depth-specific text tags for generic Markdown blockquotes in `ui/markdown_preview`.
- [x] 1.2 Update generic `Tag::BlockQuote(None)` handling so rendered quote blocks insert visible rail glyphs and apply depth-aware indentation while keeping raw `>` markers out of preview text.
- [x] 1.3 Track generic blockquote depth separately from typed `Tag::BlockQuote(Some(kind))` alert callouts so alerts continue through the existing typed callout path.
- [x] 1.4 Preserve supported inline formatting and link-span behavior inside generic blockquotes, and avoid making every generic quote body line implicit emphasis.

## 2. Tests

- [x] 2.1 Strengthen the simple blockquote widget test so it asserts rendered quote rails, quoted text order, and absence of raw `>` source markers.
- [x] 2.2 Add widget coverage for nested generic blockquotes written with adjacent markers such as `>>>`, including depth-specific rail or tag assertions.
- [x] 2.3 Add widget coverage for nested generic blockquotes written with spaced markers such as `> > >`, proving they render with the same depth hierarchy as adjacent markers.
- [x] 2.4 Add widget coverage for inline emphasis, strong text, inline code, and supported links inside generic blockquotes.
- [x] 2.5 Add a regression test proving GitHub alert callouts remain distinguishable from generic rail-styled blockquotes.

## 3. Documentation And Samples

- [x] 3.1 Update `samples/markdown-test.md` with nested generic blockquotes that exercise visible rails and depth.
- [x] 3.2 Update `README.md` so the Markdown preview feature description mentions nested blockquote rails rather than only generic blockquote support.
- [x] 3.3 Update `AGENTS.md` or local agent rules if the implementation introduces a durable Markdown-preview rendering pattern future agents should preserve.

## 4. Validation

- [x] 4.1 Run `openspec validate harden-markdown-blockquote-preview --strict`.
- [x] 4.2 Run focused Markdown preview widget tests for the new blockquote behavior.
- [x] 4.3 Run `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` after implementation changes.
