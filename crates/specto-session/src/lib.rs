//! An open document, as both apps hold it: the files its pages come from, the page list as edited,
//! the viewer, the text layer, the selection and the search.
//!
//! No toolkit and no pixels: what a frontend adds is its tile cache and how it gets called back.
//!
//! The pages shown come from an [`EditDoc`], so they can be rotated, deleted, moved and inserted,
//! including pages of other files. Each file is a source, and the first ([`MAIN`]) is the one
//! opened, whose outline the document has. Each page keeps its page label from its file. Positions
//! in the viewer are where a page is now. Caches of what doesn't change with editing (text, search
//! text, links) are keyed by the page of the file it comes from.
//!
//! A session doesn't own the render scheduler: the desktop app shares one between its windows, the
//! Android app has one per document. The calls that open or close files take it as an argument.

mod annots;
mod draft;
mod forms;
mod preview;
mod search;
mod text;
mod trim;

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

use specto_annot::Annotation;
use specto_doc::{AnnotId, Edit, EditDoc, PageAnnotChanges, PageRef, Rotation, SourceId, Version};
use specto_geom::{Crop, PageGeometry, PdfBox, ViewPoint};
use specto_render::OpenError;
use specto_render::scheduler::Scheduler;
use specto_render::tile::DocId;
use specto_source::{Link, NamedAction, PageLabels, PdfData, SourceDocument, Target};
use specto_text::SearchText;
use specto_view::{Selection, Settings, ViewPage, Viewer};

pub use annots::{DraftKind, Grip, TextBoxPlace, ViewShape};
pub use draft::{Draft, RestoreError};
pub use forms::{FieldClick, FieldHover};
pub use preview::LinkPreview;
pub use search::{Indexed, SearchStatus};
pub use text::TextLayer;

use search::{Indexer, Search};
use text::TextCache;

/// The file a document is opened from, which is its first source.
pub const MAIN: SourceId = SourceId(0);

/// A page of one of the files.
type FilePage = (SourceId, u32);

/// What following a link or an outline entry did ([`Session::follow`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Followed {
    /// Nothing: the page it leads to was deleted, or isn't in the file.
    Nothing,
    /// The view went to where it leads in the document, which may be where it was already, such as
    /// the next page from the last.
    Moved,
    /// It leads outside the document. Opening it, after asking, is up to the app.
    Uri(String),
}

/// What a frontend chooses when opening a document.
#[derive(Clone, Debug)]
pub struct Options {
    /// The viewer's settings: gaps, margins and the like.
    pub settings: Settings,
    /// Full text pages kept (for selection and highlights), the least recently used going first.
    ///
    /// A page takes about 100 bytes per character, so not a whole long document. Search uses the
    /// compact `SearchText` instead.
    pub text_pages: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            text_pages: 64,
        }
    }
}

/// A file the document's pages come from.
struct Source {
    /// Structure (outline, links, metadata), separate from rendering.
    doc: SourceDocument,
    /// The file and its password, for the search indexer and saving.
    data: PdfData,
    password: Option<String>,
    /// The scheduler's name for it.
    doc_id: DocId,
    /// The geometry of each page of the file.
    geometries: Vec<PageGeometry>,
    /// Links per page of the file, parsed on first use.
    links: HashMap<u32, Vec<Link>>,
    /// The annotations each page has in the file, read on first use.
    annots: HashMap<u32, Arc<[Arc<Annotation>]>>,
    /// The file's page labels, if it has any that differ from 1, 2, 3 and so on.
    labels: Option<PageLabels>,
    /// Every page's text prepared for searching, filled in the background.
    index: Vec<Option<SearchText>>,
    /// The box of what pages draw (`None` for a page that draws nothing), for trimming margins.
    /// Measured when a page is first shown trimmed.
    content: HashMap<u32, Option<PdfBox>>,
}

impl Source {
    fn open(
        scheduler: &Scheduler,
        data: PdfData,
        password: Option<&str>,
    ) -> Result<Self, OpenError> {
        let opened = scheduler.open(data.clone(), password)?;
        let doc = match SourceDocument::open(data.clone(), password) {
            Ok(s) => s,
            Err(e) => {
                scheduler.close(opened.id);
                return Err(e);
            }
        };
        Ok(Self {
            labels: doc.page_labels().filter(|l| !l.is_plain()),
            doc,
            data,
            password: password.map(str::to_owned),
            doc_id: opened.id,
            index: vec![None; opened.pages.len()],
            geometries: opened.pages,
            links: HashMap::new(),
            annots: HashMap::new(),
            content: HashMap::new(),
        })
    }
}

pub struct Session {
    /// The files pages come from, by [`SourceId`]: [`MAIN`] first, then files pages were inserted
    /// from. Kept while undo can bring their pages back.
    sources: Vec<Source>,
    /// Page labels, if the document has any that differ from 1, 2, 3 and so on.
    ///
    /// Each page keeps the label it has in its file, through edits (see `PageLabels::of_pages`).
    labels: Option<PageLabels>,
    /// The pages as edited.
    edit: EditDoc,
    pub viewer: Viewer,
    /// Text layers of recently used pages, extracted on first use.
    texts: TextCache,
    /// The text selection, if any.
    pub selection: Option<Selection>,
    /// Pages rendered from a copy of theirs instead of from their file (see [`annots`]), by page of
    /// the file.
    variants: HashMap<FilePage, annots::Variant>,
    /// The annotation being drawn with the pointer, if any.
    draft: Option<annots::Draft>,
    /// The annotation being moved or resized with the pointer, if any.
    adjust: Option<annots::Adjust>,
    /// An annotation the renderer draws that is taken out of its page for the time being, because
    /// it is being moved, resized or typed into.
    held: Option<annots::Held>,
    /// The fields of the main file's form, read when first asked for.
    form: Option<Arc<[specto_forms::Field]>>,
    /// The text field of the form that is being typed into, if any: shown empty meanwhile.
    typing_field: Option<specto_doc::FieldId>,
    /// The field the keyboard is on, and which of its widgets: see `forms`.
    field_focus: Option<(specto_doc::FieldId, usize)>,
    /// The text box that is being typed into, if any: the position of its page, and which it
    /// is. What is typed goes to this one, whatever is picked by then.
    typing_into: Option<(usize, AnnotId)>,
    /// The annotation that is picked (to delete it, later to change it), if any: the position of
    /// its page, and which it is (from [`annotation_at`](Self::annotation_at)).
    pub picked: Option<(usize, AnnotId)>,
    /// The indexer thread, if one was started and not stopped.
    indexer: Option<Arc<Indexer>>,
    /// The current search, if any.
    search: Option<Search>,
    /// The margins of pages are trimmed in the view.
    trim: bool,
}

impl Session {
    /// Parses `data` and registers it with `scheduler` for rendering.
    ///
    /// Before the session is dropped, [`close`](Self::close) should take its files out of the
    /// scheduler again.
    pub fn open(
        scheduler: &Scheduler,
        data: PdfData,
        password: Option<&str>,
        options: Options,
    ) -> Result<Self, OpenError> {
        let source = Source::open(scheduler, data, password)?;
        let labels = source.labels.clone();
        let edit = EditDoc::from_source(MAIN, source.geometries.len() as u32);
        let sources = vec![source];
        let variants = HashMap::new();
        let pages = view_pages(&edit, &sources, &variants, &[]);
        let mut session = Self {
            sources,
            labels,
            edit,
            viewer: Viewer::new(pages, options.settings),
            texts: TextCache::new(options.text_pages),
            selection: None,
            variants,
            draft: None,
            adjust: None,
            held: None,
            form: None,
            typing_field: None,
            field_focus: None,
            typing_into: None,
            picked: None,
            indexer: None,
            search: None,
            trim: false,
        };
        // Fields whose values the file leaves to the reader to show get their looks now.
        if session.form_fields().iter().any(|f| f.unshown) {
            session.sync_variants(scheduler);
        }
        Ok(session)
    }

    /// Opens another file to insert pages from.
    ///
    /// Returns its id and page count. Its pages are in the document once inserted.
    pub fn add_source(
        &mut self,
        scheduler: &Scheduler,
        data: PdfData,
        password: Option<&str>,
    ) -> Result<(SourceId, u32), OpenError> {
        let source = Source::open(scheduler, data, password)?;
        let id = SourceId(self.sources.len() as u32);
        let count = source.geometries.len() as u32;
        self.sources.push(source);
        // The indexer has a list of pages to do, so start over to include the new ones.
        self.stop_indexing();
        Ok((id, count))
    }

    /// Takes the session's files out of `scheduler`, the one it was opened with, and stops the
    /// search indexer. Nothing of the session renders afterwards.
    pub fn close(&mut self, scheduler: &Scheduler) {
        for s in &self.sources {
            scheduler.close(s.doc_id);
        }
        for (_, v) in self.variants.drain() {
            scheduler.close(v.doc_id);
        }
        self.stop_indexing();
    }

    /// The file opened, parsed: its metadata, outline and so on.
    pub fn main(&self) -> &SourceDocument {
        &self.sources[0].doc
    }

    /// The outline of the file opened.
    pub fn outline(&self) -> Vec<specto_source::OutlineItem> {
        self.main().outline()
    }

    /// The pages as edited.
    ///
    /// They change through the session only, which keeps the viewer and everything else in step.
    pub fn edit(&self) -> &EditDoc {
        &self.edit
    }

    /// Records that the state at `version` (see [`SaveJob::version`]) is what is on disk now.
    pub fn mark_saved(&mut self, version: Version) {
        self.edit.mark_saved_version(version);
    }

    /// The viewer, the text layer and the selection, borrowed together, as the selection functions
    /// need them all.
    pub fn text(&mut self) -> (&Viewer, TextLayer<'_>, &mut Option<Selection>) {
        (
            &self.viewer,
            TextLayer {
                sources: &self.sources,
                pages: self.edit.pages(),
                cache: &mut self.texts,
            },
            &mut self.selection,
        )
    }

    /// The selected text, if anything is selected.
    pub fn selected_text(&mut self) -> Option<String> {
        let (_, mut texts, selection) = self.text();
        let text = selection.as_ref()?.text(&mut texts);
        (!text.is_empty()).then_some(text)
    }

    /// The document's own page numbering (such as "iv" or "A-3"), if it has one that differs from
    /// 1, 2, 3 and so on, and its pages are still those of the file, in their original order.
    pub fn labels(&self) -> Option<&PageLabels> {
        self.labels.as_ref()
    }

    /// The label of page `i` (such as "iv" or "A-3"), or its number.
    pub fn page_label(&self, i: usize) -> String {
        self.labels
            .as_ref()
            .map_or_else(|| (i + 1).to_string(), |l| l.label(i))
    }

    /// The page the user means by `text`: a label, else a page number.
    pub fn find_page(&self, text: &str) -> Option<usize> {
        let count = self.viewer.page_count();
        self.labels.as_ref().and_then(|l| l.find(text)).or_else(|| {
            text.trim()
                .parse::<usize>()
                .ok()
                .filter(|n| (1..=count).contains(n))
                .map(|n| n - 1)
        })
    }

    /// The target of the link under `p` (viewport coordinates), if any.
    ///
    /// Destinations name pages of the file the link is in, which is returned with it.
    pub fn link_at(&mut self, p: ViewPoint) -> Option<(SourceId, Target)> {
        let (position, link) = self.link_under(p)?;
        Some((self.edit.pages().get(position)?.source, link.target))
    }

    /// Does what a link or an outline entry of file `source` asks for.
    ///
    /// Where the reader was is remembered for going back, except when only turning a page.
    pub fn follow(&mut self, source: SourceId, target: &Target) -> Followed {
        match target {
            Target::Destination(d) => {
                // Destinations name pages of the file, which may have moved or been deleted since.
                let Some(page) = self.position_of(source, d.page) else {
                    return Followed::Nothing;
                };
                self.viewer
                    .jump(|v| v.go_to_pdf_position(page, d.left, d.top));
            }
            Target::Named(NamedAction::NextPage) => self.viewer.next_page(),
            Target::Named(NamedAction::PrevPage) => self.viewer.previous_page(),
            Target::Named(NamedAction::FirstPage) => {
                self.viewer.jump(|v| v.go_to_page(0));
            }
            Target::Named(NamedAction::LastPage) => {
                self.viewer
                    .jump(|v| v.go_to_page(v.page_count().saturating_sub(1)));
            }
            Target::Uri(uri) => return Followed::Uri(uri.clone()),
        }
        Followed::Moved
    }

    /// The link under `p` (viewport coordinates), and the position of the page it is on.
    fn link_under(&mut self, p: ViewPoint) -> Option<(usize, Link)> {
        let (page, at) = self.viewer.hit_test(p)?;
        let view_page = self.viewer.pages().get(page)?;
        let pdf = view_page.displayed().page_to_pdf().transform_point(at);
        let file_page = *self.edit.pages().get(page)?;
        let source = self.sources.get_mut(file_page.source.0 as usize)?;
        let doc = &source.doc;
        let links = source
            .links
            .entry(file_page.page)
            .or_insert_with(|| doc.links(file_page.page as usize));
        // The topmost link is the last one drawn.
        let link = links.iter().rev().find(|l| l.contains(pdf))?;
        Some((page, link.clone()))
    }
}

/// The viewer's pages for the pages of `edit`: each rendered from its file, or from the copy of it
/// in `variants`, which has the one page. Each has the crop in `crops` at its position, if any.
fn view_pages(
    edit: &EditDoc,
    sources: &[Source],
    variants: &HashMap<FilePage, annots::Variant>,
    crops: &[Option<Crop>],
) -> Vec<ViewPage> {
    edit.pages()
        .iter()
        .enumerate()
        .map(|(position, p)| {
            let source = &sources[p.source.0 as usize];
            let (doc, page) = match variants.get(&(p.source, p.page)) {
                Some(v) => (v.doc_id, 0),
                None => (source.doc_id, p.page),
            };
            ViewPage {
                doc,
                page,
                geometry: source.geometries[p.page as usize],
                crop: crops.get(position).copied().flatten(),
                rotation: p.rotation,
            }
        })
        .collect()
}

/// Pages taken out of a session to put into another one, as when dragging pages from one window to
/// another: the files they come from (the data is shared, not copied) and each page with the
/// rotation the user gave it.
#[derive(Clone, Debug)]
pub struct PageBundle {
    /// Each file and its password.
    files: Vec<(PdfData, Option<String>)>,
    /// Index in `files`, page of that file, rotation.
    pages: Vec<(usize, u32, Rotation)>,
    /// What was filled into the form of the document's main file, and which of `files` that is,
    /// if pages of it are in the bundle. For rendering those pages as filled in (printing).
    /// Another document doesn't take it over with the pages: its form is its own file's.
    form: Option<(usize, Vec<(specto_doc::FieldId, specto_doc::FieldValue)>)>,
    /// What was done to the annotations of those pages: index in `files`, page of that file, the
    /// changes. Only pages with any.
    annots: Vec<(usize, u32, PageAnnotChanges)>,
}

impl PageBundle {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// The files, each with its password.
    pub fn files(&self) -> &[(PdfData, Option<String>)] {
        &self.files
    }

    /// The pages: index in [`files`](Self::files), page of that file, and the rotation the user
    /// gave it.
    pub fn pages(&self) -> &[(usize, u32, Rotation)] {
        &self.pages
    }

    /// The page at `position` of [`pages`](Self::pages) as a file of its own, unrotated and not
    /// encrypted, if anything was done to its annotations: for rendering it as edited, which its
    /// own file can't give (printing). `None` if the page is as in its file.
    pub fn annotated_page(
        &self,
        position: usize,
    ) -> Option<Result<Vec<u8>, specto_write::WriteError>> {
        let &(file, page, _) = self.pages.get(position)?;
        let changes = self
            .annots
            .iter()
            .find(|(f, p, _)| (*f, *p) == (file, page))
            .map(|(_, _, changes)| changes.clone());
        // What was filled into the form goes into every page of its file: which fields a page
        // shows isn't known here, and a page without any is none the worse for it.
        let form: &[_] = match &self.form {
            Some((of, values)) if *of == file => values,
            _ => &[],
        };
        if changes.is_none() && form.is_empty() {
            return None;
        }
        let (data, password) = &self.files[file];
        let source = specto_write::Source {
            id: MAIN,
            data,
            password: password.as_deref(),
        };
        Some(specto_write::write_filled(
            &[source],
            &[PageRef::new(MAIN, page)],
            &HashMap::from([((MAIN, page), changes.unwrap_or_default())]),
            form,
            false,
        ))
    }
}

/// What saving needs, taken from the session so the file can be written on another thread.
pub struct SaveJob {
    /// Each source's file and password, the main one first.
    files: Vec<(SourceId, PdfData, Option<String>)>,
    pages: Vec<PageRef>,
    /// What was done to the annotations of those pages.
    annots: specto_write::AnnotChanges,
    /// What was filled into the form.
    form: Vec<(specto_doc::FieldId, specto_doc::FieldValue)>,
    /// The state being saved, to mark as saved when done.
    pub version: Version,
}

impl SaveJob {
    /// The new file's bytes.
    pub fn write(&self) -> Result<Vec<u8>, specto_write::WriteError> {
        specto_write::write_saved(&self.sources(), &self.pages, &self.annots, &self.form)
    }

    /// The new file's bytes without the document's password protection: for a print service, which
    /// can't be given a password.
    pub fn write_unprotected(&self) -> Result<Vec<u8>, specto_write::WriteError> {
        specto_write::write_filled(
            &self.sources(),
            &self.pages,
            &self.annots,
            &self.form,
            false,
        )
    }

    /// Writes the new file to `path`, replacing it safely if it exists.
    pub fn save(&self, path: &std::path::Path) -> Result<(), specto_write::WriteError> {
        specto_write::save_edited(path, &self.sources(), &self.pages, &self.annots, &self.form)
    }

    fn sources(&self) -> Vec<specto_write::Source<'_>> {
        self.files
            .iter()
            .map(|(id, data, password)| specto_write::Source {
                id: *id,
                data,
                password: password.as_deref(),
            })
            .collect()
    }
}

impl Session {
    // Editing

    /// A copy of what's needed to save the document as it is now.
    pub fn save_job(&self) -> SaveJob {
        self.save_pages(self.edit.pages().to_vec())
    }

    /// A copy of what's needed to save `pages` (the main file's catalog comes along).
    pub fn save_pages(&self, pages: Vec<PageRef>) -> SaveJob {
        let used: HashSet<SourceId> = pages.iter().map(|p| p.source).collect();
        let files = self
            .sources
            .iter()
            .enumerate()
            .map(|(i, s)| (SourceId(i as u32), s))
            .filter(|(id, _)| *id == MAIN || used.contains(id))
            .map(|(id, s)| (id, s.data.clone(), s.password.clone()))
            .collect();
        let annots = pages
            .iter()
            .map(|p| (p.source, p.page))
            .collect::<HashSet<_>>()
            .into_iter()
            .map(|(source, page)| ((source, page), self.edit.annotation_changes(source, page)))
            .filter(|(_, changes)| !changes.is_empty())
            .collect();
        SaveJob {
            files,
            pages,
            annots,
            form: self.edit.form_values(),
            version: self.edit.version(),
        }
    }

    /// The pages at `which` (in document order), to insert into another session with
    /// [`insert_bundle`](Self::insert_bundle).
    pub fn bundle(&self, which: &[usize]) -> PageBundle {
        let mut which: Vec<usize> = which.to_vec();
        which.sort_unstable();
        which.dedup();
        let mut files: Vec<(PdfData, Option<String>)> = Vec::new();
        let mut file_of: HashMap<SourceId, usize> = HashMap::new();
        let mut pages = Vec::new();
        let mut annots: Vec<(usize, u32, PageAnnotChanges)> = Vec::new();
        for p in which.iter().filter_map(|&i| self.edit.pages().get(i)) {
            let file = *file_of.entry(p.source).or_insert_with(|| {
                let s = &self.sources[p.source.0 as usize];
                files.push((s.data.clone(), s.password.clone()));
                files.len() - 1
            });
            pages.push((file, p.page, p.rotation));
            let changes = self.edit.annotation_changes(p.source, p.page);
            let known = annots.iter().any(|(f, n, _)| (*f, *n) == (file, p.page));
            if !changes.is_empty() && !known {
                annots.push((file, p.page, changes));
            }
        }
        let filled = self.edit.form_values();
        let form = file_of
            .get(&MAIN)
            .filter(|_| !filled.is_empty())
            .map(|&file| (file, filled));
        PageBundle {
            files,
            pages,
            form,
            annots,
        }
    }

    /// Inserts the pages of `bundle` (from another session) before position `at`, as one edit,
    /// keeping their rotation and what was done to their annotations there (unless this session has
    /// edited the annotations of the same page of the same file itself).
    ///
    /// Files the session already has (the same data) are reused, the others opened with
    /// `scheduler`. Returns where the pages ended up, or the error opening a file (then nothing is
    /// inserted).
    pub fn insert_bundle(
        &mut self,
        scheduler: &Scheduler,
        at: usize,
        bundle: &PageBundle,
    ) -> Result<Option<Range<usize>>, OpenError> {
        let mut ids = Vec::with_capacity(bundle.files.len());
        for (data, password) in &bundle.files {
            let known = self
                .sources
                .iter()
                .position(|s| s.data.same(data))
                .map(|i| SourceId(i as u32));
            let id = match known {
                Some(id) => id,
                None => {
                    self.add_source(scheduler, data.clone(), password.as_deref())?
                        .0
                }
            };
            ids.push(id);
        }
        let refs = bundle
            .pages
            .iter()
            .map(|&(file, page, rotation)| PageRef {
                source: ids[file],
                page,
                rotation,
            })
            .collect();
        let annots = bundle
            .annots
            .iter()
            .map(|(file, page, changes)| (ids[*file], *page, changes.clone()))
            .collect();
        let range = self.edit.insert_annotated(at, refs, annots);
        if range.is_some() {
            self.pages_changed();
            self.sync_variants(scheduler);
        }
        Ok(range)
    }

    /// Turns the pages at `which` clockwise by `by`. Returns whether anything changed.
    pub fn rotate(&mut self, which: &[usize], by: Rotation) -> bool {
        self.edit.rotate(which, by) && self.pages_changed()
    }

    /// Removes the pages at `which` (not all of them). Returns whether anything changed.
    pub fn delete(&mut self, which: &[usize]) -> bool {
        self.edit.delete(which) && self.pages_changed()
    }

    /// Moves the pages at `which` before position `to`. Returns where they ended up.
    pub fn move_pages(&mut self, which: &[usize], to: usize) -> Option<Range<usize>> {
        let range = self.edit.move_pages(which, to)?;
        self.pages_changed();
        Some(range)
    }

    /// Inserts all pages of each of `sources` (from [`add_source`](Self::add_source)), in order,
    /// before position `at`, as one edit. Returns where they ended up.
    pub fn insert(&mut self, at: usize, sources: &[SourceId]) -> Option<Range<usize>> {
        let mut refs = Vec::new();
        for &source in sources {
            let count = self.sources.get(source.0 as usize)?.geometries.len() as u32;
            refs.extend((0..count).map(|p| PageRef::new(source, p)));
        }
        let range = self.edit.insert(at, refs)?;
        self.pages_changed();
        Some(range)
    }

    /// Takes back the last edit and returns what it was. `scheduler` is the one the session was
    /// opened with.
    pub fn undo(&mut self, scheduler: &Scheduler) -> Option<Edit> {
        let edit = self.edit.undo()?;
        self.undone_or_redone(scheduler, edit);
        Some(edit)
    }

    /// Does again what [`undo`](Self::undo) took back.
    pub fn redo(&mut self, scheduler: &Scheduler) -> Option<Edit> {
        let edit = self.edit.redo()?;
        self.undone_or_redone(scheduler, edit);
        Some(edit)
    }

    /// After undo or redo of `edit`: the pages are as they were unless it was one to them.
    ///
    /// What they are rendered from may have changed with an annotation of a file.
    fn undone_or_redone(&mut self, scheduler: &Scheduler, edit: Edit) {
        self.picked = None;
        if edit.is_annotation() {
            self.sync_variants(scheduler);
        } else {
            self.pages_changed();
        }
    }

    /// The position of page `page` of file `source` (a link's target, for example), if it's in the
    /// document.
    pub fn position_of(&self, source: SourceId, page: usize) -> Option<usize> {
        self.edit
            .pages()
            .iter()
            .position(|p| p.source == source && p.page as usize == page)
    }

    /// Updates everything that depends on the page list.
    ///
    /// Selections and search results hold positions, so the selection is dropped and the search
    /// runs again.
    fn pages_changed(&mut self) -> bool {
        let crops = self.crops();
        let pages = view_pages(&self.edit, &self.sources, &self.variants, &crops);
        let sources = &self.sources;
        self.labels = PageLabels::of_pages(self.edit.pages().iter().map(|p| {
            (
                sources[p.source.0 as usize].labels.as_ref(),
                p.page as usize,
            )
        }));
        self.viewer.set_pages(pages);
        self.selection = None;
        self.picked = None;
        self.draft = None;
        self.adjust = None;
        self.held = None;
        self.typing_into = None;
        self.search_again();
        true
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop_indexing();
    }
}
