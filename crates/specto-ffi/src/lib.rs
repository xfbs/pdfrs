//! UniFFI surface for the Android app.
//!
//! A [`ViewSession`] is one open document on screen: it owns the document itself (a
//! `specto_session::Session`, the same as in the desktop app: the file, its pages, the viewer,
//! text, selection and search), the render scheduler and the bookkeeping for tiles. Kotlin forwards
//! gestures, asks for a [`Frame`] and draws it. See `docs/ARCHITECTURE.md` ("Android").
//!
//! UniFFI is used for everything that happens occasionally (opening, modes, viewport changes). Its
//! Kotlin bindings go through JNA, and a single call costs a few hundred microseconds on a phone,
//! so what runs every frame goes through plain JNI instead (`jni.rs`, backed by [`hot`]):
//! scrolling, zooming, planning a frame and collecting finished tiles.
//!
//! Tile pixels never cross UniFFI either, which was measured as too slow. Kotlin owns one `Bitmap`
//! per tile, keyed by an opaque id, and fills it through JNI. Which tiles exist is decided here:
//! the cache bookkeeping lives in Rust, and [`Frame::evicted`] tells Kotlin which bitmaps to drop.

pub mod crash;
mod drafts;
pub mod hot;
#[cfg(target_os = "android")]
mod jni;
mod search;
mod tiles;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, Weak};
use std::time::Instant;

use specto_annot::{Color, MarkupStyle, Segment};
use specto_doc::{FieldId, SourceId};
use specto_forms::FieldKind;
use specto_geom::{DeviceRect, ViewPoint, ViewVector};
use specto_render::scheduler::Scheduler;
use specto_render::{Bitmap, OpenError};
use specto_session::{FieldClick, ViewShape};
use specto_session::{MAIN, Options, Session};
use specto_source::{NamedAction, OutlineItem, PdfData, Target};
use specto_view::{
    Granularity, Selection, Settings, TextPages, over_text, text_char, text_position, text_quads,
};

pub use drafts::DraftStore;
use tiles::TileBook;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PdfError {
    #[error("the document is password protected")]
    PasswordRequired,
    #[error("the document uses unsupported or invalid encryption")]
    UnsupportedEncryption,
    #[error("not a valid PDF file")]
    Invalid,
    // Not `message`: in the generated Kotlin exception that would clash with `Throwable.message`,
    // and the bindings wouldn't compile.
    #[error("couldn't read the file: {reason}")]
    Io { reason: String },
    #[error("couldn't save the document: {reason}")]
    Save { reason: String },
}

impl From<OpenError> for PdfError {
    fn from(e: OpenError) -> Self {
        match e {
            OpenError::PasswordRequired => Self::PasswordRequired,
            OpenError::UnsupportedEncryption | OpenError::InvalidEncryption => {
                Self::UnsupportedEncryption
            }
            OpenError::Invalid => Self::Invalid,
        }
    }
}

/// Implemented in Kotlin.
#[uniffi::export(with_foreign)]
pub trait ViewListener: Send + Sync {
    /// Finished tiles are waiting in [`ViewSession::take_finished`].
    ///
    /// Called from a render thread, so hop to the main thread first.
    fn tiles_ready(&self);

    /// A search got further: it found more, or has been through more of the document.
    ///
    /// [`ViewSession::search_status`] and the next frame show what changed (the view may have moved
    /// to a first match). Called from a background thread, like [`tiles_ready`](Self::tiles_ready).
    fn search_changed(&self);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum LayoutMode {
    SinglePage,
    Continuous,
    TwoPage,
    TwoPageContinuous,
}

/// How the document is presented, as offered in the app's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ViewMode {
    /// One page at a time, each fitted to the screen, turned by swiping left and right.
    Paged,
    /// All pages in one column fitted to the width, scrolled up and down.
    Continuous,
}

/// One line of the outline (table of contents), which is a tree flattened in reading order.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct OutlineEntry {
    pub title: String,
    /// Nesting level, 0 for top-level items.
    pub depth: u32,
    /// Zero-based page the item leads to, if it leads into this document and the page is still in
    /// it.
    pub page: Option<u32>,
    /// Whether deeper entries follow that belong to this one.
    pub has_children: bool,
}

/// What an edit did, for naming it ("Undo Rotate").
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum EditKind {
    Rotate,
    Delete,
    Move,
    Insert,
    /// An annotation was added, deleted or changed. `pages` is then 1.
    AddAnnotation,
    RemoveAnnotation,
    ChangeAnnotation,
    /// A field of the form was filled in. `pages` is then 1.
    FillField,
    /// The fields of the form were set back to what they hold at first. `pages` is then 1.
    ResetForm,
}

/// An edit that can be undone or redone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EditStep {
    pub kind: EditKind,
    /// How many pages it changed.
    pub pages: u32,
}

impl From<specto_doc::Edit> for EditStep {
    fn from(edit: specto_doc::Edit) -> Self {
        use specto_doc::Edit;
        let (kind, pages) = match edit {
            Edit::Rotate { pages } => (EditKind::Rotate, pages),
            Edit::Delete { pages } => (EditKind::Delete, pages),
            Edit::Move { pages } => (EditKind::Move, pages),
            Edit::Insert { pages } => (EditKind::Insert, pages),
            Edit::AddAnnotation(_) => (EditKind::AddAnnotation, 1),
            Edit::RemoveAnnotation(_) => (EditKind::RemoveAnnotation, 1),
            Edit::ChangeAnnotation(_) => (EditKind::ChangeAnnotation, 1),
            Edit::FillField => (EditKind::FillField, 1),
            Edit::ResetForm => (EditKind::ResetForm, 1),
        };
        Self {
            kind,
            pages: pages as u32,
        }
    }
}

/// Where the document stands as something being edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct EditState {
    /// Whether it differs from what was opened or last saved.
    pub modified: bool,
    /// What [`ViewSession::undo`] would take back, if anything.
    pub undo: Option<EditStep>,
    /// What [`ViewSession::redo`] would bring back, if anything.
    pub redo: Option<EditStep>,
}

/// What activating a link or outline entry did.
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum Activated {
    /// There was nothing to activate (or nothing we support).
    Nothing,
    /// The view went somewhere else in the document: plan a new frame.
    Moved,
    /// An external link. Opening it, after asking the user, is up to the app.
    Uri { uri: String },
}

/// What the app remembers about a document between sessions, to continue where the reader left off.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct SavedView {
    /// Zero-based page at the top of the view.
    pub page: u32,
    /// How far down that page the view starts, as a fraction of its height.
    pub page_offset: f64,
    /// [`ViewMode::Paged`] rather than continuous.
    pub paged: bool,
    /// Logical pixels per point if the reader had zoomed in, or `None` if the view was fitted.
    pub zoom: Option<f64>,
    /// Two pages side by side.
    pub two_pages: bool,
    /// With two pages: the first page on its own, like a book's cover.
    pub cover_alone: bool,
    /// Fitted means the page's width rather than the whole page (see
    /// [`ViewSession::set_fit_width`]).
    pub fit_width: bool,
}

/// What a document says about itself (its `/Info` dictionary and header), for a properties dialog.
///
/// Any of it may be missing or wrong: it is whatever the program that wrote the file put there.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DocumentInfo {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    /// The program the document was written in.
    pub creator: Option<String>,
    /// The program that turned it into a PDF.
    pub producer: Option<String>,
    /// ISO 8601 with the offset from UTC, such as `2024-03-05T14:22:10+01:00`.
    pub created: Option<String>,
    pub modified: Option<String>,
    /// The PDF version the file states, such as "1.7".
    pub pdf_version: String,
}

/// A distance in logical pixels (dp).
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct Offset {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum ZoomMode {
    Fixed,
    FitWidth,
    FitPage,
}

/// A rectangle in device pixels, relative to the top-left of the view.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl From<DeviceRect> for Rect {
    fn from(r: DeviceRect) -> Self {
        Self {
            x: r.origin.x as f32,
            y: r.origin.y as f32,
            width: r.size.width as f32,
            height: r.size.height as f32,
        }
    }
}

/// A point in device pixels, relative to the top-left of the view.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Why a stretch of text is highlighted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum HighlightKind {
    /// It is selected.
    Selection,
    /// The search found it.
    Match,
    /// The search found it, and it is the match being looked at.
    CurrentMatch,
}

/// How a search stands, for a search bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SearchStatus {
    /// Matches found so far.
    pub matches: u32,
    /// Which of them is the current one, counted from 0, or `None` if nothing has been found yet.
    pub current: Option<u32>,
    /// How many pages have been searched. The rest follow in the background.
    pub pages_searched: u32,
    pub pages: u32,
}

/// Text to highlight: the part of one line that is selected or was found, as the four corners of a
/// quadrilateral (lines can be rotated).
///
/// "Start" and "end" are along the line, "bottom" is its baseline side.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct Highlight {
    pub kind: HighlightKind,
    pub start_bottom: Point,
    pub end_bottom: Point,
    pub end_top: Point,
    pub start_top: Point,
}

/// One end of the selection on screen: the edge of its first or last line, where a handle for
/// moving that end belongs.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct SelectionEdge {
    pub bottom: Point,
    pub top: Point,
}

/// How selected text is marked (see [`ViewSession::mark_selection`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum Markup {
    Highlight,
    Underline,
    StrikeOut,
}

/// What a tap on a field of the form did, or asks for (see [`ViewSession::tap_field`]).
#[derive(Clone, Debug, PartialEq, uniffi::Enum)]
pub enum FieldTap {
    /// No field that can be filled in is there.
    Nothing,
    /// A box was ticked or unticked, or a radio button chosen: plan a new frame.
    Changed,
    /// A text field, to ask for its text. Follow with [`ViewSession::set_field_text`].
    Text {
        field: u64,
        /// What the form calls the field for the user, if it says.
        label: Option<String>,
        /// It takes more than one line.
        multiline: bool,
        /// What is typed is to be hidden.
        password: bool,
        /// How many characters it takes at most, if it says.
        max_length: Option<u32>,
        /// What it holds.
        text: String,
    },
    /// A menu or a list, to choose one of `options` from. Follow with
    /// [`ViewSession::choose_option`].
    Choose {
        field: u64,
        label: Option<String>,
        options: Vec<String>,
        /// For a menu that also takes text of one's own: what it shows now. Follow with
        /// [`ViewSession::set_field_text`] for text that was typed.
        typed: Option<String>,
        /// Which of the options it holds, if any of them.
        chosen: Option<u32>,
    },
    /// A list that takes several choices, to choose any of `options` from. Follow with
    /// [`ViewSession::choose_options`].
    ChooseSeveral {
        field: u64,
        label: Option<String>,
        options: Vec<String>,
        /// Which of the options it holds, in the order of the list.
        chosen: Vec<u32>,
    },
}

/// A field's id as the frontend has it: the number of its object and, in the low 16 bits, its
/// generation.
fn field_number(field: FieldId) -> u64 {
    u64::from(field.0) << 16 | u64::from(field.1)
}

fn field_id(number: u64) -> FieldId {
    FieldId((number >> 16) as u32, number as u16)
}

/// A piece of a [`ShapeDraw`]'s outline, from where the one before ended.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Enum)]
pub enum SegmentDraw {
    Line {
        to: Point,
    },
    /// A cubic Bézier curve.
    Curve {
        c1: Point,
        c2: Point,
        to: Point,
    },
}

/// A piece of an annotation that the frontend draws over a page: those made or changed since the
/// file was read, and ones the file gives no look for.
///
/// (The rest are part of the page as rendered.) An outline in device pixels, filled, drawn along
/// with a pen, or both. Colors are 0xAARRGGBB.
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ShapeDraw {
    /// Which of [`Frame::pages`] it is on (an index): clip it to that page.
    pub page: u32,
    pub fill: Option<u32>,
    /// The fill darkens what is under it, as a highlighter does (the blend mode Multiply), instead
    /// of covering it.
    pub multiply: bool,
    pub stroke: Option<u32>,
    pub stroke_width: f32,
    /// The pen is round: round ends and joins.
    pub round: bool,
    /// The outline ends where it starts.
    pub closed: bool,
    pub start: Point,
    pub segments: Vec<SegmentDraw>,
}

#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct TileDraw {
    /// Id of a tile previously handed out by [`ViewSession::take_finished`].
    pub id: u64,
    pub dest: Rect,
    /// The tile maps one to one onto device pixels: draw without filtering.
    pub exact: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PageDraw {
    /// Zero-based page index.
    pub page: u32,
    /// Fill this white, then draw `tiles` in order, clipped to it.
    pub rect: Rect,
    pub tiles: Vec<TileDraw>,
    /// The page takes long to render (it has huge images, or a render of it was slow) and what is
    /// visible of it isn't sharp yet: show that it is being worked on.
    ///
    /// Other pages are sharp within milliseconds and never say so.
    pub busy: bool,
}

/// Everything needed to draw the view once.
#[derive(Clone, Debug, uniffi::Record)]
pub struct Frame {
    pub pages: Vec<PageDraw>,
    /// Tiles dropped from the cache since the last frame: release their bitmaps.
    pub evicted: Vec<u64>,
    /// Every visible tile is sharp. If not, more tiles are on their way.
    pub complete: bool,
    /// The zoom has been still long enough that pages are drawn from tiles at the exact device
    /// scale, one to one.
    ///
    /// While zooming they come from a few fixed zoom levels and are scaled.
    pub settled: bool,
    /// Call [`ViewSession::frame`] again after this many milliseconds even if nothing else happens:
    /// that is when the view settles and sharpens.
    pub settles_in_ms: Option<u32>,
    /// Device pixels per PDF point that pages are drawn at.
    pub device_scale: f64,
    /// Zero-based index of the page the user is looking at. With two pages, the first of the pair.
    pub current_page: u32,
    /// The last page shown together with it: the same page, or with two pages the second of the
    /// pair.
    pub last_page: u32,
    /// Logical pixels (dp) per PDF point.
    pub zoom: f64,
    pub zoom_mode: ZoomMode,
    /// The zoom relative to the view mode's fitted state (the whole page when paged, the width when
    /// continuous): 1.0 when fitted.
    pub zoom_factor: f64,
    /// Whether the view is [`ViewMode::Paged`].
    pub paged: bool,
    /// Whether pages are shown two side by side.
    pub two_pages: bool,
    /// With two pages: whether the first page is on its own.
    pub cover_alone: bool,
    /// First page of the range set with [`ViewSession::set_thumbnail_range`].
    pub thumbnail_first: u32,
    /// For each page of that range, the id of its thumbnail tile, or 0 while it is still being
    /// rendered.
    pub thumbnails: Vec<u64>,
    /// Whether any text is selected, on screen or not.
    pub selection: bool,
    /// The selected text and the search's matches on the visible pages: draw these over the pages,
    /// in order.
    pub highlights: Vec<Highlight>,
    /// Where the selection starts and ends, for its handles, with `None` for an end that isn't on a
    /// visible page.
    pub selection_start: Option<SelectionEdge>,
    pub selection_end: Option<SelectionEdge>,
    /// The annotations to draw over the visible pages, in order, under the highlights. With
    /// [`ViewSession::set_highlight_fields`], a tint over each field that can be filled in too.
    pub shapes: Vec<ShapeDraw>,
    /// The four corners of the box around the annotation that is picked
    /// ([`ViewSession::pick_annotation_at`]), or around the field of the form that is being filled
    /// in ([`ViewSession::tap_field`]). Empty if neither is, or it is on no visible page.
    pub picked: Vec<Point>,
    /// The annotations to draw over the thumbnails, in order.
    ///
    /// `page` is an index into [`Frame::thumbnails`]. Positions and pen widths are in units of
    /// which [`THUMBNAIL_UNITS`] make the width of that page, from its top left corner.
    pub thumbnail_shapes: Vec<ShapeDraw>,
}

/// The width of every page in the units of [`Frame::thumbnail_shapes`].
pub const THUMBNAIL_UNITS: f64 = 1000.0;

/// Size of a page as displayed, in PDF points.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

/// A rendered tile waiting to be copied into a `Bitmap` of this size.
#[derive(Clone, Copy, Debug, uniffi::Record)]
pub struct FinishedTile {
    pub id: u64,
    pub width: u32,
    pub height: u32,
}

struct State {
    /// The document: its file, pages, viewer, text, selection and search, as the desktop app has it
    /// too.
    session: Session,
    tiles: TileBook,
    /// The last frame encoded for [`hot::frame`], kept so a caller whose buffer was too small can
    /// fetch it again without planning a new one.
    encoded: Vec<u8>,
    /// Pages whose thumbnails the frontend is showing (first, count).
    thumbnails: (u32, u32),
    /// What the view is fitted to unless the reader zoomed, and returns to: `FitPage` or
    /// `FitWidth`.
    fit: specto_view::ZoomMode,
    /// The state of the pages last written as a copy on the side (see `ViewSession::save_to_fd`),
    /// which may yet become the saved one.
    written: Option<specto_doc::Version>,
    /// Whether frames tint the fields of the form that can be filled in.
    highlight_fields: bool,
}

/// What fields are tinted with (0xAARRGGBB).
const FIELD_TINT: u32 = 0x2E35_84E4;

/// How many pages' text is kept (the least recently used go first). A page takes about 100 bytes
/// per character.
const TEXT_PAGES: usize = 24;

impl State {
    /// Moves the end of the selection that isn't its anchor to the text nearest to `p`. A selection
    /// never shrinks to nothing this way.
    fn select_to(&mut self, p: ViewPoint) {
        let (viewer, mut texts, selection) = self.session.text();
        let (Some(s), Some(to)) = (selection.as_mut(), text_position(viewer, &mut texts, p)) else {
            return;
        };
        let before = s.clone();
        s.extend(&mut texts, to);
        if s.is_empty() {
            *s = before;
        }
    }

    /// Does what a link or outline entry of file `source` asks for.
    fn follow(&mut self, source: SourceId, target: &Target) -> Activated {
        // Destinations name pages of the file. Where such a page is now is the session's to say (it
        // may be gone).
        let destination = match target {
            Target::Destination(d) => match self.session.position_of(source, d.page) {
                Some(page) => Some(page),
                None => return Activated::Nothing,
            },
            _ => None,
        };
        // Where the reader was is remembered for going back, except when only turning a page.
        let viewer = &mut self.session.viewer;
        match target {
            Target::Destination(d) => {
                let page = destination.unwrap_or(d.page);
                viewer.jump(|v| v.go_to_pdf_position(page, d.left, d.top));
            }
            Target::Named(NamedAction::NextPage) => viewer.next_page(),
            Target::Named(NamedAction::PrevPage) => viewer.previous_page(),
            Target::Named(NamedAction::FirstPage) => {
                viewer.jump(|v| v.go_to_page(0));
            }
            Target::Named(NamedAction::LastPage) => {
                viewer.jump(|v| v.go_to_page(v.page_count().saturating_sub(1)));
            }
            Target::Uri(uri) => return Activated::Uri { uri: uri.clone() },
        }
        Activated::Moved
    }
}

/// Page positions as the session takes them.
fn positions(pages: &[u32]) -> Vec<usize> {
    pages.iter().map(|&p| p as usize).collect()
}

/// Flattens the outline tree in reading order.
fn flatten_outline(
    items: Vec<OutlineItem>,
    depth: u32,
    out: &mut Vec<(OutlineEntry, Option<Target>)>,
) {
    for item in items {
        out.push((
            OutlineEntry {
                title: item.title,
                depth,
                // Where the page is now: filled in when asked for.
                page: None,
                has_children: !item.children.is_empty(),
            },
            item.target,
        ));
        flatten_outline(item.children, depth + 1, out);
    }
}

struct Shared {
    scheduler: Scheduler,
    listener: Arc<dyn ViewListener>,
    state: Mutex<State>,
    /// The outline with what each entry does, read once at open.
    outline: Vec<(OutlineEntry, Option<Target>)>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn frame(&self, state: &mut State) -> Frame {
        let State {
            session,
            tiles,
            thumbnails,
            fit,
            highlight_fields,
            ..
        } = state;
        tiles.begin_frame();
        let now = Instant::now();
        let viewer = &mut session.viewer;
        let mut planned = viewer.frame(now, |key| tiles.knows(key));
        let expensive = |i: usize| {
            let page = viewer.pages().get(i);
            page.is_some_and(|p| self.scheduler.is_expensive(p.doc, p.page))
        };
        let pages = planned
            .pages
            .iter()
            .map(|page| PageDraw {
                page: page.page as u32,
                rect: page.rect.into(),
                busy: !page.complete && expensive(page.page),
                tiles: page
                    .layers
                    .iter()
                    .filter_map(|layer| {
                        Some(TileDraw {
                            id: tiles.use_tile(&layer.key)?,
                            dest: layer.dest.into(),
                            exact: layer.exact,
                        })
                    })
                    .collect(),
            })
            .collect();
        // Thumbnails are the pages' placeholder tiles, so the page view and the thumbnail grid
        // share one render. They are requested after everything the page view wants: the scheduler
        // has one queue, and each frame replaces it.
        let (thumbnail_first, count) = *thumbnails;
        let thumbnail_ids = (thumbnail_first..thumbnail_first.saturating_add(count))
            .map(|page| {
                let Some(key) = viewer.placeholder_key(page as usize) else {
                    return 0;
                };
                tiles.use_tile(&key).unwrap_or_else(|| {
                    if !tiles.knows(&key) && !planned.requests.contains(&key) {
                        planned.requests.push(key);
                    }
                    0
                })
            })
            .collect();
        self.scheduler.request(planned.requests);

        // The annotations that aren't part of the pages as rendered.
        let px = session.viewer.device_scale().get() / session.viewer.zoom();
        let mut shapes = Vec::new();
        for (slot, page) in planned.pages.iter().enumerate() {
            let overlay = session.overlay(page.page);
            shapes.extend(overlay.iter().map(|s| shape_draw(slot as u32, s, px)));
        }
        // A tint over the fields that can be filled in, on top of those.
        if *highlight_fields {
            let positions: Vec<usize> = planned.pages.iter().map(|p| p.page).collect();
            for (position, corners) in session.field_outlines(&positions) {
                let Some(slot) = positions.iter().position(|p| *p == position) else {
                    continue;
                };
                let [start, rest @ ..] = corners.map(|p| Point {
                    x: (p.x * px) as f32,
                    y: (p.y * px) as f32,
                });
                shapes.push(ShapeDraw {
                    page: slot as u32,
                    fill: Some(FIELD_TINT),
                    multiply: false,
                    stroke: None,
                    stroke_width: 0.0,
                    round: false,
                    closed: true,
                    start,
                    segments: rest.map(|to| SegmentDraw::Line { to }).to_vec(),
                });
            }
        }
        // The same over the thumbnails, each in units of its own page's width.
        let mut thumbnail_shapes = Vec::new();
        for (slot, position) in (thumbnail_first..thumbnail_first.saturating_add(count)).enumerate()
        {
            let position = position as usize;
            let size = session
                .viewer
                .pages()
                .get(position)
                .map(|p| p.displayed().size());
            let Some(size) = size.filter(|s| s.width > 0.0) else {
                continue;
            };
            let height = THUMBNAIL_UNITS * size.height / size.width;
            let overlay = session.overlay_in(position, THUMBNAIL_UNITS, height);
            thumbnail_shapes.extend(overlay.iter().map(|s| shape_draw(slot as u32, s, 1.0)));
        }
        let visible = |position: usize| planned.pages.iter().any(|p| p.page == position);
        let outlined = session
            .picked_outline()
            .or_else(|| session.focused_field_outline());
        let picked = match outlined {
            Some((position, corners)) if visible(position) => corners
                .iter()
                .map(|p| Point {
                    x: (p.x * px) as f32,
                    y: (p.y * px) as f32,
                })
                .collect(),
            _ => Vec::new(),
        };

        // What the search found and what is selected on the visible pages. Positions come in
        // logical pixels, and everything in a frame is in device pixels.
        let found: Vec<_> = planned
            .pages
            .iter()
            .map(|page| (page.page, session.matches_on(page.page)))
            .filter(|(_, matches)| !matches.is_empty())
            .collect();
        let (viewer, mut texts, selection) = session.text();
        let px = viewer.device_scale().get() / viewer.zoom();
        let point = |p: ViewPoint| Point {
            x: (p.x * px) as f32,
            y: (p.y * px) as f32,
        };
        let highlight = |kind: HighlightKind, q: &[ViewPoint; 4]| Highlight {
            kind,
            start_bottom: point(q[0]),
            end_bottom: point(q[1]),
            end_top: point(q[2]),
            start_top: point(q[3]),
        };
        let mut highlights = Vec::new();
        for (page, matches) in found {
            let Some(text) = texts.text_page(page) else {
                continue;
            };
            for (found, current) in matches {
                let kind = if current {
                    HighlightKind::CurrentMatch
                } else {
                    HighlightKind::Match
                };
                let quads = text_quads(viewer, &text, page, found.chars.clone());
                highlights.extend(quads.iter().map(|q| highlight(kind, q)));
            }
        }
        let selection = selection.as_ref().filter(|s| !s.is_empty());
        let (mut selection_start, mut selection_end) = (None, None);
        if let Some(selection) = selection {
            let (start, end) = selection.range();
            for page in &planned.pages {
                let quads = selection.view_quads(viewer, &mut texts, page.page);
                if let (true, Some(q)) = (page.page == start.page, quads.first()) {
                    selection_start = Some(SelectionEdge {
                        bottom: point(q[0]),
                        top: point(q[3]),
                    });
                }
                if let (true, Some(q)) = (page.page == end.page, quads.last()) {
                    selection_end = Some(SelectionEdge {
                        bottom: point(q[1]),
                        top: point(q[2]),
                    });
                }
                highlights.extend(quads.iter().map(|q| highlight(HighlightKind::Selection, q)));
            }
        }
        Frame {
            shapes,
            picked,
            thumbnail_shapes,
            selection: selection.is_some(),
            highlights,
            selection_start,
            selection_end,
            thumbnail_first,
            thumbnails: thumbnail_ids,
            pages,
            evicted: tiles.take_evicted(),
            complete: planned.complete,
            settled: viewer.is_settled(now),
            settles_in_ms: planned
                .settles_at
                .map(|at| at.saturating_duration_since(now).as_millis() as u32),
            device_scale: viewer.device_scale().get(),
            current_page: viewer.current_spread().first().map_or(0, |&p| p as u32),
            last_page: viewer.current_spread().last().map_or(0, |&p| p as u32),
            zoom_factor: viewer.zoom_relative_to(*fit),
            paged: viewer.settings().swipe,
            two_pages: viewer.layout_mode().is_two_page(),
            cover_alone: viewer.settings().cover_alone,
            zoom: viewer.zoom(),
            zoom_mode: match viewer.zoom_mode() {
                specto_view::ZoomMode::Fixed => ZoomMode::Fixed,
                specto_view::ZoomMode::FitWidth => ZoomMode::FitWidth,
                specto_view::ZoomMode::FitPage => ZoomMode::FitPage,
            },
        }
    }

    fn take_finished(&self) -> Vec<FinishedTile> {
        let finished = self.scheduler.take_finished();
        let mut state = self.lock();
        let State { session, tiles, .. } = &mut *state;
        let viewer = &session.viewer;
        tiles.add_finished(finished, |key| {
            // Keys name a page of a file, while the viewer goes by position.
            let position = viewer
                .pages()
                .iter()
                .position(|p| p.doc == key.doc && p.page == key.page);
            position.is_some_and(|i| viewer.placeholder_key(i) == Some(*key))
        })
    }
}

/// An annotation's shape as a frame has it: `px` of the frame's units per unit of `shape`, on the
/// page at index `page` of the frame.
fn shape_draw(page: u32, shape: &ViewShape, px: f64) -> ShapeDraw {
    let point = |p: ViewPoint| Point {
        x: (p.x * px) as f32,
        y: (p.y * px) as f32,
    };
    let argb = |c: specto_annot::Color, opacity: f32| {
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
        byte(opacity) << 24 | byte(c.r) << 16 | byte(c.g) << 8 | byte(c.b)
    };
    match shape {
        ViewShape::Fill {
            quad,
            color,
            opacity,
            multiply,
        } => ShapeDraw {
            page,
            fill: Some(argb(*color, *opacity)),
            multiply: *multiply,
            stroke: None,
            stroke_width: 0.0,
            round: false,
            closed: true,
            start: point(quad[0]),
            segments: quad[1..]
                .iter()
                .map(|&p| SegmentDraw::Line { to: point(p) })
                .collect(),
        },
        ViewShape::Stroke {
            from,
            to,
            width,
            color,
            opacity,
        } => ShapeDraw {
            page,
            fill: None,
            multiply: false,
            stroke: Some(argb(*color, *opacity)),
            stroke_width: (width * px) as f32,
            round: false,
            closed: false,
            start: point(*from),
            segments: vec![SegmentDraw::Line { to: point(*to) }],
        },
        ViewShape::Path {
            path,
            fill,
            stroke,
            opacity,
        } => ShapeDraw {
            page,
            fill: fill.map(|c| argb(c, *opacity)),
            multiply: false,
            stroke: stroke.map(|(c, _)| argb(c, *opacity)),
            stroke_width: stroke.map_or(0.0, |(_, width)| (width * px) as f32),
            round: true,
            closed: path.closed,
            start: point(path.start),
            segments: path
                .segments
                .iter()
                .map(|s| match *s {
                    Segment::Line(p) => SegmentDraw::Line { to: point(p) },
                    Segment::Curve(a, b, p) => SegmentDraw::Curve {
                        c1: point(a),
                        c2: point(b),
                        to: point(p),
                    },
                })
                .collect(),
        },
    }
}

/// The fit a view mode starts with: the whole page when swiping through pages, the width when
/// scrolling.
fn default_fit(paged: bool) -> specto_view::ZoomMode {
    fit_mode(!paged)
}

fn fit_mode(fit_width: bool) -> specto_view::ZoomMode {
    if fit_width {
        specto_view::ZoomMode::FitWidth
    } else {
        specto_view::ZoomMode::FitPage
    }
}

/// The layout for swiping or scrolling through single pages or pairs.
fn layout_for(paged: bool, two_pages: bool) -> specto_view::LayoutMode {
    use specto_view::LayoutMode::{Continuous, SinglePage, TwoPage, TwoPageContinuous};
    match (paged, two_pages) {
        (true, false) => SinglePage,
        (true, true) => TwoPage,
        (false, false) => Continuous,
        (false, true) => TwoPageContinuous,
    }
}

/// Live sessions by id, so the JNI function can find one without being handed a raw pointer. A
/// closed session simply isn't found.
static SESSIONS: LazyLock<Mutex<HashMap<u64, Weak<Shared>>>> = LazyLock::new(Mutex::default);
static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

fn find_session(id: u64) -> Option<Arc<Shared>> {
    let sessions = SESSIONS.lock().unwrap_or_else(|e| e.into_inner());
    sessions.get(&id)?.upgrade()
}

/// The Rust half of `Native.copyTile`: hands the pixels of finished tile `tile` of session
/// `session` to `write`, which copies them into the frontend's bitmap.
///
/// If `write` returns `true`, the tile counts as cached from now on. Returns `false` for unknown
/// sessions and tiles, or if `write` did.
pub fn claim_tile(session: u64, tile: u64, write: impl FnOnce(&Bitmap) -> bool) -> bool {
    let Some(session) = find_session(session) else {
        return false;
    };
    session.lock().tiles.claim(tile, write)
}

/// One document shown in one view.
#[derive(uniffi::Object)]
pub struct ViewSession {
    id: u64,
    shared: Arc<Shared>,
}

/// The bytes of a file to open, read once and reusable, for instance to try again with a password.
#[derive(uniffi::Object)]
pub struct PdfFile(PdfData);

#[uniffi::export]
impl PdfFile {
    /// Takes over the open file descriptor `fd` (from `ParcelFileDescriptor.detachFd()`) and reads
    /// the file, or maps it if it's large (see [`PdfData`]).
    ///
    /// The descriptor is closed either way. Pipes, as some document providers hand out, are read.
    #[uniffi::constructor]
    pub fn from_fd(fd: i32) -> Result<Arc<Self>, PdfError> {
        if fd < 0 {
            return Err(PdfError::Io {
                reason: "invalid file descriptor".into(),
            });
        }
        #[cfg(unix)]
        {
            use std::os::fd::FromRawFd;
            // SAFETY: the caller hands over ownership of an open descriptor (detachFd), so nothing
            // else closes or uses it.
            #[allow(unsafe_code)]
            let file = unsafe { std::fs::File::from_raw_fd(fd) };
            PdfData::from_file(file)
                .map(|d| Arc::new(Self(d)))
                .map_err(|e| PdfError::Io {
                    reason: e.to_string(),
                })
        }
        #[cfg(not(unix))]
        Err(PdfError::Io {
            reason: "file descriptors are not supported here".into(),
        })
    }

    /// Bytes already in memory.
    #[uniffi::constructor]
    pub fn from_bytes(data: Vec<u8>) -> Arc<Self> {
        Arc::new(Self(data.into()))
    }

    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }

    /// Whether the file is mapped rather than read into memory.
    pub fn is_mapped(&self) -> bool {
        self.0.is_mapped()
    }

    /// Up to `length` bytes starting at `offset`: fewer at the end of the file, none beyond it.
    ///
    /// For recognising a file by parts of its content without copying all of it.
    pub fn read(&self, offset: u64, length: u32) -> Vec<u8> {
        let size = self.0.len() as u64;
        let start = offset.min(size);
        let end = start.saturating_add(u64::from(length)).min(size);
        self.0[start as usize..end as usize].to_vec()
    }
}

#[uniffi::export]
impl ViewSession {
    /// Opens a PDF. `cache_bytes` is the budget for tile bitmaps.
    #[uniffi::constructor]
    pub fn open(
        data: Vec<u8>,
        password: Option<String>,
        cache_bytes: u64,
        listener: Arc<dyn ViewListener>,
    ) -> Result<Arc<Self>, PdfError> {
        Self::open_data(data.into(), password, cache_bytes, listener)
    }

    /// Opens `file`, as [`open`](Self::open) does.
    #[uniffi::constructor]
    pub fn open_file(
        file: Arc<PdfFile>,
        password: Option<String>,
        cache_bytes: u64,
        listener: Arc<dyn ViewListener>,
    ) -> Result<Arc<Self>, PdfError> {
        Self::open_data(file.0.clone(), password, cache_bytes, listener)
    }
}

impl ViewSession {
    /// Writes the document as edited to the descriptor `fd`, which is closed either way, with its
    /// password protection or without. Returns the version of the document that was written.
    fn write_to_fd(&self, fd: i32, protected: bool) -> Result<specto_doc::Version, PdfError> {
        if fd < 0 {
            return Err(PdfError::Save {
                reason: "invalid file descriptor".into(),
            });
        }
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::fd::FromRawFd;
            // SAFETY: the caller hands over ownership of an open descriptor (detachFd), so nothing
            // else closes or uses it.
            #[allow(unsafe_code)]
            let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
            let job = self.shared.lock().session.save_job();
            // All of it in memory first, so that a failure writes nothing.
            let data = if protected {
                job.write()
            } else {
                job.write_unprotected()
            }
            .map_err(|e| PdfError::Save {
                reason: e.to_string(),
            })?;
            file.write_all(&data)
                .and_then(|()| file.flush())
                .map_err(|e| PdfError::Save {
                    reason: e.to_string(),
                })?;
            // Cuts off what a longer file had beyond this, and gets it onto the disk. Neither works
            // on everything a descriptor can be (a pipe to a document provider), and neither is
            // needed there.
            let _ = file.set_len(data.len() as u64);
            let _ = file.sync_all();
            Ok(job.version)
        }
        #[cfg(not(unix))]
        Err(PdfError::Save {
            reason: "file descriptors are not supported here".into(),
        })
    }
}

impl ViewSession {
    fn open_data(
        data: PdfData,
        password: Option<String>,
        cache_bytes: u64,
        listener: Arc<dyn ViewListener>,
    ) -> Result<Arc<Self>, PdfError> {
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get().clamp(2, 4));
        let on_tiles = listener.clone();
        let scheduler = Scheduler::new(threads, move || on_tiles.tiles_ready());
        let options = Options {
            settings: Settings {
                // Phone screens are narrow: spend less of them on chrome.
                gap: 8.0,
                margin: 8.0,
                ..Settings::default()
            },
            text_pages: TEXT_PAGES,
        };
        let session = Session::open(&scheduler, data, password.as_deref(), options)?;
        let mut outline = Vec::new();
        flatten_outline(session.outline(), 0, &mut outline);
        let shared = Arc::new(Shared {
            scheduler,
            listener,
            state: Mutex::new(State {
                session,
                tiles: TileBook::new(cache_bytes as usize),
                encoded: Vec::new(),
                thumbnails: (0, 0),
                // Sessions start out scrolling.
                fit: default_fit(false),
                written: None,
                highlight_fields: false,
            }),
            outline,
        });
        let id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
        SESSIONS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, Arc::downgrade(&shared));
        Ok(Arc::new(Self { id, shared }))
    }
}

#[uniffi::export]
impl ViewSession {
    /// Identifies this session to the JNI functions in `Native`.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// How many pages the document has now (deleting pages changes it).
    pub fn page_count(&self) -> u32 {
        self.shared.lock().session.viewer.page_count() as u32
    }

    /// The pages' printed numbers (such as "i", "ii", "1" or "A-3"), one per page, for documents
    /// that number their pages other than 1, 2, 3. `None` otherwise: then a page's number is its
    /// position.
    pub fn page_labels(&self) -> Option<Vec<String>> {
        let state = self.shared.lock();
        let labels = state.session.labels()?;
        Some(
            (0..state.session.viewer.page_count())
                .map(|i| labels.label(i))
                .collect(),
        )
    }

    /// The zero-based page the user means by `text`: a printed page number (exactly, then ignoring
    /// case), else a position counted from 1.
    pub fn find_page(&self, text: String) -> Option<u32> {
        let state = self.shared.lock();
        state.session.find_page(text.trim()).map(|i| i as u32)
    }

    /// What the document says about itself.
    pub fn info(&self) -> DocumentInfo {
        let state = self.shared.lock();
        let source = state.session.main();
        let m = source.metadata();
        DocumentInfo {
            title: m.title,
            author: m.author,
            subject: m.subject,
            keywords: m.keywords,
            creator: m.creator,
            producer: m.producer,
            created: m.created.map(|d| d.to_string()),
            modified: m.modified.map(|d| d.to_string()),
            pdf_version: source.version().to_owned(),
        }
    }

    /// The outline (table of contents), flattened in reading order. Empty if the document has none.
    pub fn outline(&self) -> Vec<OutlineEntry> {
        let state = self.shared.lock();
        let entries = self.shared.outline.iter().map(|(entry, target)| {
            // Destinations name pages of the file, which may have moved or been deleted.
            let page = match target {
                Some(Target::Destination(d)) => state.session.position_of(MAIN, d.page),
                _ => None,
            };
            OutlineEntry {
                page: page.map(|p| p as u32),
                ..entry.clone()
            }
        });
        entries.collect()
    }

    /// Goes where outline entry `index` (into [`outline`](Self::outline)) leads.
    pub fn activate_outline(&self, index: u32) -> Activated {
        match self.shared.outline.get(index as usize) {
            Some((_, Some(target))) => self.shared.lock().follow(MAIN, target),
            _ => Activated::Nothing,
        }
    }

    /// A tap at (`x`, `y`) in logical pixels from the view's top-left: follows the link there, if
    /// there is one.
    pub fn tap(&self, x: f64, y: f64) -> Activated {
        let mut state = self.shared.lock();
        match state.session.link_at(ViewPoint::new(x, y)) {
            Some((source, target)) => state.follow(source, &target),
            None => Activated::Nothing,
        }
    }

    // Annotations
    //
    // Marks on the text, and whatever else a file brings along (notes, drawings, text boxes). What
    // a frame must draw of them is in [`Frame::shapes`]. The rest is part of the pages as rendered.
    // Adding and deleting one are edits like those of the pages: undone with [`undo`](Self::undo),
    // saved with the document.

    /// Marks the selected text: a highlight, or a line under or through it.
    ///
    /// The selection is given up. Returns whether anything was marked (not without a selection).
    pub fn mark_selection(&self, markup: Markup) -> bool {
        // The colors the desktop app starts out with.
        let (style, color) = match markup {
            Markup::Highlight => (MarkupStyle::Highlight, Color::new(1.0, 0.92, 0.23)),
            Markup::Underline => (MarkupStyle::Underline, Color::new(0.88, 0.11, 0.14)),
            Markup::StrikeOut => (MarkupStyle::StrikeOut, Color::new(0.88, 0.11, 0.14)),
        };
        self.shared.lock().session.mark_selection(style, color)
    }

    /// A tap at (`x`, `y`) in logical pixels: picks the topmost annotation there, or none if there
    /// is none (or only one its file doesn't let be changed).
    ///
    /// Returns whether one is picked now. The picked one is outlined ([`Frame::picked`]) and is
    /// what [`remove_picked_annotation`](Self::remove_picked_annotation) deletes.
    pub fn pick_annotation_at(&self, x: f64, y: f64) -> bool {
        let mut state = self.shared.lock();
        let picked = state.session.annotation_at(ViewPoint::new(x, y));
        state.session.picked = picked;
        picked.is_some()
    }

    /// Lets go of the picked annotation. Returns whether one was picked.
    pub fn clear_picked_annotation(&self) -> bool {
        self.shared.lock().session.picked.take().is_some()
    }

    /// Deletes the picked annotation.
    ///
    /// Returns whether there was one. Fails for an annotation of a file that can't be saved either:
    /// its page has to be written anew without it.
    pub fn remove_picked_annotation(&self) -> Result<bool, PdfError> {
        self.shared
            .lock()
            .session
            .remove_picked_annotation(&self.shared.scheduler)
            .map_err(|e| PdfError::Save {
                reason: e.to_string(),
            })
    }

    // Forms
    //
    // The fields of the file that was opened. What is filled in shows in the pages as rendered,
    // and is an edit like those of the pages: undone with [`undo`](Self::undo), saved with the
    // document.

    /// A tap at (`x`, `y`) in logical pixels on a field of the form that can be filled in.
    ///
    /// Ticks or unticks a checkbox and chooses a radio button right away. For a text field, a
    /// menu or a list it says what to ask the user for. That field is outlined ([`Frame::picked`])
    /// until [`end_field`](Self::end_field), and a picked annotation is let go of.
    pub fn tap_field(&self, x: f64, y: f64) -> FieldTap {
        let mut state = self.shared.lock();
        let session = &mut state.session;
        let Some(click) = session.click_field(&self.shared.scheduler, ViewPoint::new(x, y)) else {
            return FieldTap::Nothing;
        };
        let fields = session.form_fields();
        let field_of = |id: FieldId| fields.iter().find(|f| f.id == id);
        let label = |id: FieldId| field_of(id).and_then(|f| f.label.clone());
        let tap = match click {
            FieldClick::Changed | FieldClick::Focused => FieldTap::Changed,
            FieldClick::Text {
                field,
                multiline,
                password,
                text,
                ..
            }
            | FieldClick::Edit {
                field,
                multiline,
                password,
                text,
                ..
            } => FieldTap::Text {
                field: field_number(field),
                label: label(field),
                multiline,
                password,
                max_length: field_of(field).and_then(|f| match f.kind {
                    FieldKind::Text { max_length, .. } => max_length,
                    _ => None,
                }),
                text,
            },
            // A menu that offers nothing and takes text of one's own is asked for as text.
            FieldClick::Choose {
                field,
                labels,
                typed: Some(text),
                ..
            } if labels.is_empty() => FieldTap::Text {
                field: field_number(field),
                label: label(field),
                multiline: false,
                password: false,
                max_length: None,
                text,
            },
            FieldClick::ChooseSeveral {
                field,
                labels,
                chosen,
                ..
            } => FieldTap::ChooseSeveral {
                field: field_number(field),
                label: label(field),
                options: labels,
                chosen: chosen.iter().map(|&i| i as u32).collect(),
            },
            FieldClick::Choose {
                field,
                labels,
                chosen,
                typed,
                ..
            } => FieldTap::Choose {
                field: field_number(field),
                label: label(field),
                options: labels,
                typed,
                chosen: chosen.map(|i| i as u32),
            },
        };
        session.picked = None;
        if tap == FieldTap::Changed {
            // Nothing is left to ask for.
            session.clear_field_focus();
        }
        tap
    }

    /// Fills `text` into the text field `field`, as far as the field takes it, or into a menu that
    /// takes text of one's own. Returns whether that changed what it holds.
    pub fn set_field_text(&self, field: u64, text: String) -> bool {
        let mut state = self.shared.lock();
        (state.session).set_field_text(&self.shared.scheduler, field_id(field), &text)
    }

    /// Chooses option `index` of the menu or list `field`. Returns whether that changed what it
    /// holds.
    pub fn choose_option(&self, field: u64, index: u32) -> bool {
        let mut state = self.shared.lock();
        (state.session).choose_option(&self.shared.scheduler, field_id(field), index as usize)
    }

    /// Chooses the options `chosen` of `field`, a list that takes several, and no others. Returns
    /// whether that changed what it holds.
    pub fn choose_options(&self, field: u64, chosen: Vec<u32>) -> bool {
        let chosen: Vec<usize> = chosen.iter().map(|&i| i as usize).collect();
        let mut state = self.shared.lock();
        (state.session).choose_options(&self.shared.scheduler, field_id(field), &chosen)
    }

    /// Whether the document has a form with a field that can be filled in.
    pub fn has_form(&self) -> bool {
        self.shared.lock().session.has_form()
    }

    /// Sets every field that can be filled in back to what it holds at first, as one edit.
    /// Returns whether that changed anything.
    pub fn reset_form(&self) -> bool {
        let mut state = self.shared.lock();
        state.session.reset_form(&self.shared.scheduler)
    }

    /// Whether frames tint every field that can be filled in ([`Frame::shapes`]), to show where
    /// they are. Off at first.
    pub fn set_highlight_fields(&self, on: bool) {
        self.shared.lock().highlight_fields = on;
    }

    /// The field that [`tap_field`](Self::tap_field) asked about is done with, filled in or not:
    /// it is no longer outlined. Returns whether one was.
    pub fn end_field(&self) -> bool {
        self.shared.lock().session.clear_field_focus()
    }

    // Editing
    //
    // Pages are named by their position at the time of the call, from 0. Every edit can be undone.
    // After one, positions have changed: ask again for the page count, sizes, labels and outline,
    // and plan a new frame. A selection is dropped, and a search runs again by itself.

    /// Turns `pages` by a quarter, clockwise or the other way. Returns whether anything changed.
    pub fn rotate_pages(&self, pages: Vec<u32>, clockwise: bool) -> bool {
        let by = if clockwise {
            specto_geom::Rotation::Cw90
        } else {
            specto_geom::Rotation::Cw270
        };
        self.shared.lock().session.rotate(&positions(&pages), by)
    }

    /// Removes `pages` from the document.
    ///
    /// Not all of them: a document keeps at least one page. Returns whether anything changed.
    pub fn delete_pages(&self, pages: Vec<u32>) -> bool {
        self.shared.lock().session.delete(&positions(&pages))
    }

    /// Moves `pages`, in their order in the document, to before the page now at position `to` (the
    /// page count: to the end).
    ///
    /// Returns the position the first of them ends up at, or `None` if nothing moved.
    pub fn move_pages(&self, pages: Vec<u32>, to: u32) -> Option<u32> {
        let moved = self
            .shared
            .lock()
            .session
            .move_pages(&positions(&pages), to as usize)?;
        Some(moved.start as u32)
    }

    /// Takes back the last edit.
    ///
    /// Returns what it was, or `None` if there was nothing to undo.
    pub fn undo(&self) -> Option<EditStep> {
        self.shared
            .lock()
            .session
            .undo(&self.shared.scheduler)
            .map(EditStep::from)
    }

    /// Does again what [`undo`](Self::undo) took back.
    pub fn redo(&self) -> Option<EditStep> {
        self.shared
            .lock()
            .session
            .redo(&self.shared.scheduler)
            .map(EditStep::from)
    }

    pub fn edit_state(&self) -> EditState {
        let state = self.shared.lock();
        let edit = state.session.edit();
        EditState {
            modified: edit.is_modified(),
            undo: edit.undo_edit().map(EditStep::from),
            redo: edit.redo_edit().map(EditStep::from),
        }
    }

    /// Writes the document as edited, as a new PDF, to the open file descriptor `fd` (from
    /// `ParcelFileDescriptor.detachFd()`, opened for writing), which is closed either way.
    ///
    /// Takes a while for a large document: call it off the main thread.
    ///
    /// With `counts_as_saved`, the document is unmodified once written (edits made meanwhile stay
    /// unsaved). Without, this is a copy on the side, to share for instance, or to put in the
    /// document's place afterwards (then [`written_is_saved`](Self::written_is_saved) says so).
    ///
    /// The session goes on reading the file it was opened from. That file may be the one written to
    /// only if it isn't mapped ([`PdfFile::is_mapped`]): a mapped file must not change while open.
    pub fn save_to_fd(&self, fd: i32, counts_as_saved: bool) -> Result<(), PdfError> {
        let version = self.write_to_fd(fd, true)?;
        let mut state = self.shared.lock();
        if counts_as_saved {
            state.session.mark_saved(version);
        } else {
            state.written = Some(version);
        }
        Ok(())
    }

    /// Writes the document as edited to `fd` like [`save_to_fd`](Self::save_to_fd), but without its
    /// password protection, and without it counting as saved or as the copy
    /// [`written_is_saved`](Self::written_is_saved) is about: what the system's print service is
    /// given, which can't be told a password.
    pub fn print_to_fd(&self, fd: i32) -> Result<(), PdfError> {
        self.write_to_fd(fd, false).map(|_| ())
    }

    /// The copy last written with [`save_to_fd`](Self::save_to_fd) has become the saved document
    /// (it was put where the document is kept): the document is unmodified, unless it was edited
    /// since that copy. Does nothing if no copy was written.
    pub fn written_is_saved(&self) {
        let mut state = self.shared.lock();
        if let Some(version) = state.written.take() {
            state.session.mark_saved(version);
        }
    }

    // Text selection
    //
    // Positions are in logical pixels from the view's top-left, like taps. The selection shows up
    // in frames: [`Frame::highlights`] and the edges for its handles.

    /// A long press at (`x`, `y`): selects the word there and returns true, or does nothing and
    /// returns false if there is no text there.
    ///
    /// Moving on with [`select_to`](Self::select_to) then selects word by word.
    pub fn select_word_at(&self, x: f64, y: f64) -> bool {
        let mut state = self.shared.lock();
        let (viewer, mut texts, selection) = state.session.text();
        let p = ViewPoint::new(x, y);
        if !over_text(viewer, &mut texts, p) {
            return false;
        }
        let Some(at) = text_char(viewer, &mut texts, p) else {
            return false;
        };
        let word = Selection::new(&mut texts, at, Granularity::Word);
        if word.is_empty() {
            return false;
        }
        *selection = Some(word);
        true
    }

    /// Takes hold of one end of the selection (its end, or else its start), as when a handle is
    /// touched: [`select_to`](Self::select_to) then moves that end, character by character, and the
    /// other one stays.
    pub fn grab_selection(&self, end: bool) {
        let mut state = self.shared.lock();
        let (_, mut texts, selection) = state.session.text();
        let Some(current) = selection.as_ref() else {
            return;
        };
        let (start, stop) = current.range();
        let (fixed, moving) = if end { (start, stop) } else { (stop, start) };
        let mut held = Selection::new(&mut texts, fixed, Granularity::Char);
        held.extend(&mut texts, moving);
        *selection = Some(held);
    }

    /// Extends the selection to the text nearest to (`x`, `y`): from the word first selected, or
    /// moving the end last grabbed.
    ///
    /// `Native.selectTo` does the same without UniFFI's overhead, for drags.
    pub fn select_to(&self, x: f64, y: f64) {
        hot::select_to(self.id, x, y);
    }

    /// Selects the whole document.
    pub fn select_all(&self) {
        let mut state = self.shared.lock();
        let pages = state.session.viewer.page_count();
        state.session.selection = Some(Selection::all(pages));
    }

    pub fn clear_selection(&self) {
        self.shared.lock().session.selection = None;
    }

    /// The selected text, for copying: lines and pages separated by newlines.
    ///
    /// `None` if nothing is selected or the selection holds no text. Reads every selected page,
    /// which takes a while for a long document selected as a whole.
    pub fn selected_text(&self) -> Option<String> {
        self.shared.lock().session.selected_text()
    }

    // Search
    //
    // Matches show up in frames like the selection does ([`Frame::highlights`]). The document is
    // read for searching once, in the background, starting with the first search. Until that is
    // done a search keeps finding more, and says so through [`ViewListener::search_changed`].

    /// Searches for `text`, ignoring case and accents, and goes to the first match from the current
    /// page on.
    ///
    /// Each call starts afresh, so this can follow a search field as it is typed into. Text with
    /// nothing to search for ends the search.
    pub fn search(&self, text: String) {
        let mut state = self.shared.lock();
        if state.session.search(&text) {
            state.session.reveal_match();
        }
        if state.session.search_status().is_some() {
            search::start_indexing(&self.shared, &mut state);
        }
    }

    /// Goes to the next (or previous) match. After the last one comes the first again.
    pub fn search_step(&self, forward: bool) {
        let mut state = self.shared.lock();
        if state.session.search_step(forward) {
            state.session.reveal_match();
        }
    }

    pub fn end_search(&self) {
        self.shared.lock().session.end_search();
    }

    /// How the search stands, or `None` if there is none.
    pub fn search_status(&self) -> Option<SearchStatus> {
        let status = self.shared.lock().session.search_status()?;
        Some(SearchStatus {
            matches: status.matches as u32,
            current: status.current.map(|i| i as u32),
            pages_searched: status.pages_indexed as u32,
            pages: status.pages as u32,
        })
    }

    /// Sizes of all pages as displayed, for the thumbnail grid's cells for instance.
    pub fn page_sizes(&self) -> Vec<PageSize> {
        let state = self.shared.lock();
        state
            .session
            .viewer
            .pages()
            .iter()
            .map(|page| {
                let size = page.displayed().size();
                PageSize {
                    width: size.width as f32,
                    height: size.height as f32,
                }
            })
            .collect()
    }

    /// The pages whose thumbnails are on screen (`count` may be 0).
    ///
    /// Frames report their tiles in [`Frame::thumbnails`], keep them cached, and request the ones
    /// still missing.
    pub fn set_thumbnail_range(&self, first: u32, count: u32) {
        hot::set_thumbnail_range(self.id, first, count);
    }

    /// View size in logical pixels (dp) and device pixels per logical pixel (the display density,
    /// such as 2.625).
    pub fn set_viewport(&self, width: f64, height: f64, scale: f64) {
        hot::set_viewport(self.id, width, height, scale);
    }

    /// Scrolls the content by this many logical pixels.
    pub fn scroll_by(&self, dx: f64, dy: f64) {
        self.shared
            .lock()
            .session
            .viewer
            .scroll_by(ViewVector::new(dx, dy));
    }

    /// Multiplies the zoom, keeping the content under (`x`, `y`) in place (logical pixels from the
    /// view's top-left).
    pub fn zoom_by(&self, factor: f64, x: f64, y: f64) {
        self.shared
            .lock()
            .session
            .viewer
            .zoom_by(factor, ViewPoint::new(x, y));
    }

    /// Switches between swiping through pages and scrolling, keeping the page.
    ///
    /// Sessions start out [`ViewMode::Continuous`]. A change of mode also returns to the mode's own
    /// fit: the whole page when swiping, the width when scrolling (see
    /// [`set_fit_width`](Self::set_fit_width)).
    pub fn set_view_mode(&self, mode: ViewMode) {
        let mut state = self.shared.lock();
        let paged = mode == ViewMode::Paged;
        if paged != state.session.viewer.settings().swipe {
            state.fit = default_fit(paged);
        }
        let fit = state.fit;
        let viewer = &mut state.session.viewer;
        viewer.set_layout_mode(layout_for(paged, viewer.layout_mode().is_two_page()));
        viewer.set_swipe(paged);
        viewer.set_zoom_mode(fit);
    }

    /// Chooses what the view is fitted to: the page's width, with the rest of a taller page reached
    /// by scrolling, or the whole page.
    ///
    /// Also undoes any zooming. The choice holds until the view mode changes.
    pub fn set_fit_width(&self, fit_width: bool) {
        let mut state = self.shared.lock();
        state.fit = fit_mode(fit_width);
        let fit = state.fit;
        state.session.viewer.set_zoom_mode(fit);
    }

    /// Shows pages two side by side (or one at a time again), in either view mode, keeping the
    /// page.
    ///
    /// With `cover_alone`, the first page stands on its own, so that a book's left and right pages
    /// end up left and right.
    pub fn set_spread(&self, two_pages: bool, cover_alone: bool) {
        let mut state = self.shared.lock();
        let fit = state.fit;
        let viewer = &mut state.session.viewer;
        let layout = layout_for(viewer.settings().swipe, two_pages);
        if layout == viewer.layout_mode() && cover_alone == viewer.settings().cover_alone {
            return;
        }
        viewer.set_layout_mode(layout);
        viewer.set_cover_alone(cover_alone);
        viewer.set_zoom_mode(fit);
    }

    /// Where the reader is, to be stored and passed to [`restore_view`](Self::restore_view) the
    /// next time the document is opened.
    pub fn saved_view(&self) -> SavedView {
        let state = self.shared.lock();
        let view = state.session.viewer.view_state();
        SavedView {
            page: view.page as u32,
            page_offset: view.page_offset,
            paged: state.session.viewer.settings().swipe,
            zoom: (view.zoom_mode == specto_view::ZoomMode::Fixed).then_some(view.zoom),
            two_pages: view.layout.is_two_page(),
            cover_alone: view.cover_alone,
            fit_width: state.fit == specto_view::ZoomMode::FitWidth,
        }
    }

    /// Continues where [`saved_view`](Self::saved_view) left off.
    ///
    /// Call once the viewport is set. Values that no longer fit (the file changed) are clamped.
    pub fn restore_view(&self, saved: SavedView) {
        self.set_view_mode(if saved.paged {
            ViewMode::Paged
        } else {
            ViewMode::Continuous
        });
        self.set_spread(saved.two_pages, saved.cover_alone);
        self.set_fit_width(saved.fit_width);
        let mut state = self.shared.lock();
        let fit = state.fit;
        let viewer = &mut state.session.viewer;
        let mut view = viewer.view_state();
        view.page = saved.page as usize;
        view.page_offset = saved.page_offset;
        match saved.zoom {
            Some(zoom) => {
                view.zoom_mode = specto_view::ZoomMode::Fixed;
                view.zoom = zoom;
            }
            None => view.zoom_mode = fit,
        }
        viewer.restore_view_state(&view);
    }

    /// Back to the fitted view (after zooming in).
    pub fn fit(&self) {
        let mut state = self.shared.lock();
        let fit = state.fit;
        state.session.viewer.set_zoom_mode(fit);
    }

    /// Call when a drag or pinch ends in paged mode: how far the view still has to scroll to come
    /// to rest on a page (animate `scrollBy` over it), or `None` if it rests where it is.
    ///
    /// (`vx`, `vy`) is the scrolling velocity at release in logical pixels per second. A flick
    /// turns the page.
    pub fn snap_delta(&self, vx: f64, vy: f64) -> Option<Offset> {
        let state = self.shared.lock();
        let delta = state.session.viewer.snap_delta(ViewVector::new(vx, vy))?;
        Some(Offset {
            x: delta.x,
            y: delta.y,
        })
    }

    /// A pinch (or zoom animation) starts.
    ///
    /// Until [`end_zoom_gesture`](Self::end_zoom_gesture), no exact-scale tiles are rendered for
    /// the zooms passed through.
    pub fn begin_zoom_gesture(&self) {
        self.shared.lock().session.viewer.begin_zoom_gesture();
    }

    pub fn end_zoom_gesture(&self) {
        self.shared.lock().session.viewer.end_zoom_gesture();
    }

    pub fn set_zoom_mode(&self, mode: ZoomMode) {
        self.shared.lock().session.viewer.set_zoom_mode(match mode {
            ZoomMode::Fixed => specto_view::ZoomMode::Fixed,
            ZoomMode::FitWidth => specto_view::ZoomMode::FitWidth,
            ZoomMode::FitPage => specto_view::ZoomMode::FitPage,
        });
    }

    pub fn set_layout_mode(&self, mode: LayoutMode) {
        self.shared
            .lock()
            .session
            .viewer
            .set_layout_mode(match mode {
                LayoutMode::SinglePage => specto_view::LayoutMode::SinglePage,
                LayoutMode::Continuous => specto_view::LayoutMode::Continuous,
                LayoutMode::TwoPage => specto_view::LayoutMode::TwoPage,
                LayoutMode::TwoPageContinuous => specto_view::LayoutMode::TwoPageContinuous,
            });
    }

    /// Shows `page` (zero-based) at the top of the view.
    ///
    /// For following the reader, as a strip of thumbnails does. A page the reader asked for is for
    /// [`jump_to_page`](Self::jump_to_page).
    pub fn go_to_page(&self, page: u32) {
        self.shared.lock().session.viewer.go_to_page(page as usize);
    }

    /// Shows `page` (zero-based) at the top of the view, and remembers where the reader was for
    /// [`go_back`](Self::go_back): for a page number the reader typed.
    pub fn jump_to_page(&self, page: u32) {
        let mut state = self.shared.lock();
        (state.session.viewer).jump(|v| v.go_to_page(page as usize));
    }

    /// Whether there is a place to go back to: the reader followed a link or an outline entry,
    /// went to a search result, a markup or a page by its number.
    pub fn can_go_back(&self) -> bool {
        self.shared.lock().session.viewer.can_go_back()
    }

    /// Whether [`go_back`](Self::go_back) left a place to return to.
    pub fn can_go_forward(&self) -> bool {
        self.shared.lock().session.viewer.can_go_forward()
    }

    /// Returns to where the reader was before the last jump. Returns whether there was one.
    pub fn go_back(&self) -> bool {
        self.shared.lock().session.viewer.go_back()
    }

    /// Undoes the last [`go_back`](Self::go_back). Returns whether there was one to undo.
    pub fn go_forward(&self) -> bool {
        self.shared.lock().session.viewer.go_forward()
    }

    /// Goes to the next or previous page: the next pair with two pages, and the top of the
    /// neighboring page when scrolling continuously. Nothing happens at either end of the document.
    pub fn turn_page(&self, forward: bool) {
        let mut state = self.shared.lock();
        if forward {
            state.session.viewer.next_page();
        } else {
            state.session.viewer.previous_page();
        }
    }

    /// Plans what to draw now and queues the tiles that are still missing.
    ///
    /// Call after anything changed: input, a new viewport, or new tiles.
    pub fn frame(&self) -> Frame {
        self.shared.frame(&mut self.shared.lock())
    }

    /// Tiles that finished rendering.
    ///
    /// For each, create a bitmap of the given size and pass it to `Native.copyTile`, then call
    /// [`frame`](Self::frame). Tiles not copied before the next call are discarded.
    pub fn take_finished(&self) -> Vec<FinishedTile> {
        self.shared.take_finished()
    }

    /// Drops every tile the last frame didn't use (`onTrimMemory`). Their ids show up in the next
    /// frame's `evicted`.
    pub fn trim_memory(&self) {
        self.shared.lock().tiles.trim();
    }
}

impl Drop for ViewSession {
    fn drop(&mut self) {
        SESSIONS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

#[cfg(test)]
mod tests;
