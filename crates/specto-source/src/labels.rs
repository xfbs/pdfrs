//! Page labels (`/PageLabels`): the page "numbers" printed in the document, such as "i, ii, iii"
//! followed by "1, 2, 3", or "A-1".

/// Numbering style of a label range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelStyle {
    /// 1, 2, 3
    Decimal,
    /// I, II, III
    UpperRoman,
    /// i, ii, iii
    LowerRoman,
    /// A to Z, then AA, BB and so on
    UpperLetters,
    /// a to z, then aa, bb and so on
    LowerLetters,
    /// Prefix only, no number.
    None,
}

/// One range: from page `start` on, labels are `prefix` followed by a number in `style`, counting
/// from `first`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelRange {
    pub start: usize,
    pub style: LabelStyle,
    pub prefix: String,
    pub first: u32,
}

/// The label of one page, in its parts: "A-3" is prefix "A-", number 3 in decimal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageLabel {
    pub style: LabelStyle,
    pub prefix: String,
    /// Not shown in style `None`.
    pub number: u32,
}

impl PageLabel {
    /// The plain page number `n`.
    pub fn number(n: usize) -> Self {
        Self {
            style: LabelStyle::Decimal,
            prefix: String::new(),
            number: n.min(u32::MAX as usize) as u32,
        }
    }

    /// The label as shown: "iv", "A-3".
    pub fn text(&self) -> String {
        let n = u64::from(self.number);
        let number = match self.style {
            LabelStyle::Decimal => n.to_string(),
            LabelStyle::UpperRoman => roman(n),
            LabelStyle::LowerRoman => roman(n).to_lowercase(),
            LabelStyle::UpperLetters => letters(n),
            LabelStyle::LowerLetters => letters(n).to_lowercase(),
            LabelStyle::None => String::new(),
        };
        format!("{}{number}", self.prefix)
    }
}

/// The labels of a document.
///
/// Pages before the first range (if a broken file doesn't start at 0) get plain decimal numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageLabels {
    /// Sorted by `start`, without duplicates.
    ranges: Vec<LabelRange>,
    page_count: usize,
}

impl PageLabels {
    /// Builds the labels from `ranges`, which may be in any order.
    ///
    /// Ranges starting beyond the last page are dropped. Returns `None` if nothing remains.
    pub fn new(mut ranges: Vec<LabelRange>, page_count: usize) -> Option<Self> {
        ranges.retain(|r| r.start < page_count);
        ranges.sort_by_key(|r| r.start);
        ranges.dedup_by_key(|r| r.start);
        (!ranges.is_empty()).then_some(Self { ranges, page_count })
    }

    /// The label of page `index` (zero-based).
    pub fn label(&self, index: usize) -> String {
        self.page(index).text()
    }

    /// The label of page `index`, in its parts.
    pub fn page(&self, index: usize) -> PageLabel {
        let i = self.ranges.partition_point(|r| r.start <= index);
        let Some(range) = i.checked_sub(1).map(|i| &self.ranges[i]) else {
            return PageLabel::number(index + 1);
        };
        let n = range.first as u64 + (index - range.start) as u64;
        PageLabel {
            style: range.style,
            prefix: range.prefix.clone(),
            number: n.min(u32::MAX as u64) as u32,
        }
    }

    /// The labels of pages labeled one by one, as ranges: a range goes on while the labels continue
    /// each other, or `None` for no pages.
    fn from_pages(labels: &[PageLabel]) -> Option<Self> {
        let mut ranges: Vec<LabelRange> = Vec::new();
        for (i, label) in labels.iter().enumerate() {
            let continues = i > 0 && {
                let before = &labels[i - 1];
                before.style == label.style
                    && before.prefix == label.prefix
                    && (label.style == LabelStyle::None
                        || before.number.checked_add(1) == Some(label.number))
            };
            if !continues {
                ranges.push(LabelRange {
                    start: i,
                    style: label.style,
                    prefix: label.prefix.clone(),
                    first: label.number,
                });
            }
        }
        Self::new(ranges, labels.len())
    }

    /// The labels of a document put together from pages of several files.
    ///
    /// `pages` has, for each page, the labels of its file (if it has any worth showing) and its
    /// page there. Each page keeps the label it has in its file. Pages of files without labels are
    /// numbered by their position. `None` if no page has a label of its own, or the result is just
    /// 1, 2, 3 and so on.
    pub fn of_pages<'a>(
        pages: impl IntoIterator<Item = (Option<&'a PageLabels>, usize)>,
    ) -> Option<Self> {
        let mut any = false;
        let labels: Vec<PageLabel> = pages
            .into_iter()
            .enumerate()
            .map(|(position, (labels, page))| match labels {
                Some(l) => {
                    any = true;
                    l.page(page)
                }
                None => PageLabel::number(position + 1),
            })
            .collect();
        if !any {
            return None;
        }
        Self::from_pages(&labels).filter(|l| !l.is_plain())
    }

    /// The first page whose label is `text`: exactly, then ignoring case.
    pub fn find(&self, text: &str) -> Option<usize> {
        let text = text.trim();
        let labels: Vec<String> = (0..self.page_count).map(|i| self.label(i)).collect();
        labels
            .iter()
            .position(|l| l == text)
            .or_else(|| labels.iter().position(|l| l.eq_ignore_ascii_case(text)))
    }

    /// Whether every label is just the page number (1, 2, 3 and so on), in which case there's no
    /// point showing labels.
    pub fn is_plain(&self) -> bool {
        (0..self.page_count).all(|i| self.label(i) == (i + 1).to_string())
    }

    pub fn ranges(&self) -> &[LabelRange] {
        &self.ranges
    }
}

/// Roman numerals, with `M` repeated for large numbers, as other viewers do.
fn roman(mut n: u64) -> String {
    if n == 0 {
        return String::new();
    }
    const DIGITS: [(u64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    // Absurdly large values from broken files: don't build huge strings.
    n = n.min(100_000);
    let mut out = String::new();
    for (value, digits) in DIGITS {
        while n >= value {
            out.push_str(digits);
            n -= value;
        }
    }
    out
}

/// A to Z, then AA to ZZ, then AAA and so on: the letter repeated, as the specification says.
fn letters(n: u64) -> String {
    if n == 0 {
        return String::new();
    }
    let letter = (b'A' + ((n - 1) % 26) as u8) as char;
    let count = ((n - 1) / 26 + 1).min(1000) as usize;
    std::iter::repeat_n(letter, count).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(start: usize, style: LabelStyle, prefix: &str, first: u32) -> LabelRange {
        LabelRange {
            start,
            style,
            prefix: prefix.into(),
            first,
        }
    }

    #[test]
    fn styles() {
        assert_eq!(roman(1994), "MCMXCIV");
        assert_eq!(roman(4), "IV");
        assert_eq!(letters(1), "A");
        assert_eq!(letters(26), "Z");
        assert_eq!(letters(27), "AA");
        assert_eq!(letters(53), "AAA");
    }

    #[test]
    fn book_with_front_matter_and_appendix() {
        // i to iv, then 1 to 5, then A-1 and on.
        let labels = PageLabels::new(
            vec![
                range(9, LabelStyle::Decimal, "A-", 1),
                range(0, LabelStyle::LowerRoman, "", 1),
                range(4, LabelStyle::Decimal, "", 1),
            ],
            11,
        )
        .unwrap();
        let all: Vec<String> = (0..11).map(|i| labels.label(i)).collect();
        assert_eq!(
            all,
            [
                "i", "ii", "iii", "iv", "1", "2", "3", "4", "5", "A-1", "A-2"
            ]
        );
        assert_eq!(labels.find("iv"), Some(3));
        assert_eq!(labels.find("IV"), Some(3)); // case-insensitive fallback
        assert_eq!(labels.find("3"), Some(6));
        assert_eq!(labels.find("A-2"), Some(10));
        assert_eq!(labels.find("nope"), None);
        assert!(!labels.is_plain());
    }

    #[test]
    fn labels_of_pages_put_together() {
        // i to iv, 1 to 5, A-1, A-2, as above.
        let book = PageLabels::new(
            vec![
                range(0, LabelStyle::LowerRoman, "", 1),
                range(4, LabelStyle::Decimal, "", 1),
                range(9, LabelStyle::Decimal, "A-", 1),
            ],
            11,
        )
        .unwrap();
        // ii deleted, 3 moved to the end, two pages of an unlabeled file inserted after iv.
        let pages = [0, 2, 3]
            .into_iter()
            .map(|p| (Some(&book), p))
            .chain([(None, 0), (None, 1)])
            .chain([4, 5, 7, 8, 9, 10, 6].into_iter().map(|p| (Some(&book), p)));
        let labels = PageLabels::of_pages(pages).unwrap();
        let all: Vec<String> = (0..12).map(|i| labels.label(i)).collect();
        assert_eq!(
            all,
            [
                "i", "iii", "iv", "4", "5", "1", "2", "4", "5", "A-1", "A-2", "3"
            ]
        );
        // As few ranges as that takes: i, iii to iv, 4 to 5, 1 to 2, 4 to 5, A-1 to A-2, 3.
        assert_eq!(labels.ranges().len(), 7);
        // Kept in order, the same ranges as the file's.
        let same = PageLabels::of_pages((0..11).map(|p| (Some(&book), p))).unwrap();
        assert_eq!(same.ranges(), book.ranges());
        // Nothing labeled: no labels.
        assert!(PageLabels::of_pages([(None, 0), (None, 1)]).is_none());
        // Prefix-only labels continue whatever the number.
        let covers = PageLabels::from_pages(&[
            PageLabel {
                style: LabelStyle::None,
                prefix: "Cover".into(),
                number: 1,
            },
            PageLabel {
                style: LabelStyle::None,
                prefix: "Cover".into(),
                number: 7,
            },
        ])
        .unwrap();
        assert_eq!(covers.ranges().len(), 1);
    }

    #[test]
    fn edge_cases() {
        // A range that doesn't start at page 0: earlier pages are numbered.
        let l = PageLabels::new(vec![range(2, LabelStyle::None, "Cover", 1)], 4).unwrap();
        assert_eq!(
            (0..4).map(|i| l.label(i)).collect::<Vec<_>>(),
            ["1", "2", "Cover", "Cover"]
        );
        // Plain decimal labels are reported as such.
        let plain = PageLabels::new(vec![range(0, LabelStyle::Decimal, "", 1)], 5).unwrap();
        assert!(plain.is_plain());
        // Ranges past the end are dropped.
        assert!(PageLabels::new(vec![range(9, LabelStyle::Decimal, "", 1)], 5).is_none());
    }
}
