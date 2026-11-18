//! Annotations: markup that lies over a page without being part of its content (highlights,
//! drawings, shapes, notes).
//!
//! An [`Annotation`] here is plain data in the coordinates of its page of the file (PDF space), so
//! it stays where it is when the page is rotated or moved in the document. [`import()`] reads the
//! ones a file already has. What a document's annotations currently are (those of its files, minus
//! the deleted, plus the new) is kept by `specto-doc`. This crate knows nothing of documents.
//!
//! Not every annotation in a file is markup: links belong to `specto-source`, form fields
//! belong to the forms crate, and popups are windows of other annotations. [`import()`] leaves those
//! out, and saving leaves them alone.

mod import;
mod photo;
mod shapes;
mod stored;
mod text;

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specto_geom::{PdfBox, PdfPoint, PdfQuad};
pub use specto_source::Date;

pub use import::import;
pub use photo::{InkSettings, ink_from_photo};
pub use shapes::{
    AppearanceStream, GraphicsState, Path, Segment, Shape, appearance, look_mark, shapes,
};
pub use stored::with_image_loader;
pub use text::{
    FontFamily, TEXT_MARGIN, TextFont, box_height, encode as encode_text, text_width, wrap,
};

/// How wide and high the mark of a note is, in points.
pub const NOTE_SIZE: f64 = 18.0;

/// A color as PDF annotations have it: red, green and blue, each 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }
}

/// How text is marked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarkupStyle {
    Highlight,
    Underline,
    StrikeOut,
    /// A wavy underline. Read from files, and nothing here makes one.
    Squiggly,
}

/// What an annotation is, with what only that kind has.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AnnotKind {
    /// Marked text: one quad per stretch of text, usually one per line.
    TextMarkup {
        style: MarkupStyle,
        quads: Vec<PdfQuad>,
    },
    /// Freehand strokes, each a run of points.
    Ink {
        strokes: Vec<Vec<PdfPoint>>,
    },
    /// A rectangle along the annotation's `rect`.
    Square {
        fill: Option<Color>,
    },
    /// An ellipse inside the annotation's `rect`.
    Circle {
        fill: Option<Color>,
    },
    Line {
        from: PdfPoint,
        to: PdfPoint,
    },
    /// A note: a small mark on the page whose text (the annotation's `contents`) is read elsewhere.
    ///
    /// Only for notes made here. One of another program is [`AnnotKind::Other`].
    Note,
    /// A box of text on the page (the annotation's `contents`), set in `font` at `size` points in
    /// the annotation's color, broken into lines to fit the width of its `rect`. Only for text
    /// boxes made here.
    FreeText {
        size: f64,
        font: TextFont,
    },
    /// A picture placed on the page: the photo of a signature, for instance.
    ///
    /// Only for stamps made here. One of another program is [`AnnotKind::Other`].
    Stamp(StampArt),
    /// Markup of a kind this crate has no parameters for (a text box, a note, a stamp, a polygon
    /// and so on), named by its `/Subtype`.
    ///
    /// It is shown as the file draws it, and can be deleted but not changed.
    Other {
        subtype: String,
    },
}

/// What a stamp shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StampArt {
    /// An image, shared rather than copied when the annotation is.
    Image(Arc<StampImage>),
    /// The image the file has for it (a stamp written here and read again).
    ///
    /// It stretches to the annotation's area by itself, so the stamp can be moved and resized
    /// without the image in hand.
    InFile,
}

/// The pixels of an image stamp: RGBA, 8 bits each, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StampImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A kind without its parameters, for naming an annotation ("Undo Highlight") and for choosing how
/// to treat it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnnotType {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
    Ink,
    Square,
    Circle,
    Line,
    Note,
    FreeText,
    Stamp,
    Other,
}

impl AnnotKind {
    pub fn annot_type(&self) -> AnnotType {
        match self {
            Self::TextMarkup { style, .. } => match style {
                MarkupStyle::Highlight => AnnotType::Highlight,
                MarkupStyle::Underline => AnnotType::Underline,
                MarkupStyle::StrikeOut => AnnotType::StrikeOut,
                MarkupStyle::Squiggly => AnnotType::Squiggly,
            },
            Self::Ink { .. } => AnnotType::Ink,
            Self::Square { .. } => AnnotType::Square,
            Self::Circle { .. } => AnnotType::Circle,
            Self::Line { .. } => AnnotType::Line,
            Self::Note => AnnotType::Note,
            Self::FreeText { .. } => AnnotType::FreeText,
            Self::Stamp(_) => AnnotType::Stamp,
            Self::Other { .. } => AnnotType::Other,
        }
    }
}

/// The `/F` flags of an annotation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flags(pub u32);

impl Flags {
    pub const HIDDEN: u32 = 1 << 1;
    pub const PRINT: u32 = 1 << 2;
    pub const NO_VIEW: u32 = 1 << 5;
    pub const READ_ONLY: u32 = 1 << 6;
    pub const LOCKED: u32 = 1 << 7;

    /// What a new annotation gets: it is printed with the page.
    pub const NEW: Self = Self(Self::PRINT);

    /// Whether the annotation is shown on screen.
    pub fn is_visible(self) -> bool {
        self.0 & (Self::HIDDEN | Self::NO_VIEW) == 0
    }

    /// Whether the file allows changing or deleting the annotation.
    pub fn is_editable(self) -> bool {
        self.0 & (Self::READ_ONLY | Self::LOCKED) == 0
    }
}

/// Where in its file an annotation was read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Origin {
    /// Its position in the page's `/Annots` array, counting every entry.
    pub index: u32,
    /// Its object number and generation, unless it is written straight into the array.
    pub object: Option<(u32, u16)>,
}

/// How an annotation gets its look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    /// As the file draws it (its `/AP` stream), untouched.
    ///
    /// What exactly that draws isn't known: it may be more than the annotation's values say
    /// (arrowheads, a dashed border).
    Original,
    /// As the file draws it, and that is a look written from [`appearance`] for these very values
    /// (see [`look_mark`]): drawing it anew after a change loses nothing.
    Own,
    /// The file gives none, so it is drawn from the annotation's parameters.
    Missing,
    /// Made or changed here: drawn from the parameters, and an appearance is written for other
    /// programs when the document is saved.
    Generated,
}

/// One annotation on a page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub kind: AnnotKind,
    /// The area it takes up (`/Rect`), normalized.
    pub rect: PdfBox,
    /// Its color (`/C`). For shapes, that of the outline.
    pub color: Option<Color>,
    /// 0 (invisible) to 1 (opaque): `/CA`.
    pub opacity: f32,
    /// The width of strokes and outlines, in points.
    pub line_width: f64,
    /// Who made it (`/T`).
    pub author: Option<String>,
    /// The note that goes with it (`/Contents`).
    pub contents: Option<String>,
    /// When it was last changed (`/M`).
    pub modified: Option<Date>,
    /// A name unique on its page (`/NM`), if the file gives one.
    pub name: Option<String>,
    pub flags: Flags,
    /// Where it was read from, or `None` for one made here.
    pub origin: Option<Origin>,
    pub appearance: Appearance,
}

impl Annotation {
    /// A new annotation of `kind` over `rect`, with nothing else set.
    pub fn new(kind: AnnotKind, rect: PdfBox) -> Self {
        Self {
            kind,
            rect,
            color: None,
            opacity: 1.0,
            line_width: 1.0,
            author: None,
            contents: None,
            modified: None,
            name: None,
            flags: Flags::NEW,
            origin: None,
            appearance: Appearance::Generated,
        }
    }

    /// Marked text over `quads` (see [`AnnotKind::TextMarkup`]), in `color`. `None` without quads.
    pub fn text_markup(style: MarkupStyle, quads: Vec<PdfQuad>, color: Color) -> Option<Self> {
        let rect = quads
            .iter()
            .map(PdfQuad::bounds)
            .reduce(|a, b| a.union(&b))?;
        let mut a = Self::new(AnnotKind::TextMarkup { style, quads }, rect);
        a.color = Some(color);
        Some(a)
    }

    pub fn annot_type(&self) -> AnnotType {
        self.kind.annot_type()
    }

    /// Whether the annotation is drawn from its values alone, so that it can be changed and drawn
    /// anew: one made or changed here, or one a file has no look or only a look written here for.
    ///
    /// Text markup always is: its values say all there is to it.
    pub fn is_redrawable(&self) -> bool {
        appearance(self).is_some()
            && (self.appearance != Appearance::Original
                || matches!(self.kind, AnnotKind::TextMarkup { .. }))
    }

    /// Whether the annotation can be moved: a drawing, a shape, a note or a text box (text markup
    /// belongs to its text) that is [redrawable](Self::is_redrawable).
    pub fn is_adjustable(&self) -> bool {
        match self.kind {
            AnnotKind::TextMarkup { .. } | AnnotKind::Other { .. } => false,
            // The file's image goes where the stamp does.
            AnnotKind::Stamp(StampArt::InFile) => self.appearance == Appearance::Own,
            _ => self.is_redrawable(),
        }
    }

    /// Whether resizing keeps the annotation's proportions: a picture.
    pub fn keeps_proportions(&self) -> bool {
        matches!(self.kind, AnnotKind::Stamp(_))
    }

    /// A stamp of `image`, `width` wide and as high as the image's proportions make it, with its
    /// middle at `at`.
    pub fn stamp(at: PdfPoint, width: f64, image: Arc<StampImage>) -> Self {
        let height = width * f64::from(image.height.max(1)) / f64::from(image.width.max(1));
        let rect = PdfBox::new(
            PdfPoint::new(at.x - width / 2.0, at.y - height / 2.0),
            PdfPoint::new(at.x + width / 2.0, at.y + height / 2.0),
        );
        let mut a = Self::new(AnnotKind::Stamp(StampArt::Image(image)), rect);
        a.line_width = 0.0;
        a
    }

    /// A drawing of `strokes`, `width` wide with its middle at `at`.
    ///
    /// The strokes are in a box of `size` (width and height) with y pointing down, as a screen has
    /// it: a signature drawn somewhere else, placed here.
    pub fn drawing(
        at: PdfPoint,
        width: f64,
        strokes: &[Vec<(f64, f64)>],
        size: (f64, f64),
        color: Color,
        line_width: f64,
    ) -> Option<Self> {
        if strokes.iter().all(Vec::is_empty) || size.0 <= 0.0 || size.1 <= 0.0 {
            return None;
        }
        let scale = width / size.0;
        let height = size.1 * scale;
        let placed: Vec<Vec<PdfPoint>> = strokes
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.iter()
                    .map(|&(x, y)| {
                        PdfPoint::new(
                            at.x - width / 2.0 + x * scale,
                            at.y + height / 2.0 - y * scale,
                        )
                    })
                    .collect()
            })
            .collect();
        let all: Vec<PdfPoint> = placed.iter().flatten().copied().collect();
        let rect = PdfBox::from_points(&all).inflate(line_width / 2.0, line_width / 2.0);
        let mut a = Self::new(AnnotKind::Ink { strokes: placed }, rect);
        a.color = Some(color);
        a.line_width = line_width;
        Some(a)
    }

    /// Whether the annotation can also be resized: not a note, which is a mark of one size.
    pub fn is_resizable(&self) -> bool {
        self.is_adjustable() && self.kind != AnnotKind::Note
    }

    /// Whether the annotation's text is what it shows, rather than a comment that goes with it.
    pub fn is_text_box(&self) -> bool {
        matches!(self.kind, AnnotKind::FreeText { .. })
    }

    /// A note at `at` (its top left corner), in `color`.
    pub fn note(at: PdfPoint, color: Color) -> Self {
        let rect = PdfBox::new(
            PdfPoint::new(at.x, at.y - NOTE_SIZE),
            PdfPoint::new(at.x + NOTE_SIZE, at.y),
        );
        let mut a = Self::new(AnnotKind::Note, rect);
        a.color = Some(color);
        a.line_width = 0.0;
        a
    }

    /// A text box of `text` in `font` at `size` points and `color`, its top left corner at `at` and
    /// `width` wide, and as high as the text needs.
    pub fn text_box(
        at: PdfPoint,
        width: f64,
        text: &str,
        (size, font): (f64, TextFont),
        color: Color,
    ) -> Self {
        let rect = PdfBox::new(at, PdfPoint::new(at.x + width, at.y));
        let mut a = Self::new(AnnotKind::FreeText { size, font }, rect);
        a.color = Some(color);
        a.line_width = 0.0;
        a.contents = Some(text.to_owned());
        a.fit_text();
        a
    }

    /// For a text box: makes it as high as its text needs at its width, keeping its top edge where
    /// it is. Its look is to be made anew.
    pub fn fit_text(&mut self) {
        let AnnotKind::FreeText { size, font } = self.kind else {
            return;
        };
        let width = (self.rect.width() - 2.0 * TEXT_MARGIN).max(size);
        let lines = wrap(self.contents.as_deref().unwrap_or(""), size, width, font).len();
        self.rect.min.y = self.rect.max.y - box_height(lines, size, font);
        self.rect.max.x = self.rect.min.x + width + 2.0 * TEXT_MARGIN;
        self.appearance = Appearance::Generated;
    }

    /// The box of what the annotation draws, without the width of its line: the corners a rectangle
    /// was dragged between, the points of a stroke. This is what handles for resizing sit on.
    pub fn frame(&self) -> PdfBox {
        match &self.kind {
            AnnotKind::Ink { strokes } => PdfBox::from_points(strokes.iter().flatten()),
            AnnotKind::Line { from, to } => PdfBox::from_points([*from, *to]),
            AnnotKind::Square { .. } | AnnotKind::Circle { .. } => {
                let inset = (self.line_width / 2.0)
                    .min(self.rect.width() / 2.0)
                    .min(self.rect.height() / 2.0)
                    .max(0.0);
                self.rect.inflate(-inset, -inset)
            }
            _ => self.rect,
        }
    }

    /// The annotation with every point of it put through `f`, which moves or scales but doesn't
    /// turn: for moving and resizing a drawing or a shape. Its look is to be made anew.
    pub fn transformed(&self, f: impl Fn(PdfPoint) -> PdfPoint) -> Self {
        let mut a = self.clone();
        let half = self.line_width / 2.0;
        let frame = self.frame();
        let around = |points: &[PdfPoint]| PdfBox::from_points(points).inflate(half, half);
        match &mut a.kind {
            AnnotKind::Ink { strokes } => {
                for p in strokes.iter_mut().flatten() {
                    *p = f(*p);
                }
                let all: Vec<PdfPoint> = strokes.iter().flatten().copied().collect();
                a.rect = around(&all);
            }
            AnnotKind::Line { from, to } => {
                (*from, *to) = (f(*from), f(*to));
                a.rect = around(&[*from, *to]);
            }
            AnnotKind::TextMarkup { quads, .. } => {
                for q in quads.iter_mut() {
                    q.points = q.points.map(&f);
                }
                a.rect = quads
                    .iter()
                    .map(PdfQuad::bounds)
                    .reduce(|x, y| x.union(&y))
                    .unwrap_or(a.rect);
            }
            _ => a.rect = around(&[f(frame.min), f(frame.max)]),
        }
        // The image a file has for a stamp follows the stamp's area as it is: nothing to make anew.
        if a.kind != AnnotKind::Stamp(StampArt::InFile) {
            a.appearance = Appearance::Generated;
        }
        // A text box is as high as its text needs at its new width.
        a.fit_text();
        a
    }

    /// How near a line a point must be to count as on it: half the line's width, and enough to hit
    /// a thin one.
    fn reach(&self) -> f64 {
        (self.line_width / 2.0).max(4.0)
    }

    /// Whether `p` is on the annotation: on its marked text for text markup, near the line for
    /// drawings and lines, else within its area.
    pub fn contains(&self, p: PdfPoint) -> bool {
        match &self.kind {
            AnnotKind::TextMarkup { quads, .. } => quads.iter().any(|q| q.contains(p)),
            // Near the line, not anywhere in the rectangle around it, which for a long stroke is
            // most of the page.
            AnnotKind::Ink { strokes } => strokes.iter().any(|s| match s[..] {
                [only] => (p - only).length() <= self.reach(),
                _ => s
                    .windows(2)
                    .any(|w| distance(p, w[0], w[1]) <= self.reach()),
            }),
            AnnotKind::Line { from, to } => distance(p, *from, *to) <= self.reach(),
            _ => self.rect.contains_inclusive(p),
        }
    }
}

/// How far `p` is from the line between `a` and `b`.
fn distance(p: PdfPoint, a: PdfPoint, b: PdfPoint) -> f64 {
    let (ab, ap) = (b - a, p - a);
    let length = ab.square_length();
    let t = if length > 0.0 {
        (ap.dot(ab) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p - (a + ab * t)).length()
}
