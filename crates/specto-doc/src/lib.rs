//! The edit model: what a document being edited consists of.
//!
//! Editing never changes the files a document came from. An [`EditDoc`] is a list of [`PageRef`]s,
//! each naming a page of one of the source files plus the rotation the user added. Rotating,
//! deleting, moving and inserting pages only change this list. The bytes of a new file are produced
//! when saving. Which source is which, and how to read it, is up to the frontend: here a source is
//! just a [`SourceId`].
//!
//! Undo keeps a copy of the page list from before each edit. A [`PageRef`] is 12 bytes, so even for
//! 10,000 pages a step costs 120 KB, and undo can't drift out of step with redo the way
//! hand-written inverse commands can.
//!
//! Annotations are edited the same way (see [`annots`](AnnotId)): the document keeps what was done
//! to them, each undo step has that as it was (shared until it changes), and page and annotation
//! edits are undone in the order they were made.

mod annots;
mod state;

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
pub use specto_annot::AnnotType;
pub use specto_forms::{FieldId, FieldValue};
pub use specto_geom::Rotation;

use annots::AnnotEdits;
pub use annots::{AnnotId, PageAnnotChanges};
pub use state::EditState;

/// How many edits can be undone.
pub const UNDO_LIMIT: usize = 100;

/// One of the files a document's pages come from, as numbered by the frontend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SourceId(pub u32);

/// A page of the document: page `page` of source `source`, turned by `rotation` on top of its own
/// `/Rotate`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageRef {
    pub source: SourceId,
    pub page: u32,
    pub rotation: Rotation,
}

impl PageRef {
    pub fn new(source: SourceId, page: u32) -> Self {
        Self {
            source,
            page,
            rotation: Rotation::None,
        }
    }
}

/// What an edit did, for the frontends to name it in their Undo and Redo menu items ("Undo Rotate
/// Page", "Undo Highlight"). The count is of pages affected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edit {
    Rotate {
        pages: usize,
    },
    Delete {
        pages: usize,
    },
    Move {
        pages: usize,
    },
    Insert {
        pages: usize,
    },
    /// A field of the form was filled in, or what it held was changed.
    FillField,
    /// The fields of the form were set back to what they hold at first.
    ResetForm,
    AddAnnotation(AnnotType),
    RemoveAnnotation(AnnotType),
    ChangeAnnotation(AnnotType),
}

impl Edit {
    /// Whether the edit was to annotations or the form, not to pages: those are as they were.
    pub fn is_annotation(self) -> bool {
        matches!(
            self,
            Self::AddAnnotation(_)
                | Self::RemoveAnnotation(_)
                | Self::ChangeAnnotation(_)
                | Self::FillField
                | Self::ResetForm
        )
    }
}

/// One state of an [`EditDoc`] (see [`EditDoc::version`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version(u64);

/// The pages and annotation edits as they were, for undo or redo.
#[derive(Clone, Debug)]
struct Step {
    edit: Edit,
    pages: Vec<PageRef>,
    annots: Arc<AnnotEdits>,
    /// What was filled into the form, by field.
    form: Arc<BTreeMap<FieldId, FieldValue>>,
    version: u64,
}

/// A document being edited.
#[derive(Clone, Debug)]
pub struct EditDoc {
    pages: Vec<PageRef>,
    annots: Arc<AnnotEdits>,
    /// What was filled into the form, by field.
    form: Arc<BTreeMap<FieldId, FieldValue>>,
    /// The number the next new annotation gets.
    next_annot: u64,
    /// Identifies the current state.
    ///
    /// Each edit gets a new one, and undo and redo bring back old ones. Compared with `saved` for
    /// "modified".
    version: u64,
    next_version: u64,
    saved: Option<u64>,
    undo: Vec<Step>,
    redo: Vec<Step>,
}

impl EditDoc {
    /// A document of `pages`, as just opened or saved.
    pub fn new(pages: Vec<PageRef>) -> Self {
        Self {
            pages,
            annots: Arc::default(),
            form: Arc::default(),
            next_annot: 0,
            version: 0,
            next_version: 1,
            saved: Some(0),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// All pages of `source`, unrotated: a file as opened.
    pub fn from_source(source: SourceId, page_count: u32) -> Self {
        Self::new((0..page_count).map(|i| PageRef::new(source, i)).collect())
    }

    pub fn pages(&self) -> &[PageRef] {
        &self.pages
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Whether there are changes since opening or the last save.
    pub fn is_modified(&self) -> bool {
        self.saved != Some(self.version)
    }

    /// Records that the current state was saved.
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.version);
    }

    /// Identifies the current state, for [`mark_saved_version`]: a save in the background takes a
    /// copy of the pages, and when it's done, only that state counts as saved, not edits made
    /// meanwhile.
    ///
    /// [`mark_saved_version`]: Self::mark_saved_version
    pub fn version(&self) -> Version {
        Version(self.version)
    }

    /// Records that the state `version` was saved.
    pub fn mark_saved_version(&mut self, version: Version) {
        self.saved = Some(version.0);
    }

    /// The sources that pages still come from, in order of first use.
    pub fn sources(&self) -> Vec<SourceId> {
        let mut out: Vec<SourceId> = Vec::new();
        for p in &self.pages {
            if !out.contains(&p.source) {
                out.push(p.source);
            }
        }
        out
    }

    /// Turns the pages at positions `which` clockwise by `by`.
    ///
    /// Duplicates and positions out of range are ignored. Returns whether anything changed.
    pub fn rotate(&mut self, which: &[usize], by: Rotation) -> bool {
        let which = self.valid(which);
        if which.is_empty() || by == Rotation::None {
            return false;
        }
        self.record(Edit::Rotate { pages: which.len() });
        for i in which {
            let p = &mut self.pages[i];
            p.rotation =
                Rotation::from_quarter_turns(p.rotation.quarter_turns() + by.quarter_turns());
        }
        true
    }

    /// Removes pages `which`.
    ///
    /// A document keeps at least one page, so deleting all of them does nothing. Returns whether
    /// anything changed.
    pub fn delete(&mut self, which: &[usize]) -> bool {
        let which = self.valid(which);
        if which.is_empty() || which.len() == self.pages.len() {
            return false;
        }
        self.record(Edit::Delete { pages: which.len() });
        let mut k = 0;
        let mut i = 0;
        self.pages.retain(|_| {
            let drop = which.get(k) == Some(&i);
            k += usize::from(drop);
            i += 1;
            !drop
        });
        true
    }

    /// Moves pages `which` to before the page at position `to`, keeping their order.
    ///
    /// `to` is counted before the move, and the page count means the end. Returns where they ended
    /// up, or `None` if nothing moved.
    pub fn move_pages(&mut self, which: &[usize], to: usize) -> Option<Range<usize>> {
        let which = self.valid(which);
        if which.is_empty() {
            return None;
        }
        let to = to.min(self.pages.len());
        // Pages before the target that move don't count towards it.
        let start = to - which.iter().filter(|&&i| i < to).count();
        let range = start..start + which.len();
        let moved: Vec<PageRef> = which.iter().map(|&i| self.pages[i]).collect();
        let mut rest: Vec<PageRef> = Vec::with_capacity(self.pages.len());
        let mut k = 0;
        for (i, &p) in self.pages.iter().enumerate() {
            if which.get(k) == Some(&i) {
                k += 1;
            } else {
                rest.push(p);
            }
        }
        rest.splice(start..start, moved);
        if rest == self.pages {
            return None;
        }
        self.record(Edit::Move { pages: which.len() });
        self.pages = rest;
        Some(range)
    }

    /// Inserts `pages` before position `at` (the page count means the end). Returns where they
    /// ended up.
    pub fn insert(&mut self, at: usize, pages: Vec<PageRef>) -> Option<Range<usize>> {
        if pages.is_empty() {
            return None;
        }
        let at = at.min(self.pages.len());
        let range = at..at + pages.len();
        self.record(Edit::Insert { pages: pages.len() });
        self.pages.splice(at..at, pages);
        Some(range)
    }

    /// Like [`insert`](Self::insert), for pages copied from another document that come with what
    /// was done to their annotations there (source, page of it, the changes): one edit.
    pub fn insert_annotated(
        &mut self,
        at: usize,
        pages: Vec<PageRef>,
        annotations: Vec<(SourceId, u32, PageAnnotChanges)>,
    ) -> Option<Range<usize>> {
        let range = self.insert(at, pages)?;
        self.adopt_annotation_changes(annotations);
        Some(range)
    }

    /// What was filled into field `field` of the main file's form, if anything was: what it
    /// holds instead of the value the file has for it.
    pub fn field_value(&self, field: FieldId) -> Option<&FieldValue> {
        self.form.get(&field)
    }

    /// Everything that was filled into the form, by field: what saving writes.
    pub fn form_values(&self) -> Vec<(FieldId, FieldValue)> {
        self.form.iter().map(|(id, v)| (*id, v.clone())).collect()
    }

    /// Fills `value` into field `field`. `in_file` is what the file has for it: filled in
    /// again, the field is as in the file and nothing is kept for it. Returns whether that
    /// changed what the field holds.
    pub fn fill_field(
        &mut self,
        field: FieldId,
        value: FieldValue,
        in_file: Option<&FieldValue>,
    ) -> bool {
        let now = self.form.get(&field).or(in_file);
        if now == Some(&value) {
            return false;
        }
        self.record(Edit::FillField);
        let form = Arc::make_mut(&mut self.form);
        if in_file == Some(&value) {
            form.remove(&field);
        } else {
            form.insert(field, value);
        }
        true
    }

    /// Fills each of `fields` with its second value, as one edit. The third is what the file has
    /// for the field (see [`fill_field`](Self::fill_field)). Returns whether that changed
    /// anything.
    pub fn reset_form(&mut self, fields: Vec<(FieldId, FieldValue, Option<FieldValue>)>) -> bool {
        let changes =
            |form: &BTreeMap<FieldId, FieldValue>,
             (field, value, in_file): &(FieldId, FieldValue, Option<FieldValue>)| {
                form.get(field).or(in_file.as_ref()) != Some(value)
            };
        if !fields.iter().any(|f| changes(&self.form, f)) {
            return false;
        }
        self.record(Edit::ResetForm);
        let form = Arc::make_mut(&mut self.form);
        for (field, value, in_file) in fields {
            if in_file.as_ref() == Some(&value) {
                form.remove(&field);
            } else {
                form.insert(field, value);
            }
        }
        true
    }

    /// What [`undo`](Self::undo) would undo.
    pub fn undo_edit(&self) -> Option<Edit> {
        self.undo.last().map(|s| s.edit)
    }

    /// What [`redo`](Self::redo) would redo.
    pub fn redo_edit(&self) -> Option<Edit> {
        self.redo.last().map(|s| s.edit)
    }

    /// Undoes the last edit and returns it.
    pub fn undo(&mut self) -> Option<Edit> {
        let step = self.undo.pop()?;
        let edit = step.edit;
        let current = self.swap(step);
        self.redo.push(current);
        Some(edit)
    }

    /// Redoes the last undone edit and returns it.
    pub fn redo(&mut self) -> Option<Edit> {
        let step = self.redo.pop()?;
        let edit = step.edit;
        let current = self.swap(step);
        self.undo.push(current);
        Some(edit)
    }

    /// Makes `step` current and returns the state it replaced, under the same edit.
    fn swap(&mut self, step: Step) -> Step {
        let old = Step {
            edit: step.edit,
            pages: std::mem::replace(&mut self.pages, step.pages),
            annots: std::mem::replace(&mut self.annots, step.annots),
            form: std::mem::replace(&mut self.form, step.form),
            version: self.version,
        };
        self.version = step.version;
        old
    }

    /// Saves the current state for undo before an edit, which then gets a new version.
    fn record(&mut self, edit: Edit) {
        self.undo.push(Step {
            edit,
            pages: self.pages.clone(),
            annots: self.annots.clone(),
            form: self.form.clone(),
            version: self.version,
        });
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.version = self.next_version;
        self.next_version += 1;
    }

    /// `which`, sorted, without duplicates or positions out of range.
    fn valid(&self, which: &[usize]) -> Vec<usize> {
        let mut v: Vec<usize> = which
            .iter()
            .copied()
            .filter(|&i| i < self.pages.len())
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}
