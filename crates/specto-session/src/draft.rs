//! Drafts: what was done to a document, to keep until it is saved, and to bring back into a session
//! of the same file after the app was ended.
//!
//! Where a draft is kept is up to the frontend (see `specto-store`). A session only makes one and
//! takes one back.

use specto_doc::{EditDoc, EditState, SourceId};
use specto_render::OpenError;
use specto_render::scheduler::Scheduler;
use specto_source::PdfData;

use crate::{MAIN, Session};

/// What was done to a document, with the files its pages were inserted from.
#[derive(Clone, Debug)]
pub struct Draft {
    /// The edits. Source 0 is the file the document is opened from, and each other source is the
    /// file of the same number in `files`, counting from 1.
    pub state: EditState,
    /// The files pages were inserted from.
    pub files: Vec<PdfData>,
}

/// Why a draft can't be taken back.
#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("a file of the draft can't be opened: {0}")]
    Open(#[from] OpenError),
    #[error("the draft names pages its files don't have")]
    Mismatch,
}

impl Session {
    /// What was done to the document, to keep as a draft.
    ///
    /// `None` where a file of the document has a password: a draft would keep what was typed into
    /// it, and the file it came from, unprotected.
    pub fn draft(&self) -> Option<Draft> {
        let mut state = self.edit.state();
        let used = state.sources();
        let protected = (self.sources.iter().enumerate())
            .filter(|(i, _)| *i == 0 || used.contains(&SourceId(*i as u32)))
            .any(|(_, s)| s.password.is_some());
        if protected {
            return None;
        }
        let inserted: Vec<SourceId> = used.into_iter().filter(|s| *s != MAIN).collect();
        let renumbered = state.renumber_sources(|s| {
            if s == MAIN {
                return Some(MAIN);
            }
            let i = inserted.iter().position(|i| *i == s)?;
            Some(SourceId(i as u32 + 1))
        });
        // The sources of the pages are those used.
        debug_assert!(renumbered);
        let files = (inserted.iter())
            .map(|s| self.sources[s.0 as usize].data.clone())
            .collect();
        Some(Draft { state, files })
    }

    /// Takes the edits of `draft` over, in place of any the document has. They can't be undone,
    /// and count as not saved.
    ///
    /// The draft's files are opened with `scheduler`, the one the session was opened with. A draft
    /// that names pages the files don't have is an error, and the document is left as it was.
    pub fn restore(&mut self, scheduler: &Scheduler, draft: Draft) -> Result<(), RestoreError> {
        let mut ids = Vec::with_capacity(draft.files.len());
        for data in draft.files {
            ids.push(self.add_source(scheduler, data, None)?.0);
        }
        let mut state = draft.state;
        let renumbered = state.renumber_sources(|s| match s.0 {
            0 => Some(MAIN),
            n => ids.get(n as usize - 1).copied(),
        });
        if !renumbered {
            return Err(RestoreError::Mismatch);
        }
        let edit = EditDoc::restored(state);
        let page_count = |s: SourceId| self.sources.get(s.0 as usize).map(|s| s.geometries.len());
        let fits = |(source, page): (SourceId, u32)| {
            page_count(source).is_some_and(|count| (page as usize) < count)
        };
        let pages = edit.pages().iter().map(|p| (p.source, p.page));
        if edit.page_count() == 0 || !pages.chain(edit.pages_with_annotation_edits()).all(fits) {
            return Err(RestoreError::Mismatch);
        }
        self.edit = edit;
        self.pages_changed();
        self.sync_variants(scheduler);
        Ok(())
    }
}
