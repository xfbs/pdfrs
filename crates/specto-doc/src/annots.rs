//! The annotations of a document being edited.
//!
//! The annotations a file has are read from it when wanted (see `specto_annot::import`) and never
//! change. What an [`EditDoc`] keeps is only what the user did: which of those are deleted or
//! changed, and the new ones. That is small however many annotations the files have, costs nothing
//! for pages nobody looked at, and is what saving needs to know.
//!
//! Annotations belong to a page of a file, not to a place in the document: they stay with their
//! page when it is moved or rotated, and are back when a deleted page is. Copies of one page share
//! them.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specto_annot::{AnnotKind, Annotation, Origin, StampArt, StampImage};

use crate::{Edit, EditDoc, SourceId};

/// Names one annotation of a document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AnnotId {
    /// One a file has: entry `index` of the `/Annots` of page `page` of `source` (an
    /// [`Origin`](specto_annot::Origin)'s index).
    File {
        source: SourceId,
        page: u32,
        index: u32,
    },
    /// One made while editing. The number is never used again.
    New(u64),
}

/// What was done to the annotations, relative to the files.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct AnnotEdits {
    /// Annotations of the files that are deleted or changed, by source, page and index.
    #[serde(with = "crate::state::pairs")]
    changed: BTreeMap<(SourceId, u32, u32), Changed>,
    /// New ones, in the order they were made (later ones lie on top).
    added: Vec<Added>,
}

/// What became of an annotation of a file.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Changed {
    /// Where the file has it, with its object number: saving checks that it finds the same
    /// annotation there.
    origin: Origin,
    /// What it is now, or `None` if it was deleted.
    to: Option<Arc<Annotation>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Added {
    id: u64,
    source: SourceId,
    page: u32,
    annotation: Arc<Annotation>,
}

/// What to do to the annotations of one page of a file when saving it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageAnnotChanges {
    /// Entries of the page's `/Annots` to drop.
    pub removed: Vec<Origin>,
    /// Entries to write anew.
    pub replaced: Vec<(Origin, Arc<Annotation>)>,
    /// Annotations to append, in order.
    pub added: Vec<Arc<Annotation>>,
}

impl PageAnnotChanges {
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.replaced.is_empty() && self.added.is_empty()
    }
}

impl EditDoc {
    /// The annotations of page `page` of `source` as edited, given the ones the file has there
    /// (`imported`, in the file's order): bottom to top.
    pub fn annotations(
        &self,
        source: SourceId,
        page: u32,
        imported: &[Arc<Annotation>],
    ) -> Vec<(AnnotId, Arc<Annotation>)> {
        let edits = &self.annots;
        let of_file = imported.iter().filter_map(|a| {
            let index = a.origin?.index;
            let id = AnnotId::File {
                source,
                page,
                index,
            };
            match edits.changed.get(&(source, page, index)) {
                None => Some((id, a.clone())),
                Some(Changed { to: None, .. }) => None,
                Some(Changed { to: Some(to), .. }) => Some((id, to.clone())),
            }
        });
        let added = edits
            .added
            .iter()
            .filter(|a| a.source == source && a.page == page)
            .map(|a| (AnnotId::New(a.id), a.annotation.clone()));
        of_file.chain(added).collect()
    }

    /// Whether any annotation was added, changed or deleted (and not undone), whatever was saved
    /// since.
    pub fn has_annotation_edits(&self) -> bool {
        !self.annots.changed.is_empty() || !self.annots.added.is_empty()
    }

    /// The pages of files whose annotations were edited in any way: source and page, each once.
    pub fn pages_with_annotation_edits(&self) -> Vec<(SourceId, u32)> {
        let mut pages: Vec<(SourceId, u32)> = self
            .annots
            .changed
            .keys()
            .map(|&(source, page, _)| (source, page))
            .chain(self.annots.added.iter().map(|a| (a.source, a.page)))
            .collect();
        pages.sort_unstable();
        pages.dedup();
        pages
    }

    /// What saving has to do to the annotations of page `page` of `source`.
    pub fn annotation_changes(&self, source: SourceId, page: u32) -> PageAnnotChanges {
        let mut out = PageAnnotChanges::default();
        let all = (source, page, 0)..=(source, page, u32::MAX);
        for change in self.annots.changed.range(all).map(|(_, c)| c) {
            match &change.to {
                None => out.removed.push(change.origin),
                Some(a) => out.replaced.push((change.origin, a.clone())),
            }
        }
        out.added = self
            .annots
            .added
            .iter()
            .filter(|a| a.source == source && a.page == page)
            .map(|a| a.annotation.clone())
            .collect();
        out
    }

    /// Puts a new annotation on page `page` of `source`, on top of the others.
    pub fn add_annotation(
        &mut self,
        source: SourceId,
        page: u32,
        annotation: Annotation,
    ) -> AnnotId {
        self.add_annotations(vec![(source, page, annotation)])[0]
    }

    /// Puts new annotations on their pages (source, page of it, annotation) as one edit, named
    /// after the first: marked text that runs over several pages is one annotation per page.
    /// Without any, nothing happens.
    pub fn add_annotations(
        &mut self,
        annotations: Vec<(SourceId, u32, Annotation)>,
    ) -> Vec<AnnotId> {
        let Some((_, _, first)) = annotations.first() else {
            return Vec::new();
        };
        self.record(Edit::AddAnnotation(first.annot_type()));
        let edits = Arc::make_mut(&mut self.annots);
        annotations
            .into_iter()
            .map(|(source, page, annotation)| {
                let id = self.next_annot;
                self.next_annot += 1;
                edits.added.push(Added {
                    id,
                    source,
                    page,
                    annotation: Arc::new(annotation),
                });
                AnnotId::New(id)
            })
            .collect()
    }

    /// Takes over what another document did to the annotations of pages (source, page of it, the
    /// changes), for pages copied from it.
    ///
    /// Pages whose annotations were edited here already keep theirs: a page has one set of
    /// annotations, wherever it came from. This is part of the edit being made, so nothing is
    /// recorded for undo here.
    pub(crate) fn adopt_annotation_changes(
        &mut self,
        changes: Vec<(SourceId, u32, PageAnnotChanges)>,
    ) {
        for (source, page, c) in changes {
            if c.is_empty() || !self.annotation_changes(source, page).is_empty() {
                continue;
            }
            let edits = Arc::make_mut(&mut self.annots);
            for origin in c.removed {
                let to = None;
                edits
                    .changed
                    .insert((source, page, origin.index), Changed { origin, to });
            }
            for (origin, a) in c.replaced {
                let to = Some(a);
                edits
                    .changed
                    .insert((source, page, origin.index), Changed { origin, to });
            }
            for annotation in c.added {
                let id = self.next_annot;
                self.next_annot += 1;
                edits.added.push(Added {
                    id,
                    source,
                    page,
                    annotation,
                });
            }
        }
    }

    /// Deletes the annotation `id`, which is `annotation` (as [`annotations`](Self::annotations)
    /// gave it). Returns whether there was something to delete.
    pub fn remove_annotation(&mut self, id: AnnotId, annotation: &Annotation) -> bool {
        if !self.has_annotation(id) || !origin_fits(id, annotation) {
            return false;
        }
        self.record(Edit::RemoveAnnotation(annotation.annot_type()));
        self.set_annotation(id, annotation.origin, None);
        true
    }

    /// Replaces the annotation `id` with `annotation`: the same one, changed (for one of a file,
    /// with the origin it had). Returns whether there was one to change.
    pub fn change_annotation(&mut self, id: AnnotId, annotation: Annotation) -> bool {
        if !self.has_annotation(id) || !origin_fits(id, &annotation) {
            return false;
        }
        self.record(Edit::ChangeAnnotation(annotation.annot_type()));
        self.set_annotation(id, annotation.origin, Some(Arc::new(annotation)));
        true
    }

    /// Makes the annotation `id` (of a file: the one at `origin`) into `to`, or deletes it.
    fn set_annotation(&mut self, id: AnnotId, origin: Option<Origin>, to: Option<Arc<Annotation>>) {
        let edits = Arc::make_mut(&mut self.annots);
        match (id, origin) {
            (
                AnnotId::File {
                    source,
                    page,
                    index,
                },
                Some(origin),
            ) => {
                edits
                    .changed
                    .insert((source, page, index), Changed { origin, to });
            }
            // Ruled out by `origin_fits`.
            (AnnotId::File { .. }, None) => {}
            (AnnotId::New(n), _) => match to {
                Some(to) => {
                    if let Some(a) = edits.added.iter_mut().find(|a| a.id == n) {
                        a.annotation = to;
                    }
                }
                None => edits.added.retain(|a| a.id != n),
            },
        }
    }

    /// Whether `id` can be there: a new one that is, or one of a file that isn't deleted (whether
    /// the file has it, only the caller knows).
    fn has_annotation(&self, id: AnnotId) -> bool {
        match id {
            AnnotId::File {
                source,
                page,
                index,
            } => !matches!(
                self.annots.changed.get(&(source, page, index)),
                Some(Changed { to: None, .. })
            ),
            AnnotId::New(n) => self.annots.added.iter().any(|a| a.id == n),
        }
    }
}

/// Whether `annotation` can be the one `id` names: one of a file says where in the file it is, and
/// that is where `id` points.
fn origin_fits(id: AnnotId, annotation: &Annotation) -> bool {
    match id {
        AnnotId::File { index, .. } => annotation.origin.is_some_and(|o| o.index == index),
        AnnotId::New(_) => true,
    }
}

impl AnnotEdits {
    /// Gives each source the number `new` has for it, and drops what was done on pages of sources
    /// it has none for.
    pub(crate) fn renumber_sources(&mut self, new: impl Fn(SourceId) -> Option<SourceId>) {
        self.changed = std::mem::take(&mut self.changed)
            .into_iter()
            .filter_map(|((source, page, index), c)| Some(((new(source)?, page, index), c)))
            .collect();
        self.added.retain_mut(|a| match new(a.source) {
            Some(source) => {
                a.source = source;
                true
            }
            None => false,
        });
    }

    /// The images of the stamps among the annotations, each once.
    pub(crate) fn stamp_images(&self) -> Vec<Arc<StampImage>> {
        let annotations = (self.changed.values().filter_map(|c| c.to.as_ref()))
            .chain(self.added.iter().map(|a| &a.annotation));
        let mut images: Vec<Arc<StampImage>> = Vec::new();
        for a in annotations {
            if let AnnotKind::Stamp(StampArt::Image(image)) = &a.kind
                && !images.iter().any(|i| Arc::ptr_eq(i, image) || i == image)
            {
                images.push(image.clone());
            }
        }
        images
    }
}
