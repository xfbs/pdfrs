//! Searching a page's text.
//!
//! [`SearchText`] is a compact form of a page for matching: the text folded for comparison
//! (lowercase, accents removed, typographic quotes made plain), words separated by single spaces,
//! lines too, and a word broken by a hyphen at the end of a line joined again. Each byte maps back
//! to the character it came from, so matches become character ranges, which the text page turns
//! into highlights. It takes about 5 bytes per character, so a whole document's worth can be kept,
//! unlike the full [`TextPage`]s.
//!
//! Matching follows content order (like selection), so right-to-left text drawn in visual order
//! isn't found yet.

use std::ops::Range;

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::TextPage;

/// A page's text prepared for searching.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchText {
    /// The folded text.
    text: String,
    /// For each byte of `text`, the character of the page it came from.
    map: Vec<u32>,
}

/// What to search for, folded like [`SearchText`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query(String);

impl Query {
    /// Folds `text` for matching, or returns `None` if nothing is left to search for.
    pub fn new(text: &str) -> Option<Self> {
        let mut out = String::new();
        for word in text.split_whitespace() {
            if !out.is_empty() {
                out.push(' ');
            }
            word.chars().for_each(|c| fold(c, |f| out.push(f)));
        }
        (!out.is_empty()).then_some(Self(out))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl SearchText {
    pub fn new(page: &TextPage) -> Self {
        let mut s = Self::default();
        for (n, line) in page.lines.iter().enumerate() {
            if n > 0 {
                // A word broken over two lines with a hyphen is joined. Otherwise lines are
                // separated like words.
                if s.ends_with_hyphen() && starts_with_lowercase(page, line) {
                    s.pop_char();
                } else {
                    s.push(' ', line.chars.start);
                }
            }
            for (k, word) in page.words[line.words.clone()].iter().enumerate() {
                if k > 0 {
                    s.push(' ', word.chars.start);
                }
                for i in word.chars.clone() {
                    for c in page.chars[i].text.chars() {
                        fold(c, |f| s.push(f, i));
                    }
                }
            }
        }
        s
    }

    /// The characters of each match of `query`, in order, not overlapping.
    pub fn find(&self, query: &Query) -> Vec<Range<usize>> {
        self.text
            .match_indices(query.as_str())
            .map(|(at, m)| {
                let start = self.map[at] as usize;
                let end = self.map[at + m.len() - 1] as usize + 1;
                start..end
            })
            .collect()
    }

    /// The folded text (for tests and debugging).
    pub fn as_str(&self) -> &str {
        &self.text
    }

    fn push(&mut self, c: char, from: usize) {
        self.text.push(c);
        let from = u32::try_from(from).unwrap_or(u32::MAX);
        self.map.resize(self.text.len(), from);
    }

    fn ends_with_hyphen(&self) -> bool {
        self.text
            .chars()
            .next_back()
            .is_some_and(|c| matches!(c, '-' | '\u{00AD}' | '\u{2010}'))
    }

    fn pop_char(&mut self) {
        if self.text.pop().is_some() {
            self.map.truncate(self.text.len());
        }
    }
}

fn starts_with_lowercase(page: &TextPage, line: &crate::Line) -> bool {
    page.chars[line.chars.clone()]
        .iter()
        .flat_map(|c| c.text.chars())
        .next()
        .is_some_and(char::is_lowercase)
}

/// Folds one character for comparison: lowercase, compatibility forms decomposed (ligatures,
/// full-width forms), accents dropped, typographic quotes made plain. Can produce several
/// characters ("ﬁ" becomes "fi").
fn fold(c: char, mut out: impl FnMut(char)) {
    for lower in c.to_lowercase() {
        for d in lower.nfkd().filter(|&d| !is_combining_mark(d)) {
            out(match d {
                '\u{2018}' | '\u{2019}' | '\u{201B}' | '\u{2032}' => '\'',
                '\u{201C}' | '\u{201D}' | '\u{201F}' | '\u{2033}' => '"',
                d => d,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding() {
        let q = |s: &str| Query::new(s).map(|q| q.0);
        assert_eq!(q("  Café   Crème "), Some("cafe creme".into()));
        assert_eq!(q("ﬁne"), Some("fine".into()));
        assert_eq!(q("don’t"), Some("don't".into()));
        assert_eq!(q("ÄÖÜ ß"), Some("aou ß".into()));
        assert_eq!(q(" \t "), None);
    }
}
