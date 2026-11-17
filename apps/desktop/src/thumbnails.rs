//! The thumbnail sidebar: a `GtkListView` of pages, so only visible rows exist.
//!
//! Several thumbnails can be selected (click, Shift+click, *Primary*+click), for editing them
//! together. Selecting a single one goes to its page, and a single selection follows the current
//! page. A selection of several stays until the user changes it.
//!
//! Thumbnails draw the viewer's placeholder tiles from the shared cache, so each page is rendered
//! once for both the sidebar and the canvas.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, gio, glib, graphene, gsk};

use crate::document::{PageBundle, SharedDocument};

/// Thumbnail width in logical pixels.
const WIDTH: f64 = 120.0;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Thumbnail {
        pub document: RefCell<Option<SharedDocument>>,
        pub page: Cell<Option<usize>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Thumbnail {
        const NAME: &'static str = "SpectoThumbnail";
        type Type = super::Thumbnail;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Thumbnail {}

    impl WidgetImpl for Thumbnail {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::ConstantSize
        }

        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let size = match orientation {
                gtk::Orientation::Horizontal => WIDTH,
                _ => self.aspect().map_or(WIDTH * 1.3, |a| WIDTH * a),
            }
            .round() as i32;
            (size, size, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let rect = graphene::Rect::new(0.0, 0.0, obj.width() as f32, obj.height() as f32);
            snapshot.append_outset_shadow(
                &gsk::RoundedRect::from_rect(rect, 0.0),
                &gdk::RGBA::new(0.0, 0.0, 0.0, 0.25),
                0.0,
                1.0,
                0.0,
                2.0,
            );
            let (Some(doc), Some(page)) = (self.document.borrow().clone(), self.page.get()) else {
                snapshot.append_color(&gdk::RGBA::WHITE, &rect);
                return;
            };
            let mut d = doc.borrow_mut();
            let key = d.viewer.thumbnail_key(page);
            // The annotations the app draws itself, as on the page view.
            let overlay = d
                .session
                .overlay_in(page, obj.width() as f64, obj.height() as f64);
            let texture = key.and_then(|key| d.cache.get(&key));
            crate::canvas::draw_page(snapshot, &rect, overlay, |snapshot| {
                if let Some(texture) = texture {
                    // Not `append_scaled_texture`: GSK renders that at logical resolution on HiDPI
                    // surfaces (see canvas.rs).
                    snapshot.append_texture(texture, &rect);
                }
            });
        }
    }

    impl Thumbnail {
        /// Height / width of the page as displayed.
        fn aspect(&self) -> Option<f64> {
            let doc = self.document.borrow().clone()?;
            let d = doc.borrow();
            let page = d.viewer.page_whole(self.page.get()?)?;
            let s = page.displayed().size();
            Some(s.height / s.width)
        }
    }
}

glib::wrapper! {
    pub struct Thumbnail(ObjectSubclass<imp::Thumbnail>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Thumbnail {
    fn new(document: SharedDocument) -> Self {
        let obj: Self = glib::Object::new();
        obj.set_halign(gtk::Align::Center);
        obj.imp().document.replace(Some(document));
        obj
    }

    fn set_page(&self, page: Option<usize>) {
        self.imp().page.set(page);
        self.queue_resize();
        self.queue_draw();
    }
}

/// What a thumbnail drag carries: the document it's from (as an address, only compared), the
/// positions of the pages dragged (to move them within it) and the pages themselves (to copy them
/// into another document).
struct DraggedPages {
    document: usize,
    pages: Vec<usize>,
    bundle: PageBundle,
}

/// Distance from the top or bottom edge, in pixels, within which a drag scrolls the sidebar.
const AUTOSCROLL_EDGE: f64 = 40.0;

/// The sidebar for one document.
pub struct Sidebar {
    pub widget: gtk::Overlay,
    scrolled: gtk::ScrolledWindow,
    list: gtk::ListView,
    /// One item per page. The items only stand for positions.
    model: gtk::StringList,
    document: SharedDocument,
    selection: gtk::MultiSelection,
    /// Set while the selection follows the canvas, so it doesn't jump back.
    following: Rc<Cell<bool>>,
    drops: Drops,
}

impl Sidebar {
    /// `on_activate` is called with a page index when the user picks a thumbnail, `on_move` with
    /// the pages dragged and the position they were dropped before (counted before the move), and
    /// `on_files` and `on_pages` with files, or pages of another document, dropped and the position
    /// they were dropped before.
    pub fn new(
        document: SharedDocument,
        on_activate: impl Fn(usize) + 'static,
        on_move: impl Fn(Vec<usize>, usize) + 'static,
        on_files: impl Fn(Vec<gio::File>, usize) + 'static,
        on_pages: impl Fn(PageBundle, usize) + 'static,
    ) -> Self {
        let count = document.borrow().viewer.page_count();
        let labels: Vec<String> = (1..=count).map(|i| i.to_string()).collect();
        let model = gtk::StringList::new(&labels.iter().map(String::as_str).collect::<Vec<_>>());
        let selection = gtk::MultiSelection::new(Some(model.clone()));
        let following = Rc::new(Cell::new(false));
        let doc_address = Rc::as_ptr(&document) as usize;
        let drops = Drops {
            pending: Rc::default(),
            on_move: Rc::new(on_move),
            on_files: Rc::new(on_files),
            on_pages: Rc::new(on_pages),
        };

        let factory = gtk::SignalListItemFactory::new();
        let doc = document.clone();
        let (sel, f, d) = (selection.clone(), following.clone(), drops.clone());
        factory.connect_setup(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("ListItem");
            let thumbnail = Thumbnail::new(doc.clone());
            let label = gtk::Label::new(None);
            label.add_css_class("caption");
            label.add_css_class("dim-label");
            let column = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(4)
                .margin_top(6)
                .margin_bottom(6)
                .build();
            column.append(&thumbnail);
            column.append(&label);
            column.add_controller(drag_source(item, &thumbnail, &sel, &f, &doc, &d));
            item.set_child(Some(&column));
        });
        let doc = document.clone();
        factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("ListItem");
            let page = item.position() as usize;
            let column = item.child().expect("child");
            let thumbnail = column
                .first_child()
                .and_downcast::<Thumbnail>()
                .expect("thumbnail");
            let label = thumbnail
                .next_sibling()
                .and_downcast::<gtk::Label>()
                .expect("label");
            thumbnail.set_page(Some(page));
            let mut d = doc.borrow_mut();
            label.set_label(&d.page_label(page));
            d.thumbnails_shown.insert(page);
            d.flush_requests();
        });
        let doc = document.clone();
        factory.connect_unbind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("ListItem");
            let column = item.child().expect("child");
            let thumbnail = column
                .first_child()
                .and_downcast::<Thumbnail>()
                .expect("thumbnail");
            if let Some(page) = thumbnail.imp().page.take() {
                doc.borrow_mut().thumbnails_shown.remove(&page);
            }
        });

        let list = gtk::ListView::new(Some(selection.clone()), Some(factory));
        list.add_css_class("navigation-sidebar");
        list.set_single_click_activate(false);

        let f = following.clone();
        selection.connect_selection_changed(move |s, _, _| {
            let selected = s.selection();
            if !f.get() && selected.size() == 1 {
                on_activate(selected.nth(0) as usize);
            }
        });

        let scrolled = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&list)
            .build();
        let widget = gtk::Overlay::builder().child(&scrolled).build();
        let indicator = DropIndicator::new(&widget);
        list.add_controller(drop_target(
            &list,
            &scrolled,
            indicator,
            doc_address,
            drops.clone(),
        ));
        Self {
            widget,
            scrolled,
            list,
            model,
            document,
            selection,
            following,
            drops,
        }
    }

    /// Does what dropping `pages` at window height `y` does, for tests: the virtual pointer of the
    /// headless checks can't drag reliably.
    pub fn drop_at(&self, pages: Vec<usize>, window: &impl IsA<gtk::Widget>, y: f64) {
        let Some(y) = window
            .compute_point(&self.list, &graphene::Point::new(0.0, y as f32))
            .map(|p| f64::from(p.y()))
        else {
            return;
        };
        if let Some((to, _)) = drop_place(&self.list, y) {
            let on_move = self.drops.on_move.clone();
            glib::idle_add_local_once(move || on_move(pages, to));
        }
    }

    /// Does what dropping pages of another document at window height `y` does, for tests.
    pub fn drop_bundle_at(&self, bundle: PageBundle, window: &impl IsA<gtk::Widget>, y: f64) {
        let y = window
            .compute_point(&self.list, &graphene::Point::new(0.0, y as f32))
            .map(|p| f64::from(p.y()));
        if let Some((to, _)) = y.and_then(|y| drop_place(&self.list, y)) {
            let on_pages = self.drops.on_pages.clone();
            glib::idle_add_local_once(move || on_pages(bundle, to));
        }
    }

    /// Does what dropping `files` at window height `y` does, for tests.
    pub fn drop_files_at(&self, files: Vec<gio::File>, window: &impl IsA<gtk::Widget>, y: f64) {
        let y = window
            .compute_point(&self.list, &graphene::Point::new(0.0, y as f32))
            .map(|p| f64::from(p.y()));
        if let Some((to, _)) = y.and_then(|y| drop_place(&self.list, y)) {
            (self.drops.on_files)(files, to);
        }
    }

    /// Shows the document's pages again after an edit: every row is bound anew, so thumbnails get
    /// their new size, rotation and label.
    ///
    /// Then `selected` are selected (none: the current page will be, through
    /// [`follow`](Self::follow)).
    pub fn pages_changed(&self, selected: &[usize]) {
        let count = self.document.borrow().viewer.page_count();
        let labels: Vec<String> = (1..=count).map(|i| i.to_string()).collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let scroll = self.scrolled.vadjustment().value();
        self.following.set(true);
        self.model.splice(0, self.model.n_items(), &labels);
        self.set_selected(selected);
        self.following.set(false);
        self.scrolled.vadjustment().set_value(scroll);
    }

    /// The selected pages, in order.
    pub fn selected(&self) -> Vec<usize> {
        let set = self.selection.selection();
        (0..set.size())
            .map(|i| set.nth(i as u32) as usize)
            .collect()
    }

    /// Selects every page (*Primary*+A in the sidebar).
    pub fn select_all(&self) {
        self.selection.select_all();
    }

    /// Selects `pages` (and nothing else) without going to them.
    pub fn select(&self, pages: &[usize]) {
        self.following.set(true);
        self.set_selected(pages);
        self.following.set(false);
    }

    fn set_selected(&self, pages: &[usize]) {
        let set = gtk::Bitset::new_empty();
        for &p in pages {
            set.add(p as u32);
        }
        let all = gtk::Bitset::new_range(0, self.model.n_items());
        self.selection.set_selection(&set, &all);
    }

    /// Whether `widget` is in the sidebar (to tell if it has the focus).
    pub fn contains(&self, widget: &gtk::Widget) -> bool {
        widget.is_ancestor(&self.widget)
    }

    /// Selects `page` and scrolls it into view, without reporting it back, unless several pages are
    /// selected: that selection stays.
    pub fn follow(&self, page: usize) {
        let selected = self.selected();
        if selected.len() > 1 || selected == [page] {
            return;
        }
        self.select(&[page]);
        if self.list.is_mapped() {
            self.list
                .scroll_to(page as u32, gtk::ListScrollFlags::NONE, None);
        }
    }

    /// The thumbnails on screen: page and bounds relative to `to`, for tests.
    pub fn visible(&self, to: &impl IsA<gtk::Widget>) -> Vec<(usize, graphene::Rect)> {
        let mut out = Vec::new();
        let mut child = self.list.first_child();
        while let Some(row) = child {
            if let Some(t) = row
                .first_child()
                .and_then(|col| col.first_child())
                .and_downcast::<Thumbnail>()
                && let Some(page) = t.imp().page.get()
                && t.is_mapped()
                && let Some(bounds) = t.compute_bounds(to)
            {
                out.push((page, bounds));
            }
            child = row.next_sibling();
        }
        out.sort_by_key(|(p, _)| *p);
        out
    }

    /// Redraws visible thumbnails (after new tiles arrived).
    pub fn refresh(&self) {
        let mut child = self.list.first_child();
        while let Some(row) = child {
            if let Some(t) = row
                .first_child()
                .and_then(|col| col.first_child())
                .and_downcast::<Thumbnail>()
            {
                t.queue_draw();
            }
            child = row.next_sibling();
        }
    }
}

/// Pages to move, and the position to move them before.
type Move = (Vec<usize>, usize);

/// A drop of thumbnails waiting to be carried out, and what does it.
///
/// The drop target only records the move. The drag source makes it once the drag has ended. Moving
/// rebuilds the list, and doing that while the compositor was still finishing the drag made the
/// next drag fail to start (seen with Mutter).
#[derive(Clone)]
struct Drops {
    pending: Rc<Cell<Option<Move>>>,
    on_move: Rc<dyn Fn(Vec<usize>, usize)>,
    /// Files dropped, inserted at once.
    on_files: Rc<dyn Fn(Vec<gio::File>, usize)>,
    /// Pages of another document dropped, inserted after the drop.
    on_pages: Rc<dyn Fn(PageBundle, usize)>,
}

/// Dragging a thumbnail drags the selected pages if it's one of them, else just its page (which
/// becomes the selection).
fn drag_source(
    item: &gtk::ListItem,
    thumbnail: &Thumbnail,
    selection: &gtk::MultiSelection,
    following: &Rc<Cell<bool>>,
    document: &SharedDocument,
    drops: &Drops,
) -> gtk::DragSource {
    let source = gtk::DragSource::new();
    // Moved within the document, copied into another one.
    source.set_actions(gdk::DragAction::MOVE | gdk::DragAction::COPY);
    let doc = Rc::downgrade(document);
    let drops = drops.clone();
    source.connect_drag_end(move |_, _, _| {
        if let Some((pages, to)) = drops.pending.take() {
            let on_move = drops.on_move.clone();
            glib::idle_add_local_once(move || on_move(pages, to));
        }
    });
    let item = item.downgrade();
    let (thumbnail, selection, following) = (
        thumbnail.downgrade(),
        selection.downgrade(),
        following.clone(),
    );
    source.connect_prepare(move |source, x, y| {
        let (item, selection) = (item.upgrade()?, selection.upgrade()?);
        let position = item.position();
        if position == gtk::INVALID_LIST_POSITION {
            return None;
        }
        if !selection.is_selected(position) {
            // Without going to the page: dragging doesn't move the view.
            following.set(true);
            selection.select_item(position, true);
            following.set(false);
        }
        let set = selection.selection();
        let pages: Vec<usize> = (0..set.size())
            .map(|i| set.nth(i as u32) as usize)
            .collect();
        let doc = doc.upgrade()?;
        let document = Rc::as_ptr(&doc) as usize;
        let bundle = doc.borrow().bundle(&pages);
        if let Some(t) = thumbnail.upgrade() {
            let icon = gtk::WidgetPaintable::new(Some(&t));
            source.set_icon(Some(&icon), x as i32, y as i32);
        }
        let value = glib::BoxedAnyObject::new(DraggedPages {
            document,
            pages,
            bundle,
        });
        Some(gdk::ContentProvider::for_value(&value.to_value()))
    });
    source
}

/// The line between thumbnails showing where a drop would put the pages.
#[derive(Clone)]
struct DropIndicator {
    line: gtk::DrawingArea,
    /// Its middle, in the overlay's coordinates.
    y: Rc<Cell<f64>>,
}

impl DropIndicator {
    fn new(overlay: &gtk::Overlay) -> Self {
        let line = gtk::DrawingArea::builder()
            .can_target(false)
            .visible(false)
            .build();
        // Drawn in the accent color (the "accent" class sets it as the widget's color).
        line.add_css_class("accent");
        line.set_draw_func(|area, cr, w, h| {
            let c = area.color();
            cr.set_source_rgba(
                c.red().into(),
                c.green().into(),
                c.blue().into(),
                c.alpha().into(),
            );
            let r = f64::from(h) / 2.0;
            cr.new_sub_path();
            cr.arc(
                r,
                r,
                r,
                std::f64::consts::FRAC_PI_2,
                3.0 * std::f64::consts::FRAC_PI_2,
            );
            cr.arc(
                f64::from(w) - r,
                r,
                r,
                -std::f64::consts::FRAC_PI_2,
                std::f64::consts::FRAC_PI_2,
            );
            cr.close_path();
            let _ = cr.fill();
        });
        overlay.add_overlay(&line);
        let y = Rc::new(Cell::new(0.0_f64));
        let at = y.clone();
        overlay.connect_get_child_position(move |overlay, child| {
            const THICKNESS: i32 = 4;
            let width = (WIDTH as i32 + 24).min(overlay.width());
            Some(gdk::Rectangle::new(
                (overlay.width() - width) / 2,
                at.get().round() as i32 - THICKNESS / 2,
                width.max(child.width()),
                THICKNESS,
            ))
        });
        Self { line, y }
    }

    fn show_at(&self, y: f64) {
        self.y.set(y);
        self.line.set_visible(true);
        if let Some(parent) = self.line.parent() {
            parent.queue_allocate();
        }
    }

    fn hide(&self) {
        self.line.set_visible(false);
    }
}

/// The rows on screen: page and bounds relative to `list`.
fn rows(list: &gtk::ListView) -> Vec<(usize, graphene::Rect)> {
    let mut out = Vec::new();
    let mut child = list.first_child();
    while let Some(row) = child {
        if let Some(t) = row
            .first_child()
            .and_then(|col| col.first_child())
            .and_downcast::<Thumbnail>()
            && let Some(page) = t.imp().page.get()
            // Rows kept off screen aren't laid out (no size).
            && row.is_mapped()
            && row.height() > 0
            && let Some(bounds) = row.compute_bounds(list)
        {
            out.push((page, bounds));
        }
        child = row.next_sibling();
    }
    out.sort_by_key(|(p, _)| *p);
    out
}

/// Where a drop at `y` (in the list) would put pages: before which position, and the boundary
/// between rows to show it at.
fn drop_place(list: &gtk::ListView, y: f64) -> Option<(usize, f64)> {
    let rows = rows(list);
    let y = y as f32;
    match rows.iter().find(|(_, r)| y < r.y() + r.height() / 2.0) {
        Some((page, r)) => Some((*page, f64::from(r.y()))),
        None => rows
            .last()
            .map(|(page, r)| (page + 1, f64::from(r.y() + r.height()))),
    }
}

/// Whether `drop` brings files (from a file manager), not thumbnails.
fn has_files(drop: &gdk::Drop) -> bool {
    drop.formats().contains_type(gdk::FileList::static_type())
}

/// The thumbnails `drop` brings, if any: what the drag in this process carries.
fn dragged(drop: &gdk::Drop) -> Option<glib::BoxedAnyObject> {
    drop.drag()?
        .content()
        .value(glib::BoxedAnyObject::static_type())
        .ok()?
        .get::<glib::BoxedAnyObject>()
        .ok()
}

/// Whether `object` holds thumbnails of the document at `document`.
fn from_document(object: &glib::BoxedAnyObject, document: usize) -> bool {
    object
        .try_borrow::<DraggedPages>()
        .is_ok_and(|d| d.document == document)
}

/// Takes thumbnails or files dropped on the list: shows where they'd go while dragging, scrolls
/// near the edges, and reports the move or the files.
fn drop_target(
    list: &gtk::ListView,
    scrolled: &gtk::ScrolledWindow,
    indicator: DropIndicator,
    document: usize,
    drops: Drops,
) -> gtk::DropTarget {
    let target = gtk::DropTarget::new(
        glib::Type::INVALID,
        gdk::DragAction::MOVE | gdk::DragAction::COPY,
    );
    target.set_types(&[
        glib::BoxedAnyObject::static_type(),
        gdk::FileList::static_type(),
    ]);
    // Files, or thumbnails of any window.
    target.connect_accept(move |_, drop| {
        has_files(drop) || dragged(drop).is_some_and(|b| b.try_borrow::<DraggedPages>().is_ok())
    });
    let autoscroll = Autoscroll::new(scrolled);
    let (l, ind, auto) = (list.downgrade(), indicator.clone(), autoscroll.clone());
    target.connect_motion(move |target, _, y| {
        let Some(list) = l.upgrade() else {
            return gdk::DragAction::empty();
        };
        let own = target
            .current_drop()
            .and_then(|d| dragged(&d))
            .is_some_and(|b| from_document(&b, document));
        let action = if own {
            gdk::DragAction::MOVE
        } else {
            gdk::DragAction::COPY
        };
        auto.set_speed(edge_speed(y, f64::from(list.height())));
        match drop_place(&list, y) {
            Some((_, at)) => {
                let at = list
                    .compute_point(
                        &ind.line.parent().expect("in the overlay"),
                        &graphene::Point::new(0.0, at as f32),
                    )
                    .map_or(at, |p| f64::from(p.y()));
                ind.show_at(at);
                action
            }
            None => {
                ind.hide();
                gdk::DragAction::empty()
            }
        }
    });
    let (ind, auto) = (indicator.clone(), autoscroll.clone());
    target.connect_leave(move |_| {
        ind.hide();
        auto.set_speed(0.0);
    });
    let l = list.downgrade();
    target.connect_drop(move |_, value, _, y| {
        indicator.hide();
        autoscroll.set_speed(0.0);
        let Some(list) = l.upgrade() else {
            return false;
        };
        let Some((to, _)) = drop_place(&list, y) else {
            return false;
        };
        if let Ok(files) = value.get::<gdk::FileList>() {
            (drops.on_files)(files.files(), to);
            return true;
        }
        let Ok(b) = value.get::<glib::BoxedAnyObject>() else {
            return false;
        };
        let Ok(dragged) = b.try_borrow::<DraggedPages>() else {
            return false;
        };
        if dragged.document != document {
            // Copied in, after the drop (opening files may take a moment).
            let (bundle, on_pages) = (dragged.bundle.clone(), drops.on_pages.clone());
            glib::idle_add_local_once(move || on_pages(bundle, to));
            return true;
        }
        let pages = dragged.pages.clone();
        drops.pending.set(Some((pages, to)));
        true
    });
    target
}

/// How fast to scroll (pixels per frame, negative up) with the pointer at `y` in a view `height`
/// high.
fn edge_speed(y: f64, height: f64) -> f64 {
    let from_bottom = height - y;
    if y < AUTOSCROLL_EDGE {
        -(AUTOSCROLL_EDGE - y.max(0.0)) / 2.0
    } else if from_bottom < AUTOSCROLL_EDGE {
        (AUTOSCROLL_EDGE - from_bottom.max(0.0)) / 2.0
    } else {
        0.0
    }
}

/// Scrolls a scrolled window every frame while a speed is set.
#[derive(Clone)]
struct Autoscroll {
    scrolled: glib::WeakRef<gtk::ScrolledWindow>,
    speed: Rc<Cell<f64>>,
    tick: Rc<RefCell<Option<gtk::TickCallbackId>>>,
}

impl Autoscroll {
    fn new(scrolled: &gtk::ScrolledWindow) -> Self {
        Self {
            scrolled: scrolled.downgrade(),
            speed: Rc::new(Cell::new(0.0)),
            tick: Rc::new(RefCell::new(None)),
        }
    }

    fn set_speed(&self, speed: f64) {
        self.speed.set(speed);
        if speed == 0.0 {
            if let Some(id) = self.tick.borrow_mut().take() {
                id.remove();
            }
            return;
        }
        if self.tick.borrow().is_some() {
            return;
        }
        let Some(scrolled) = self.scrolled.upgrade() else {
            return;
        };
        let speed = self.speed.clone();
        let id = scrolled.add_tick_callback(move |s, _| {
            let adj = s.vadjustment();
            adj.set_value(adj.value() + speed.get());
            glib::ControlFlow::Continue
        });
        self.tick.replace(Some(id));
    }
}
