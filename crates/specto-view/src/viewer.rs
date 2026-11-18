use specto_geom::{
    Crop, DeviceIntRect, DevicePoint, DeviceRect, LayoutPoint, LayoutRect, LayoutVector,
    PageGeometry, PagePoint, PageSize, PdfBox, PdfPoint, Rotation, ViewPoint, ViewRect, ViewSize,
    ViewVector,
};
use specto_render::Content;
use std::time::{Duration, Instant};

use specto_render::tile::{
    DocId, TILE_SIZE, TileKey, TileScale, level_for_scale, level_scale, tiles_covering,
};

use crate::layout::{
    LaidSpread, Layout, LayoutMode, LayoutParams, StripParams, group_spreads, layout, layout_strip,
    spread_points,
};

/// A release faster than this (logical pixels per second) turns the page in swipe mode, however
/// short the drag was.
const FLICK_VELOCITY: f64 = 250.0;

/// A page as the viewer sees it: which page of which document, and how it is shown.
///
/// These come from the edit model, so reordering or merging is just a new list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewPage {
    pub doc: DocId,
    pub page: u32,
    /// Geometry as stored in the file (its own `/Rotate` included). The pages the viewer lays out
    /// ([`Viewer::pages`]) have `crop` applied to it.
    pub geometry: PageGeometry,
    /// The part of the page to show in the layouts of single pages and pairs, if not all of it, as
    /// with margins trimmed. The continuous layouts show pages whole.
    pub crop: Option<Crop>,
    /// Extra clockwise rotation (the user's "rotate page").
    pub rotation: Rotation,
}

impl ViewPage {
    /// Geometry as displayed.
    pub fn displayed(&self) -> PageGeometry {
        self.geometry.rotated(self.rotation)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ZoomMode {
    /// Zoom stays where the user put it.
    Fixed,
    /// The widest spread fills the viewport width.
    #[default]
    FitWidth,
    /// The largest spread fits entirely in the viewport.
    FitPage,
}

impl ZoomMode {
    /// A stable name, for settings files and actions.
    pub fn name(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::FitWidth => "fit-width",
            Self::FitPage => "fit-page",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Fixed, Self::FitWidth, Self::FitPage]
            .into_iter()
            .find(|m| m.name() == name)
    }
}

/// What a frontend remembers about a document between sessions.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewState {
    /// The page at the top of the viewport.
    pub page: usize,
    /// How far down that page the viewport starts, as a fraction of the page's height (so it
    /// survives a different window size or zoom).
    ///
    /// Negative if it starts in the margin or gap above the page.
    pub page_offset: f64,
    pub layout: LayoutMode,
    pub cover_alone: bool,
    pub zoom_mode: ZoomMode,
    /// Logical pixels per point, only used with `ZoomMode::Fixed`.
    pub zoom: f64,
}

#[derive(Clone, Debug)]
pub struct Settings {
    /// Gap between spreads and between the pages of a spread, logical pixels.
    pub gap: f64,
    /// Space around the content, logical pixels.
    pub margin: f64,
    pub min_zoom: f64,
    pub max_zoom: f64,
    /// In two-page modes, show the first page on its own.
    pub cover_alone: bool,
    /// What tiles contain: by default, the page with the annotations its file has.
    pub content: Content,
    /// Prefetch tiles this many viewport heights above and below.
    pub prefetch: f64,
    /// Swipe paging for the paged layout modes (touch screens): all spreads are laid out side by
    /// side, each fitted to the viewport on its own, and scrolling can slide to the neighboring
    /// spread.
    ///
    /// The frontend lets the view come to rest with [`Viewer::snap_delta`]. Without it, paged modes
    /// lay out only the current spread and change it with [`Viewer::next_page`] and
    /// [`Viewer::previous_page`].
    pub swipe: bool,
    /// How long the zoom must stay unchanged (and no zoom gesture be active) before the view counts
    /// as settled and switches from √2-level tiles to tiles at the exact device scale.
    pub settle_delay: Duration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            gap: 12.0,
            margin: 12.0,
            min_zoom: 0.05,
            max_zoom: 32.0,
            cover_alone: false,
            content: Content::PageAndAnnotations,
            prefetch: 0.5,
            swipe: false,
            settle_delay: Duration::from_millis(250),
        }
    }
}

/// What to draw for one frame, in device pixels relative to the viewport.
#[derive(Clone, Debug, Default)]
pub struct Frame {
    pub pages: Vec<PageDraw>,
    /// Tiles to render, highest priority first. Pass to `Scheduler::request`.
    pub requests: Vec<TileKey>,
    /// Every visible tile is available at full sharpness.
    pub complete: bool,
    /// The view is still zooming (or just stopped) and is drawn from √2-level tiles.
    ///
    /// Once settled it switches to exact-scale tiles: redraw at this time even if nothing else
    /// happens.
    pub settles_at: Option<Instant>,
}

/// One visible page.
#[derive(Clone, Debug)]
pub struct PageDraw {
    pub page: usize,
    /// The page, snapped to whole device pixels.
    ///
    /// Fill it white, then draw `layers` in order, clipped to this rectangle.
    pub rect: DeviceRect,
    pub layers: Vec<TileDraw>,
    /// Every visible tile of this page is available at full sharpness.
    pub complete: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct TileDraw {
    pub key: TileKey,
    pub dest: DeviceRect,
    /// The tile maps one to one onto device pixels: draw without filtering.
    pub exact: bool,
}

/// A content position that should stay put across relayouts (zoom, resize, mode changes): a point
/// within a spread, in points, and where it was on screen.
///
/// Points in a margin or gap are clamped to the spread's edge. The rest is kept in logical pixels,
/// since margins and gaps don't scale with zoom.
struct Anchor {
    spread: usize,
    rel: (f64, f64),
    outside: (f64, f64),
    view: ViewPoint,
}

/// A position in the document that survives a different window size or zoom: a page, and how far
/// down it the viewport starts as a fraction of its height (see [`ViewState::page_offset`]).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    page: usize,
    offset: f64,
}

/// How many places Back remembers. The oldest are forgotten.
const MAX_HISTORY: usize = 100;

/// How far apart two places on a page must be to count as different, as a fraction of the page's
/// height.
const SAME_PLACE: f64 = 0.05;

/// UI-agnostic viewer state: layout, zoom and scrolling, plus planning which tiles to draw and
/// request. Frontends feed it input and draw its [`Frame`]s.
pub struct Viewer {
    /// The pages as given, with their crops.
    given: Vec<ViewPage>,
    /// The pages as laid out: cropped in the layouts that crop, whole in the others.
    pages: Vec<ViewPage>,
    sizes: Vec<PageSize>,
    settings: Settings,
    mode: LayoutMode,
    zoom_mode: ZoomMode,
    /// Logical pixels per point.
    zoom: f64,
    viewport: ViewSize,
    /// Device pixels per logical pixels.
    scale: f64,
    /// Layout position of the viewport's top-left corner.
    scroll: LayoutVector,
    spreads: Vec<Vec<usize>>,
    page_spread: Vec<usize>,
    /// Current spread in paged modes.
    current: usize,
    layout: Layout,
    /// Device scale of the last frame. A change restarts the settle delay.
    last_device_scale: Option<TileScale>,
    /// A zoom gesture ended since the last frame (restarts the settle delay).
    zoom_dirty: bool,
    /// When the device scale last changed, as seen by `frame`.
    zoom_changed_at: Option<Instant>,
    /// A zoom gesture (pinch) is in progress.
    gesture: bool,
    /// Whether `frame` has run yet.
    ///
    /// The first frame is settled right away: nothing is zooming, so there's no reason to render
    /// levels first.
    framed: bool,
    /// Swipe mode: logical pixels per point of each spread. `zoom` is the current spread's.
    spread_zoom: Vec<f64>,
    /// Swipe mode with `ZoomMode::Fixed`: zoom relative to fitting the page (1 = the whole page is
    /// visible), shared by all spreads.
    factor: f64,
    /// Where the reader jumped away from, the oldest first: what Back returns to.
    back: Vec<Place>,
    /// Where Back was taken from, the oldest first: what Forward returns to.
    forward: Vec<Place>,
    /// Whether the last layout was a swipe strip.
    was_strip: bool,
    /// Swipe mode: the fit the reader had before zooming in, which a page turn returns to.
    strip_fit: ZoomMode,
}

/// The tile scales one page is drawn with.
struct Scales {
    /// Device pixels per point the page is shown at (quantized).
    device: TileScale,
    /// The √2 level at or above `device`.
    level: TileScale,
    /// What sharp tiles are: `device` once settled, `level` while zooming.
    target: TileScale,
    /// What to show where sharp tiles are missing, coarsest first.
    fallbacks: [TileScale; 4],
}

impl Scales {
    fn new(device_px_per_point: f64, settled: bool) -> Self {
        let device = TileScale::exact(device_px_per_point);
        let l = level_for_scale(device.get());
        let level = TileScale::from_level(l);
        // Exact tiles only when they differ from the level (they're the same key when the zoom
        // lands on a level, such as 100% at scale 2).
        let target = if settled { device } else { level };
        let fallbacks = if target == level {
            [l - 2, l - 1, l + 1, l + 2]
        } else {
            [l - 2, l - 1, l, l + 1]
        };
        Self {
            device,
            level,
            target,
            fallbacks: fallbacks.map(TileScale::from_level),
        }
    }
}

/// Adds `place` to `places`, forgetting the oldest beyond [`MAX_HISTORY`].
fn remember(places: &mut Vec<Place>, place: Place) {
    if places.len() == MAX_HISTORY {
        places.remove(0);
    }
    places.push(place);
}

impl Viewer {
    pub fn new(pages: Vec<ViewPage>, settings: Settings) -> Self {
        let mut v = Self {
            given: Vec::new(),
            pages: Vec::new(),
            sizes: Vec::new(),
            settings,
            mode: LayoutMode::default(),
            zoom_mode: ZoomMode::default(),
            zoom: 1.0,
            viewport: ViewSize::new(800.0, 600.0),
            scale: 1.0,
            scroll: LayoutVector::zero(),
            spreads: Vec::new(),
            page_spread: Vec::new(),
            current: 0,
            layout: Layout::default(),
            last_device_scale: None,
            zoom_dirty: false,
            zoom_changed_at: None,
            gesture: false,
            framed: false,
            spread_zoom: Vec::new(),
            factor: 1.0,
            back: Vec::new(),
            forward: Vec::new(),
            was_strip: false,
            strip_fit: ZoomMode::default(),
        };
        v.set_pages(pages);
        v
    }

    // Queries

    /// The pages as laid out, with the crop applied where the layout crops.
    pub fn pages(&self) -> &[ViewPage] {
        &self.pages
    }

    /// Whether the layout shows the part of each page its crop says, as opposed to whole pages.
    pub fn crops_pages(&self) -> bool {
        !self.mode.is_continuous()
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn layout_mode(&self) -> LayoutMode {
        self.mode
    }

    pub fn zoom_mode(&self) -> ZoomMode {
        self.zoom_mode
    }

    /// Logical pixels per PDF point.
    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn viewport(&self) -> ViewSize {
        self.viewport
    }

    pub fn scale_factor(&self) -> f64 {
        self.scale
    }

    /// Size of the laid-out content (for scrollbars).
    pub fn content_size(&self) -> specto_geom::LayoutSize {
        self.layout.size
    }

    /// Layout position of the viewport's top-left corner (for scrollbars).
    pub fn scroll_offset(&self) -> LayoutVector {
        self.scroll
    }

    /// Where page `i` is on screen, or `None` if it isn't laid out (paged modes without swiping).
    pub fn page_rect(&self, i: usize) -> Option<ViewRect> {
        let r = (*self.layout.page_rects.get(i)?)?;
        let o = self.offset_of(self.page_spread[i]);
        Some(ViewRect::new(
            ViewPoint::new(r.origin.x - o.x, r.origin.y - o.y),
            ViewSize::new(r.size.width, r.size.height),
        ))
    }

    /// The page the user is looking at: in continuous modes the one with the largest visible area
    /// (the last page once scrolled to the very end), in paged modes the first of the current
    /// spread, and while swiping the first of the spread under the middle of the viewport.
    pub fn current_page(&self) -> usize {
        if self.is_strip() {
            let center = self.scroll.x + self.viewport.width / 2.0;
            let spreads = &self.layout.spreads;
            let i = spreads
                .partition_point(|s| s.slot.max_x() < center)
                .min(spreads.len().saturating_sub(1));
            return spreads.get(i).map_or(0, |s| s.pages[0]);
        }
        if !self.mode.is_continuous() {
            return self.spreads.get(self.current).map_or(0, |s| s[0]);
        }
        let view = self.view_layout_rect();
        // At the end of the document the last page may never get the largest share of the viewport,
        // but it is where the user has arrived.
        let scrollable = self.layout.size.height - self.viewport.height;
        if scrollable > 0.0 && self.scroll.y >= scrollable - 0.5 {
            return self.pages.len().saturating_sub(1);
        }
        self.spreads_near(0.0)
            .flat_map(|(_, s)| s.pages.iter().copied())
            .filter_map(|i| {
                let r = self.layout.page_rects[i]?;
                let area = r.intersection(&view).map_or(0.0, |r| r.area());
                Some((i, area))
            })
            .fold(None, |best: Option<(usize, f64)>, (i, a)| match best {
                Some((_, ba)) if ba >= a => best,
                _ => Some((i, a)),
            })
            .map_or(0, |(i, _)| i)
    }

    /// The pages shown together with [`current_page`](Self::current_page): the pair it belongs to
    /// in the two-page modes, otherwise just itself.
    pub fn current_spread(&self) -> &[usize] {
        self.page_spread
            .get(self.current_page())
            .and_then(|&s| self.spreads.get(s))
            .map_or(&[], Vec::as_slice)
    }

    /// The page under `p` and the position on it, in page space (points).
    pub fn hit_test(&self, p: ViewPoint) -> Option<(usize, PagePoint)> {
        self.spreads_near(0.0).find_map(|(s, spread)| {
            let o = self.offset_of(s);
            let lp = LayoutPoint::new(p.x + o.x, p.y + o.y);
            let zoom = self.zoom_of(s);
            spread.pages.iter().find_map(|&i| {
                let r = self.layout.page_rects[i]?;
                r.contains(lp).then(|| {
                    (
                        i,
                        PagePoint::new((lp.x - r.origin.x) / zoom, (lp.y - r.origin.y) / zoom),
                    )
                })
            })
        })
    }

    /// Where a point on page `i` is on screen.
    pub(crate) fn page_to_view(&self, i: usize, p: PagePoint) -> Option<ViewPoint> {
        let r = self.page_rect(i)?;
        let zoom = self.zoom_of(self.page_spread[i]);
        Some(ViewPoint::new(
            r.origin.x + p.x * zoom,
            r.origin.y + p.y * zoom,
        ))
    }

    /// Where a point in page `i`'s PDF user space is on screen.
    pub fn pdf_to_view(&self, i: usize, p: PdfPoint) -> Option<ViewPoint> {
        let page = self
            .pages
            .get(i)?
            .displayed()
            .pdf_to_page()
            .transform_point(p);
        self.page_to_view(i, page)
    }

    /// The point in page `i`'s PDF user space that is at `p` on screen, whether `p` is on the page
    /// or beside it: the inverse of [`pdf_to_view`](Self::pdf_to_view).
    pub fn view_to_pdf(&self, i: usize, p: ViewPoint) -> Option<PdfPoint> {
        let r = self.page_rect(i)?;
        let zoom = self.zoom_of(self.page_spread[i]);
        let page = PagePoint::new((p.x - r.origin.x) / zoom, (p.y - r.origin.y) / zoom);
        Some(
            self.pages
                .get(i)?
                .displayed()
                .page_to_pdf()
                .transform_point(page),
        )
    }

    /// Scrolls so that `area` (PDF points of page `i`) is on screen, unless it already is: for
    /// search results.
    ///
    /// It ends up a third of the way down the view, and horizontally only moves if it has to.
    pub fn reveal(&mut self, i: usize, area: PdfBox) {
        if i >= self.pages.len() {
            return;
        }
        let corners = [
            area.min,
            PdfPoint::new(area.max.x, area.min.y),
            area.max,
            PdfPoint::new(area.min.x, area.max.y),
        ];
        let rect = |v: &Self| {
            let points: Option<Vec<ViewPoint>> =
                corners.iter().map(|&p| v.pdf_to_view(i, p)).collect();
            Some(specto_geom::euclid::Box2D::from_points(points?))
        };
        // Pages of other spreads aren't laid out in the paged modes, and while swiping only the
        // current spread scrolls.
        let shown = self.page_rect(i).is_some()
            && (!self.is_strip() || self.page_spread[i] == self.current);
        if !shown {
            self.go_to_page(i);
        }
        let Some(r) = rect(self) else {
            return;
        };
        let (w, h) = (self.viewport.width, self.viewport.height);
        const MARGIN: f64 = 16.0;
        let dx = if r.min.x >= MARGIN && r.max.x <= w - MARGIN {
            0.0
        } else if r.width() > w - 2.0 * MARGIN {
            r.min.x - MARGIN
        } else {
            r.center().x - w / 2.0
        };
        let dy = if r.min.y >= MARGIN && r.max.y <= h - MARGIN {
            0.0
        } else {
            r.min.y - h / 3.0
        };
        if dx != 0.0 || dy != 0.0 {
            let delta = ViewVector::new(dx, dy);
            if self.is_strip() {
                self.scroll_in_page_by(delta);
            } else {
                self.scroll_by(delta);
            }
        }
    }

    /// The visible page nearest to `p`, and `p` in that page's PDF user space, also when `p` is
    /// outside the page (a margin, the gap between pages, beyond the viewport). For dragging a text
    /// selection.
    pub fn nearest_pdf_point(&self, p: ViewPoint) -> Option<(usize, PdfPoint)> {
        let (i, page) = self
            .spreads_near(0.0)
            .flat_map(|(s, spread)| spread.pages.iter().map(move |&i| (s, i)))
            .filter_map(|(s, i)| {
                let r = self.layout.page_rects[i]?;
                let o = self.offset_of(s);
                let lp = LayoutPoint::new(p.x + o.x, p.y + o.y);
                let dx = (r.min_x() - lp.x).max(lp.x - r.max_x()).max(0.0);
                let dy = (r.min_y() - lp.y).max(lp.y - r.max_y()).max(0.0);
                let zoom = self.zoom_of(s);
                let page = PagePoint::new((lp.x - r.origin.x) / zoom, (lp.y - r.origin.y) / zoom);
                Some((dx.hypot(dy), i, page))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, i, page)| (i, page))?;
        let pdf = self.pages[i]
            .displayed()
            .page_to_pdf()
            .transform_point(page);
        Some((i, pdf))
    }

    // Changes

    /// Replaces the page list (after reorder, delete, merge, rotate), trying to keep the current
    /// page in view.
    pub fn set_pages(&mut self, pages: Vec<ViewPage>) {
        self.given = pages;
        let keep = self.current_page();
        self.lay_out_pages();
        self.regroup();
        if !self.pages.is_empty() {
            self.show_page(keep.min(self.pages.len() - 1));
        } else {
            self.relayout(None);
        }
    }

    /// Replaces the page list with the same pages, each with another part of itself to show, as
    /// when margins are trimmed. Returns whether what is shown changed.
    ///
    /// What is at the top of the viewport stays there, as far as it is still shown.
    pub fn set_page_areas(&mut self, pages: Vec<ViewPage>) -> bool {
        if pages == self.given {
            return false;
        }
        self.given = pages;
        let place = self.place();
        let at = self.pages.get(place.page).map(|p| {
            let g = p.displayed();
            let y = place.offset * g.size().height;
            g.page_to_pdf().transform_point(PagePoint::new(0.0, y))
        });
        let before = std::mem::take(&mut self.pages);
        self.lay_out_pages();
        if self.pages == before {
            return false;
        }
        self.regroup();
        if self.pages.is_empty() {
            self.relayout(None);
            return true;
        }
        self.show_page(place.page.min(self.pages.len() - 1));
        if let (Some(at), Some(p)) = (at, self.pages.get(place.page)) {
            let g = p.displayed();
            let y = g.pdf_to_page().transform_point(at).y;
            self.go_to_place(Place {
                page: place.page,
                offset: y / g.size().height,
            });
        }
        true
    }

    /// Replaces the page list with one of the same pages in the same sizes that are only rendered
    /// from elsewhere (another document, or another page of one): nothing moves.
    ///
    /// If a size did change, this is [`set_page_areas`](Self::set_page_areas).
    pub fn set_page_sources(&mut self, pages: Vec<ViewPage>) {
        let same = pages.len() == self.given.len()
            && pages.iter().zip(&self.given).all(|(p, q)| {
                p.geometry == q.geometry && p.crop == q.crop && p.rotation == q.rotation
            });
        if same {
            self.given = pages;
            self.lay_out_pages();
        } else {
            self.set_page_areas(pages);
        }
    }

    /// The pages as given, laid out as the layout shows them: cropped or whole.
    fn lay_out_pages(&mut self) {
        let crop = self.crops_pages();
        self.pages = (self.given.iter())
            .map(|p| match p.crop {
                Some(c) if crop => ViewPage {
                    geometry: p.geometry.cropped(c),
                    ..*p
                },
                _ => ViewPage { crop: None, ..*p },
            })
            .collect();
        self.sizes = self.pages.iter().map(|p| p.displayed().size()).collect();
    }

    /// Viewport size in logical pixels and the device scale factor (such as 2.0 on a Retina
    /// display, 1.25 on a fractionally scaled one).
    pub fn set_viewport(&mut self, size: ViewSize, scale_factor: f64) {
        let anchor = self.anchor_at(ViewPoint::new(self.viewport.width / 2.0, 0.0));
        self.viewport = size;
        self.scale = scale_factor.max(0.1);
        self.relayout(anchor.map(|a| Anchor {
            view: ViewPoint::new(size.width / 2.0, 0.0),
            ..a
        }));
        self.confine_to_current();
    }

    /// Turns swipe paging on or off (see [`Settings::swipe`]), keeping the page.
    pub fn set_swipe(&mut self, swipe: bool) {
        if swipe == self.settings.swipe {
            return;
        }
        let page = self.current_page();
        self.settings.swipe = swipe;
        self.show_page(page);
    }

    pub fn set_layout_mode(&mut self, mode: LayoutMode) {
        if mode == self.mode {
            return;
        }
        let page = self.current_page();
        self.mode = mode;
        self.lay_out_pages();
        self.regroup();
        self.show_page(page);
    }

    pub fn set_cover_alone(&mut self, cover_alone: bool) {
        if cover_alone == self.settings.cover_alone {
            return;
        }
        let page = self.current_page();
        self.settings.cover_alone = cover_alone;
        self.regroup();
        self.show_page(page);
    }

    pub fn set_zoom_mode(&mut self, mode: ZoomMode) {
        self.zoom_mode = mode;
        if mode != ZoomMode::Fixed {
            self.strip_fit = mode;
        }
        let anchor = self.anchor_at(ViewPoint::new(self.viewport.width / 2.0, 0.0));
        self.relayout(anchor);
        self.confine_to_current();
    }

    /// Sets a fixed zoom, keeping the content under `anchor` in place.
    ///
    /// While swiping, only the current page is zoomed: its neighbors stay fitted, turning the page
    /// returns to the fitted view, and a page can't be made smaller than fitting the viewport.
    pub fn set_zoom(&mut self, zoom: f64, anchor: ViewPoint) {
        let a = self.anchor_at(anchor);
        self.zoom_mode = ZoomMode::Fixed;
        if self.is_strip() {
            let fit = self
                .spread_fit(self.current, ZoomMode::FitPage)
                .unwrap_or(1.0);
            self.factor = (zoom / fit).clamp(1.0, (self.settings.max_zoom / fit).max(1.0));
        } else {
            self.zoom = zoom.clamp(self.settings.min_zoom, self.settings.max_zoom);
        }
        self.relayout(a);
    }

    /// Multiplies the zoom (pinch, ctrl+scroll), keeping `anchor` in place.
    pub fn zoom_by(&mut self, factor: f64, anchor: ViewPoint) {
        self.set_zoom(self.zoom * factor, anchor);
    }

    /// The current position and view settings, to restore later.
    pub fn view_state(&self) -> ViewState {
        let Place { page, offset } = self.place();
        ViewState {
            page,
            page_offset: offset,
            layout: self.mode,
            cover_alone: self.settings.cover_alone,
            zoom_mode: self.zoom_mode,
            zoom: self.zoom,
        }
    }

    /// Where the viewport starts.
    fn place(&self) -> Place {
        let top = self.scroll.y.max(0.0);
        let page = if self.is_strip() {
            // Vertical scrolling belongs to the current spread.
            self.spreads.get(self.current).map_or(0, |s| s[0])
        } else {
            // The first laid-out page whose bottom is below the viewport top.
            self.spreads_near(0.0)
                .flat_map(|(_, s)| s.pages.iter().copied())
                .find(|&i| self.layout.page_rects[i].is_some_and(|r| r.max_y() > top))
                .unwrap_or_else(|| self.current_page())
        };
        let offset = self
            .layout
            .page_rects
            .get(page)
            .copied()
            .flatten()
            .map_or(0.0, |r| {
                // Slightly negative when the viewport starts in the margin or gap above the page
                // (at the very top of the document, for example).
                ((top - r.origin.y) / r.size.height).clamp(-0.5, 1.0)
            });
        Place { page, offset }
    }

    /// Scrolls so that the viewport starts `offset` of the way down `page`, which is on screen.
    fn scroll_within(&mut self, page: usize, offset: f64) {
        if let Some(Some(r)) = self.layout.page_rects.get(page) {
            let offset = if offset.is_finite() {
                offset.clamp(-0.5, 1.0)
            } else {
                0.0
            };
            self.scroll.y = r.origin.y + offset * r.size.height;
            self.clamp_scroll();
        }
    }

    // Back and forward

    /// Does `go`, which takes the reader somewhere else (a link, an outline entry, a page number),
    /// and remembers where they were for [`go_back`](Self::go_back). Returns whether the view
    /// moved.
    ///
    /// A `go` that leaves the view where it was isn't remembered. One that moves it makes
    /// [`go_forward`](Self::go_forward) forget what it had.
    pub fn jump(&mut self, go: impl FnOnce(&mut Self)) -> bool {
        let from = self.place();
        go(self);
        let to = self.place();
        if from.page == to.page && (from.offset - to.offset).abs() < SAME_PLACE {
            return false;
        }
        remember(&mut self.back, from);
        self.forward.clear();
        true
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Returns to where the reader was before the last jump. Returns whether there was one.
    pub fn go_back(&mut self) -> bool {
        let Some(place) = self.back.pop() else {
            return false;
        };
        let here = self.place();
        remember(&mut self.forward, here);
        self.go_to_place(place);
        true
    }

    /// Undoes the last [`go_back`](Self::go_back). Returns whether there was one to undo.
    pub fn go_forward(&mut self) -> bool {
        let Some(place) = self.forward.pop() else {
            return false;
        };
        let here = self.place();
        remember(&mut self.back, here);
        self.go_to_place(place);
        true
    }

    /// Shows `place`, or the last page if the document has fewer pages by now: places are kept by
    /// page number, whatever is done to the pages meanwhile.
    fn go_to_place(&mut self, place: Place) {
        if self.pages.is_empty() {
            return;
        }
        let page = place.page.min(self.pages.len() - 1);
        self.go_to_page(page);
        self.scroll_within(page, place.offset);
    }

    /// Restores a [`ViewState`].
    ///
    /// Out-of-range values (the file may have changed and have fewer pages now) are clamped.
    pub fn restore_view_state(&mut self, state: &ViewState) {
        if self.pages.is_empty() {
            return;
        }
        self.mode = state.layout;
        self.settings.cover_alone = state.cover_alone;
        self.lay_out_pages();
        self.regroup();
        self.zoom_mode = state.zoom_mode;
        if state.zoom.is_finite() {
            self.zoom = state.zoom;
            // Swipe mode zooms relative to the fitted page. Have the layout derive that from `zoom`
            // for the restored page, as it does when the mode is entered.
            self.was_strip = false;
        }
        let page = state.page.min(self.pages.len() - 1);
        self.show_page(page);
        self.scroll_within(page, state.page_offset);
    }

    /// A zoom gesture (pinch) started: until [`end_zoom_gesture`](Self::end_zoom_gesture) the view
    /// never counts as settled, so pausing mid-pinch doesn't render (and cache) exact-scale tiles
    /// for an intermediate zoom.
    pub fn begin_zoom_gesture(&mut self) {
        self.gesture = true;
    }

    /// The zoom gesture ended, and the view settles after `settle_delay`.
    pub fn end_zoom_gesture(&mut self) {
        self.gesture = false;
        self.zoom_dirty = true;
    }

    /// Whether a frame planned at `now` would use exact-scale tiles.
    pub fn is_settled(&self, now: Instant) -> bool {
        !self.gesture
            && !self.zoom_dirty
            && self
                .last_device_scale
                .is_none_or(|s| s == self.settle_signature())
            && self
                .zoom_changed_at
                .is_none_or(|t| now.saturating_duration_since(t) >= self.settings.settle_delay)
    }

    /// Device pixels per point actually used for drawing: the zoom times the device scale,
    /// quantized like tile scales so exact tiles map one to one.
    pub fn device_scale(&self) -> TileScale {
        TileScale::exact(self.zoom * self.scale)
    }

    /// Sets the layout position of the viewport's top-left corner (scrollbars).
    pub fn set_scroll_offset(&mut self, offset: LayoutVector) {
        self.scroll = offset;
        self.clamp_scroll();
        self.adopt_slot();
    }

    /// The low-resolution whole-page tile for page `i`, shown while sharp tiles load. Also suitable
    /// for thumbnails of a page that isn't cropped, so both share one render.
    pub fn placeholder_key(&self, i: usize) -> Option<TileKey> {
        self.placeholder_of(self.pages.get(i)?)
    }

    /// Page `i` whole, as thumbnails show it, whatever part of it the layout shows.
    pub fn page_whole(&self, i: usize) -> Option<ViewPage> {
        self.given.get(i).map(|p| ViewPage { crop: None, ..*p })
    }

    /// The low-resolution tile of page `i` whole, for its thumbnail. The same as its placeholder
    /// unless the page is cropped.
    pub fn thumbnail_key(&self, i: usize) -> Option<TileKey> {
        self.placeholder_of(&self.page_whole(i)?)
    }

    fn placeholder_of(&self, page: &ViewPage) -> Option<TileKey> {
        Some(TileKey {
            doc: page.doc,
            page: page.page,
            rotation: page.rotation,
            crop: page.crop,
            content: self.settings.content,
            scale: TileScale::from_level(placeholder_level(&page.displayed())?),
            tx: 0,
            ty: 0,
        })
    }

    /// Scrolls by `delta`.
    ///
    /// While swiping, this can slide to the neighboring spreads. Call
    /// [`snap_delta`](Self::snap_delta) when the drag ends.
    pub fn scroll_by(&mut self, delta: ViewVector) {
        self.scroll += LayoutVector::new(delta.x, delta.y);
        self.clamp_scroll();
        self.adopt_slot();
    }

    /// Like [`scroll_by`](Self::scroll_by), but while swiping it stops at the edges of the current
    /// spread: for flings, which shouldn't turn pages.
    pub fn scroll_in_page_by(&mut self, delta: ViewVector) {
        self.scroll += LayoutVector::new(delta.x, delta.y);
        self.clamp_scroll();
        self.confine_to_current();
    }

    /// While swiping: how far the view still has to scroll to come to rest after a drag or pinch
    /// that left it between two spreads, or `None` if it rests where it is.
    ///
    /// `velocity` is that of the scrolling at release, in logical pixels per second with the sign
    /// of [`scroll_by`](Self::scroll_by) deltas: a flick turns the page even after a short drag,
    /// and otherwise the spread covering more than half of the viewport wins.
    ///
    /// The frontend animates [`scroll_by`](Self::scroll_by) over this distance. At most one page is
    /// turned per drag.
    pub fn snap_delta(&self, velocity: ViewVector) -> Option<ViewVector> {
        if !self.is_strip() {
            return None;
        }
        let spreads = &self.layout.spreads;
        let (x0, x1) = self.rest_range(spreads.get(self.current)?);
        let x = self.scroll.x;
        let half = self.viewport.width / 2.0;
        let target = if x > x1 + 0.01 {
            match spreads.get(self.current + 1) {
                Some(next)
                    if velocity.x > FLICK_VELOCITY
                        || (x - x1 > half && velocity.x > -FLICK_VELOCITY) =>
                {
                    self.rest_range(next).0
                }
                _ => x1,
            }
        } else if x < x0 - 0.01 {
            match self.current.checked_sub(1).and_then(|p| spreads.get(p)) {
                Some(previous)
                    if velocity.x < -FLICK_VELOCITY
                        || (x0 - x > half && velocity.x < FLICK_VELOCITY) =>
                {
                    self.rest_range(previous).1
                }
                _ => x0,
            }
        } else {
            return None;
        };
        Some(ViewVector::new(target - x, 0.0))
    }

    /// Shows page `i` at the top of the viewport.
    ///
    /// While swiping, going to another page (or pair) leaves a zoomed-in view for the fitted one.
    pub fn go_to_page(&mut self, i: usize) {
        if self.is_strip()
            && self.zoom_mode == ZoomMode::Fixed
            && self.page_spread.get(i).is_some_and(|&s| s != self.current)
        {
            self.zoom_mode = self.strip_fit;
            // Nothing is zooming on the page that comes into view.
            self.keep_settled();
        }
        self.show_page(i);
    }

    /// [`go_to_page`](Self::go_to_page) without touching the zoom: for changes of the layout around
    /// a page the reader stays on.
    fn show_page(&mut self, i: usize) {
        let Some(&spread) = self.page_spread.get(i) else {
            return;
        };
        self.current = spread;
        self.relayout(None);
        if self.is_strip() {
            if let Some(s) = self.layout.spreads.get(spread) {
                self.scroll = LayoutVector::new(s.slot.origin.x, 0.0);
            }
            self.clamp_scroll();
            return;
        }
        if let Some(r) = self.layout.page_rects[i] {
            let spread_top = self.layout.spreads[spread - self.layout.first_spread]
                .rect
                .origin
                .y;
            self.scroll.y = spread_top.min(r.origin.y) - self.settings.margin;
        }
        self.clamp_scroll();
    }

    /// Goes to a position given in PDF user space of page `i`, as link and outline destinations
    /// specify it: `top` ends up at the top of the viewport (below the margin), and `left` at the
    /// left edge if the page is wider than the viewport.
    ///
    /// Missing coordinates keep the page top or the current horizontal scroll.
    pub fn go_to_pdf_position(&mut self, i: usize, left: Option<f64>, top: Option<f64>) {
        self.go_to_page(i);
        if left.is_none() && top.is_none() {
            return;
        }
        let (Some(page), Some(Some(r))) = (self.pages.get(i), self.layout.page_rects.get(i)) else {
            return;
        };
        let g = page.displayed();
        let visible = g.visible_box();
        let size = g.size();
        let p = g.pdf_to_page().transform_point(specto_geom::PdfPoint::new(
            left.unwrap_or(visible.min.x),
            top.unwrap_or(visible.max.y),
        ));
        // Destinations outside the page (it happens) are clamped to it.
        let (x, y) = (p.x.clamp(0.0, size.width), p.y.clamp(0.0, size.height));
        let margin = self.settings.margin;
        self.scroll.y = r.origin.y + y * self.zoom - margin;
        if left.is_some() {
            self.scroll.x = r.origin.x + x * self.zoom - margin;
        }
        self.clamp_scroll();
        // While swiping, the target page stays the one on screen.
        self.confine_to_current();
    }

    pub fn next_page(&mut self) {
        let spread = self
            .page_spread
            .get(self.current_page())
            .copied()
            .unwrap_or(0);
        if let Some(s) = self.spreads.get(spread + 1) {
            self.go_to_page(s[0]);
        }
    }

    pub fn previous_page(&mut self) {
        let spread = self
            .page_spread
            .get(self.current_page())
            .copied()
            .unwrap_or(0);
        if spread > 0 {
            self.go_to_page(self.spreads[spread - 1][0]);
        }
    }

    // Drawing

    /// Plans a frame at time `now`. `cached` tells whether the frontend has a tile.
    ///
    /// While zooming, pages are drawn from √2-level tiles (rendered at or above the device scale
    /// and downsampled), so continuous zooming reuses a few cached levels. Once the view is settled
    /// (no zoom change for `settle_delay` and no gesture in progress), it requests tiles at the
    /// exact device scale and draws them one to one, with level tiles as fallback.
    pub fn frame(&mut self, now: Instant, cached: impl Fn(&TileKey) -> bool) -> Frame {
        let signature = self.settle_signature();
        let rescaled = self.last_device_scale.replace(signature) != Some(signature);
        if (std::mem::take(&mut self.zoom_dirty) || rescaled) && self.framed {
            self.zoom_changed_at = Some(now);
        }
        self.framed = true;
        let settled = self.is_settled(now);

        // What to prepare beyond the viewport: while swiping, the spreads to the left and right, so
        // that a swipe shows them at once. Otherwise a band above and below.
        let (band_x, band_y) = if self.is_strip() {
            (self.viewport.width, 0.0)
        } else {
            (0.0, self.settings.prefetch * self.viewport.height)
        };
        let viewport_dev = DeviceRect::new(
            DevicePoint::zero(),
            (
                self.viewport.width * self.scale,
                self.viewport.height * self.scale,
            )
                .into(),
        );
        let band_dev = viewport_dev.inflate(band_x * self.scale, band_y * self.scale);

        let mut frame = Frame {
            complete: true,
            ..Frame::default()
        };
        // Whether some visible page would look different once settled.
        let mut sharpens = false;
        let mut missing_sharp = Vec::new();
        let mut prefetch = Vec::new();
        let mut visible_pages = Vec::new();

        for (s, spread) in self.spreads_near(band_x.max(band_y)) {
            let offset = self.offset_of(s);
            // Every page of a spread has the spread's zoom. Spreads differ while swiping, where
            // each is fitted to the viewport on its own.
            let scales = Scales::new(self.zoom_of(s) * self.scale, settled);
            let target = scales.target;
            for &i in &spread.pages {
                let Some(r) = self.layout.page_rects[i] else {
                    continue;
                };
                let page = &self.pages[i];
                let displayed = page.displayed();
                let origin = DevicePoint::new(
                    ((r.origin.x - offset.x) * self.scale).round(),
                    ((r.origin.y - offset.y) * self.scale).round(),
                );
                // Sized from the quantized device scale, exactly like an exact-scale page bitmap.
                let px = displayed.pixel_size(scales.device.get());
                let rect = DeviceRect::new(origin, (px.width as f64, px.height as f64).into());
                let placeholder = placeholder_level(&displayed)
                    .map(TileScale::from_level)
                    .filter(|&p| p < target);
                let key = |scale: TileScale, (tx, ty): (u32, u32)| TileKey {
                    doc: page.doc,
                    page: page.page,
                    rotation: page.rotation,
                    crop: page.crop,
                    content: self.settings.content,
                    scale,
                    tx,
                    ty,
                };

                let Some(on_screen) = rect.intersection(&viewport_dev) else {
                    // In the prefetch band only.
                    if let Some(area) = rect.intersection(&band_dev) {
                        if let Some(p) = placeholder
                            && !cached(&key(p, (0, 0)))
                        {
                            prefetch.insert(0, key(p, (0, 0)));
                        }
                        let a = scaled_area(rect, area, &displayed, target);
                        for t in tiles_covering(&displayed, target, a) {
                            if !cached(&key(target, t)) {
                                prefetch.push(key(target, t));
                            }
                        }
                    }
                    continue;
                };
                visible_pages.push(i);
                sharpens |= scales.device != scales.level;

                let mut layers = Vec::new();
                // The placeholder is drawn only under gaps (below), but kept requested: it doubles
                // as the thumbnail and makes scrolling back instant.
                let mut placeholder_layer = None;
                if let Some(p) = placeholder {
                    let k = key(p, (0, 0));
                    if cached(&k) {
                        placeholder_layer = tile_draw(k, page, rect);
                    } else {
                        frame.requests.push(k);
                    }
                }

                let area = scaled_area(rect, on_screen, &displayed, target);
                let mut sharp = Vec::new();
                let mut missing = false;
                for t in tiles_covering(&displayed, target, area) {
                    let k = key(target, t);
                    if cached(&k) {
                        sharp.push(k);
                    } else {
                        missing = true;
                        missing_sharp.push(k);
                    }
                }
                if missing {
                    frame.complete = false;
                    layers.extend(placeholder_layer);
                    // Fill gaps with whatever neighboring levels are cached, coarser first so finer
                    // ones end up on top.
                    for &f in &scales.fallbacks {
                        if placeholder.is_some_and(|p| f <= p) {
                            continue;
                        }
                        let a = scaled_area(rect, on_screen, &displayed, f);
                        for t in tiles_covering(&displayed, f, a) {
                            let k = key(f, t);
                            if cached(&k) {
                                layers.extend(tile_draw(k, page, rect));
                            }
                        }
                    }
                }
                layers.extend(sharp.into_iter().filter_map(|k| tile_draw(k, page, rect)));
                frame.pages.push(PageDraw {
                    page: i,
                    rect,
                    layers,
                    complete: !missing,
                });
            }
        }
        frame.settles_at = match self.zoom_changed_at {
            Some(t) if !settled && !self.gesture && sharpens => {
                Some(t + self.settings.settle_delay)
            }
            _ => None,
        };

        // Placeholders for visible pages first (already queued), then sharp visible tiles, then
        // neighboring placeholders and the prefetch band.
        frame.requests.extend(missing_sharp);
        if let (Some(&first), Some(&last)) =
            (visible_pages.iter().min(), visible_pages.iter().max())
        {
            for i in first.saturating_sub(2)..(last + 3).min(self.pages.len()) {
                let target =
                    Scales::new(self.zoom_of(self.page_spread[i]) * self.scale, settled).target;
                if let Some(k) = self.placeholder_key(i).filter(|k| k.scale < target)
                    && !cached(&k)
                    && !frame.requests.contains(&k)
                {
                    frame.requests.push(k);
                }
            }
        }
        frame.requests.extend(prefetch);
        // Drop duplicates (a placeholder can be both a neighbor and in the prefetch band), keeping
        // the first, highest-priority occurrence.
        let mut seen = std::collections::HashSet::new();
        frame.requests.retain(|k| seen.insert(*k));
        frame
    }

    // Internals

    fn regroup(&mut self) {
        self.spreads = group_spreads(
            self.pages.len(),
            self.mode.is_two_page(),
            self.settings.cover_alone,
        );
        self.page_spread = vec![0; self.pages.len()];
        for (s, pages) in self.spreads.iter().enumerate() {
            for &p in pages {
                self.page_spread[p] = s;
            }
        }
        self.current = self.current.min(self.spreads.len().saturating_sub(1));
    }

    /// The zoom relative to what `fit` (`FitWidth` or `FitPage`) would give here: 1.0 when the view
    /// is fitted that way, 2.0 when zoomed in to twice that. For "zoom in / back to fit" toggles
    /// such as double tap.
    pub fn zoom_relative_to(&self, fit: ZoomMode) -> f64 {
        let fitted = if self.is_strip() {
            self.spread_fit(self.current, fit)
        } else {
            self.fit_zoom_for(fit)
        };
        fitted.map_or(1.0, |f| self.zoom / f)
    }

    fn fit_zoom(&self) -> Option<f64> {
        self.fit_zoom_for(self.zoom_mode)
    }

    fn fit_zoom_for(&self, mode: ZoomMode) -> Option<f64> {
        let spreads: &[Vec<usize>] = if self.mode.is_continuous() {
            &self.spreads
        } else {
            std::slice::from_ref(self.spreads.get(self.current)?)
        };
        let m = self.settings.margin;
        let mut zoom = f64::INFINITY;
        for s in spreads {
            let (w, h) = spread_points(s, &self.sizes);
            let gaps = self.settings.gap * (s.len().saturating_sub(1)) as f64;
            zoom = zoom.min((self.viewport.width - 2.0 * m - gaps) / w);
            if mode == ZoomMode::FitPage {
                zoom = zoom.min((self.viewport.height - 2.0 * m) / h);
            }
        }
        zoom.is_finite().then_some(zoom)
    }

    /// Whether the spreads are laid out side by side for swiping.
    fn is_strip(&self) -> bool {
        self.settings.swipe && !self.mode.is_continuous()
    }

    /// Logical pixels per point of spread `s` (an index into `self.spreads`).
    fn zoom_of(&self, s: usize) -> f64 {
        if self.is_strip() {
            self.spread_zoom.get(s).copied().unwrap_or(self.zoom)
        } else {
            self.zoom
        }
    }

    /// What to subtract from layout coordinates of spread `s` to get view coordinates.
    ///
    /// While swiping, vertical scrolling belongs to the current spread. Its neighbors are shown
    /// from their top, wherever the current one has been panned to.
    fn offset_of(&self, s: usize) -> LayoutVector {
        if self.is_strip() && s != self.current {
            LayoutVector::new(self.scroll.x, 0.0)
        } else {
            self.scroll
        }
    }

    /// What a change of restarts the settle delay: the device scale, or while swiping the zoom
    /// relative to fitting the page (turning to a page of another size changes the scale, but
    /// nothing is zooming).
    fn settle_signature(&self) -> TileScale {
        if self.is_strip() {
            let factor = match self.zoom_mode {
                ZoomMode::Fixed => self.factor,
                _ => 1.0,
            };
            TileScale::exact(factor * self.scale)
        } else {
            self.device_scale()
        }
    }

    /// The zoom at which spread `s` fits the viewport: its width, or for `FitPage` all of it.
    fn spread_fit(&self, s: usize, mode: ZoomMode) -> Option<f64> {
        let pages = self.spreads.get(s)?;
        let m = self.settings.margin;
        let (w, h) = spread_points(pages, &self.sizes);
        let gaps = self.settings.gap * (pages.len().saturating_sub(1)) as f64;
        let mut zoom = (self.viewport.width - 2.0 * m - gaps) / w;
        if mode == ZoomMode::FitPage {
            zoom = zoom.min((self.viewport.height - 2.0 * m) / h);
        }
        (zoom.is_finite() && zoom > 0.0).then_some(zoom)
    }

    /// The scroll positions at which the viewport lies within `spread`'s slot.
    fn rest_range(&self, spread: &LaidSpread) -> (f64, f64) {
        let x0 = spread.slot.origin.x;
        (x0, (spread.slot.max_x() - self.viewport.width).max(x0))
    }

    /// While swiping: keeps the viewport within the current spread's slot.
    fn confine_to_current(&mut self) {
        if self.is_strip()
            && let Some(s) = self.layout.spreads.get(self.current)
        {
            let (x0, x1) = self.rest_range(s);
            self.scroll.x = self.scroll.x.clamp(x0, x1);
        }
    }

    /// While swiping: once the viewport lies entirely within another spread's slot, that spread
    /// becomes the current one.
    ///
    /// It was shown from its top while sliding in, so that is where its scrolling starts.
    fn adopt_slot(&mut self) {
        if !self.is_strip() {
            return;
        }
        let (x0, x1) = (self.scroll.x, self.scroll.x + self.viewport.width);
        let spreads = &self.layout.spreads;
        let j = spreads.partition_point(|s| s.slot.max_x() < x1 - 0.5);
        if j != self.current
            && let Some(s) = spreads.get(j)
            && s.slot.min_x() <= x0 + 0.5
        {
            self.current = j;
            self.scroll.y = 0.0;
            if self.zoom_mode == ZoomMode::Fixed {
                // The page left behind returns to its fitted size, which moves everything after it:
                // realign with the new page, which was fitted all along and so doesn't change.
                self.zoom_mode = self.strip_fit;
                self.keep_settled();
                self.relayout(None);
                if let Some(s) = self.layout.spreads.get(j) {
                    self.scroll.x = s.slot.origin.x;
                }
            }
            self.zoom = self.zoom_of(j);
            self.clamp_scroll();
        }
    }

    /// After a change of zoom that the reader doesn't see as zooming (a fitted page taking the
    /// place of a zoomed one): don't restart the settle delay, so the page isn't drawn from level
    /// tiles for a moment.
    fn keep_settled(&mut self) {
        if self.last_device_scale.is_some() {
            self.last_device_scale = Some(self.settle_signature());
        }
    }

    fn relayout_strip(&mut self, anchor: Option<Anchor>) {
        let (min, max) = (self.settings.min_zoom, self.settings.max_zoom);
        let fit_page: Vec<f64> = (0..self.spreads.len())
            .map(|s| self.spread_fit(s, ZoomMode::FitPage).unwrap_or(1.0))
            .collect();
        if !self.was_strip
            && self.zoom_mode == ZoomMode::Fixed
            && let Some(fit) = fit_page.get(self.current)
        {
            // Coming from a layout with one zoom for everything: keep it for the current page.
            self.factor = (self.zoom / fit).max(1.0);
        }
        self.was_strip = true;
        self.spread_zoom = fit_page
            .iter()
            .enumerate()
            .map(|(s, &fit)| {
                let fitted = |mode| match mode {
                    ZoomMode::FitWidth => self.spread_fit(s, ZoomMode::FitWidth).unwrap_or(fit),
                    _ => fit,
                };
                let zoom = match self.zoom_mode {
                    // Only the current spread is zoomed, and the others wait fitted, as they will
                    // be when turned to.
                    ZoomMode::Fixed if s == self.current => fit * self.factor,
                    ZoomMode::Fixed => fitted(self.strip_fit),
                    mode => fitted(mode),
                };
                zoom.clamp(min, max)
            })
            .collect();
        if let Some(&zoom) = self.spread_zoom.get(self.current) {
            self.zoom = zoom;
        }
        self.layout = layout_strip(&StripParams {
            sizes: &self.sizes,
            spreads: &self.spreads,
            zooms: &self.spread_zoom,
            gap: self.settings.gap,
            margin: self.settings.margin,
            viewport: (self.viewport.width, self.viewport.height),
        });
        if let Some(a) = anchor
            && let Some(s) = self.layout.spreads.get(a.spread)
        {
            let zoom = self.spread_zoom[a.spread];
            self.scroll.x = s.rect.origin.x + a.rel.0 * zoom + a.outside.0 - a.view.x;
            if a.spread == self.current {
                self.scroll.y = s.rect.origin.y + a.rel.1 * zoom + a.outside.1 - a.view.y;
            }
        }
        self.clamp_scroll();
    }

    fn relayout(&mut self, anchor: Option<Anchor>) {
        if self.is_strip() {
            self.relayout_strip(anchor);
            return;
        }
        self.was_strip = false;
        if self.zoom_mode != ZoomMode::Fixed
            && let Some(z) = self.fit_zoom()
        {
            self.zoom = z;
        }
        self.zoom = self
            .zoom
            .clamp(self.settings.min_zoom, self.settings.max_zoom);
        let range = if self.mode.is_continuous() {
            0..self.spreads.len()
        } else if self.spreads.is_empty() {
            0..0
        } else {
            self.current..self.current + 1
        };
        self.layout = layout(&LayoutParams {
            sizes: &self.sizes,
            spreads: &self.spreads,
            range,
            zoom: self.zoom,
            gap: self.settings.gap,
            margin: self.settings.margin,
            viewport_width: self.viewport.width,
        });
        if let Some(a) = anchor
            && let Some(s) = a
                .spread
                .checked_sub(self.layout.first_spread)
                .and_then(|i| self.layout.spreads.get(i))
        {
            let lp = LayoutPoint::new(
                s.rect.origin.x + a.rel.0 * self.zoom + a.outside.0,
                s.rect.origin.y + a.rel.1 * self.zoom + a.outside.1,
            );
            self.scroll = LayoutVector::new(lp.x - a.view.x, lp.y - a.view.y);
        }
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let size = self.layout.size;
        let (vw, vh) = (self.viewport.width, self.viewport.height);
        if self.is_strip() {
            self.scroll.x = self.scroll.x.clamp(0.0, (size.width - vw).max(0.0));
            // Vertical scrolling stays within the current spread's slot.
            let slot = self.layout.spreads.get(self.current);
            let height = slot.map_or(vh, |s| s.slot.size.height);
            self.scroll.y = self.scroll.y.clamp(0.0, (height - vh).max(0.0));
            return;
        }
        self.scroll.x = if size.width <= vw {
            0.0
        } else {
            self.scroll.x.clamp(0.0, size.width - vw)
        };
        // Content shorter than the viewport is centered vertically.
        self.scroll.y = if size.height <= vh {
            -(vh - size.height) / 2.0
        } else {
            self.scroll.y.clamp(0.0, size.height - vh)
        };
    }

    fn to_layout(&self, p: ViewPoint) -> LayoutPoint {
        LayoutPoint::new(p.x + self.scroll.x, p.y + self.scroll.y)
    }

    fn view_layout_rect(&self) -> LayoutRect {
        LayoutRect::new(
            LayoutPoint::new(self.scroll.x, self.scroll.y),
            (self.viewport.width, self.viewport.height).into(),
        )
    }

    /// Laid-out spreads within `band` logical pixels of the viewport along the direction they are
    /// arranged in, with their index into `self.spreads`.
    fn spreads_near(&self, band: f64) -> impl Iterator<Item = (usize, &LaidSpread)> {
        let spreads = &self.layout.spreads;
        let (first, end) = if self.is_strip() {
            let (x0, x1) = (
                self.scroll.x - band,
                self.scroll.x + self.viewport.width + band,
            );
            (
                spreads.partition_point(|s| s.slot.max_x() < x0),
                spreads.partition_point(|s| s.slot.origin.x <= x1),
            )
        } else {
            let (y0, y1) = (
                self.scroll.y - band,
                self.scroll.y + self.viewport.height + band,
            );
            (
                spreads.partition_point(|s| s.rect.max_y() < y0),
                spreads.partition_point(|s| s.rect.origin.y <= y1),
            )
        };
        let offset = self.layout.first_spread + first;
        spreads[first..end.max(first)]
            .iter()
            .enumerate()
            .map(move |(k, s)| (offset + k, s))
    }

    fn anchor_at(&self, view: ViewPoint) -> Option<Anchor> {
        let spreads = &self.layout.spreads;
        if spreads.is_empty() || self.zoom <= 0.0 {
            return None;
        }
        let (i, lp) = if self.is_strip() {
            // The spread whose slot contains the point, or the next one (gaps).
            let x = view.x + self.scroll.x;
            let i = spreads
                .partition_point(|s| s.slot.max_x() < x)
                .min(spreads.len() - 1);
            (i, LayoutPoint::new(x, view.y + self.offset_of(i).y))
        } else {
            // The spread containing the point, or the next one below it (gaps).
            let lp = self.to_layout(view);
            let i = spreads
                .partition_point(|s| s.rect.max_y() < lp.y)
                .min(spreads.len() - 1);
            (i, lp)
        };
        let spread = self.layout.first_spread + i;
        let zoom = self.zoom_of(spread);
        let r = spreads[i].rect;
        let (dx, dy) = (lp.x - r.origin.x, lp.y - r.origin.y);
        let (cx, cy) = (dx.clamp(0.0, r.size.width), dy.clamp(0.0, r.size.height));
        Some(Anchor {
            spread,
            rel: (cx / zoom, cy / zoom),
            outside: (dx - cx, dy - cy),
            view,
        })
    }
}

/// The largest level at which the whole page fits in one tile, used for the blurry placeholder
/// shown before sharp tiles arrive. Capped so tiny pages don't get large placeholders.
pub fn placeholder_level(g: &PageGeometry) -> Option<i32> {
    let s = g.size();
    let max = s.width.max(s.height);
    if max <= 0.0 {
        return None;
    }
    let mut l = ((TILE_SIZE as f64 / max).log2() * 2.0).floor() as i32;
    while l > -40 {
        let px = g.pixel_size(level_scale(l));
        if px.width <= TILE_SIZE && px.height <= TILE_SIZE {
            return Some(l.min(-2));
        }
        l -= 1;
    }
    None
}

/// Converts part of a page's device rectangle into pixels of the page bitmap at `scale`, rounded
/// outwards.
fn scaled_area(
    page: DeviceRect,
    part: DeviceRect,
    g: &PageGeometry,
    scale: TileScale,
) -> DeviceIntRect {
    let full = g.pixel_size(scale.get());
    let fx = full.width as f64 / page.size.width;
    let fy = full.height as f64 / page.size.height;
    let x0 = ((part.min_x() - page.min_x()) * fx).floor().max(0.0);
    let y0 = ((part.min_y() - page.min_y()) * fy).floor().max(0.0);
    let x1 = ((part.max_x() - page.min_x()) * fx).ceil().max(0.0);
    let y1 = ((part.max_y() - page.min_y()) * fy).ceil().max(0.0);
    DeviceIntRect::new((x0 as u32, y0 as u32).into(), (x1 as u32, y1 as u32).into())
}

/// Where a tile lands on screen, given the page's device rectangle.
fn tile_draw(key: TileKey, page: &ViewPage, rect: DeviceRect) -> Option<TileDraw> {
    let region = key.region(&page.geometry)?;
    let full = page.displayed().pixel_size(key.scale.get());
    let fx = rect.size.width / full.width as f64;
    let fy = rect.size.height / full.height as f64;
    Some(TileDraw {
        key,
        dest: DeviceRect::new(
            DevicePoint::new(
                rect.origin.x + region.min.x as f64 * fx,
                rect.origin.y + region.min.y as f64 * fy,
            ),
            (region.width() as f64 * fx, region.height() as f64 * fy).into(),
        ),
        // Page origins are whole device pixels, so scale 1 means one to one.
        exact: fx == 1.0 && fy == 1.0,
    })
}
