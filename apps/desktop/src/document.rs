//! An open document: the session shared with the Android app, plus the tile cache of textures, used
//! by the canvas and the thumbnail sidebar of one window.
//!
//! What a document is (its files, the pages as edited, the viewer, text, selection and search) is
//! `specto_session::Session`, which a [`Document`] dereferences to. Here is what belongs to GTK:
//! the textures, which tiles are requested for the canvas and the sidebar, and how the search
//! indexer's results get to the main loop.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashSet};
use std::ops::{Deref, DerefMut, Range};
use std::rc::Rc;
use std::sync::{Arc, OnceLock, RwLock};

use gtk::prelude::*;
use gtk::{gdk, glib};
use specto_doc::{Edit, Rotation, SourceId};
use specto_render::OpenError;
use specto_render::scheduler::{Client, Scheduler};
use specto_render::tile::{TileCache, TileKey};
use specto_session::{Options, Session};
use specto_source::PdfData;
use specto_view::placeholder_level;

pub use specto_session::{Indexed, MAIN, PageBundle, SaveJob, TextLayer};

const CACHE_BUDGET: usize = 384 << 20;

pub type SharedDocument = Rc<RefCell<Document>>;

/// One scheduler (and worker pool) for the whole app.
///
/// Each document gets a client of it, so windows don't each start a pool or cancel each other's
/// requests.
fn scheduler() -> &'static Scheduler {
    static SCHEDULER: OnceLock<Scheduler> = OnceLock::new();
    SCHEDULER.get_or_init(|| {
        let threads = std::thread::available_parallelism().map_or(2, |n| n.get());
        Scheduler::new(threads, || {})
    })
}

pub struct Document {
    pub name: String,
    /// The document itself.
    ///
    /// Fields and methods of it are reached through the document (`d.viewer`, `d.search(...)`).
    /// Name it where the borrow checker has to see that it is apart from the cache.
    pub session: Session,
    pub cache: TileCache<gdk::Texture>,
    /// Placeholder levels in use: those tiles double as thumbnails, so the cache evicts them only
    /// after everything else.
    placeholder_levels: Arc<RwLock<BTreeSet<i32>>>,
    client: Client,
    /// Requests from the canvas's last frame, highest priority first.
    canvas_requests: Vec<TileKey>,
    /// Pages whose thumbnails are currently on screen in the sidebar.
    pub thumbnails_shown: BTreeSet<usize>,
    /// Tiles that failed to render, never requested again.
    failed: HashSet<TileKey>,
}

impl Deref for Document {
    type Target = Session;

    fn deref(&self) -> &Session {
        &self.session
    }
}

impl DerefMut for Document {
    fn deref_mut(&mut self) -> &mut Session {
        &mut self.session
    }
}

impl Document {
    /// Parses `data` and starts render workers.
    ///
    /// `wake` is called from a worker thread when tiles are ready. It should get the main loop to
    /// call [`receive_tiles`](Self::receive_tiles).
    pub fn open(
        name: String,
        data: PdfData,
        password: Option<&str>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, OpenError> {
        let session = Session::open(scheduler(), data, password, Options::default())?;
        let client = scheduler().client(wake);
        let levels: Arc<RwLock<BTreeSet<i32>>> = Arc::default();
        let evict_last = levels.clone();
        let doc = Self {
            name,
            session,
            cache: TileCache::new(CACHE_BUDGET).with_evict_last(move |k| {
                k.scale
                    .level()
                    .is_some_and(|l| evict_last.read().is_ok_and(|s| s.contains(&l)))
            }),
            placeholder_levels: levels,
            client,
            canvas_requests: Vec::new(),
            thumbnails_shown: BTreeSet::new(),
            failed: HashSet::new(),
        };
        doc.update_placeholder_levels();
        Ok(doc)
    }

    /// Opens another file to insert pages from.
    ///
    /// Returns its id and page count. Its pages are in the document once inserted.
    pub fn add_source(
        &mut self,
        data: PdfData,
        password: Option<&str>,
    ) -> Result<(SourceId, u32), OpenError> {
        self.session.add_source(scheduler(), data, password)
    }

    /// Whether page `i` is slow to render (huge images, or a render that took longer than the
    /// scheduler's `slow_after`).
    pub fn is_expensive(&self, i: usize) -> bool {
        self.viewer
            .pages()
            .get(i)
            .is_some_and(|p| scheduler().is_expensive(p.doc, p.page))
    }

    pub fn set_canvas_requests(&mut self, requests: Vec<TileKey>) {
        if requests != self.canvas_requests {
            self.canvas_requests = requests;
            self.flush_requests();
        }
    }

    /// Sends the combined request list: canvas first, then sidebar thumbnails.
    ///
    /// (`Scheduler::request` replaces the whole queue, so it must see both.)
    pub fn flush_requests(&self) {
        let thumbnails = self
            .thumbnails_shown
            .iter()
            .filter_map(|&i| self.viewer.thumbnail_key(i))
            .filter(|k| !self.cache.contains(k));
        let all = self.canvas_requests.iter().copied().chain(thumbnails);
        self.client
            .request(all.filter(|k| !self.failed.contains(k)));
    }

    /// Moves finished tiles into the cache. Returns whether any arrived.
    pub fn receive_tiles(&mut self) -> bool {
        let finished = self.client.take_finished();
        let any = !finished.is_empty();
        for f in finished {
            match f.result {
                Ok(bitmap) => {
                    let bytes = bitmap.data.len();
                    let texture = gdk::MemoryTexture::new(
                        bitmap.width as i32,
                        bitmap.height as i32,
                        gdk::MemoryFormat::R8g8b8a8Premultiplied,
                        &glib::Bytes::from_owned(bitmap.data),
                        bitmap.width as usize * 4,
                    );
                    self.cache.insert(f.key, texture.upcast(), bytes);
                }
                Err(e) => {
                    glib::g_warning!("specto", "rendering {:?} failed: {e}", f.key);
                    self.failed.insert(f.key);
                }
            }
        }
        any
    }

    /// Starts indexing the document's pages not indexed yet in a background thread, from position
    /// `from` onwards and then from the start, unless that is already running or done.
    ///
    /// The receiver gets each page's search text, to pass to `indexed`.
    pub fn start_indexing(&mut self, from: usize) -> Option<async_channel::Receiver<Indexed>> {
        let (tx, rx) = async_channel::unbounded();
        self.session
            .start_indexing(from, move |page| tx.send_blocking(page).is_ok())
            .then_some(rx)
    }

    /// Shows the pages with their margins trimmed, or whole. Returns whether that changed.
    pub fn set_trim_margins(&mut self, on: bool) -> bool {
        self.session.set_trim_margins(on) && self.pages_resized()
    }

    /// Measures the pages about to be shown, if they are shown trimmed and aren't measured yet.
    /// Returns whether pages changed size.
    pub fn trim_shown(&mut self) -> bool {
        self.session.trim_shown() && self.pages_resized()
    }

    // Editing
    //
    // The session's edits, each followed by what the tiles need.

    /// Turns the pages at `which` clockwise by `by`. Returns whether anything changed.
    pub fn rotate(&mut self, which: &[usize], by: Rotation) -> bool {
        self.session.rotate(which, by) && self.pages_changed()
    }

    /// Removes the pages at `which` (not all of them). Returns whether anything changed.
    pub fn delete(&mut self, which: &[usize]) -> bool {
        self.session.delete(which) && self.pages_changed()
    }

    /// Moves the pages at `which` before position `to`. Returns where they ended up.
    pub fn move_pages(&mut self, which: &[usize], to: usize) -> Option<Range<usize>> {
        let range = self.session.move_pages(which, to)?;
        self.pages_changed();
        Some(range)
    }

    /// Inserts all pages of each of `sources` (from [`add_source`](Self::add_source)), in order,
    /// before position `at`, as one edit. Returns where they ended up.
    pub fn insert(&mut self, at: usize, sources: &[SourceId]) -> Option<Range<usize>> {
        let range = self.session.insert(at, sources)?;
        self.pages_changed();
        Some(range)
    }

    /// Inserts the pages of `bundle` (from another document) before position `at`, as one edit.
    ///
    /// Returns where they ended up, or the error opening one of their files.
    pub fn insert_bundle(
        &mut self,
        at: usize,
        bundle: &PageBundle,
    ) -> Result<Option<Range<usize>>, OpenError> {
        let range = self.session.insert_bundle(scheduler(), at, bundle)?;
        if range.is_some() {
            self.pages_changed();
        }
        Ok(range)
    }

    /// Deletes the annotation that is picked (see the session's function of this name).
    ///
    /// Deleting one of the file's changes what its page is rendered from.
    pub fn remove_picked_annotation(&mut self) -> Result<bool, specto_write::WriteError> {
        let removed = self.session.remove_picked_annotation(scheduler())?;
        if removed {
            self.pages_changed();
        }
        Ok(removed)
    }

    /// Gives the picked annotation another color (see the session's function of this name).
    pub fn recolor_picked_annotation(
        &mut self,
        color: specto_annot::Color,
    ) -> Result<bool, specto_write::WriteError> {
        let changed = self.session.recolor_picked_annotation(scheduler(), color)?;
        if changed {
            self.pages_changed();
        }
        Ok(changed)
    }

    /// Starts moving or resizing the annotation at `p` (see the session's function of this name).
    /// `false` also if its page can't be shown without it.
    pub fn begin_adjust(&mut self, p: specto_geom::ViewPoint) -> bool {
        let began = self.session.begin_adjust(scheduler(), p).unwrap_or(false);
        if began {
            self.pages_changed();
        }
        began
    }

    /// Ends moving or resizing and returns whether the annotation changed.
    pub fn finish_adjust(&mut self) -> bool {
        let changed = self.session.finish_adjust(scheduler());
        self.pages_changed();
        changed
    }

    /// Gives up moving or resizing.
    pub fn cancel_adjust(&mut self) {
        self.session.cancel_adjust(scheduler());
        self.pages_changed();
    }

    /// Puts a text box at `p` (see the session's function of this name).
    pub fn add_text_box(
        &mut self,
        p: specto_geom::ViewPoint,
        text: &str,
        style: (f64, specto_annot::TextFont),
        color: specto_annot::Color,
    ) -> bool {
        let added = self
            .session
            .add_text_box(scheduler(), p, text, style, color);
        if added {
            self.pages_changed();
        }
        added
    }

    /// Puts a picture on the page at `p` (see the session's function of this name).
    pub fn add_stamp(
        &mut self,
        p: specto_geom::ViewPoint,
        width: f64,
        image: std::sync::Arc<specto_annot::StampImage>,
    ) -> bool {
        let added = self.session.add_stamp(scheduler(), p, width, image);
        if added {
            self.pages_changed();
        }
        added
    }

    /// A click at `p` on a form field (see the session's function of this name).
    pub fn click_field(&mut self, p: specto_geom::ViewPoint) -> Option<specto_session::FieldClick> {
        let clicked = self.session.click_field(scheduler(), p);
        if clicked == Some(specto_session::FieldClick::Changed) {
            self.pages_changed();
        }
        clicked
    }

    /// Tab or Shift+Tab on the form (see the session's function of this name).
    pub fn tab_field(&mut self, forward: bool) -> Option<specto_session::FieldClick> {
        self.session.tab_field(scheduler(), forward)
    }

    /// Space or Enter on the field that has the keyboard (see the session's function of this
    /// name).
    pub fn press_focused_field(&mut self) -> Option<specto_session::FieldClick> {
        let clicked = self.session.press_focused_field(scheduler());
        if clicked == Some(specto_session::FieldClick::Changed) {
            self.pages_changed();
        }
        clicked
    }

    /// Fills text into a text field of the form (see the session's function of this name).
    pub fn set_field_text(&mut self, field: specto_doc::FieldId, text: &str) -> bool {
        let changed = self.session.set_field_text(scheduler(), field, text);
        if changed {
            self.pages_changed();
        }
        changed
    }

    /// Sets the form back to what it holds at first (see the session's function of this name).
    pub fn reset_form(&mut self) -> bool {
        let changed = self.session.reset_form(scheduler());
        if changed {
            self.pages_changed();
        }
        changed
    }

    /// Chooses options of a list that takes several (see the session's function of this name).
    pub fn choose_options(&mut self, field: specto_doc::FieldId, chosen: &[usize]) -> bool {
        let changed = self.session.choose_options(scheduler(), field, chosen);
        if changed {
            self.pages_changed();
        }
        changed
    }

    /// Chooses an option of a choice field (see the session's function of this name).
    pub fn choose_option(&mut self, field: specto_doc::FieldId, index: usize) -> bool {
        let changed = self.session.choose_option(scheduler(), field, index);
        if changed {
            self.pages_changed();
        }
        changed
    }

    /// Starts typing into a text field of the form (see the session's function of this name).
    pub fn begin_field_text(&mut self, field: specto_doc::FieldId) {
        self.session.begin_field_text(scheduler(), field);
        self.pages_changed();
    }

    /// Ends typing into a text field of the form, with its new text or without a change.
    pub fn end_field_text(&mut self, text: Option<&str>) -> bool {
        let changed = self.session.end_field_text(scheduler(), text);
        self.pages_changed();
        changed
    }

    /// Starts typing into the picked text box (see the session's function of this name).
    pub fn begin_text_edit(&mut self) -> Option<(specto_session::TextBoxPlace, String)> {
        let editing = self.session.begin_text_edit(scheduler());
        if editing.is_some() {
            self.pages_changed();
        }
        editing
    }

    /// Ends typing into the picked text box, with its new text or without a change.
    pub fn end_text_edit(
        &mut self,
        text: Option<(&str, (f64, specto_annot::TextFont))>,
    ) -> Result<bool, specto_write::WriteError> {
        let changed = self.session.end_text_edit(scheduler(), text);
        self.pages_changed();
        changed
    }

    /// Sets the picked text box in another size and font (see the session's function of this name).
    pub fn restyle_picked_text_box(
        &mut self,
        style: (f64, specto_annot::TextFont),
    ) -> Result<bool, specto_write::WriteError> {
        let changed = self.session.restyle_picked_text_box(scheduler(), style)?;
        if changed {
            self.pages_changed();
        }
        Ok(changed)
    }

    /// Makes a text box at `place` (see `Session::add_text_box_on`).
    pub fn add_text_box_at(&mut self, place: &specto_session::TextBoxPlace, text: &str) -> bool {
        let added = self.session.add_text_box_on(
            scheduler(),
            place.position,
            place.at,
            place.width,
            text,
            (place.size, place.font),
            place.color,
        );
        if added {
            self.pages_changed();
        }
        added
    }

    /// Gives the picked annotation another text (see the session's function of this name).
    pub fn set_picked_text(&mut self, text: &str) -> Result<bool, specto_write::WriteError> {
        let changed = self.session.set_picked_text(scheduler(), text)?;
        if changed {
            self.pages_changed();
        }
        Ok(changed)
    }

    pub fn undo(&mut self) -> Option<Edit> {
        let edit = self.session.undo(scheduler())?;
        self.pages_changed();
        Some(edit)
    }

    pub fn redo(&mut self) -> Option<Edit> {
        let edit = self.session.redo(scheduler())?;
        self.pages_changed();
        Some(edit)
    }

    /// Updates the placeholder levels the tile cache keeps longest: those of the pages as laid out,
    /// and of the pages whole for their thumbnails.
    fn update_placeholder_levels(&self) {
        let v = &self.viewer;
        let levels = (v.pages().iter().copied())
            .chain((0..v.page_count()).filter_map(|i| v.page_whole(i)))
            .filter_map(|p| placeholder_level(&p.displayed()))
            .collect();
        if let Ok(mut l) = self.placeholder_levels.write() {
            *l = levels;
        }
    }

    /// The pages changed size, but are the same pages in the same places.
    fn pages_resized(&mut self) -> bool {
        self.update_placeholder_levels();
        true
    }

    /// The session's page list changed: updates what here depends on it.
    fn pages_changed(&mut self) -> bool {
        self.update_placeholder_levels();
        self.thumbnails_shown.clear();
        true
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        self.session.close(scheduler());
    }
}
