// SPDX-License-Identifier: GPL-3.0-or-later

//! Markdown preview widget — read-only rendered view of Markdown content.
//!
//! Uses `pulldown-cmark` to parse CommonMark and applies `GtkTextTag`s to a
//! `GtkTextView` buffer for native rendering. Supports headings (h1-h6),
//! bold, italic, strikethrough, inline code, fenced code blocks, links,
//! blockquotes, lists (ordered and unordered), and horizontal rules.
//!
//! Two display states:
//! - **Content mode**: scrolled text view with rendered Markdown
//! - **Placeholder mode**: `AdwStatusPage` with "Not a Markdown file" message

mod imp;

use glib::Object;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::glib;
use gtk4::prelude::*;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use imp::{
    TAG_BLOCKQUOTE, TAG_BOLD, TAG_CODE, TAG_CODE_BLOCK, TAG_HRULE, TAG_ITALIC, TAG_LINK,
    TAG_LIST_ITEM, TAG_STRIKETHROUGH, heading_tag_name,
};

glib::wrapper! {
    pub struct LushtextMarkdownPreview(ObjectSubclass<imp::LushtextMarkdownPreview>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl LushtextMarkdownPreview {
    #[must_use]
    pub fn new() -> Self {
        Object::builder().build()
    }

    /// Render Markdown content into the text view, replacing any previous content.
    ///
    /// Switches to content mode (text view visible, placeholder hidden).
    /// The rendering walks the `pulldown-cmark` event stream and maps each
    /// element to `GtkTextTag`s on the buffer.
    pub fn render_markdown(&self, markdown: &str) {
        self.show_content_view();

        let imp = self.imp();
        let buffer = imp.text_view.buffer();

        // Clear previous content.
        buffer.set_text("");

        let parser = Parser::new_ext(markdown, Options::empty());
        let mut iter = buffer.end_iter();

        // Tag stack: tracks which TextTag names are currently active.
        // When we insert text, all tags in the stack are applied.
        let mut tag_stack: Vec<String> = Vec::new();

        // Track list nesting: None = not in a list, Some(None) = unordered,
        // Some(Some(n)) = ordered starting at n.
        let mut list_stack: Vec<Option<u64>> = Vec::new();

        // Track whether we need a paragraph separator before the next block.
        let mut needs_block_separator = false;

        for event in parser {
            match event {
                Event::Start(tag) => {
                    match tag {
                        Tag::Heading { level, .. } => {
                            if needs_block_separator {
                                buffer.insert(&mut iter, "\n");
                            }
                            let idx = heading_level_to_index(level);
                            tag_stack.push(heading_tag_name(idx));
                            needs_block_separator = false;
                        }
                        Tag::Paragraph => {
                            if needs_block_separator {
                                buffer.insert(&mut iter, "\n");
                            }
                            needs_block_separator = false;
                        }
                        Tag::BlockQuote(_) => {
                            if needs_block_separator {
                                buffer.insert(&mut iter, "\n");
                            }
                            tag_stack.push(TAG_BLOCKQUOTE.to_string());
                            needs_block_separator = false;
                        }
                        Tag::CodeBlock(_kind) => {
                            if needs_block_separator {
                                buffer.insert(&mut iter, "\n");
                            }
                            tag_stack.push(TAG_CODE_BLOCK.to_string());
                            needs_block_separator = false;
                        }
                        Tag::List(start_num) => {
                            if needs_block_separator {
                                buffer.insert(&mut iter, "\n");
                            }
                            list_stack.push(start_num);
                            needs_block_separator = false;
                        }
                        Tag::Item => {
                            // Insert bullet or number prefix based on list type.
                            let prefix = match list_stack.last() {
                                Some(Some(start)) => {
                                    // For ordered lists, compute the current item number.
                                    // pulldown-cmark gives us the start number; we increment
                                    // by counting items in this list level.
                                    let num = *start;
                                    format!("{num}. ")
                                }
                                _ => "\u{2022} ".to_string(), // bullet: •
                            };
                            let tags: Vec<&str> =
                                tag_stack.iter().map(std::string::String::as_str).collect();
                            let mut all_tags = tags.clone();
                            all_tags.push(TAG_LIST_ITEM);
                            insert_with_tags(&buffer, &mut iter, &prefix, &all_tags);
                            tag_stack.push(TAG_LIST_ITEM.to_string());
                        }
                        Tag::Emphasis => {
                            tag_stack.push(TAG_ITALIC.to_string());
                        }
                        Tag::Strong => {
                            tag_stack.push(TAG_BOLD.to_string());
                        }
                        Tag::Strikethrough => {
                            tag_stack.push(TAG_STRIKETHROUGH.to_string());
                        }
                        Tag::Link { .. } => {
                            tag_stack.push(TAG_LINK.to_string());
                        }
                        // Skip elements we don't render (images, tables, metadata, etc.)
                        _ => {}
                    }
                }
                Event::End(tag_end) => {
                    match tag_end {
                        TagEnd::Heading(_) => {
                            pop_tag(&mut tag_stack);
                            buffer.insert(&mut iter, "\n");
                            needs_block_separator = true;
                        }
                        TagEnd::Paragraph => {
                            buffer.insert(&mut iter, "\n");
                            needs_block_separator = true;
                        }
                        TagEnd::BlockQuote(_) => {
                            pop_tag(&mut tag_stack);
                            needs_block_separator = true;
                        }
                        TagEnd::CodeBlock => {
                            // Ensure code block ends with a newline for clean separation.
                            let tags: Vec<&str> =
                                tag_stack.iter().map(std::string::String::as_str).collect();
                            insert_with_tags(&buffer, &mut iter, "\n", &tags);
                            pop_tag(&mut tag_stack);
                            needs_block_separator = true;
                        }
                        TagEnd::List(_) => {
                            list_stack.pop();
                            needs_block_separator = true;
                        }
                        TagEnd::Item => {
                            pop_tag(&mut tag_stack);
                            buffer.insert(&mut iter, "\n");
                            // Increment ordered list counter for the next item.
                            if let Some(Some(n)) = list_stack.last_mut() {
                                *n += 1;
                            }
                        }
                        TagEnd::Emphasis
                        | TagEnd::Strong
                        | TagEnd::Strikethrough
                        | TagEnd::Link => {
                            pop_tag(&mut tag_stack);
                        }
                        _ => {}
                    }
                }
                Event::Text(text) => {
                    let tags: Vec<&str> =
                        tag_stack.iter().map(std::string::String::as_str).collect();
                    insert_with_tags(&buffer, &mut iter, &text, &tags);
                }
                Event::Code(code) => {
                    // Inline code: apply the code tag in addition to any active stack tags.
                    let mut tags: Vec<&str> =
                        tag_stack.iter().map(std::string::String::as_str).collect();
                    tags.push(TAG_CODE);
                    insert_with_tags(&buffer, &mut iter, &code, &tags);
                }
                Event::SoftBreak => {
                    buffer.insert(&mut iter, " ");
                }
                Event::HardBreak => {
                    buffer.insert(&mut iter, "\n");
                }
                Event::Rule => {
                    if needs_block_separator {
                        buffer.insert(&mut iter, "\n");
                    }
                    insert_with_tags(
                        &buffer,
                        &mut iter,
                        "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}",
                        &[TAG_HRULE],
                    );
                    buffer.insert(&mut iter, "\n");
                    needs_block_separator = true;
                }
                // Skip HTML, math, footnotes — out of scope for native rendering.
                _ => {}
            }
        }
    }

    /// Clear the rendered content and show the placeholder for non-Markdown files.
    pub fn show_placeholder(&self, description: &str) {
        let imp = self.imp();
        imp.placeholder.set_description(Some(description));
        imp.scrolled_window.set_visible(false);
        imp.placeholder.set_visible(true);
        imp.text_view.buffer().set_text("");
        imp.showing_content.set(false);
    }

    /// Clear the rendered content without showing the placeholder.
    pub fn clear(&self) {
        self.imp().text_view.buffer().set_text("");
    }

    /// Whether the widget is currently showing rendered Markdown content.
    #[must_use]
    pub fn is_showing_content(&self) -> bool {
        self.imp().showing_content.get()
    }

    /// Get the rendered text content from the internal buffer.
    /// Useful for verifying rendering output in tests.
    #[must_use]
    pub fn buffer_text(&self) -> String {
        let buffer = self.imp().text_view.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .to_string()
    }

    /// Whether the text view is editable (should always be false).
    #[must_use]
    pub fn is_editable(&self) -> bool {
        self.imp().text_view.is_editable()
    }

    /// Whether the cursor is visible in the text view (should always be false).
    #[must_use]
    pub fn is_cursor_visible(&self) -> bool {
        self.imp().text_view.is_cursor_visible()
    }

    /// Look up a tag by name in the internal buffer's tag table.
    /// Returns true if the tag exists.
    #[must_use]
    pub fn has_tag(&self, name: &str) -> bool {
        self.imp()
            .text_view
            .buffer()
            .tag_table()
            .lookup(name)
            .is_some()
    }

    /// Switch to content mode: text view visible, placeholder hidden.
    fn show_content_view(&self) {
        let imp = self.imp();
        if !imp.showing_content.get() {
            imp.scrolled_window.set_visible(true);
            imp.placeholder.set_visible(false);
            imp.showing_content.set(true);
        }
    }
}

impl Default for LushtextMarkdownPreview {
    fn default() -> Self {
        Self::new()
    }
}

/// Insert text at the given iter with the specified tag names applied.
fn insert_with_tags(
    buffer: &gtk4::TextBuffer,
    iter: &mut gtk4::TextIter,
    text: &str,
    tag_names: &[&str],
) {
    if tag_names.is_empty() {
        buffer.insert(iter, text);
        return;
    }

    let start_offset = iter.offset();
    buffer.insert(iter, text);
    let start = buffer.iter_at_offset(start_offset);

    for name in tag_names {
        if let Some(tag) = buffer.tag_table().lookup(name) {
            buffer.apply_tag(&tag, &start, iter);
        }
    }
}

/// Convert a `HeadingLevel` to a 0-based index for the tag name array.
fn heading_level_to_index(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 0,
        HeadingLevel::H2 => 1,
        HeadingLevel::H3 => 2,
        HeadingLevel::H4 => 3,
        HeadingLevel::H5 => 4,
        HeadingLevel::H6 => 5,
    }
}

/// Pop the last tag from the stack. No-op if the stack is empty.
fn pop_tag(stack: &mut Vec<String>) {
    stack.pop();
}
