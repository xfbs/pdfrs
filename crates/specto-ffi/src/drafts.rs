//! Drafts: unsaved edits kept on disk while the app is in the background, to bring back if the
//! system ends it meanwhile. See `specto-store`.

use std::io::ErrorKind::{InvalidData, NotFound};
use std::sync::Arc;
use std::time::Duration;

use specto_store::{Store, StoreError};

use crate::{PdfError, PdfFile, ViewSession};

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

    /// The file the draft of `key` was made on, where the document was saved over it since: what
    /// to open, in place of the file, for [`ViewSession::restore_draft`]. `None` otherwise.
    pub fn base(&self, key: String) -> Result<Option<Arc<PdfFile>>, PdfError> {
        let base = self.0.base(&key, &key).map_err(io_error)?;
        Ok(base.map(|data| Arc::new(PdfFile(data))))
    }
}

#[uniffi::export]
impl ViewSession {
    /// Keeps what was done to the document in `store` as the draft of `key`, which names the file
    /// by its content.
    ///
    /// Without unsaved edits, or for a document with a password, there is no draft, and any the
    /// document had is deleted. Returns whether there is one.
    ///
    /// Once the document was saved over its file ([`written_is_saved`](Self::written_is_saved)),
    /// the edits count from a file that isn't there any more, and the draft keeps it.
    pub fn keep_draft(&self, store: &Arc<DraftStore>, key: String) -> Result<bool, PdfError> {
        let (draft, base) = {
            let state = self.shared.lock();
            let session = &state.session;
            let draft = (session.edit().is_modified())
                .then(|| session.draft())
                .flatten();
            (draft, state.saved_over.then(|| session.data().clone()))
        };
        match &draft {
            Some(draft) => (store.0)
                .save_draft(&key, &key, draft, base.as_ref())
                .map_err(io_error)?,
            None => store.0.delete_draft(&key).map_err(io_error)?,
        }
        Ok(draft.is_some())
    }

    /// Takes over the draft of `key` in `store`, if there is one: the document is then as it was
    /// left, with nothing to undo. Returns whether there was one.
    ///
    /// Where the draft keeps the file it was made on ([`DraftStore::base`]), the session must have
    /// been opened from that. A draft that can't be taken over is deleted, and the error says why.
    /// One that couldn't be read for now stays.
    pub fn restore_draft(&self, store: &Arc<DraftStore>, key: String) -> Result<bool, PdfError> {
        let loaded =
            (store.0.base(&key, &key)).and_then(|base| Ok((base, store.0.load_draft(&key, &key)?)));
        let (base, draft) = match loaded {
            Ok((_, None)) => return Ok(false),
            Ok((base, Some(draft))) => (base, draft),
            // A file it names is missing or isn't one: the draft is damaged.
            Err(StoreError::Io(e)) if !matches!(e.kind(), NotFound | InvalidData) => {
                return Err(io_error(e));
            }
            Err(e) => {
                let _ = store.0.delete_draft(&key);
                return Err(io_error(e));
            }
        };
        let mut state = self.shared.lock();
        let restored = match &base {
            Some(base) if **base != **state.session.data() => Err(PdfError::Io {
                reason: "the draft is of another file".into(),
            }),
            _ => (state.session.restore(&self.shared.scheduler, draft)).map_err(io_error),
        };
        match restored {
            Ok(()) => {
                state.saved_over = base.is_some();
                Ok(true)
            }
            Err(e) => {
                drop(state);
                let _ = store.0.delete_draft(&key);
                Err(e)
            }
        }
    }
}
