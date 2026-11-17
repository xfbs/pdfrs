//! Drafts: unsaved edits kept on disk while the app is in the background, to bring back if the
//! system ends it meanwhile. See `specto-store`.

use std::sync::Arc;
use std::time::Duration;

use specto_store::Store;

use crate::{PdfError, ViewSession};

/// Where the app keeps drafts of documents with unsaved edits.
#[derive(uniffi::Object)]
pub struct DraftStore(Store);

fn io_error(e: impl ToString) -> PdfError {
    PdfError::Io {
        reason: e.to_string(),
    }
}

#[uniffi::export]
impl DraftStore {
    /// The store in the directory `dir`, made if it isn't there.
    #[uniffi::constructor]
    pub fn open(dir: String) -> Result<Arc<Self>, PdfError> {
        Store::open(dir)
            .map(|s| Arc::new(Self(s)))
            .map_err(io_error)
    }

    /// Deletes the draft of the document `key`, if there is one.
    pub fn delete(&self, key: String) -> Result<(), PdfError> {
        self.0.delete_draft(&key).map_err(io_error)
    }

    /// Deletes drafts that weren't written or opened for `days` days, and what only they used.
    pub fn prune(&self, days: u32) -> Result<(), PdfError> {
        let age = Duration::from_secs(u64::from(days) * 24 * 60 * 60);
        self.0.prune(age).map_err(io_error)
    }
}

#[uniffi::export]
impl ViewSession {
    /// Keeps what was done to the document in `store` as the draft of `key`, which names the file
    /// by its content.
    ///
    /// Without unsaved edits, or for a document with a password, there is no draft, and any the
    /// document had is deleted. Returns whether there is one.
    pub fn keep_draft(&self, store: &Arc<DraftStore>, key: String) -> Result<bool, PdfError> {
        let draft = {
            let state = self.shared.lock();
            let session = &state.session;
            session
                .edit()
                .is_modified()
                .then(|| session.draft())
                .flatten()
        };
        match &draft {
            Some(draft) => store.0.save_draft(&key, &key, draft).map_err(io_error)?,
            None => store.0.delete_draft(&key).map_err(io_error)?,
        }
        Ok(draft.is_some())
    }

    /// Takes over the draft of `key` in `store`, if there is one: the document is then as it was
    /// left, with nothing to undo. Returns whether there was one.
    ///
    /// A draft that can't be taken over is deleted, and the error says why.
    pub fn restore_draft(&self, store: &Arc<DraftStore>, key: String) -> Result<bool, PdfError> {
        let restored = match store.0.load_draft(&key, &key) {
            Ok(None) => return Ok(false),
            Ok(Some(draft)) => {
                let mut state = self.shared.lock();
                (state.session.restore(&self.shared.scheduler, draft)).map_err(io_error)
            }
            Err(e) => Err(io_error(e)),
        };
        if restored.is_err() {
            let _ = store.0.delete_draft(&key);
        }
        restored.map(|()| true)
    }
}
