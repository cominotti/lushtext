// SPDX-License-Identifier: GPL-3.0-or-later

//! The path half of GtkSourceView's language guess, as plain data a worker can
//! run (`bound-post-open-settle`, design D2).
//!
//! The application's language catalog (`ui/language_catalog.rs`) builds its
//! `GtkSourceLanguageManager` on a worker at startup and copies each
//! language's id and filename globs into a [`LanguageGlobTable`], in the
//! manager's own id order. The load planning worker then guesses a document's
//! language **id** from that table, so the activation dispatch neither loads
//! the language list nor guesses. At attach time the GTK thread's
//! `gtk_source_language_manager_guess_language` stays authoritative (review
//! R4); a widget test compares the two for every glob of every shipped
//! language.
//!
//! The rule transcribed is GtkSourceView 5.20's
//! `gtk_source_language_manager_guess_language` with no content type, as
//! LushText calls it (`gtksourcelanguagemanager.c`, `pick_langs_for_filename`):
//! the first language, in id order, with a non-empty glob that
//! `g_pattern_match_simple` matches against the **whole** file name string
//! LushText passes (the full path's display name). `g_pattern_match_simple`
//! knows only `*` (any run of characters, the empty one included) and `?`
//! (exactly one character); every other character, `[` included, is literal,
//! and nothing can be escaped.

/// One language's id and filename globs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanguageGlobs {
    /// The GtkSourceView language id.
    pub id: String,
    /// Its filename globs, as `gtk_source_language_get_globs` returns them.
    pub globs: Vec<String>,
}

/// Every language's globs, in the language manager's id order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LanguageGlobTable {
    languages: Vec<LanguageGlobs>,
}

impl LanguageGlobTable {
    /// A table of `languages`, which must be in the manager's id order.
    #[must_use]
    pub fn new(languages: Vec<LanguageGlobs>) -> Self {
        Self { languages }
    }

    /// The id of the language GtkSourceView guesses for `file_name` (the
    /// string LushText passes to `guess_language`: the full path), or `None`.
    #[must_use]
    pub fn guess(&self, file_name: &str) -> Option<&str> {
        self.languages
            .iter()
            .find(|language| {
                language
                    .globs
                    .iter()
                    .any(|glob| !glob.is_empty() && glob_matches(glob, file_name))
            })
            .map(|language| language.id.as_str())
    }

    /// How many languages the table holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.languages.len()
    }

    /// Whether the table holds no language.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.languages.is_empty()
    }

    /// Every language, in id order.
    pub fn languages(&self) -> impl Iterator<Item = &LanguageGlobs> {
        self.languages.iter()
    }
}

/// Whether `pattern` matches the whole of `text` under
/// `g_pattern_match_simple`'s rules: `*` matches any run of characters, `?`
/// exactly one character, and everything else itself.
#[must_use]
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    // The last `*` seen and the text position it is matched up to: on a
    // mismatch, that star absorbs one more character and matching resumes.
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some('?') => {
                p += 1;
                t += 1;
            }
            Some(&literal) if literal == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((star_p, star_t)) => {
                    p = star_p + 1;
                    t = star_t + 1;
                    star = Some((star_p, star_t + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> LanguageGlobTable {
        LanguageGlobTable::new(vec![
            LanguageGlobs {
                id: "c".into(),
                globs: vec!["*.c".into(), "*.h".into()],
            },
            LanguageGlobs {
                id: "cpp".into(),
                globs: vec!["*.cpp".into(), "*.h".into(), String::new()],
            },
            LanguageGlobs {
                id: "cmake".into(),
                globs: vec!["*.cmake".into(), "CMakeLists.txt".into()],
            },
            LanguageGlobs {
                id: "html".into(),
                globs: vec!["*.html".into(), "*.htm".into()],
            },
            LanguageGlobs {
                id: "makefile".into(),
                globs: vec!["[Mm]akefile".into(), "*.mk".into()],
            },
        ])
    }

    #[test]
    fn stars_and_question_marks_follow_g_pattern_match_simple() {
        assert!(glob_matches("*.html", "/home/a/index.html"));
        assert!(glob_matches("*.html", ".html"));
        assert!(!glob_matches("*.html", "/home/a/index.html.bak"));
        assert!(glob_matches("*", ""));
        assert!(glob_matches("a?c", "abc"));
        assert!(glob_matches("a?c", "a🙂c"));
        assert!(!glob_matches("a?c", "ac"));
        assert!(glob_matches("*a*b*", "xxaxxbxx"));
        assert!(!glob_matches("*a*b*", "xxbxxaxx"));
        assert!(glob_matches("**.rs", "lib.rs"));
        // No character classes and no escapes: `[` and `\` are literal.
        assert!(!glob_matches("[Mm]akefile", "Makefile"));
        assert!(glob_matches("[Mm]akefile", "[Mm]akefile"));
        assert!(glob_matches("a\\*", "a\\xyz"));
    }

    #[test]
    fn the_first_language_in_id_order_wins() {
        let table = table();
        // `*.h` is in both C and C++; C comes first.
        assert_eq!(table.guess("/src/x.h"), Some("c"));
        assert_eq!(table.guess("/src/x.cpp"), Some("cpp"));
        assert_eq!(
            table.guess("/w/page.HTML"),
            None,
            "globs are case-sensitive"
        );
        assert_eq!(table.guess("/w/page.htm"), Some("html"));
        // The whole path is matched, so a glob without a leading star never
        // matches a full path (GtkSourceView's own behaviour for LushText).
        assert_eq!(table.guess("/p/CMakeLists.txt"), None);
        assert_eq!(table.guess("CMakeLists.txt"), Some("cmake"));
        assert_eq!(table.guess("/p/notes.txt"), None);
        assert_eq!(table.len(), 5);
        assert!(!table.is_empty());
    }

    #[test]
    fn an_empty_glob_matches_nothing() {
        let table = LanguageGlobTable::new(vec![LanguageGlobs {
            id: "empty".into(),
            globs: vec![String::new()],
        }]);
        assert_eq!(table.guess(""), None);
        assert_eq!(table.guess("x"), None);
    }
}
