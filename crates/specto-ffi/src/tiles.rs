//! Bookkeeping for the tiles whose pixels live in Kotlin `Bitmap`s.
//!
//! Like `specto_render::tile::TileCache`, but it stores no pixels, reports what it evicts (so
//! Kotlin can release the bitmap), and is `Send`.

use std::collections::{HashMap, HashSet};

use specto_render::Bitmap;
use specto_render::scheduler::Finished;
use specto_render::tile::TileKey;

use crate::FinishedTile;

struct Cached {
    id: u64,
    bytes: usize,
    last_used: u64,
    /// A page's low-resolution placeholder: evicted last, because it is what makes scrolling back
    /// instant, and thumbnails reuse it.
    placeholder: bool,
}

struct Pending {
    key: TileKey,
    bitmap: Bitmap,
    placeholder: bool,
}

pub(crate) struct TileBook {
    budget: usize,
    bytes: usize,
    frame: u64,
    next_id: u64,
    cached: HashMap<TileKey, Cached>,
    /// Rendered, but not yet copied into a bitmap.
    pending: HashMap<u64, Pending>,
    /// Tiles that failed to render.
    ///
    /// Treated as present so they aren't requested again on every frame. They just draw nothing.
    failed: HashSet<TileKey>,
    evicted: Vec<u64>,
}

impl TileBook {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            budget,
            bytes: 0,
            frame: 0,
            next_id: 1,
            cached: HashMap::new(),
            pending: HashMap::new(),
            failed: HashSet::new(),
            evicted: Vec::new(),
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Whether the viewer should consider `key` done (drawable or failed).
    pub(crate) fn knows(&self, key: &TileKey) -> bool {
        self.cached.contains_key(key) || self.failed.contains(key)
    }

    /// The id to draw for `key`, marking it as used in this frame.
    pub(crate) fn use_tile(&mut self, key: &TileKey) -> Option<u64> {
        let tile = self.cached.get_mut(key)?;
        tile.last_used = self.frame;
        Some(tile.id)
    }

    pub(crate) fn take_evicted(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.evicted)
    }

    pub(crate) fn add_finished(
        &mut self,
        finished: Vec<Finished>,
        is_placeholder: impl Fn(&TileKey) -> bool,
    ) -> Vec<FinishedTile> {
        self.pending.clear();
        let mut out = Vec::new();
        // A tile can be rendered twice: it stays requested until the frontend has collected it.
        // Hand out each key once, or the frontend ends up with a bitmap that nothing here refers to
        // (and never evicts).
        let mut seen = HashSet::new();
        for f in finished {
            match f.result {
                Ok(bitmap) if !self.cached.contains_key(&f.key) && seen.insert(f.key) => {
                    let id = self.next_id;
                    self.next_id += 1;
                    out.push(FinishedTile {
                        id,
                        width: bitmap.width,
                        height: bitmap.height,
                    });
                    let placeholder = is_placeholder(&f.key);
                    self.pending.insert(
                        id,
                        Pending {
                            key: f.key,
                            bitmap,
                            placeholder,
                        },
                    );
                }
                Ok(_) => {}
                Err(_) => {
                    self.failed.insert(f.key);
                }
            }
        }
        out
    }

    pub(crate) fn claim(&mut self, id: u64, write: impl FnOnce(&Bitmap) -> bool) -> bool {
        let Some(tile) = self.pending.remove(&id) else {
            return false;
        };
        if self.cached.contains_key(&tile.key) || !write(&tile.bitmap) {
            return false;
        }
        let bytes = tile.bitmap.data.len();
        self.bytes += bytes;
        self.cached.insert(
            tile.key,
            Cached {
                id,
                bytes,
                last_used: self.frame,
                placeholder: tile.placeholder,
            },
        );
        if self.bytes > self.budget {
            self.evict_to(self.budget * 3 / 4);
        }
        true
    }

    pub(crate) fn trim(&mut self) {
        self.evict_to(0);
    }

    /// Evicts least recently used tiles until at most `target` bytes remain, sparing those used in
    /// the current frame.
    ///
    /// Placeholders go last: unlike on desktop they can't simply stay forever, since at up to 0.5
    /// MB per page a long document would exceed the whole budget.
    fn evict_to(&mut self, target: usize) {
        let mut candidates: Vec<(bool, u64, TileKey)> = self
            .cached
            .iter()
            .filter(|(_, t)| t.last_used < self.frame)
            .map(|(k, t)| (t.placeholder, t.last_used, *k))
            .collect();
        candidates.sort_unstable();
        for (_, _, key) in candidates {
            if self.bytes <= target {
                break;
            }
            if let Some(tile) = self.cached.remove(&key) {
                self.bytes -= tile.bytes;
                self.evicted.push(tile.id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use specto_geom::Rotation;
    use specto_render::Content;
    use specto_render::tile::{DocId, TileScale};

    use super::*;

    fn finished(page: u32) -> Finished {
        Finished {
            key: TileKey {
                doc: DocId::from_raw(0),
                page,
                rotation: Rotation::None,
                crop: None,
                content: Content::PageOnly,
                scale: TileScale::from_level(0),
                tx: 0,
                ty: 0,
            },
            result: Ok(Bitmap {
                width: 1,
                height: 1,
                data: vec![0; 4],
            }),
            elapsed: Duration::ZERO,
        }
    }

    #[test]
    fn a_tile_rendered_twice_is_handed_out_once() {
        let mut book = TileBook::new(1 << 20);
        // Twice in one batch.
        let batch = book.add_finished(vec![finished(0), finished(0), finished(1)], |_| false);
        assert_eq!(batch.len(), 2);
        for tile in &batch {
            assert!(book.claim(tile.id, |_| true));
        }
        // And again after it is cached.
        assert!(book.add_finished(vec![finished(0)], |_| false).is_empty());

        // Every id handed out is either still drawable or reported as evicted.
        book.begin_frame();
        book.trim();
        let mut evicted = book.take_evicted();
        evicted.sort_unstable();
        let mut handed_out: Vec<u64> = batch.iter().map(|t| t.id).collect();
        handed_out.sort_unstable();
        assert_eq!(evicted, handed_out);
        assert_eq!(book.bytes, 0);
    }

    #[test]
    fn placeholders_are_evicted_last() {
        // Room for four 4-byte tiles.
        let mut book = TileBook::new(16);
        book.begin_frame();
        let placeholder = book.add_finished(vec![finished(0)], |_| true);
        assert!(book.claim(placeholder[0].id, |_| true));
        for page in 1..=4 {
            book.begin_frame();
            let tile = book.add_finished(vec![finished(page)], |_| false);
            assert!(book.claim(tile[0].id, |_| true));
        }
        // The oldest tile is the placeholder, yet it survived the evictions.
        assert!(book.knows(&finished(0).key));
        assert!(!book.take_evicted().is_empty());
        // It still goes when memory has to be given back.
        book.begin_frame();
        book.trim();
        assert!(!book.knows(&finished(0).key));
    }

    #[test]
    fn unclaimed_tiles_are_dropped_and_rendered_again() {
        let mut book = TileBook::new(1 << 20);
        let first = book.add_finished(vec![finished(0)], |_| false);
        // The frontend never claimed it, and the next batch supersedes it.
        let second = book.add_finished(vec![finished(0)], |_| false);
        assert!(!book.claim(first[0].id, |_| true));
        assert!(book.claim(second[0].id, |_| true));
        // A failed copy leaves the tile uncached, so it gets requested again.
        let third = book.add_finished(vec![finished(1)], |_| false);
        assert!(!book.claim(third[0].id, |_| false));
        assert!(!book.knows(&finished(1).key));
    }
}
