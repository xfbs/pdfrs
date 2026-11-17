//! Text in annotations: text boxes are set in one of the fonts every PDF program has (Helvetica,
//! Times and Courier, each also bold and italic), so nothing needs embedding.
//!
//! That limits them to the characters of Windows' Latin code page for now.
//!
//! The same line breaks are used wherever a text box is measured or written, so its text sits the
//! same on screen and in the file.

/// A family of fonts.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum FontFamily {
    /// Helvetica.
    #[default]
    Sans,
    /// Times.
    Serif,
    /// Courier.
    Mono,
}

/// The font of a text box: one of the twelve that need no embedding.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct TextFont {
    pub family: FontFamily,
    pub bold: bool,
    pub italic: bool,
}

impl TextFont {
    /// What PDF calls the font (`/BaseFont`).
    pub fn pdf_name(self) -> &'static str {
        match (self.family, self.bold, self.italic) {
            (FontFamily::Sans, false, false) => "Helvetica",
            (FontFamily::Sans, true, false) => "Helvetica-Bold",
            (FontFamily::Sans, false, true) => "Helvetica-Oblique",
            (FontFamily::Sans, true, true) => "Helvetica-BoldOblique",
            (FontFamily::Serif, false, false) => "Times-Roman",
            (FontFamily::Serif, true, false) => "Times-Bold",
            (FontFamily::Serif, false, true) => "Times-Italic",
            (FontFamily::Serif, true, true) => "Times-BoldItalic",
            (FontFamily::Mono, false, false) => "Courier",
            (FontFamily::Mono, true, false) => "Courier-Bold",
            (FontFamily::Mono, false, true) => "Courier-Oblique",
            (FontFamily::Mono, true, true) => "Courier-BoldOblique",
        }
    }

    /// The short name the font goes by in a text box's resources and its default appearance string:
    /// the ones other programs use too.
    pub fn resource_name(self) -> &'static str {
        match (self.family, self.bold, self.italic) {
            (FontFamily::Sans, false, false) => "Helv",
            (FontFamily::Sans, true, false) => "HeBo",
            (FontFamily::Sans, false, true) => "HeOb",
            (FontFamily::Sans, true, true) => "HeBO",
            (FontFamily::Serif, false, false) => "TiRo",
            (FontFamily::Serif, true, false) => "TiBo",
            (FontFamily::Serif, false, true) => "TiIt",
            (FontFamily::Serif, true, true) => "TiBI",
            (FontFamily::Mono, false, false) => "Cour",
            (FontFamily::Mono, true, false) => "CoBo",
            (FontFamily::Mono, false, true) => "CoOb",
            (FontFamily::Mono, true, true) => "CoBO",
        }
    }

    /// The font that goes by `name` in a text box's default appearance string (see
    /// [`resource_name`](Self::resource_name)).
    pub fn from_resource_name(name: &str) -> Option<Self> {
        let families = [FontFamily::Sans, FontFamily::Serif, FontFamily::Mono];
        families
            .into_iter()
            .flat_map(|family| {
                [(false, false), (true, false), (false, true), (true, true)].map(
                    |(bold, italic)| Self {
                        family,
                        bold,
                        italic,
                    },
                )
            })
            .find(|f| f.resource_name() == name)
    }

    /// The width of each character from code 32 on, in thousandths of the font size, or `None` for
    /// Courier, where every character is 600 wide.
    ///
    /// (Helvetica's slanted fonts are as wide as its upright ones.)
    fn widths(self) -> Option<&'static [u16; 224]> {
        match (self.family, self.bold, self.italic) {
            (FontFamily::Sans, false, _) => Some(&HELVETICA),
            (FontFamily::Sans, true, _) => Some(&HELVETICA_BOLD),
            (FontFamily::Serif, false, false) => Some(&TIMES),
            (FontFamily::Serif, true, false) => Some(&TIMES_BOLD),
            (FontFamily::Serif, false, true) => Some(&TIMES_ITALIC),
            (FontFamily::Serif, true, true) => Some(&TIMES_BOLD_ITALIC),
            (FontFamily::Mono, _, _) => None,
        }
    }

    /// How far above the baseline the family's tallest letters reach, and how far below it the
    /// longest tails, as parts of the font size.
    fn ascent_descent(self) -> (f64, f64) {
        match self.family {
            FontFamily::Sans => (0.718, 0.207),
            FontFamily::Serif => (0.683, 0.217),
            FontFamily::Mono => (0.629, 0.157),
        }
    }
}

// The widths of the fonts' characters, from their metrics as Adobe publishes them. Codes are those
// of Windows' Latin code page (`WinAnsiEncoding`).

#[rustfmt::skip]
const HELVETICA: [u16; 224] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556,
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556,
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556,
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, 0,
    556, 0, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0,
    0, 222, 222, 333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 0, 500, 667,
    278, 333, 556, 556, 556, 556, 260, 556, 333, 737, 370, 556, 584, 333, 737, 333,
    400, 584, 333, 333, 333, 556, 537, 278, 333, 333, 365, 556, 834, 834, 834, 611,
    667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278,
    722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611,
    556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500,
];

#[rustfmt::skip]
const HELVETICA_BOLD: [u16; 224] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278,
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611,
    975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778,
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556,
    333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611,
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584, 0,
    556, 0, 278, 556, 500, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0,
    0, 278, 278, 500, 500, 350, 556, 1000, 333, 1000, 556, 333, 944, 0, 500, 667,
    278, 333, 556, 556, 556, 556, 280, 556, 333, 737, 370, 556, 584, 333, 737, 333,
    400, 584, 333, 333, 333, 611, 556, 278, 333, 333, 365, 556, 834, 834, 834, 611,
    722, 722, 722, 722, 722, 722, 1000, 722, 667, 667, 667, 667, 278, 278, 278, 278,
    722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667, 611,
    556, 556, 556, 556, 556, 556, 889, 556, 556, 556, 556, 556, 278, 278, 278, 278,
    611, 611, 611, 611, 611, 611, 611, 584, 611, 611, 611, 611, 611, 556, 611, 556,
];

#[rustfmt::skip]
const TIMES: [u16; 224] = [
    250, 333, 408, 500, 500, 833, 778, 180, 333, 333, 500, 564, 250, 333, 250, 278,
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 278, 278, 564, 564, 564, 444,
    921, 722, 667, 667, 722, 611, 556, 722, 722, 333, 389, 722, 611, 889, 722, 722,
    556, 722, 667, 556, 611, 722, 722, 944, 722, 722, 611, 333, 278, 333, 469, 500,
    333, 444, 500, 444, 500, 444, 333, 500, 500, 278, 278, 500, 278, 778, 500, 500,
    500, 500, 333, 389, 278, 500, 500, 722, 500, 500, 444, 480, 200, 480, 541, 0,
    500, 0, 333, 500, 444, 1000, 500, 500, 333, 1000, 556, 333, 889, 0, 611, 0,
    0, 333, 333, 444, 444, 350, 500, 1000, 333, 980, 389, 333, 722, 0, 444, 722,
    250, 333, 500, 500, 500, 500, 200, 500, 333, 760, 276, 500, 564, 333, 760, 333,
    400, 564, 300, 300, 333, 500, 453, 250, 333, 300, 310, 500, 750, 750, 750, 444,
    722, 722, 722, 722, 722, 722, 889, 667, 611, 611, 611, 611, 333, 333, 333, 333,
    722, 722, 722, 722, 722, 722, 722, 564, 722, 722, 722, 722, 722, 722, 556, 500,
    444, 444, 444, 444, 444, 444, 667, 444, 444, 444, 444, 444, 278, 278, 278, 278,
    500, 500, 500, 500, 500, 500, 500, 564, 500, 500, 500, 500, 500, 500, 500, 500,
];

#[rustfmt::skip]
const TIMES_BOLD: [u16; 224] = [
    250, 333, 555, 500, 500, 1000, 833, 278, 333, 333, 500, 570, 250, 333, 250, 278,
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500,
    930, 722, 667, 722, 722, 667, 611, 778, 778, 389, 500, 778, 667, 944, 722, 778,
    611, 778, 722, 556, 667, 722, 722, 1000, 722, 722, 667, 333, 278, 333, 581, 500,
    333, 500, 556, 444, 556, 444, 333, 500, 556, 278, 333, 556, 278, 833, 556, 500,
    556, 556, 444, 389, 333, 556, 500, 722, 500, 500, 444, 394, 220, 394, 520, 0,
    500, 0, 333, 500, 500, 1000, 500, 500, 333, 1000, 556, 333, 1000, 0, 667, 0,
    0, 333, 333, 500, 500, 350, 500, 1000, 333, 1000, 389, 333, 722, 0, 444, 722,
    250, 333, 500, 500, 500, 500, 220, 500, 333, 747, 300, 500, 570, 333, 747, 333,
    400, 570, 300, 300, 333, 556, 540, 250, 333, 300, 330, 500, 750, 750, 750, 500,
    722, 722, 722, 722, 722, 722, 1000, 722, 667, 667, 667, 667, 389, 389, 389, 389,
    722, 722, 778, 778, 778, 778, 778, 570, 778, 722, 722, 722, 722, 722, 611, 556,
    500, 500, 500, 500, 500, 500, 722, 444, 444, 444, 444, 444, 278, 278, 278, 278,
    500, 556, 500, 500, 500, 500, 500, 570, 500, 556, 556, 556, 556, 500, 556, 500,
];

#[rustfmt::skip]
const TIMES_ITALIC: [u16; 224] = [
    250, 333, 420, 500, 500, 833, 778, 214, 333, 333, 500, 675, 250, 333, 250, 278,
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 675, 675, 675, 500,
    920, 611, 611, 667, 722, 611, 611, 722, 722, 333, 444, 667, 556, 833, 667, 722,
    611, 722, 611, 500, 556, 722, 611, 833, 611, 556, 556, 389, 278, 389, 422, 500,
    333, 500, 500, 444, 500, 444, 278, 500, 500, 278, 278, 444, 278, 722, 500, 500,
    500, 500, 389, 389, 278, 500, 444, 667, 444, 444, 389, 400, 275, 400, 541, 0,
    500, 0, 333, 500, 556, 889, 500, 500, 333, 1000, 500, 333, 944, 0, 556, 0,
    0, 333, 333, 556, 556, 350, 500, 889, 333, 980, 389, 333, 667, 0, 389, 556,
    250, 389, 500, 500, 500, 500, 275, 500, 333, 760, 276, 500, 675, 333, 760, 333,
    400, 675, 300, 300, 333, 500, 523, 250, 333, 300, 310, 500, 750, 750, 750, 500,
    611, 611, 611, 611, 611, 611, 889, 667, 611, 611, 611, 611, 333, 333, 333, 333,
    722, 667, 722, 722, 722, 722, 722, 675, 722, 722, 722, 722, 722, 556, 611, 500,
    500, 500, 500, 500, 500, 500, 667, 444, 444, 444, 444, 444, 278, 278, 278, 278,
    500, 500, 500, 500, 500, 500, 500, 675, 500, 500, 500, 500, 500, 444, 500, 444,
];

#[rustfmt::skip]
const TIMES_BOLD_ITALIC: [u16; 224] = [
    250, 389, 555, 500, 500, 833, 778, 278, 333, 333, 500, 570, 250, 333, 250, 278,
    500, 500, 500, 500, 500, 500, 500, 500, 500, 500, 333, 333, 570, 570, 570, 500,
    832, 667, 667, 667, 722, 667, 667, 722, 778, 389, 500, 667, 611, 889, 722, 722,
    611, 722, 667, 556, 611, 722, 667, 889, 667, 611, 611, 333, 278, 333, 570, 500,
    333, 500, 500, 444, 500, 444, 333, 500, 556, 278, 278, 500, 278, 778, 556, 500,
    500, 500, 389, 389, 278, 556, 444, 667, 500, 444, 389, 348, 220, 348, 570, 0,
    500, 0, 333, 500, 500, 1000, 500, 500, 333, 1000, 556, 333, 944, 0, 611, 0,
    0, 333, 333, 500, 500, 350, 500, 1000, 333, 1000, 389, 333, 722, 0, 389, 611,
    250, 389, 500, 500, 500, 500, 220, 500, 333, 747, 266, 500, 606, 333, 747, 333,
    400, 570, 300, 300, 333, 576, 500, 250, 333, 300, 300, 500, 750, 750, 750, 500,
    667, 667, 667, 667, 667, 667, 944, 667, 667, 667, 667, 667, 389, 389, 389, 389,
    722, 722, 722, 722, 722, 722, 722, 570, 722, 722, 722, 722, 722, 611, 611, 500,
    500, 500, 500, 500, 500, 500, 722, 444, 444, 444, 444, 444, 278, 278, 278, 278,
    500, 556, 500, 500, 500, 500, 500, 570, 500, 556, 556, 556, 556, 444, 500, 444,
];

/// The distance between baselines, as a multiple of the font size.
const LEADING: f64 = 1.2;

/// The space between a text box's edge and its text, in points.
pub const TEXT_MARGIN: f64 = 2.0;

/// The byte for `c` in Windows' Latin code page, which text boxes are written in, or `None` for
/// characters it doesn't have.
pub fn winansi(c: char) -> Option<u8> {
    let code = c as u32;
    match code {
        0x20..=0x7E | 0xA0..=0xFF => Some(code as u8),
        _ => {
            const HIGH: [(char, u8); 27] = [
                ('€', 0x80),
                ('‚', 0x82),
                ('ƒ', 0x83),
                ('„', 0x84),
                ('…', 0x85),
                ('†', 0x86),
                ('‡', 0x87),
                ('ˆ', 0x88),
                ('‰', 0x89),
                ('Š', 0x8A),
                ('‹', 0x8B),
                ('Œ', 0x8C),
                ('Ž', 0x8E),
                ('‘', 0x91),
                ('’', 0x92),
                ('“', 0x93),
                ('”', 0x94),
                ('•', 0x95),
                ('–', 0x96),
                ('—', 0x97),
                ('˜', 0x98),
                ('™', 0x99),
                ('š', 0x9A),
                ('›', 0x9B),
                ('œ', 0x9C),
                ('ž', 0x9E),
                ('Ÿ', 0x9F),
            ];
            HIGH.iter().find(|(h, _)| *h == c).map(|(_, b)| *b)
        }
    }
}

/// `text` as the bytes a text box is written in: characters the code page doesn't have become
/// question marks.
pub fn encode(text: &str) -> Vec<u8> {
    text.chars().map(|c| winansi(c).unwrap_or(b'?')).collect()
}

/// How wide `text` is in `font` at font size `size`, in points.
pub fn text_width(text: &str, size: f64, font: TextFont) -> f64 {
    let widths = font.widths();
    let thousandths: u32 = encode(text)
        .iter()
        .map(|&b| match widths {
            Some(w) => u32::from(w[usize::from(b.max(32)) - 32]),
            None => 600,
        })
        .sum();
    f64::from(thousandths) * size / 1000.0
}

/// `text` broken into lines no wider than `width` in `font` at font size `size`: at its own line
/// breaks, and between words where a line would get too long. A word longer than a line is broken
/// where it has to be.
pub fn wrap(text: &str, size: f64, width: f64, font: TextFont) -> Vec<String> {
    let wide = |s: &str| text_width(s, size, font);
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let with_word = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if wide(&with_word) <= width || line.is_empty() {
                line = with_word;
            } else {
                lines.push(std::mem::replace(&mut line, word.to_owned()));
            }
            // A word too long for a line of its own.
            while wide(&line) > width && line.chars().count() > 1 {
                let mut fits = 0;
                for (i, _) in line.char_indices().skip(1) {
                    if wide(&line[..i]) > width {
                        break;
                    }
                    fits = i;
                }
                let at = if fits == 0 {
                    line.char_indices().nth(1).map_or(line.len(), |(i, _)| i)
                } else {
                    fits
                };
                let rest = line.split_off(at);
                lines.push(std::mem::replace(&mut line, rest));
            }
        }
        lines.push(line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// How high a text box of `lines` lines in `font` at font size `size` is, margins included.
pub fn box_height(lines: usize, size: f64, font: TextFont) -> f64 {
    let lines = lines.max(1) as f64;
    let (ascent, descent) = font.ascent_descent();
    2.0 * TEXT_MARGIN + (ascent + descent) * size + (lines - 1.0) * LEADING * size
}

/// Where the first baseline of a text box is below its top edge, and how far apart baselines are.
pub(crate) fn baselines(size: f64, font: TextFont) -> (f64, f64) {
    (TEXT_MARGIN + font.ascent_descent().0 * size, LEADING * size)
}

#[cfg(test)]
mod tests {
    use super::{FontFamily, TextFont, box_height, encode, text_width, wrap};

    const SANS: TextFont = TextFont {
        family: FontFamily::Sans,
        bold: false,
        italic: false,
    };

    #[test]
    fn widths_and_bytes() {
        // Helvetica: a space is 278, "A" 667, "i" 222 thousandths.
        assert!((text_width("A i", 10.0, SANS) - 11.67).abs() < 1e-9);
        // Bold is wider, Times narrower, and Courier the same for all.
        let bold = TextFont { bold: true, ..SANS };
        let serif = TextFont {
            family: FontFamily::Serif,
            ..SANS
        };
        let mono = TextFont {
            family: FontFamily::Mono,
            ..bold
        };
        assert!(text_width("A i", 10.0, bold) > text_width("A i", 10.0, SANS));
        assert!(text_width("minimum", 10.0, serif) < text_width("minimum", 10.0, SANS));
        assert!((text_width("A i", 10.0, mono) - 18.0).abs() < 1e-9);
        assert_eq!(encode("Größe “x” €"), b"Gr\xf6\xdfe \x93x\x94 \x80");
        assert_eq!(encode("日本"), b"??");
    }

    #[test]
    fn fonts_by_name() {
        let font = TextFont {
            family: FontFamily::Serif,
            bold: true,
            italic: true,
        };
        assert_eq!(font.pdf_name(), "Times-BoldItalic");
        assert_eq!(
            TextFont::from_resource_name(font.resource_name()),
            Some(font)
        );
        assert_eq!(TextFont::from_resource_name("Helv"), Some(SANS));
        assert_eq!(TextFont::from_resource_name("F1"), None);
    }

    #[test]
    fn wrapping() {
        // "aaa" is 16.68 wide at size 10, a space 2.78.
        assert_eq!(wrap("aaa aaa aaa", 10.0, 40.0, SANS), ["aaa aaa", "aaa"]);
        assert_eq!(wrap("aaa\n\naaa", 10.0, 40.0, SANS), ["aaa", "", "aaa"]);
        assert_eq!(wrap("", 10.0, 40.0, SANS), [""]);
        // Too long for a line: broken where it must be.
        assert_eq!(wrap("aaaaaaaa", 10.0, 17.0, SANS), ["aaa", "aaa", "aa"]);
        assert_eq!(wrap("W", 10.0, 1.0, SANS), ["W"]);
        assert!(box_height(2, 10.0, SANS) > box_height(1, 10.0, SANS));
    }
}
