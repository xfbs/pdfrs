//! Read access to the structure of a PDF: opening and decryption, metadata, the outline (table of
//! contents) and links.
//!
//! Rendering lives in `specto-render`. This crate is about everything else a viewer reads from the
//! file. Everything here is defensive: outline and name trees in broken files can be cyclic or
//! huge, so every walk is bounded.

mod data;
mod labels;
mod text;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hayro_syntax::object::dict::keys::{
    A, ANNOTS, D, DEST, DESTS, FIRST, KIDS, LIMITS, NAMES, NEXT, NUMS, OUTLINES, P, PAGE_LABELS,
    RECT, S, ST, SUBTYPE, TITLE, URI,
};
use hayro_syntax::object::{
    Array, DateTime, Dict, FromBytes, MaybeRef, Name, Object, ObjectIdentifier,
};
use hayro_syntax::{DecryptionError, LoadPdfError, Pdf, PdfVersion};
use specto_geom::{PdfBox, PdfPoint};

pub use data::{MAP_THRESHOLD, PdfData};
pub use hayro_syntax;
pub use labels::{LabelRange, LabelStyle, PageLabel, PageLabels};
pub use text::decode_text;

/// Outline items beyond this many are ignored (a guard against huge or cyclic outlines in broken
/// files).
const MAX_OUTLINE_ITEMS: usize = 100_000;
const MAX_DEPTH: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    #[error("the document is password protected")]
    PasswordRequired,
    #[error("the document uses an unsupported encryption scheme")]
    UnsupportedEncryption,
    #[error("the document's encryption dictionary is invalid")]
    InvalidEncryption,
    #[error("not a valid PDF file")]
    Invalid,
}

/// Parses (and if needed decrypts) a PDF.
///
/// Parsing is lazy: this reads the cross-reference table and page tree, not the page contents.
pub fn open_pdf(data: PdfData, password: Option<&str>) -> Result<Pdf, OpenError> {
    Pdf::new_with_password(Arc::new(data), password.unwrap_or("")).map_err(|e| match e {
        LoadPdfError::Decryption(DecryptionError::PasswordProtected) => OpenError::PasswordRequired,
        LoadPdfError::Decryption(DecryptionError::UnsupportedAlgorithm) => {
            OpenError::UnsupportedEncryption
        }
        LoadPdfError::Decryption(_) => OpenError::InvalidEncryption,
        LoadPdfError::Invalid => OpenError::Invalid,
    })
}

/// Document information (the `/Info` dictionary).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub created: Option<Date>,
    pub modified: Option<Date>,
}

/// What an encrypted file allows to be done with it (the `/P` flags of its `/Encrypt` dictionary).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permissions {
    pub printing: bool,
    /// Changing the document other than by annotating it or filling in its form.
    pub changing: bool,
    /// Copying text and pictures out of it.
    pub copying: bool,
    /// Adding annotations and filling in the form.
    pub annotating: bool,
}

/// The trailer dictionary the end of the file points to: that of a cross-reference table, or the
/// dictionary of a cross-reference stream.
///
/// `None` for a file so damaged that its end doesn't lead there.
fn trailer(data: &[u8]) -> Option<Dict<'_>> {
    let find = |hay: &[u8], needle: &[u8]| hay.windows(needle.len()).position(|w| w == needle);
    let tail = &data[data.len().saturating_sub(2048)..];
    let start = tail.windows(9).rposition(|w| w == b"startxref")? + 9;
    let digits: String = (tail[start..].iter())
        .skip_while(|b| b.is_ascii_whitespace())
        .take_while(|b| b.is_ascii_digit())
        .map(|&b| b as char)
        .collect();
    let mut rest = data.get(digits.parse::<usize>().ok()?..)?;
    if rest.starts_with(b"xref") {
        rest = &rest[find(rest, b"trailer")?..];
    }
    Dict::from_bytes(&rest[find(rest, b"<<")?..])
}

/// A date and time as a document states it: on the clock of wherever it was written, with that
/// place's offset from UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// Minutes east of UTC.
    pub utc_offset_minutes: i16,
}

/// ISO 8601 with the offset, such as `2024-03-05T14:22:10+01:00`.
impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let offset = self.utc_offset_minutes;
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{:02}:{:02}",
            self.year,
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            if offset < 0 { '-' } else { '+' },
            offset.unsigned_abs() / 60,
            offset.unsigned_abs() % 60,
        )
    }
}

impl Date {
    /// The date and time `seconds` after the start of 1970, in UTC.
    pub fn from_unix(seconds: u64) -> Self {
        let (days, rest) = (seconds / 86_400, seconds % 86_400);
        // Days to a date in the Gregorian calendar (Howard Hinnant's `civil_from_days`), counting
        // years from March so that the leap day comes last.
        let z = days + 719_468;
        let (era, day_of_era) = (z / 146_097, z % 146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let m = (5 * day_of_year + 2) / 153;
        let day = day_of_year - (153 * m + 2) / 5 + 1;
        let month = if m < 10 { m + 3 } else { m - 9 };
        let year = year_of_era + era * 400 + u64::from(month <= 2);
        Self {
            year: year.min(9999) as u16,
            month: month as u8,
            day: day as u8,
            hour: (rest / 3600) as u8,
            minute: (rest % 3600 / 60) as u8,
            second: (rest % 60) as u8,
            utc_offset_minutes: 0,
        }
    }

    /// Reads a PDF date string: `D:YYYYMMDDHHmmSSOHH'mm'`, where everything after the year may be
    /// missing and `O` is `+`, `-` or `Z`.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes).ok()?.trim();
        let text = text.strip_prefix("D:").unwrap_or(text);
        let digits = text.bytes().take_while(u8::is_ascii_digit).count();
        let (date, zone) = text.split_at(digits);
        // Two digits at `at`, or `default` where the string has ended.
        let two = |at: usize, default: u8| match date.get(at..at + 2) {
            Some(d) => d.parse::<u8>().ok(),
            None if date.len() <= at => Some(default),
            None => None,
        };
        let year = date.get(..4)?.parse().ok()?;
        let (month, day) = (two(4, 1)?, two(6, 1)?);
        let (hour, minute, second) = (two(8, 0)?, two(10, 0)?, two(12, 0)?);
        if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }
        if hour > 23 || minute > 59 || second > 59 {
            return None;
        }
        let utc_offset_minutes = match zone.as_bytes().first() {
            Some(sign @ (b'+' | b'-')) => {
                let mut parts = zone[1..].split('\'');
                let hours: i16 = parts.next()?.parse().ok()?;
                let minutes: i16 = match parts.next() {
                    Some("") | None => 0,
                    Some(m) => m.parse().ok()?,
                };
                if hours > 23 || minutes > 59 {
                    return None;
                }
                let offset = hours * 60 + minutes;
                if *sign == b'-' { -offset } else { offset }
            }
            _ => 0,
        };
        Some(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            utc_offset_minutes,
        })
    }
}

impl From<DateTime> for Date {
    fn from(d: DateTime) -> Self {
        let hours = i16::from(d.utc_offset_hour);
        let minutes = i16::from(d.utc_offset_minute);
        Self {
            year: d.year,
            month: d.month,
            day: d.day,
            hour: d.hour,
            minute: d.minute,
            second: d.second,
            utc_offset_minutes: hours * 60 + if hours < 0 { -minutes } else { minutes },
        }
    }
}

/// Where a destination points: a page and, optionally, a position on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Destination {
    /// Zero-based page index.
    pub page: usize,
    /// The point that should end up at the top-left of the view, in PDF user space of that page.
    /// A missing coordinate keeps the current one.
    pub left: Option<f64>,
    pub top: Option<f64>,
}

impl Destination {
    /// The target position as a PDF-space point, with missing coordinates filled from `fallback`
    /// (usually the page's visible top-left corner).
    pub fn point_or(&self, fallback: PdfPoint) -> PdfPoint {
        PdfPoint::new(
            self.left.unwrap_or(fallback.x),
            self.top.unwrap_or(fallback.y),
        )
    }
}

/// Built-in navigation of the `/Named` action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedAction {
    NextPage,
    PrevPage,
    FirstPage,
    LastPage,
}

/// What activating a link or outline item does.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Go to a place in this document.
    Destination(Destination),
    /// Open a URI (usually a web page), which is left to the frontend.
    Uri(String),
    Named(NamedAction),
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutlineItem {
    pub title: String,
    /// `None` for items without a target, or with one we don't support (such as other files or
    /// JavaScript).
    pub target: Option<Target>,
    pub children: Vec<OutlineItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    /// The clickable area in PDF user space of its page.
    pub rect: PdfBox,
    pub target: Target,
}

impl Link {
    pub fn contains(&self, p: PdfPoint) -> bool {
        self.rect.min.x <= p.x
            && p.x <= self.rect.max.x
            && self.rect.min.y <= p.y
            && p.y <= self.rect.max.y
    }
}

/// An opened PDF, for everything except rendering.
pub struct SourceDocument {
    pdf: Pdf,
    /// Maps a page's object id to its page index, for resolving destinations.
    page_ids: HashMap<ObjectIdentifier, usize>,
}

impl SourceDocument {
    pub fn open(data: impl Into<PdfData>, password: Option<&str>) -> Result<Self, OpenError> {
        Ok(Self::from_pdf(open_pdf(data.into(), password)?))
    }

    pub fn from_pdf(pdf: Pdf) -> Self {
        let page_ids = pdf
            .pages()
            .iter()
            .enumerate()
            .filter_map(|(i, p)| Some((p.raw().obj_id()?, i)))
            .collect();
        Self { pdf, page_ids }
    }

    pub fn pdf(&self) -> &Pdf {
        &self.pdf
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    pub fn metadata(&self) -> Metadata {
        let m = self.pdf.metadata();
        let text = |b: &Option<Box<[u8]>>| {
            b.as_deref()
                .map(decode_text)
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        };
        Metadata {
            title: text(&m.title),
            author: text(&m.author),
            subject: text(&m.subject),
            keywords: text(&m.keywords),
            creator: text(&m.creator),
            producer: text(&m.producer),
            created: m.creation_date.map(Date::from),
            modified: m.modification_date.map(Date::from),
        }
    }

    /// The version of PDF the file says it is written in, such as "1.7".
    pub fn version(&self) -> &'static str {
        match self.pdf.version() {
            PdfVersion::Pdf10 => "1.0",
            PdfVersion::Pdf11 => "1.1",
            PdfVersion::Pdf12 => "1.2",
            PdfVersion::Pdf13 => "1.3",
            PdfVersion::Pdf14 => "1.4",
            PdfVersion::Pdf15 => "1.5",
            PdfVersion::Pdf16 => "1.6",
            PdfVersion::Pdf17 => "1.7",
            PdfVersion::Pdf20 => "2.0",
        }
    }

    /// What the file allows where it is encrypted, and `None` where it isn't.
    ///
    /// These are requests of whoever protected the file. Nothing in the file enforces them.
    pub fn permissions(&self) -> Option<Permissions> {
        let trailer = trailer(self.pdf.data().as_ref())?;
        let encrypt = match trailer.get_ref(b"Encrypt") {
            Some(reference) => self.pdf.xref().get::<Dict<'_>>(reference.into())?,
            None => trailer.get::<Dict<'_>>(b"Encrypt")?,
        };
        // A set bit allows. A file that doesn't say allows everything.
        let flags = encrypt.get::<i64>(P).unwrap_or(-1);
        Some(Permissions {
            printing: flags & 1 << 2 != 0,
            changing: flags & 1 << 3 != 0,
            copying: flags & 1 << 4 != 0,
            annotating: flags & 1 << 5 != 0,
        })
    }

    /// The document's catalog: the dictionary everything else in it hangs on.
    pub fn catalog(&self) -> Option<Dict<'_>> {
        let xref = self.pdf.xref();
        xref.get::<Dict<'_>>(xref.root_id())
    }

    /// The outline (the bookmarks, or table of contents), empty if there is none.
    pub fn outline(&self) -> Vec<OutlineItem> {
        let Some(first) = self
            .catalog()
            .and_then(|c| c.get::<Dict<'_>>(OUTLINES))
            .and_then(|o| o.get::<Dict<'_>>(FIRST))
        else {
            return Vec::new();
        };
        let mut seen = HashSet::new();
        let mut budget = MAX_OUTLINE_ITEMS;
        self.outline_level(first, 0, &mut seen, &mut budget)
    }

    fn outline_level(
        &self,
        first: Dict<'_>,
        depth: usize,
        seen: &mut HashSet<ObjectIdentifier>,
        budget: &mut usize,
    ) -> Vec<OutlineItem> {
        let mut items = Vec::new();
        let mut node = Some(first);
        while let Some(item) = node {
            if *budget == 0 {
                break;
            }
            // Siblings and children are references. A repeat means a cycle.
            if let Some(id) = item.obj_id()
                && !seen.insert(id)
            {
                break;
            }
            *budget -= 1;
            let title = item
                .get::<hayro_syntax::object::String<'_>>(TITLE)
                .map(|t| decode_text(t.as_bytes()).trim().to_owned())
                .unwrap_or_default();
            let children = match item.get::<Dict<'_>>(FIRST) {
                Some(child) if depth < MAX_DEPTH => {
                    self.outline_level(child, depth + 1, seen, budget)
                }
                _ => Vec::new(),
            };
            items.push(OutlineItem {
                title,
                target: self.target_of(&item),
                children,
            });
            node = item.get::<Dict<'_>>(NEXT);
        }
        items
    }

    /// The document's page labels, or `None` if it has none.
    pub fn page_labels(&self) -> Option<PageLabels> {
        let root = self.catalog()?.get::<Dict<'_>>(PAGE_LABELS)?;
        let mut ranges = Vec::new();
        let mut budget = 100_000;
        collect_label_ranges(&root, 0, &mut ranges, &mut budget);
        PageLabels::new(ranges, self.page_count())
    }

    /// Links on page `page`, in drawing order.
    pub fn links(&self, page: usize) -> Vec<Link> {
        let Some(p) = self.pdf.pages().get(page) else {
            return Vec::new();
        };
        let Some(annots) = p.raw().get::<Array<'_>>(ANNOTS) else {
            return Vec::new();
        };
        annots
            .iter::<Dict<'_>>()
            .take(10_000)
            .filter(|a| a.get::<Name<'_>>(SUBTYPE).as_deref() == Some(b"Link"))
            .filter_map(|a| {
                let r: Vec<f64> = a.get::<Array<'_>>(RECT)?.iter::<f64>().collect();
                let [x0, y0, x1, y1] = r[..] else {
                    return None;
                };
                Some(Link {
                    rect: PdfBox::new(
                        PdfPoint::new(x0.min(x1), y0.min(y1)),
                        PdfPoint::new(x0.max(x1), y0.max(y1)),
                    ),
                    target: self.target_of(&a)?,
                })
            })
            .collect()
    }

    /// The topmost link at `p` (PDF space) on page `page`.
    pub fn link_at(&self, page: usize, p: PdfPoint) -> Option<Link> {
        self.links(page).into_iter().rev().find(|l| l.contains(p))
    }

    /// The target of an outline item or link annotation: `/Dest` or `/A`.
    fn target_of(&self, dict: &Dict<'_>) -> Option<Target> {
        if let Some(dest) = dict.get::<Object<'_>>(DEST) {
            return self.resolve_dest(dest, 0).map(Target::Destination);
        }
        let action = dict.get::<Dict<'_>>(A)?;
        match action.get::<Name<'_>>(S)?.as_ref() {
            b"GoTo" => self
                .resolve_dest(action.get::<Object<'_>>(D)?, 0)
                .map(Target::Destination),
            b"URI" => {
                let uri = action.get::<hayro_syntax::object::String<'_>>(URI)?;
                // URIs are 7-bit ASCII by the specification. Be lenient with what's there.
                let uri = String::from_utf8_lossy(uri.as_bytes()).trim().to_owned();
                (!uri.is_empty()).then_some(Target::Uri(uri))
            }
            b"Named" => {
                let name = action.get::<Name<'_>>(b"N")?;
                Some(Target::Named(match name.as_ref() {
                    b"NextPage" => NamedAction::NextPage,
                    b"PrevPage" => NamedAction::PrevPage,
                    b"FirstPage" => NamedAction::FirstPage,
                    b"LastPage" => NamedAction::LastPage,
                    _ => return None,
                }))
            }
            // GoToR, Launch, JavaScript and the rest aren't supported.
            _ => None,
        }
    }

    /// Resolves an explicit destination (array), a named one (name or string), or a destination
    /// dictionary (`<< /D [...] >>`).
    fn resolve_dest(&self, dest: Object<'_>, depth: usize) -> Option<Destination> {
        if depth > 4 {
            return None;
        }
        match dest {
            Object::Array(a) => self.explicit_dest(&a),
            Object::Dict(d) => self.resolve_dest(d.get::<Object<'_>>(D)?, depth + 1),
            Object::Name(n) => self.resolve_dest(self.named_dest(n.as_ref())?, depth + 1),
            Object::String(s) => self.resolve_dest(self.named_dest(s.as_bytes())?, depth + 1),
            _ => None,
        }
    }

    /// `[page /XYZ left top zoom]`, `[page /Fit]`, `[page /FitH top]` and the other forms.
    fn explicit_dest(&self, a: &Array<'_>) -> Option<Destination> {
        let mut items = a.raw_iter();
        let page = match items.next()? {
            MaybeRef::Ref(r) => *self.page_ids.get(&ObjectIdentifier::from(r))?,
            // Remote destinations use page numbers. Some broken files use them for local ones too.
            MaybeRef::NotRef(Object::Number(n)) => usize::try_from(n.as_i64())
                .ok()
                .filter(|&p| p < self.page_count())?,
            _ => return None,
        };
        let kind = match items.next() {
            Some(MaybeRef::NotRef(Object::Name(n))) => n.as_ref().to_vec(),
            _ => Vec::new(),
        };
        let mut num = || match items.next() {
            Some(MaybeRef::NotRef(Object::Number(n))) => Some(n.as_f64()),
            _ => None, // null or missing: keep the current value
        };
        let (left, top) = match kind.as_slice() {
            b"XYZ" => {
                let left = num();
                (left, num())
            }
            b"FitH" | b"FitBH" => (None, num()),
            b"FitV" | b"FitBV" => (num(), None),
            b"FitR" => {
                let (l, _b, _r, t) = (num(), num(), num(), num());
                (l, t)
            }
            _ => (None, None), // Fit, FitB: the whole page
        };
        Some(Destination { page, left, top })
    }

    /// Looks up a named destination in `/Dests` (PDF 1.1) or the `/Names` name tree.
    fn named_dest(&self, name: &[u8]) -> Option<Object<'_>> {
        let catalog = self.catalog()?;
        if let Some(d) = catalog
            .get::<Dict<'_>>(DESTS)
            .and_then(|d| d.get::<Object<'_>>(name))
        {
            return Some(d);
        }
        let tree = catalog.get::<Dict<'_>>(NAMES)?.get::<Dict<'_>>(DESTS)?;
        name_tree_lookup(tree, name, 0)
    }
}

/// Collects the ranges of a page-label number tree (`/Nums` pairs of start page and label
/// dictionary, possibly spread over `/Kids`).
fn collect_label_ranges(
    node: &Dict<'_>,
    depth: usize,
    out: &mut Vec<LabelRange>,
    budget: &mut usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    if let Some(nums) = node.get::<Array<'_>>(NUMS) {
        let mut it = nums.iter::<Object<'_>>();
        while let (Some(k), Some(v)) = (it.next(), it.next()) {
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            let (Object::Number(start), Object::Dict(d)) = (k, v) else {
                continue;
            };
            let Ok(start) = usize::try_from(start.as_i64()) else {
                continue;
            };
            let style = match d.get::<Name<'_>>(S).as_deref() {
                Some(b"D") => LabelStyle::Decimal,
                Some(b"R") => LabelStyle::UpperRoman,
                Some(b"r") => LabelStyle::LowerRoman,
                Some(b"A") => LabelStyle::UpperLetters,
                Some(b"a") => LabelStyle::LowerLetters,
                _ => LabelStyle::None,
            };
            let prefix = d
                .get::<hayro_syntax::object::String<'_>>(P)
                .map(|p| decode_text(p.as_bytes()))
                .unwrap_or_default();
            let first = d
                .get::<f64>(ST)
                .filter(|v| v.is_finite() && *v >= 1.0)
                .map_or(1, |v| v.min(u32::MAX as f64) as u32);
            out.push(LabelRange {
                start,
                style,
                prefix,
                first,
            });
        }
    }
    if let Some(kids) = node.get::<Array<'_>>(KIDS) {
        for kid in kids.iter::<Dict<'_>>().take(10_000) {
            collect_label_ranges(&kid, depth + 1, out, budget);
        }
    }
}

/// Finds `key` in a name tree, descending only into kids whose `/Limits` can contain it.
fn name_tree_lookup<'a>(node: Dict<'a>, key: &[u8], depth: usize) -> Option<Object<'a>> {
    if depth > MAX_DEPTH {
        return None;
    }
    if let Some(names) = node.get::<Array<'_>>(NAMES) {
        let mut it = names.raw_iter();
        while let (Some(k), Some(v)) = (it.next(), it.next()) {
            if let MaybeRef::NotRef(Object::String(s)) = k
                && s.as_bytes() == key
            {
                return match v {
                    MaybeRef::NotRef(o) => Some(o),
                    MaybeRef::Ref(_) => names_value(&names, key),
                };
            }
        }
    }
    let kids = node.get::<Array<'_>>(KIDS)?;
    for kid in kids.iter::<Dict<'_>>().take(10_000) {
        let in_range = kid.get::<Array<'_>>(LIMITS).is_none_or(|limits| {
            let mut l = limits.iter::<hayro_syntax::object::String<'_>>();
            match (l.next(), l.next()) {
                (Some(lo), Some(hi)) => lo.as_bytes() <= key && key <= hi.as_bytes(),
                _ => true,
            }
        });
        if in_range && let Some(v) = name_tree_lookup(kid, key, depth + 1) {
            return Some(v);
        }
    }
    None
}

/// Value for `key` in a `/Names` array, resolving references (the raw iterator above doesn't).
fn names_value<'a>(names: &Array<'a>, key: &[u8]) -> Option<Object<'a>> {
    let mut it = names.iter::<Object<'_>>();
    while let (Some(k), Some(v)) = (it.next(), it.next()) {
        if let Object::String(s) = k
            && s.as_bytes() == key
        {
            return Some(v);
        }
    }
    None
}
