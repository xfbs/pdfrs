//! The page view: draws `specto_view::Frame`s and implements `GtkScrollable`, so a
//! `GtkScrolledWindow` around it provides scrollbars and kinetic scrolling.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};
use specto_geom::{DeviceRect, LayoutVector, ViewPoint, ViewSize, ViewVector};
use specto_source::Target;
use specto_view::{
    Granularity, Selection, Viewer, over_text, text_char, text_position, text_quads,
};

use crate::document::SharedDocument;
use specto_annot::Segment;
use specto_doc::SourceId;
use specto_session::{DraftKind, Grip, LinkPreview, Session, ViewShape};

/// Dragging a selection this close to the top or bottom edge (logical pixels) scrolls the view.
const AUTOSCROLL_EDGE: f64 = 24.0;

/// Handles a clicked link's URI (the window decides whether to open it). Called with where the page
/// was clicked.
type PlaceHandler = Box<dyn Fn(f64, f64)>;
type FieldKeyHandler = Box<dyn Fn(FieldKey)>;
type UriHandler = Box<dyn Fn(&str)>;

/// How long the pointer rests on a link before its target is shown.
const PREVIEW_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

/// The size of a link's preview at most, in logical pixels.
const PREVIEW_SIZE: (f64, f64) = (520.0, 220.0);

/// A link the pointer rests on.
pub struct Hover {
    preview: LinkPreview,
    /// Runs out when the target is to be shown.
    timer: Option<glib::SourceId>,
    /// Shows the target.
    popover: Option<gtk::Popover>,
}

mod imp {
    use super::*;

    #[derive(glib::Properties)]
    #[properties(wrapper_type = super::Canvas)]
    pub struct Canvas {
        #[property(override_interface = gtk::Scrollable, get, set = Self::set_hadjustment, nullable)]
        hadjustment: RefCell<Option<gtk::Adjustment>>,
        #[property(override_interface = gtk::Scrollable, get, set = Self::set_vadjustment, nullable)]
        vadjustment: RefCell<Option<gtk::Adjustment>>,
        #[property(override_interface = gtk::Scrollable, get, set, builder(gtk::ScrollablePolicy::Minimum))]
        hscroll_policy: Cell<gtk::ScrollablePolicy>,
        #[property(override_interface = gtk::Scrollable, get, set, builder(gtk::ScrollablePolicy::Minimum))]
        vscroll_policy: Cell<gtk::ScrollablePolicy>,

        pub document: RefCell<Option<SharedDocument>>,
        /// Set while we push viewer state into the adjustments, so their value-changed handlers
        /// don't feed it back.
        pub syncing: Cell<bool>,
        pub on_view_changed: RefCell<Option<Box<dyn Fn()>>>,
        pub on_selected: RefCell<Option<Box<dyn Fn()>>>,
        pub on_picked: RefCell<Option<Box<dyn Fn()>>>,
        pub on_drawn: RefCell<Option<Box<dyn Fn()>>>,
        /// What a drag draws instead of selecting text, if a drawing tool is in use: what, its
        /// color and the width of its line in points.
        pub draw_tool: Cell<Option<(DraftKind, specto_annot::Color, f64)>>,
        /// A press was on an annotation that can be moved or resized: once the pointer has moved
        /// far enough, the drag does that.
        pub may_adjust: Cell<bool>,
        /// A tool that puts something where the page is clicked (a note, a text box) is in use.
        pub place_tool: Cell<bool>,
        pub on_place: RefCell<Option<PlaceHandler>>,
        pub on_edit_text: RefCell<Option<Box<dyn Fn()>>>,
        pub on_fill: RefCell<Option<PlaceHandler>>,
        pub on_field_key: RefCell<Option<FieldKeyHandler>>,
        /// Whether the fields of the form that can be filled in are marked.
        pub highlight_fields: Cell<bool>,
        /// Called with a link's URI. The window decides whether to open it.
        pub on_open_uri: RefCell<Option<UriHandler>>,
        /// The cursor currently set: `None` (default), "pointer" over links, "text" over text.
        pub cursor: Cell<Option<&'static str>>,
        /// Clicks in the current click sequence (1, 2, 3), from the last press.
        pub presses: Cell<i32>,
        /// Where the current drag started, and whether it is selecting yet (it starts once past the
        /// drag threshold, so clicks stay clicks).
        pub drag_start: Cell<Option<(f64, f64)>>,
        pub selecting: Cell<bool>,
        /// Scrolls while a selection drag is outside the view.
        pub autoscroll: RefCell<Option<glib::SourceId>>,
        pub pointer: Cell<(f64, f64)>,
        /// Pending redraw for when the view settles (switches to exact tiles).
        pub settle_timer: RefCell<Option<glib::SourceId>>,
        /// Spinner for pages that take long to render.
        ///
        /// Only exists while one is shown: it asks for a redraw every frame while it animates.
        pub spinner: RefCell<Option<(adw::SpinnerPaintable, glib::SignalHandlerId)>>,
        /// Whether the last frame was final: every visible page sharp and nothing left to settle.
        /// Reset when the view changes.
        pub complete: Cell<bool>,
        /// The link the pointer rests on, while its target is about to be shown or is.
        pub hover: RefCell<Option<Hover>>,
    }

    impl Default for Canvas {
        fn default() -> Self {
            Self {
                hadjustment: RefCell::default(),
                vadjustment: RefCell::default(),
                hscroll_policy: Cell::new(gtk::ScrollablePolicy::Minimum),
                vscroll_policy: Cell::new(gtk::ScrollablePolicy::Minimum),
                document: RefCell::default(),
                syncing: Cell::default(),
                on_view_changed: RefCell::default(),
                on_selected: RefCell::default(),
                on_picked: RefCell::default(),
                on_drawn: RefCell::default(),
                draw_tool: Cell::default(),
                may_adjust: Cell::default(),
                place_tool: Cell::default(),
                on_place: RefCell::default(),
                on_edit_text: RefCell::default(),
                on_fill: RefCell::default(),
                on_field_key: RefCell::default(),
                highlight_fields: Cell::new(false),
                on_open_uri: RefCell::default(),
                cursor: Cell::default(),
                presses: Cell::default(),
                drag_start: Cell::default(),
                selecting: Cell::default(),
                autoscroll: RefCell::default(),
                pointer: Cell::default(),
                settle_timer: RefCell::default(),
                spinner: RefCell::default(),
                complete: Cell::default(),
                hover: RefCell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "SpectoCanvas";
        type Type = super::Canvas;
        type ParentType = gtk::Widget;
        type Interfaces = (gtk::Scrollable,);
    }

    impl Canvas {
        fn set_hadjustment(&self, adj: Option<gtk::Adjustment>) {
            self.connect_adjustment(&adj);
            self.hadjustment.replace(adj);
            self.obj().sync_adjustments();
        }

        fn set_vadjustment(&self, adj: Option<gtk::Adjustment>) {
            self.connect_adjustment(&adj);
            self.vadjustment.replace(adj);
            self.obj().sync_adjustments();
        }

        fn connect_adjustment(&self, adj: &Option<gtk::Adjustment>) {
            if let Some(adj) = adj {
                let obj = self.obj().downgrade();
                adj.connect_value_changed(move |_| {
                    let Some(obj) = obj.upgrade() else { return };
                    if obj.imp().syncing.get() {
                        return;
                    }
                    let (h, v) = (
                        obj.hadjustment().map_or(0.0, |a| a.value()),
                        obj.vadjustment().map_or(0.0, |a| a.value()),
                    );
                    obj.update_viewer(|v_| v_.set_scroll_offset(LayoutVector::new(h, v)));
                });
            }
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Canvas {}

    impl ScrollableImpl for Canvas {}

    impl WidgetImpl for Canvas {
        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            let scale = obj.device_scale();
            obj.update_viewer(|v| {
                v.set_viewport(ViewSize::new(width as f64, height as f64), scale)
            });
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let Some(doc) = self.document.borrow().clone() else {
                return;
            };
            let obj = self.obj();
            let scale = obj.device_scale();
            let mut guard = doc.borrow_mut();
            let d = &mut *guard;
            // The surface scale can change without a new allocation (moving the window to another
            // monitor).
            if d.viewer.scale_factor() != scale {
                let size = d.viewer.viewport();
                d.viewer.set_viewport(size, scale);
            }
            let now = Instant::now();
            let frame = d.session.viewer.frame(now, |k| d.cache.contains(k));
            let (dx, dy) = device_pixel_snap(&*obj, scale);
            snapshot.save();
            snapshot.translate(&graphene::Point::new(dx, dy));
            d.cache.begin_frame();
            for page in &frame.pages {
                let rect = logical(page.rect, scale);
                snapshot.append_outset_shadow(
                    &gsk::RoundedRect::from_rect(rect, 0.0),
                    &gdk::RGBA::new(0.0, 0.0, 0.0, 0.25),
                    0.0,
                    1.0,
                    0.0,
                    3.0,
                );
                let overlay = d.session.overlay(page.page);
                let cache = &mut d.cache;
                draw_page(snapshot, &rect, overlay, |snapshot| {
                    for layer in &page.layers {
                        if let Some(texture) = cache.get(&layer.key) {
                            // Not `append_scaled_texture`: on a HiDPI surface GSK renders that node
                            // at logical resolution and scales the result up, blurring every tile
                            // (measured on macOS). A plain texture node is sampled at device
                            // resolution, one to one when `layer.exact`.
                            snapshot.append_texture(texture, &logical(layer.dest, scale));
                        }
                    }
                });
            }
            obj.draw_fields(snapshot, d, &frame.pages, scale);
            obj.draw_picked(snapshot, d, &frame.pages, scale);
            obj.draw_field_focus(snapshot, d, &frame.pages, scale);
            obj.draw_matches(snapshot, d, &frame.pages, scale);
            obj.draw_selection(snapshot, d, &frame.pages, scale);
            // A spinner over expensive pages (huge images, or renders that already took > 500 ms)
            // that aren't sharp yet. Cheap pages render in milliseconds and never flash one.
            let viewport = DeviceRect::new(
                specto_geom::DevicePoint::origin(),
                (obj.width() as f64 * scale, obj.height() as f64 * scale).into(),
            );
            let busy: Vec<DeviceRect> = frame
                .pages
                .iter()
                .filter(|p| !p.complete && d.is_expensive(p.page))
                .filter_map(|p| p.rect.intersection(&viewport))
                .collect();
            obj.draw_spinners(snapshot, &busy, scale);
            snapshot.restore();
            d.set_canvas_requests(frame.requests);
            drop(guard);
            self.complete
                .set(frame.complete && frame.settles_at.is_none());
            if let Some(at) = frame.settles_at {
                obj.redraw_at(now, at);
            }
        }
    }
}

/// A key for the form field that has the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKey {
    /// Tab: to the next field.
    Next,
    /// Shift+Tab: to the one before.
    Previous,
    /// Space or Enter: ticks the field, or types into it.
    Press,
}

glib::wrapper! {
    pub struct Canvas(ObjectSubclass<imp::Canvas>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    pub fn new() -> Self {
        let obj: Self = glib::Object::new();
        obj.set_hexpand(true);
        obj.set_vexpand(true);
        obj.set_overflow(gtk::Overflow::Hidden);
        obj.set_focusable(true);
        obj.add_css_class("view");
        obj.setup_input();
        obj.connect_scale_factor_notify(|c| c.queue_resize());
        obj
    }

    pub fn set_document(&self, doc: Option<SharedDocument>) {
        self.imp().document.replace(doc);
        self.queue_resize();
        self.queue_draw();
    }

    /// Called after anything changes the view (scroll, zoom, page, mode). Called when a link to a
    /// URI is clicked.
    pub fn connect_open_uri(&self, f: impl Fn(&str) + 'static) {
        self.imp().on_open_uri.replace(Some(Box::new(f)));
    }

    /// Follows a link or outline target in file `source`.
    pub fn follow(&self, source: SourceId, target: &Target) {
        // A link out of the document leaves the view as it is.
        if let Target::Uri(uri) = target {
            if let Some(f) = self.imp().on_open_uri.borrow().as_ref() {
                f(uri);
            }
            return;
        }
        self.update_session(|s| s.follow(source, target));
    }

    fn link_at(&self, x: f64, y: f64) -> Option<(SourceId, Target)> {
        let doc = self.imp().document.borrow().clone()?;
        let mut d = doc.borrow_mut();
        d.link_at(ViewPoint::new(x, y))
    }

    /// `f` is called when the user has finished selecting text with the pointer (the button is up
    /// again) and something is selected.
    pub fn connect_selected(&self, f: impl Fn() + 'static) {
        self.imp().on_selected.replace(Some(Box::new(f)));
    }

    /// `f` is called with where the page was clicked while a placing tool is in use (see
    /// [`set_place_tool`](Self::set_place_tool)).
    pub fn connect_place(&self, f: impl Fn(f64, f64) + 'static) {
        self.imp().on_place.replace(Some(Box::new(f)));
    }

    /// `f` is called with where a form field that can be filled in was clicked.
    pub fn connect_fill(&self, f: impl Fn(f64, f64) + 'static) {
        self.imp().on_fill.replace(Some(Box::new(f)));
    }

    /// Marks the fields of the form that can be filled in, or stops marking them.
    pub fn set_highlight_fields(&self, on: bool) {
        self.imp().highlight_fields.set(on);
        self.queue_draw();
    }

    /// `f` is called with a key for the form field that has the keyboard.
    pub fn connect_field_key(&self, f: impl Fn(FieldKey) + 'static) {
        self.imp().on_field_key.replace(Some(Box::new(f)));
    }

    /// What kind of form field is at `(x, y)`, if one that can be filled in is.
    fn field_at(&self, x: f64, y: f64) -> Option<specto_session::FieldHover> {
        let doc = self.imp().document.borrow().clone()?;
        let mut d = doc.borrow_mut();
        d.field_hover(ViewPoint::new(x, y))
    }

    /// `f` is called when the text of the picked annotation is asked for: by a double click on it,
    /// or Enter.
    pub fn connect_edit_text(&self, f: impl Fn() + 'static) {
        self.imp().on_edit_text.replace(Some(Box::new(f)));
    }

    /// Makes clicks on a page put something there (see [`connect_place`](Self::connect_place))
    /// instead of picking.
    pub fn set_place_tool(&self, on: bool) {
        self.imp().place_tool.set(on);
        let (x, y) = self.pointer();
        self.update_cursor(x, y);
    }

    /// Where the picked annotation is in the widget, for a popover to point at.
    pub fn picked_area(&self) -> Option<gdk::Rectangle> {
        let doc = self.imp().document.borrow().clone()?;
        let (_, corners) = doc.borrow_mut().picked_outline()?;
        let (xs, ys) = (corners.map(|c| c.x), corners.map(|c| c.y));
        let min = |v: [f64; 4]| v.into_iter().fold(f64::INFINITY, f64::min);
        let max = |v: [f64; 4]| v.into_iter().fold(f64::NEG_INFINITY, f64::max);
        Some(gdk::Rectangle::new(
            min(xs) as i32,
            min(ys) as i32,
            (max(xs) - min(xs)).ceil() as i32,
            (max(ys) - min(ys)).ceil() as i32,
        ))
    }

    /// `f` is called when an annotation was drawn with the pointer.
    pub fn connect_drawn(&self, f: impl Fn() + 'static) {
        self.imp().on_drawn.replace(Some(Box::new(f)));
    }

    /// Makes drags draw (what, in which color, how wide a line in points) instead of selecting
    /// text, or `None` for selecting again.
    pub fn set_draw_tool(&self, tool: Option<(DraftKind, specto_annot::Color, f64)>) {
        self.imp().draw_tool.set(tool);
        let (x, y) = self.pointer();
        self.update_cursor(x, y);
    }

    /// Whether the pointer is drawing: a drag with a drawing tool that hasn't ended.
    fn is_drafting(&self) -> bool {
        self.imp()
            .document
            .borrow()
            .as_ref()
            .is_some_and(|d| d.borrow().is_drafting())
    }

    /// Whether the pointer is moving or resizing an annotation.
    fn is_adjusting(&self) -> bool {
        self.imp()
            .document
            .borrow()
            .as_ref()
            .is_some_and(|d| d.borrow().is_adjusting())
    }

    /// What a drag from here would take hold of (see `Session::grip_at`).
    fn grip_at(&self, x: f64, y: f64) -> Option<Grip> {
        let doc = self.imp().document.borrow().clone()?;
        let mut d = doc.borrow_mut();
        d.grip_at(ViewPoint::new(x, y))
    }

    /// Applies `f` to the document, then redraws.
    fn draft<R: Default>(&self, f: impl FnOnce(&mut crate::document::Document) -> R) -> R {
        let Some(doc) = self.imp().document.borrow().clone() else {
            return R::default();
        };
        let r = f(&mut doc.borrow_mut());
        // What the page is drawn from may have changed with it: not complete until a frame has
        // found all its tiles again.
        self.imp().complete.set(false);
        self.queue_draw();
        r
    }

    /// `f` is called when another annotation is picked, or none any more.
    pub fn connect_picked(&self, f: impl Fn() + 'static) {
        self.imp().on_picked.replace(Some(Box::new(f)));
    }

    fn picked_changed(&self) {
        self.queue_draw();
        if let Some(cb) = self.imp().on_picked.borrow().as_ref() {
            cb();
        }
    }

    pub fn connect_view_changed(&self, f: impl Fn() + 'static) {
        self.imp().on_view_changed.replace(Some(Box::new(f)));
    }

    /// Follows the pointer at `(x, y)` for showing where a link leads: over a link to a place in
    /// the document, its target is shown once the pointer has rested there a moment.
    fn update_link_preview(&self, x: f64, y: f64) {
        let imp = self.imp();
        let busy = imp.draw_tool.get().is_some() || imp.place_tool.get() || imp.selecting.get();
        let found = (imp.document.borrow().clone())
            .filter(|_| !busy)
            .and_then(|doc| doc.borrow_mut().link_preview(ViewPoint::new(x, y)));
        let Some(preview) = found else {
            self.hide_link_preview();
            return;
        };
        let same = (imp.hover.borrow().as_ref())
            .is_some_and(|h| h.preview.link == preview.link && h.preview.same_target(&preview));
        if same {
            return;
        }
        self.hide_link_preview();
        let this = self.downgrade();
        let timer = glib::timeout_add_local_once(PREVIEW_DELAY, move || {
            if let Some(c) = this.upgrade() {
                if let Some(hover) = c.imp().hover.borrow_mut().as_mut() {
                    hover.timer = None;
                }
                c.show_link_preview();
            }
        });
        imp.hover.replace(Some(Hover {
            preview,
            timer: Some(timer),
            popover: None,
        }));
    }

    /// Takes away what [`update_link_preview`](Self::update_link_preview) shows or is about to.
    pub fn hide_link_preview(&self) {
        let Some(hover) = self.imp().hover.take() else {
            return;
        };

        if let Some(timer) = hover.timer {
            timer.remove();
        }
        if let Some(popover) = hover.popover {
            popover.popdown();
        }
    }

    /// Renders the target of the link the pointer rests on, off the main thread, and shows it
    /// under the link if the pointer is still there by then.
    fn show_link_preview(&self) {
        let Some(preview) = (self.imp().hover.borrow().as_ref()).map(|h| h.preview.clone()) else {
            return;
        };
        let scale = self.device_scale();
        let width = PREVIEW_SIZE
            .0
            .min(f64::from(self.width()) - 48.0)
            .max(120.0);
        let pixels = |logical: f64| (logical * scale).round() as u32;
        let (w, h) = (pixels(width), pixels(PREVIEW_SIZE.1));
        let this = self.downgrade();
        glib::spawn_future_local(async move {
            let wanted = preview.clone();
            let rendered = gtk::gio::spawn_blocking(move || wanted.render(w, h)).await;
            let (Some(c), Ok(Some(bitmap))) = (this.upgrade(), rendered) else {
                return;
            };
            let mut hover = c.imp().hover.borrow_mut();
            let Some(hover) = hover.as_mut().filter(|h| {
                h.popover.is_none()
                    && h.preview.link == preview.link
                    && h.preview.same_target(&preview)
            }) else {
                return;
            };
            let texture = gdk::MemoryTexture::new(
                bitmap.width as i32,
                bitmap.height as i32,
                gdk::MemoryFormat::R8g8b8a8Premultiplied,
                &glib::Bytes::from_owned(bitmap.data),
                bitmap.width as usize * 4,
            );
            let picture = gtk::Picture::builder()
                .paintable(&texture)
                .content_fit(gtk::ContentFit::Fill)
                .width_request((f64::from(bitmap.width) / scale).round() as i32)
                .height_request((f64::from(bitmap.height) / scale).round() as i32)
                .build();
            // It only shows: it takes neither the keyboard nor the clicks meant for the page.
            let popover = gtk::Popover::builder()
                .child(&picture)
                .autohide(false)
                .can_focus(false)
                .can_target(false)
                .position(gtk::PositionType::Bottom)
                .build();
            popover.set_parent(&c);
            let link = preview.link;
            popover.set_pointing_to(Some(&gdk::Rectangle::new(
                link.origin.x.floor() as i32,
                link.origin.y.floor() as i32,
                link.size.width.ceil() as i32,
                link.size.height.ceil() as i32,
            )));
            popover.connect_closed(|popover| {
                let popover = popover.clone();
                glib::idle_add_local_once(move || popover.unparent());
            });
            popover.popup();
            hover.popover = Some(popover);
        });
    }

    /// The preview of a link's target, while one shows (for tests): its size in logical pixels,
    /// and how many of its device pixels are dark.
    pub fn link_preview_shown(&self) -> Option<(i32, i32, usize)> {
        let hover = self.imp().hover.borrow();
        let picture = (hover.as_ref()?.popover.as_ref()?.child()?)
            .downcast::<gtk::Picture>()
            .ok()?;
        let texture = picture.paintable().and_downcast::<gdk::Texture>()?;
        let mut pixels = vec![0; (texture.width() * texture.height() * 4) as usize];
        texture.download(&mut pixels, texture.width() as usize * 4);
        let dark = pixels
            .chunks(4)
            .filter(|p| p[0] < 90 && p[1] < 90 && p[2] < 90);
        Some((
            picture.width_request(),
            picture.height_request(),
            dark.count(),
        ))
    }

    /// Applies a change to the viewer, then updates scrollbars and redraws.
    pub fn update_viewer(&self, f: impl FnOnce(&mut Viewer)) {
        self.update_session(|s| f(&mut s.viewer));
    }

    /// [`update_viewer`](Self::update_viewer) for a change that needs the whole session. `None`
    /// without a document.
    fn update_session<R>(&self, f: impl FnOnce(&mut Session) -> R) -> Option<R> {
        let doc = self.imp().document.borrow().clone()?;
        let place = |v: &Viewer| (v.scroll_offset(), v.zoom(), v.layout_mode());
        let (result, moved) = {
            let mut d = doc.borrow_mut();
            let before = place(&d.viewer);
            let result = f(&mut d);
            // Pages coming into view with their margins trimmed are measured before they are laid
            // out.
            d.trim_shown();
            (result, place(&d.viewer) != before)
        };
        // What a link's preview points at moves with the view.
        if moved {
            self.hide_link_preview();
        }
        self.imp().complete.set(false);
        self.sync_adjustments();
        self.queue_draw();
        if let Some(cb) = self.imp().on_view_changed.borrow().as_ref() {
            cb();
        }
        Some(result)
    }

    /// [`update_viewer`](Self::update_viewer) for a change that takes the reader somewhere else:
    /// where they were is remembered for going back.
    pub fn jump(&self, f: impl FnOnce(&mut Viewer)) {
        self.update_viewer(|v| {
            v.jump(f);
        });
    }

    /// Whether the last frame was drawn completely, with sharp tiles.
    pub fn is_complete(&self) -> bool {
        self.imp().complete.get()
    }

    /// Read-only access to the viewer.
    pub fn with_viewer<R>(&self, f: impl FnOnce(&Viewer) -> R) -> Option<R> {
        let doc = self.imp().document.borrow().clone()?;
        let d = doc.borrow();
        Some(f(&d.viewer))
    }

    // Text selection

    /// Highlights search matches on the visible pages: all in yellow, the current one in orange
    /// (Adwaita's palette).
    fn draw_matches(
        &self,
        snapshot: &gtk::Snapshot,
        d: &mut crate::document::Document,
        pages: &[specto_view::PageDraw],
        scale: f64,
    ) {
        let yellow = gdk::RGBA::new(0.965, 0.827, 0.176, 0.5);
        let orange = gdk::RGBA::new(1.0, 0.471, 0.0, 0.55);
        for page in pages {
            let matches = d.matches_on(page.page);
            if matches.is_empty() {
                continue;
            }
            let (viewer, mut texts, _) = d.text();
            let Some(text) = specto_view::TextPages::text_page(&mut texts, page.page) else {
                continue;
            };
            snapshot.push_clip(&logical(page.rect, scale));
            for (m, current) in matches {
                let quads = text_quads(viewer, &text, page.page, m.chars);
                fill_quads(snapshot, &quads, if current { &orange } else { &yellow });
            }
            snapshot.pop();
        }
    }

    /// Fills the selected text of the visible pages with the accent color.
    fn draw_selection(
        &self,
        snapshot: &gtk::Snapshot,
        d: &mut crate::document::Document,
        pages: &[specto_view::PageDraw],
        scale: f64,
    ) {
        let (viewer, mut texts, selection) = d.text();
        let Some(selection) = selection.as_ref().filter(|s| !s.is_empty()) else {
            return;
        };
        let mut color = adw::StyleManager::default().accent_color_rgba();
        color.set_alpha(0.35);
        for page in pages {
            let quads = selection.view_quads(viewer, &mut texts, page.page);
            if quads.is_empty() {
                continue;
            }
            snapshot.push_clip(&logical(page.rect, scale));
            fill_quads(snapshot, &quads, &color);
            snapshot.pop();
        }
    }

    /// A tint over the fields of the form that can be filled in, if they are to be marked.
    fn draw_fields(
        &self,
        snapshot: &gtk::Snapshot,
        d: &mut crate::document::Document,
        pages: &[specto_view::PageDraw],
        scale: f64,
    ) {
        if !self.imp().highlight_fields.get() {
            return;
        }
        let positions: Vec<usize> = pages.iter().map(|p| p.page).collect();
        let outlines = d.field_outlines(&positions);
        let accent = adw::StyleManager::default().accent_color_rgba();
        let tint = gdk::RGBA::new(accent.red(), accent.green(), accent.blue(), 0.18);
        for page in pages {
            let quads: Vec<[ViewPoint; 4]> = outlines
                .iter()
                .filter(|(position, _)| *position == page.page)
                .map(|(_, corners)| *corners)
                .collect();
            if quads.is_empty() {
                continue;
            }
            snapshot.push_clip(&logical(page.rect, scale));
            fill_quads(snapshot, &quads, &tint);
            snapshot.pop();
        }
    }

    /// A frame around the form field that has the keyboard.
    fn draw_field_focus(
        &self,
        snapshot: &gtk::Snapshot,
        d: &mut crate::document::Document,
        pages: &[specto_view::PageDraw],
        scale: f64,
    ) {
        let Some((position, corners)) = d.focused_field_outline() else {
            return;
        };
        let Some(page) = pages.iter().find(|p| p.page == position) else {
            return;
        };
        let (xs, ys) = (corners.map(|c| c.x as f32), corners.map(|c| c.y as f32));
        let min = |v: [f32; 4]| v.into_iter().fold(f32::INFINITY, f32::min);
        let max = |v: [f32; 4]| v.into_iter().fold(f32::NEG_INFINITY, f32::max);
        let frame = graphene::Rect::new(
            min(xs) - 2.0,
            min(ys) - 2.0,
            max(xs) - min(xs) + 4.0,
            max(ys) - min(ys) + 4.0,
        );
        let builder = gsk::PathBuilder::new();
        builder.add_rounded_rect(&gsk::RoundedRect::from_rect(frame, 2.0));
        let accent = adw::StyleManager::default().accent_color_rgba();
        snapshot.push_clip(&logical(page.rect, scale));
        snapshot.append_stroke(&builder.to_path(), &gsk::Stroke::new(2.0), &accent);
        snapshot.pop();
    }

    /// A frame around the annotation that is picked.
    fn draw_picked(
        &self,
        snapshot: &gtk::Snapshot,
        d: &mut crate::document::Document,
        pages: &[specto_view::PageDraw],
        scale: f64,
    ) {
        let Some((position, corners)) = d.picked_outline() else {
            return;
        };
        let Some(page) = pages.iter().find(|p| p.page == position) else {
            return;
        };
        // A little outside the annotation, so it doesn't cover its edge.
        let (xs, ys) = (corners.map(|c| c.x as f32), corners.map(|c| c.y as f32));
        let min = |v: [f32; 4]| v.into_iter().fold(f32::INFINITY, f32::min);
        let max = |v: [f32; 4]| v.into_iter().fold(f32::NEG_INFINITY, f32::max);
        let frame = graphene::Rect::new(
            min(xs) - 3.0,
            min(ys) - 3.0,
            max(xs) - min(xs) + 6.0,
            max(ys) - min(ys) + 6.0,
        );
        let builder = gsk::PathBuilder::new();
        builder.add_rounded_rect(&gsk::RoundedRect::from_rect(frame, 3.0));
        let accent = adw::StyleManager::default().accent_color_rgba();
        snapshot.push_clip(&logical(page.rect, scale));
        snapshot.append_stroke(&builder.to_path(), &gsk::Stroke::new(1.5), &accent);
        snapshot.pop();
        // Handles to resize it by, where it can be: over the page's edge too, so one at the edge
        // can be taken.
        for handle in d.picked_handles() {
            let dot = gsk::PathBuilder::new();
            dot.add_circle(&graphene::Point::new(handle.x as f32, handle.y as f32), 4.5);
            let dot = dot.to_path();
            snapshot.append_fill(&dot, gsk::FillRule::Winding, &gdk::RGBA::WHITE);
            snapshot.append_stroke(&dot, &gsk::Stroke::new(1.5), &accent);
        }
    }

    /// Picks the annotation under the pointer, or none. Returns whether one is picked now.
    fn pick_annotation(&self, x: f64, y: f64) -> bool {
        let Some(doc) = self.imp().document.borrow().clone() else {
            return false;
        };
        let mut d = doc.borrow_mut();
        let picked = d.annotation_at(ViewPoint::new(x, y));
        let changed = d.picked != picked;
        d.picked = picked;
        drop(d);
        if changed {
            self.picked_changed();
        }
        picked.is_some()
    }

    /// For a double click that picked an annotation: whether that is marked text, where the click's
    /// word stays selected and the annotation isn't picked after all.
    fn clear_selection_on_markup(&self) -> bool {
        let Some(doc) = self.imp().document.borrow().clone() else {
            return false;
        };
        let mut d = doc.borrow_mut();
        let Some((position, id)) = d.picked else {
            return false;
        };
        let markup = d
            .annotations(position)
            .into_iter()
            .find(|(i, _)| *i == id)
            .is_some_and(|(_, a)| matches!(a.kind, specto_annot::AnnotKind::TextMarkup { .. }));
        if markup {
            d.picked = None;
        } else {
            d.selection = None;
        }
        drop(d);
        self.picked_changed();
        markup
    }

    /// Picks the annotation `id` on the page at `position` and scrolls to it (see
    /// `Session::pick_and_show`).
    pub fn pick_and_show(&self, position: usize, id: specto_doc::AnnotId) {
        let Some(doc) = self.imp().document.borrow().clone() else {
            return;
        };
        let found = doc.borrow_mut().pick_and_show(position, id);
        if found {
            // Scrollbars, a redraw, the page number.
            self.update_viewer(|_| {});
            self.picked_changed();
        }
    }

    /// Whether the picked annotation is a note.
    fn picked_is_note(&self) -> bool {
        self.imp()
            .document
            .borrow()
            .as_ref()
            .is_some_and(|d| d.borrow_mut().picked_is_note())
    }

    /// Lets go of the picked annotation. Returns whether there was one.
    pub fn unpick_annotation(&self) -> bool {
        let Some(doc) = self.imp().document.borrow().clone() else {
            return false;
        };
        let had = doc.borrow_mut().picked.take().is_some();
        if had {
            self.picked_changed();
        }
        had
    }

    /// Applies `f` to the selection (with the viewer and text layer), then redraws.
    fn edit_selection(
        &self,
        f: impl FnOnce(&Viewer, &mut crate::document::TextLayer<'_>, &mut Option<Selection>),
    ) {
        let Some(doc) = self.imp().document.borrow().clone() else {
            return;
        };
        {
            let mut d = doc.borrow_mut();
            let (viewer, mut texts, selection) = d.text();
            f(viewer, &mut texts, selection);
        }
        self.queue_draw();
    }

    /// A press: one click starts afresh (or extends with Shift), two select a word, three a line.
    fn press(&self, presses: i32, x: f64, y: f64, extend: bool) {
        let p = ViewPoint::new(x, y);
        self.edit_selection(|viewer, texts, selection| match presses {
            1 if extend && selection.is_some() => {
                if let (Some(s), Some(to)) = (selection.as_mut(), text_position(viewer, texts, p)) {
                    s.extend(texts, to);
                }
            }
            1 => *selection = None,
            n => {
                let g = if n == 2 {
                    Granularity::Word
                } else {
                    Granularity::Line
                };
                *selection = text_char(viewer, texts, p).map(|at| Selection::new(texts, at, g));
            }
        });
    }

    /// Starts a character selection where a drag began.
    fn begin_selection(&self, x: f64, y: f64) {
        self.edit_selection(|viewer, texts, selection| {
            *selection = text_position(viewer, texts, ViewPoint::new(x, y))
                .map(|at| Selection::new(texts, at, Granularity::Char));
        });
    }

    /// Extends the selection to the point under the pointer.
    fn extend_selection(&self, x: f64, y: f64) {
        self.edit_selection(|viewer, texts, selection| {
            if let (Some(s), Some(to)) = (
                selection.as_mut(),
                text_position(viewer, texts, ViewPoint::new(x, y)),
            ) {
                s.extend(texts, to);
            }
        });
    }

    pub fn select_all(&self) {
        self.edit_selection(|viewer, _, selection| {
            *selection = Some(Selection::all(viewer.page_count()));
        });
    }

    pub fn clear_selection(&self) -> bool {
        let mut had = false;
        self.edit_selection(|_, _, selection| had = selection.take().is_some());
        had
    }

    /// The selected text, if any.
    pub fn selected_text(&self) -> Option<String> {
        let doc = self.imp().document.borrow().clone()?;
        doc.borrow_mut().selected_text()
    }

    /// Copies the selection to the clipboard. False if nothing is selected.
    pub fn copy(&self) -> bool {
        let Some(text) = self.selected_text() else {
            return false;
        };
        self.clipboard().set_text(&text);
        true
    }

    /// Also offers the selection as the primary selection (middle-click paste on Linux), as other
    /// apps do.
    fn offer_primary(&self) {
        if let Some(text) = self.selected_text() {
            self.primary_clipboard().set_text(&text);
        }
    }

    /// How far the pointer is into the autoscroll zone at the top (negative) or bottom (positive)
    /// of the view: the last `AUTOSCROLL_EDGE` pixels and beyond.
    ///
    /// A maximized window's edge is the screen's, and the pointer can't go past that.
    fn autoscroll_overshoot(&self, y: f64) -> f64 {
        let height = self.height() as f64;
        if y < AUTOSCROLL_EDGE {
            y - AUTOSCROLL_EDGE
        } else if y > height - AUTOSCROLL_EDGE {
            y - (height - AUTOSCROLL_EDGE)
        } else {
            0.0
        }
    }

    /// While a selection drag is near or beyond the top or bottom of the view, scrolls towards the
    /// pointer, faster the further out it is.
    fn autoscroll(&self) {
        let imp = self.imp();
        if imp.autoscroll.borrow().is_some() {
            return;
        }
        let this = self.downgrade();
        let id = glib::timeout_add_local(std::time::Duration::from_millis(30), move || {
            let Some(c) = this.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let (x, y) = c.imp().pointer.get();
            let over = c.autoscroll_overshoot(y);
            if over == 0.0 || !c.imp().selecting.get() {
                c.imp().autoscroll.take();
                return glib::ControlFlow::Break;
            }
            let step = over.clamp(-80.0, 80.0) / 2.0;
            c.update_viewer(|v| v.scroll_by(ViewVector::new(0.0, step)));
            c.extend_selection(x, y);
            glib::ControlFlow::Continue
        });
        imp.autoscroll.replace(Some(id));
    }

    /// The cursor for the pointer at `(x, y)`: a hand over links, an I-beam over text.
    fn update_cursor(&self, x: f64, y: f64) {
        let p = ViewPoint::new(x, y);
        let name = if self.imp().draw_tool.get().is_some() || self.imp().place_tool.get() {
            Some("crosshair")
        } else if let Some(grip) = self.grip_at(x, y) {
            Some(match grip {
                Grip::Move => "move",
                Grip::Corner(_) | Grip::End(_) => "crosshair",
            })
        } else if let Some(field) = self.field_at(x, y) {
            Some(match field {
                specto_session::FieldHover::Text => "text",
                specto_session::FieldHover::Button => "pointer",
            })
        } else if self.link_at(x, y).is_some() {
            Some("pointer")
        } else {
            let mut over = false;
            if let Some(doc) = self.imp().document.borrow().clone() {
                let mut d = doc.borrow_mut();
                let (viewer, mut texts, _) = d.text();
                over = over_text(viewer, &mut texts, p);
            }
            over.then_some("text")
        };
        if name != self.imp().cursor.replace(name) {
            self.set_cursor_from_name(name);
        }
    }

    /// The last pointer position over the canvas (for tests).
    pub fn pointer(&self) -> (f64, f64) {
        self.imp().pointer.get()
    }

    /// The cursor name currently shown (for tests).
    pub fn cursor_name(&self) -> Option<&'static str> {
        self.imp().cursor.get()
    }

    /// Draws a spinner in the middle of each rectangle (device pixels), and creates or drops the
    /// animating paintable as needed.
    fn draw_spinners(&self, snapshot: &gtk::Snapshot, at: &[DeviceRect], scale: f64) {
        const SIZE: f32 = 32.0;
        const BACKING: f32 = 56.0;
        let imp = self.imp();
        if at.is_empty() {
            if let Some((paintable, handler)) = imp.spinner.take() {
                paintable.disconnect(handler);
            }
            return;
        }
        if imp.spinner.borrow().is_none() {
            let paintable = adw::SpinnerPaintable::new(Some(self));
            let this = self.downgrade();
            let handler = paintable.connect_invalidate_contents(move |_| {
                if let Some(c) = this.upgrade() {
                    c.queue_draw();
                }
            });
            imp.spinner.replace(Some((paintable, handler)));
        }
        let spinner = imp.spinner.borrow();
        let Some((paintable, _)) = spinner.as_ref() else {
            return;
        };
        for r in at {
            let center = graphene::Point::new(
                ((r.origin.x + r.size.width / 2.0) / scale) as f32,
                ((r.origin.y + r.size.height / 2.0) / scale) as f32,
            );
            let backing = gsk::RoundedRect::from_rect(
                graphene::Rect::new(
                    center.x() - BACKING / 2.0,
                    center.y() - BACKING / 2.0,
                    BACKING,
                    BACKING,
                ),
                12.0,
            );
            snapshot.append_outset_shadow(
                &backing,
                &gdk::RGBA::new(0.0, 0.0, 0.0, 0.2),
                0.0,
                1.0,
                0.0,
                4.0,
            );
            snapshot.push_rounded_clip(&backing);
            snapshot.append_color(&gdk::RGBA::new(1.0, 1.0, 1.0, 0.92), backing.bounds());
            snapshot.pop();
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                center.x() - SIZE / 2.0,
                center.y() - SIZE / 2.0,
            ));
            paintable.snapshot(snapshot, SIZE as f64, SIZE as f64);
            snapshot.restore();
        }
    }

    /// Schedules one redraw at `at` (when the view settles), unless one is already pending. A
    /// redraw then reschedules if the deadline moved.
    fn redraw_at(&self, now: Instant, at: Instant) {
        let imp = self.imp();
        if imp.settle_timer.borrow().is_some() {
            return;
        }
        let this = self.downgrade();
        let id = glib::timeout_add_local_once(at.saturating_duration_since(now), move || {
            if let Some(c) = this.upgrade() {
                c.imp().settle_timer.take();
                c.queue_draw();
            }
        });
        imp.settle_timer.replace(Some(id));
    }

    pub fn device_scale(&self) -> f64 {
        self.native()
            .and_then(|n| n.surface())
            .map_or(self.scale_factor() as f64, |s| s.scale())
    }

    /// Pushes the viewer's content size and scroll offset into the adjustments.
    fn sync_adjustments(&self) {
        let Some((content, scroll, view)) =
            self.with_viewer(|v| (v.content_size(), v.scroll_offset(), v.viewport()))
        else {
            return;
        };
        let imp = self.imp();
        imp.syncing.set(true);
        let configure = |adj: Option<gtk::Adjustment>, value: f64, upper: f64, page: f64| {
            if let Some(adj) = adj {
                // Centered short content has a negative offset, while scrollbars start at 0.
                adj.configure(
                    value.max(0.0),
                    0.0,
                    upper.max(page),
                    page * 0.1,
                    page * 0.9,
                    page,
                );
            }
        };
        configure(self.hadjustment(), scroll.x, content.width, view.width);
        configure(self.vadjustment(), scroll.y, content.height, view.height);
        imp.syncing.set(false);
    }

    fn setup_input(&self) {
        // Resting on a note, or anything with a comment, shows its text.
        self.set_has_tooltip(true);
        self.connect_query_tooltip(|c, x, y, _keyboard, tooltip| {
            let Some(doc) = c.imp().document.borrow().clone() else {
                return false;
            };
            let hint = doc
                .borrow_mut()
                .hint_at(ViewPoint::new(f64::from(x), f64::from(y)));
            match hint {
                Some(text) => {
                    tooltip.set_text(Some(&text));
                    true
                }
                None => false,
            }
        });

        let motion = gtk::EventControllerMotion::new();
        let this = self.downgrade();
        motion.connect_motion(move |_, x, y| {
            if let Some(c) = this.upgrade() {
                c.imp().pointer.set((x, y));
                if !c.imp().selecting.get() {
                    c.update_cursor(x, y);
                }
                c.update_link_preview(x, y);
            }
        });
        let this = self.downgrade();
        motion.connect_leave(move |_| {
            if let Some(c) = this.upgrade() {
                c.hide_link_preview();
            }
        });
        self.add_controller(motion);

        // Presses: one click clears the selection (Shift extends it), two select a word, three a
        // line. A click without movement on a link follows it.
        let click = gtk::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        let this = self.downgrade();
        click.connect_pressed(move |g, n_press, x, y| {
            let Some(c) = this.upgrade() else { return };
            c.grab_focus();
            c.hide_link_preview();
            c.imp().presses.set(n_press);
            // A press elsewhere takes the keyboard off the form.
            if c.field_at(x, y).is_none() {
                let had = (c.imp().document.borrow().as_ref())
                    .is_some_and(|d| d.borrow_mut().clear_field_focus());
                if had {
                    c.queue_draw();
                }
            }
            // With a drawing tool, the drag gesture has the pointer.
            if c.imp().draw_tool.get().is_some() {
                return;
            }
            let shift = g
                .current_event_state()
                .contains(gdk::ModifierType::SHIFT_MASK);
            c.press(n_press.min(3), x, y, shift);
        });
        let this = self.downgrade();
        click.connect_released(move |g, n_press, x, y| {
            let Some(c) = this.upgrade() else { return };
            if c.imp().selecting.get() || c.imp().draw_tool.get().is_some() || c.is_adjusting() {
                return;
            }
            // A second click on an annotation other than marked text (where it selects the word)
            // asks for its text.
            if n_press == 2 {
                let on_one = c.pick_annotation(x, y) && !c.clear_selection_on_markup();
                if on_one && let Some(cb) = c.imp().on_edit_text.borrow().as_ref() {
                    cb();
                }
                return;
            }
            if n_press != 1 {
                return;
            }
            // A placing tool puts its thing where the click was.
            if c.imp().place_tool.get() {
                if let Some(cb) = c.imp().on_place.borrow().as_ref() {
                    cb(x, y);
                }
                return;
            }
            // A form field is filled in.
            if c.field_at(x, y).is_some() {
                g.set_state(gtk::EventSequenceState::Claimed);
                if let Some(cb) = c.imp().on_fill.borrow().as_ref() {
                    cb(x, y);
                }
                return;
            }
            if let Some((source, target)) = c.link_at(x, y) {
                g.set_state(gtk::EventSequenceState::Claimed);
                c.follow(source, &target);
                return;
            }
            // A click on an annotation picks it, and one elsewhere lets go. A note shows its
            // text for that, as its mark says nothing.
            if c.pick_annotation(x, y)
                && c.picked_is_note()
                && let Some(cb) = c.imp().on_edit_text.borrow().as_ref()
            {
                cb();
            }
        });
        self.add_controller(click);

        // Dragging selects text: by characters after a single press (once past the drag threshold,
        // so clicks and links still work), by words or lines after a double or triple one.
        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let this = self.downgrade();
        drag.connect_drag_begin(move |g, x, y| {
            let Some(c) = this.upgrade() else { return };
            c.imp().drag_start.set(Some((x, y)));
            c.imp().selecting.set(false);
            // A drawing tool draws from the press on, on a page.
            if let Some((kind, color, width)) = c.imp().draw_tool.get() {
                c.grab_focus();
                let p = ViewPoint::new(x, y);
                if c.draft(|d| d.begin_draft(kind, color, width, p)) {
                    g.set_state(gtk::EventSequenceState::Claimed);
                }
                return;
            }
            // On a drawing or a shape, or a handle of the picked one: a drag from here moves or
            // resizes it.
            let on_one = c.grip_at(x, y).is_some();
            c.imp().may_adjust.set(on_one);
        });
        let this = self.downgrade();
        drag.connect_drag_update(move |g, dx, dy| {
            let Some(c) = this.upgrade() else { return };
            let imp = c.imp();
            let Some((x0, y0)) = imp.drag_start.get() else {
                return;
            };
            let (x, y) = (x0 + dx, y0 + dy);
            imp.pointer.set((x, y));
            if imp.draw_tool.get().is_some() {
                c.draft(|d| d.extend_draft(ViewPoint::new(x, y)));
                return;
            }
            if c.is_adjusting() {
                c.draft(|d| d.adjust_to(ViewPoint::new(x, y)));
                return;
            }
            if imp.may_adjust.get() {
                let threshold =
                    gtk::Settings::default().map_or(8, |s| s.gtk_dnd_drag_threshold()) as f64;
                if dx.hypot(dy) < threshold {
                    return;
                }
                imp.may_adjust.set(false);
                // From where the press was, so that it doesn't jump.
                if c.draft(|d| d.begin_adjust(ViewPoint::new(x0, y0))) {
                    g.set_state(gtk::EventSequenceState::Claimed);
                    c.picked_changed();
                    c.draft(|d| d.adjust_to(ViewPoint::new(x, y)));
                    return;
                }
            }
            if !imp.selecting.get() {
                let threshold =
                    gtk::Settings::default().map_or(8, |s| s.gtk_dnd_drag_threshold()) as f64;
                if imp.presses.get() == 1 && dx.hypot(dy) < threshold {
                    return;
                }
                imp.selecting.set(true);
                g.set_state(gtk::EventSequenceState::Claimed);
                if imp.presses.get() == 1 {
                    c.begin_selection(x0, y0);
                }
                c.set_cursor_from_name(Some("text"));
                imp.cursor.set(Some("text"));
            }
            c.extend_selection(x, y);
            if c.autoscroll_overshoot(y) != 0.0 {
                c.autoscroll();
            }
        });
        let this = self.downgrade();
        drag.connect_drag_end(move |_, _, _| {
            let Some(c) = this.upgrade() else { return };
            let imp = c.imp();
            imp.drag_start.set(None);
            imp.may_adjust.set(false);
            if c.is_drafting() {
                if c.draft(|d| d.finish_draft())
                    && let Some(cb) = imp.on_drawn.borrow().as_ref()
                {
                    cb();
                }
                return;
            }
            if c.is_adjusting() {
                c.draft(|d| d.finish_adjust());
                // Also when nothing changed: what the page is drawn from went there and back.
                if let Some(cb) = imp.on_drawn.borrow().as_ref() {
                    cb();
                }
                return;
            }
            if imp.selecting.replace(false) {
                c.offer_primary();
            }
            if let Some(id) = imp.autoscroll.take() {
                id.remove();
            }
            // Also after a double or triple click, which selects without dragging.
            if c.selected_text().is_some()
                && let Some(cb) = imp.on_selected.borrow().as_ref()
            {
                cb();
            }
        });
        self.add_controller(drag);

        // Ctrl+scroll zooms. Plain scrolling bubbles up to the GtkScrolledWindow.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let this = self.downgrade();
        scroll.connect_scroll(move |ctl, _dx, dy| {
            let Some(c) = this.upgrade() else {
                return glib::Propagation::Proceed;
            };
            // Ctrl+scroll zooms, and on macOS Cmd+scroll too.
            if !ctl
                .current_event_state()
                .intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::META_MASK)
            {
                return glib::Propagation::Proceed;
            }
            let step = if ctl.unit() == gdk::ScrollUnit::Wheel {
                0.15
            } else {
                0.01
            };
            let (x, y) = c.imp().pointer.get();
            c.update_viewer(|v| v.zoom_by((-dy * step).exp(), ViewPoint::new(x, y)));
            glib::Propagation::Stop
        });
        self.add_controller(scroll);

        // Touchpad / touchscreen pinch. While it lasts the view isn't "settled", so pauses
        // mid-pinch don't render exact-scale tiles for intermediate zooms.
        let pinch = gtk::GestureZoom::new();
        let last = std::rc::Rc::new(Cell::new(1.0));
        let l = last.clone();
        let this = self.downgrade();
        pinch.connect_begin(move |_, _| {
            l.set(1.0);
            if let Some(c) = this.upgrade() {
                c.update_viewer(Viewer::begin_zoom_gesture);
            }
        });
        let this = self.downgrade();
        pinch.connect_end(move |_, _| {
            if let Some(c) = this.upgrade() {
                c.update_viewer(Viewer::end_zoom_gesture);
            }
        });
        let this = self.downgrade();
        pinch.connect_cancel(move |_, _| {
            if let Some(c) = this.upgrade() {
                c.update_viewer(Viewer::end_zoom_gesture);
            }
        });
        let this = self.downgrade();
        pinch.connect_scale_changed(move |g, scale| {
            let Some(c) = this.upgrade() else { return };
            let (x, y) = g.bounding_box_center().unwrap_or((0.0, 0.0));
            let factor = scale / last.replace(scale);
            c.update_viewer(|v| v.zoom_by(factor, ViewPoint::new(x, y)));
        });
        self.add_controller(pinch);

        // Paging keys. Arrow keys and space are handled here too, so they work in paged modes where
        // there's nothing to scroll.
        let keys = gtk::EventControllerKey::new();
        let this = self.downgrade();
        keys.connect_key_pressed(move |_, key, _, state| {
            let Some(c) = this.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if state.intersects(
                gdk::ModifierType::CONTROL_MASK
                    | gdk::ModifierType::ALT_MASK
                    | gdk::ModifierType::META_MASK,
            ) {
                return glib::Propagation::Proceed;
            }
            // Enter on a picked annotation asks for its text.
            if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) && c.picked_area().is_some() {
                if let Some(cb) = c.imp().on_edit_text.borrow().as_ref() {
                    cb();
                }
                return glib::Propagation::Stop;
            }
            // With the keyboard on a form field: Tab goes to the next one (Shift+Tab to the one
            // before), Space and Enter tick it or type into it, Escape lets go of it.
            let on_field = (c.imp().document.borrow().as_ref())
                .is_some_and(|d| d.borrow().field_focus().is_some());
            if on_field {
                let back = state.contains(gdk::ModifierType::SHIFT_MASK);
                let field_key = match key {
                    gdk::Key::ISO_Left_Tab => Some(FieldKey::Previous),
                    gdk::Key::Tab | gdk::Key::KP_Tab if back => Some(FieldKey::Previous),
                    gdk::Key::Tab | gdk::Key::KP_Tab => Some(FieldKey::Next),
                    gdk::Key::space | gdk::Key::Return | gdk::Key::KP_Enter => {
                        Some(FieldKey::Press)
                    }
                    _ => None,
                };
                if let Some(field_key) = field_key {
                    if let Some(cb) = c.imp().on_field_key.borrow().as_ref() {
                        cb(field_key);
                    }
                    return glib::Propagation::Stop;
                }
                if key == gdk::Key::Escape {
                    if let Some(d) = c.imp().document.borrow().as_ref() {
                        d.borrow_mut().clear_field_focus();
                    }
                    c.queue_draw();
                    return glib::Propagation::Stop;
                }
            }
            // Delete and Backspace delete a picked annotation, and never pages: those go from the
            // thumbnails, or by the menu.
            let picked =
                (c.imp().document.borrow().as_ref()).is_some_and(|d| d.borrow().picked.is_some());
            if matches!(
                key,
                gdk::Key::Delete | gdk::Key::KP_Delete | gdk::Key::BackSpace
            ) && picked
            {
                let _ = c.activate_action("win.delete-pages", None);
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Escape && c.is_drafting() {
                c.draft(|d| d.cancel_draft());
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Escape && c.is_adjusting() {
                c.draft(|d| d.cancel_adjust());
                if let Some(cb) = c.imp().on_drawn.borrow().as_ref() {
                    cb();
                }
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Escape && (c.unpick_annotation() || c.clear_selection()) {
                return glib::Propagation::Stop;
            }
            let Some((continuous, height)) =
                c.with_viewer(|v| (v.layout_mode().is_continuous(), v.viewport().height))
            else {
                return glib::Propagation::Proceed;
            };
            let page = (height - 40.0).max(40.0);
            match key {
                gdk::Key::Page_Down | gdk::Key::space if continuous => {
                    c.update_viewer(|v| v.scroll_by(ViewVector::new(0.0, page)))
                }
                gdk::Key::Page_Up | gdk::Key::BackSpace if continuous => {
                    c.update_viewer(|v| v.scroll_by(ViewVector::new(0.0, -page)))
                }
                gdk::Key::Page_Down | gdk::Key::space | gdk::Key::Right => {
                    c.update_viewer(Viewer::next_page)
                }
                gdk::Key::Page_Up | gdk::Key::BackSpace | gdk::Key::Left => {
                    c.update_viewer(Viewer::previous_page)
                }
                gdk::Key::Down => c.update_viewer(|v| v.scroll_by(ViewVector::new(0.0, 50.0))),
                gdk::Key::Up => c.update_viewer(|v| v.scroll_by(ViewVector::new(0.0, -50.0))),
                gdk::Key::Home => c.jump(|v| v.go_to_page(0)),
                gdk::Key::End => c.jump(|v| v.go_to_page(v.page_count().saturating_sub(1))),
                _ => return glib::Propagation::Proceed,
            }
            glib::Propagation::Stop
        });
        self.add_controller(keys);
    }
}

/// Draws a page in `rect`: white, what `content` draws (its tiles), clipped to the page, and over
/// that the annotations the app draws itself (`overlay`, see `Session::overlay`).
///
/// A highlight darkens what is beneath it, so the page is the bottom of a blend with those.
pub fn draw_page(
    snapshot: &gtk::Snapshot,
    rect: &graphene::Rect,
    overlay: Vec<ViewShape>,
    content: impl FnOnce(&gtk::Snapshot),
) {
    let (darkening, covering): (Vec<ViewShape>, Vec<ViewShape>) = overlay
        .into_iter()
        .partition(|s| matches!(s, ViewShape::Fill { multiply: true, .. }));
    if !darkening.is_empty() {
        snapshot.push_blend(gsk::BlendMode::Multiply);
    }
    snapshot.append_color(&gdk::RGBA::WHITE, rect);
    snapshot.push_clip(rect);
    content(snapshot);
    snapshot.pop();
    if !darkening.is_empty() {
        snapshot.pop();
        snapshot.push_clip(rect);
        draw_shapes(snapshot, &darkening);
        snapshot.pop();
        snapshot.pop();
    }
    if !covering.is_empty() {
        snapshot.push_clip(rect);
        draw_shapes(snapshot, &covering);
        snapshot.pop();
    }
}

/// Draws the shapes of annotations, in order.
fn draw_shapes(snapshot: &gtk::Snapshot, shapes: &[ViewShape]) {
    let rgba = |c: specto_annot::Color, opacity: f32| gdk::RGBA::new(c.r, c.g, c.b, opacity);
    for shape in shapes {
        match shape {
            ViewShape::Fill {
                quad,
                color,
                opacity,
                ..
            } => fill_quads(snapshot, &[*quad], &rgba(*color, *opacity)),
            ViewShape::Stroke {
                from,
                to,
                width,
                color,
                opacity,
            } => {
                let builder = gsk::PathBuilder::new();
                builder.move_to(from.x as f32, from.y as f32);
                builder.line_to(to.x as f32, to.y as f32);
                snapshot.append_stroke(
                    &builder.to_path(),
                    &gsk::Stroke::new(*width as f32),
                    &rgba(*color, *opacity),
                );
            }
            ViewShape::Path {
                path,
                fill,
                stroke,
                opacity,
            } => {
                let builder = gsk::PathBuilder::new();
                builder.move_to(path.start.x as f32, path.start.y as f32);
                for segment in &path.segments {
                    match *segment {
                        Segment::Line(p) => builder.line_to(p.x as f32, p.y as f32),
                        Segment::Curve(a, b, p) => builder.cubic_to(
                            a.x as f32, a.y as f32, b.x as f32, b.y as f32, p.x as f32, p.y as f32,
                        ),
                    }
                }
                if path.closed {
                    builder.close();
                }
                let outline = builder.to_path();
                if let Some(fill) = fill {
                    snapshot.append_fill(&outline, gsk::FillRule::Winding, &rgba(*fill, *opacity));
                }
                if let Some((color, width)) = stroke {
                    // Round ends and corners, as the saved look has them.
                    let pen = gsk::Stroke::new(*width as f32);
                    pen.set_line_cap(gsk::LineCap::Round);
                    pen.set_line_join(gsk::LineJoin::Round);
                    snapshot.append_stroke(&outline, &pen, &rgba(*color, *opacity));
                }
            }
        }
    }
}

/// Fills quadrilaterals (view coordinates) with `color`.
fn fill_quads(snapshot: &gtk::Snapshot, quads: &[[ViewPoint; 4]], color: &gdk::RGBA) {
    if quads.is_empty() {
        return;
    }
    let builder = gsk::PathBuilder::new();
    for q in quads {
        builder.move_to(q[0].x as f32, q[0].y as f32);
        for p in &q[1..] {
            builder.line_to(p.x as f32, p.y as f32);
        }
        builder.close();
    }
    snapshot.append_fill(&builder.to_path(), gsk::FillRule::Winding, color);
}

/// Converts a rectangle in device pixels to the logical coordinates GTK snapshots use.
fn logical(r: DeviceRect, scale: f64) -> graphene::Rect {
    graphene::Rect::new(
        (r.origin.x / scale) as f32,
        (r.origin.y / scale) as f32,
        (r.size.width / scale) as f32,
        (r.size.height / scale) as f32,
    )
}

/// Logical offset that moves this widget's origin onto a whole device pixel of its surface.
///
/// At fractional scales the widget can sit at 31.25 device pixels, say (a window shadow of 25
/// pixels times 1.25), and then every tile is resampled between pixels. Measured at scale 1.25:
/// edge contrast 80.7 without this, identical to an exact render with it.
fn device_pixel_snap(widget: &impl IsA<gtk::Widget>, scale: f64) -> (f32, f32) {
    let Some(native) = widget.native() else {
        return (0.0, 0.0);
    };
    let Some(p) = widget.compute_point(&native, &graphene::Point::new(0.0, 0.0)) else {
        return (0.0, 0.0);
    };
    let (sx, sy) = native.surface_transform();
    let snap = |logical: f64| {
        let device = logical * scale;
        ((device.round() - device) / scale) as f32
    };
    (snap(p.x() as f64 + sx), snap(p.y() as f64 + sy))
}
