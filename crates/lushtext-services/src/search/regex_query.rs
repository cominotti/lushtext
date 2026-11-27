// SPDX-License-Identifier: GPL-3.0-or-later

//! Regex compilation, paragraph-hit scanning, and capture inspection helpers.
//!
//! Keeps the `regex` crate dependency encapsulated inside `lushtext-services` so
//! `lushtext-ui` never compiles or depends on `regex` directly.

use std::sync::atomic::{AtomicBool, Ordering};

use lushtext_model::document::paragraph_boundary::next_paragraph_break;

/// One regex match mapped to its 0-based GTK paragraph index, character column,
/// and match character length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParagraphRegexHit {
    pub paragraph: u64,
    pub column: u64,
    pub chars: u64,
}

/// Options governing how a find query is compiled into a regular expression.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegexCompileOptions {
    /// Interpret `query` as raw regex syntax instead of escaping literals.
    pub is_regex: bool,
    /// Wrap the pattern in word boundaries (`\b(?:...)\b`).
    pub whole_word: bool,
    /// Match ASCII/Unicode case insensitively.
    pub case_insensitive: bool,
}

/// Compiled regular-expression query for scanning a document on a worker thread.
#[derive(Clone, Debug)]
pub struct CompiledFindPattern {
    regex: regex::Regex,
}

impl CompiledFindPattern {
    /// Compile a literal or regex query for document scanning.
    ///
    /// Returns `None` if a regex pattern fails to compile.
    #[must_use]
    pub fn compile(query: &str, options: RegexCompileOptions) -> Option<Self> {
        let escaped;
        let base = if options.is_regex {
            query
        } else {
            escaped = regex::escape(query);
            escaped.as_str()
        };
        let pattern = if options.whole_word {
            format!(r"\b(?:{base})\b")
        } else {
            base.to_string()
        };
        let regex = regex::RegexBuilder::new(&pattern)
            .case_insensitive(options.case_insensitive)
            .build()
            .ok()?;
        Some(Self { regex })
    }

    /// Collect non-empty matches in `text` up to `max_hits` or cancellation,
    /// using GTK's paragraph delimiters (`\n`, `\r`, `\r\n`, U+2029).
    #[must_use]
    pub fn find_paragraph_hits(
        &self,
        text: &str,
        max_hits: usize,
        cancel: &AtomicBool,
    ) -> Vec<ParagraphRegexHit> {
        let mut hits = Vec::new();
        let mut paragraph = 0u64;
        let mut next_break = next_paragraph_break(text, 0);
        let mut column_byte = 0usize;
        let mut column = 0u64;
        for found in self.regex.find_iter(text) {
            if hits.len() >= max_hits || cancel.load(Ordering::Relaxed) {
                break;
            }
            if found.is_empty() {
                continue;
            }
            while let Some(paragraph_break) = next_break
                && paragraph_break.next_start <= found.start()
            {
                paragraph += 1;
                column_byte = paragraph_break.next_start;
                column = 0;
                next_break = next_paragraph_break(text, paragraph_break.next_start);
            }
            column += count_chars(&text[column_byte..found.start()]);
            column_byte = found.start();
            hits.push(ParagraphRegexHit {
                paragraph,
                column,
                chars: count_chars(found.as_str()),
            });
        }
        hits
    }
}

/// Compile `pattern` as a Rust regex and return its capture group names (`None`
/// per unnamed group), or `None` if `pattern` does not compile.
#[must_use]
pub fn regex_capture_names(pattern: &str) -> Option<Vec<Option<String>>> {
    let compiled = regex::Regex::new(pattern).ok()?;
    Some(
        compiled
            .capture_names()
            .map(|name| name.map(str::to_owned))
            .collect(),
    )
}

fn count_chars(text: &str) -> u64 {
    u64::try_from(text.chars().count()).unwrap_or(u64::MAX)
}

/// Test-only hooks for property/unit checks comparing Rust regex expansion
/// against translated GtkSourceView templates without exposing `regex` to `lushtext-ui`.
#[cfg(any(test, feature = "test-utils"))]
pub mod test_support {
    /// Expand a translated GRegex template against `captures` (ledger A127 model).
    fn gregex_expand(template: &str, captures: &regex::Captures<'_>) -> String {
        let mut out = String::new();
        let mut rest = template;
        while let Some(position) = rest.find('\\') {
            out.push_str(&rest[..position]);
            let tail = &rest[position + 1..];
            if let Some(after) = tail.strip_prefix('\\') {
                out.push('\\');
                rest = after;
            } else if let Some(group) = tail.strip_prefix("g<")
                && let Some(end) = group.find('>')
            {
                let name = &group[..end];
                let found = match name.parse::<usize>() {
                    Ok(number) => captures.get(number),
                    Err(_) => captures.name(name),
                };
                out.push_str(found.map_or("", |m| m.as_str()));
                rest = &group[end + 1..];
            } else {
                out.push('\\');
                rest = tail;
            }
        }
        out.push_str(rest);
        out
    }

    /// Replace all matches using the ledger A127 GRegex template semantics.
    ///
    /// # Panics
    ///
    /// Panics if `pattern` does not compile as a Rust regular expression.
    #[must_use]
    pub fn gregex_replace_all(pattern: &str, text: &str, template: &str) -> String {
        let compiled = regex::Regex::new(pattern).expect("pattern");
        let mut out = String::new();
        let mut last = 0;
        for captures in compiled.captures_iter(text) {
            let whole = captures.get(0).expect("group 0");
            out.push_str(&text[last..whole.start()]);
            out.push_str(&gregex_expand(template, &captures));
            last = whole.end();
        }
        out.push_str(&text[last..]);
        out
    }

    /// Replace all matches using native `regex::Regex::replace_all`.
    ///
    /// # Panics
    ///
    /// Panics if `pattern` does not compile as a Rust regular expression.
    #[must_use]
    pub fn rust_replace_all(pattern: &str, text: &str, template: &str) -> String {
        regex::Regex::new(pattern)
            .expect("pattern")
            .replace_all(text, template)
            .into_owned()
    }

    /// Expand `template` on the first match of `pattern` in `text` using both
    /// the GRegex model and native `Captures::expand`.
    ///
    /// # Panics
    ///
    /// Panics if `pattern` does not compile or does not match `text`.
    #[must_use]
    pub fn expand_first_capture_pair(
        pattern: &str,
        text: &str,
        gregex_template: &str,
        rust_template: &str,
    ) -> (String, String) {
        let compiled = regex::Regex::new(pattern).expect("pattern");
        let captures = compiled.captures(text).expect("match");
        let gregex = gregex_expand(gregex_template, &captures);
        let mut rust = String::new();
        captures.expand(rust_template, &mut rust);
        (gregex, rust)
    }
}
