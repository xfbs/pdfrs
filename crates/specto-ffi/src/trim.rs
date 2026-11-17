//! Trimming margins: how this session runs the shared one's measuring thread.

use std::sync::Arc;

use crate::{Shared, State};

/// Starts measuring what the pages draw in a background thread, if their margins are trimmed and
/// some aren't measured yet. The frontend hears about pages that changed size through
/// [`ViewListener::tiles_ready`](crate::ViewListener::tiles_ready), which draws a new frame.
pub(crate) fn start_measuring(shared: &Arc<Shared>, state: &mut State) {
    let session = Arc::downgrade(shared);
    let listener = shared.listener.clone();
    state.session.start_measuring(move |file| {
        // The session may have been closed meanwhile.
        let Some(shared) = session.upgrade() else {
            return false;
        };
        let changed = shared.lock().session.measured(file);
        drop(shared);
        if changed {
            listener.tiles_ready();
        }
        true
    });
}
