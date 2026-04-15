# Markdown Preview Feature Showcase

This file is meant to exercise the Markdown preview paths that LushText currently supports in its GTK-native renderer.

It includes **bold**, *italic*, ~~strikethrough~~, inline `code`, and a normal [link to the Rust website](https://www.rust-lang.org/).

---

## Headings

### Level 3 Heading

Regular paragraph text under a subheading, with enough words to make spacing and wrapping easy to inspect in the preview.

#### Level 4 Heading

Another paragraph with mixed inline formatting: **strong emphasis**, *lighter emphasis*, `inline snippets`, and ~~crossed-out text~~.

---

## Lists

### Unordered List

- Native preview renderer
- Styled headings and links
- Read-only Markdown output

### Ordered List

1. Open a Markdown document
2. Toggle the preview pane
3. Compare source and rendered output

### Task List

- [x] Tables render as native GTK widgets
- [x] Alert callouts render without raw markers
- [x] Footnotes render with numbered references
- [ ] Images are still out of scope for this sample

---

## Quotes And Callouts

> Plain blockquotes still render as quoted text.
>
> They are separate from GitHub alert callouts.

> [!NOTE]
> This note callout shows the typed alert styling path.

> [!TIP]
> Use this file to quickly spot regressions in preview rendering.

> [!IMPORTANT]
> The preview stays GTK-native instead of switching to an HTML renderer.

> [!WARNING]
> Raw HTML and browser-level parity are not the goal of this showcase.

> [!CAUTION]
> Very wide tables can still put pressure on the preview width.

---

## Code

Inline code like `cargo test -p lushtext-core markdown_preview --lib` should stay distinct from surrounding prose.

```rust
fn main() {
    let features = [
        "task lists",
        "alert callouts",
        "footnotes",
        "tables",
    ];

    for feature in features {
        println!("preview supports: {feature}");
    }
}
```

---

## Tables

Paragraph above the table so the preview can show surrounding flow before the native grid block.

| Feature | Status | Notes |
| --- | --- | --- |
| Headings | Ready | Styled with native text tags |
| Links | Ready | Presentation-only for now |
| Footnotes | Ready | Numbered in preview order |

### Alignment And Blank Cells

| Left | Center | Right |
| :--- | :----: | ----: |
| alpha | beta | 10 |
| longer left text | centered note | 2500 |
| blank note |  | 7 |

### Inline Formatting In Table Cells

| Pattern | Example | Meaning |
| --- | --- | --- |
| Bold | **Important** | Strong emphasis |
| Italic | *Optional* | Softer emphasis |
| Strike | ~~Deprecated~~ | No longer preferred |
| Code | `cargo test` | Command snippet |

Paragraph below the table to verify that normal document flow resumes after the embedded GTK table widget.

---

## Footnotes

Footnotes render inline markers in the preview,[^overview] and they can be referenced more than once in the same document.[^overview]

You can also mix a second footnote into normal prose when checking numbering behavior.[^details]

[^overview]: This footnote includes **bold text**, a [link](https://docs.rs/), and inline `code`.

[^details]:
    This definition uses a longer body so spacing is easier to inspect.

    - It can include list items
    - And multiple paragraphs

    The preview should keep the reference number and the definition number aligned.
