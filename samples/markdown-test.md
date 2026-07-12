# Markdown Preview Feature Showcase

This file is meant to exercise the Markdown preview paths that LushText
currently supports in its GTK-native renderer.

It includes **bold**, *italic*, ~~strikethrough~~, inline `code`, and several
different link and image cases that are useful for manual verification.

---

## Headings

### Level 3 Heading

Regular paragraph text under a subheading, with enough words to make spacing
and wrapping easy to inspect in the preview.

#### Level 4 Heading

Another paragraph with mixed inline formatting: **strong emphasis**, *lighter
emphasis*, `inline snippets`, and ~~crossed-out text~~.

---

## Links

These links should look clickable in preview and open through the desktop's
default external handler when activated.

- External link: [Rust website](https://www.rust-lang.org/)
- External docs link inside prose: [GTK4 Rust bindings](https://gtk-rs.org/gtk4-rs/stable/latest/)
- File-relative local link: [Open the full-color app icon](../data/icons/dev.cominotti.lushtext.svg)
- Second file-relative local link: [Open the preview card sample](assets/preview-secondary.svg)

---

## Lists

### Unordered List

- Native preview renderer
- Clickable preview links
- Read-only Markdown output

### Ordered List

1. Open a Markdown document
2. Toggle the preview pane
3. Compare source and rendered output

### Offset Ordered List

57. Offset numbering keeps the rendered marker attached to the item text.
58. Multi-digit markers still wrap with continuation text under the item text column rather than under the marker column.

### Nested Mixed Lists

1. First ordered item
   - Nested unordered child
   - Another nested child with a [clickable link](https://example.com/)
     1. Mixed ordered grandchild
     2. Another grandchild
2. Second ordered item
   - Nested child after returning to the parent list
   - A longer nested child wraps cleanly under the child item text instead of drifting back into the marker column.

### Task List

- [x] Tables render as native GTK widgets
- [x] Alert callouts render without raw markers
- [x] Reference-style and inline footnotes render with numbered references
- [x] Preview links open externally
- [x] Local Markdown images render natively
- [x] Missing or remote images show explicit fallback states

---

## Quotes And Callouts

> Plain blockquotes render with quote rails.
>> Nested blockquotes add another visible rail.
> > > Spaced nested markers keep the same quote depth.

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

Inline code like `cargo test -p lushtext-core markdown_preview --lib` should
stay distinct from surrounding prose.

```rust
fn main() {
    let features = [
        "task lists",
        "alert callouts",
        "nested blockquotes",
        "reference-style footnotes",
        "inline footnotes",
        "tables",
        "preview links",
        "local images",
    ];

    for feature in features {
        println!("preview supports: {feature}");
    }
}
```

---

## Tables

Paragraph above the table so the preview can show surrounding flow before the
native grid block.

| Feature | Status | Notes |
| --- | --- | --- |
| Headings | Ready | Styled with native text tags |
| Preview links | Ready | [Rust site](https://www.rust-lang.org/) should activate from a cell |
| Local files | Ready | [Preview card sample](assets/preview-secondary.svg) uses a file-relative link |

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
| Link | [Docs](https://docs.rs/) | Should stay clickable in the rendered table |

Paragraph below the table to verify that normal document flow resumes after the
embedded GTK table widget.

---

## Images

These image cases are useful for manual verification of the new native image
and fallback paths.

### File-Relative Local Image

![File-relative app icon](../data/icons/dev.cominotti.lushtext.svg)

### Second File-Relative Local Image

![File-relative preview card sample](assets/preview-secondary.svg)

### Unloadable Local Image Fallback

![Invalid image data](assets/invalid-preview-image.png)

### Missing Local Image Fallback

![Missing image](missing-preview-image.png)

### Remote Image Fallback

![Remote image should stay unsupported](https://example.com/remote-preview-image.png)

---

## Footnotes

Footnotes render inline markers in the preview,[^overview] and they can be
referenced more than once in the same document.[^overview]

You can also mix a second footnote into normal prose when checking numbering
behavior.[^details]

Inline footnotes lower into the same rendered footnote flow^[This inline
footnote includes **bold text**, a [link](https://docs.rs/), and inline `code`.]
without changing the Markdown source.

Inline and reference-style footnotes can appear together^[This inline note is
useful for checking mixed numbering near reference-style definitions.].

[^overview]: This footnote includes **bold text**, a [link](https://docs.rs/), and inline `code`.

[^details]:
    This definition uses a longer body so spacing is easier to inspect.

    - It can include list items
    - And multiple paragraphs

    The preview should keep the reference number and the definition number aligned.
