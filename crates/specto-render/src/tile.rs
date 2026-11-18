//! Tiles, zoom levels and a frontend-agnostic tile cache.

use std::collections::HashMap;

use specto_geom::{Crop, DeviceIntRect, PageGeometry, Rotation};

use crate::Content;

/// Edge length of a tile in device pixels.
pub const TILE_SIZE: u32 = 512;

/// Identifies a document opened in the [`Scheduler`](crate::scheduler::Scheduler).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocId(pub(crate) u32);

impl DocId {
    /// The raw id, for passing a [`TileKey`] across FFI as plain numbers.
    pub fn raw(self) -> u32 {
        self.0
    }

    /// Rebuilds an id from [`raw`](Self::raw).
    ///
    /// Ids not issued by a `Scheduler` simply match no document.
    pub fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

/// Zoom levels are powers of √2: level `l` renders at `2^(l/2)` device pixels per point.
pub fn level_scale(level: i32) -> f64 {
    2f64.powf(level as f64 / 2.0)
}

/// The smallest level whose scale is at least `scale`.
///
/// Rounding *up* means level tiles are only ever downsampled when drawn, never upscaled. Levels are
/// what the viewer uses while zooming. A settled view uses tiles at the exact device scale instead.
pub fn level_for_scale(scale: f64) -> i32 {
    // The tolerance keeps exact powers, including their fixed-point `TileScale` versions (scale 2.0
    // is level 2), from tipping over.
    (scale.log2() * 2.0 - 1e-3).ceil() as i32
}

/// Device pixels per PDF point, in fixed point (units of 2^-20), so that tile keys are plain
/// integers: hashable, and cheap to pass across FFI.
///
/// The √2 zoom levels are particular values ([`from_level`](Self::from_level)). A settled view uses
/// [`exact`](Self::exact) tiles at its device scale. Page bitmap sizes are always computed from the
/// quantized value ([`get`](Self::get)), so an exact tile maps one to one onto the page rectangle
/// the viewer draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileScale(u32);

impl TileScale {
    /// 2^20: fine enough at small scales, and up to 4096 pixels per point fits in a u32.
    const ONE: f64 = 1_048_576.0;

    /// The scale of zoom level `level`.
    pub fn from_level(level: i32) -> Self {
        Self::exact(level_scale(level))
    }

    /// `scale` device pixels per point, quantized to 2^-20.
    pub fn exact(scale: f64) -> Self {
        Self((scale * Self::ONE).round().clamp(1.0, u32::MAX as f64) as u32)
    }

    /// Device pixels per point.
    pub fn get(self) -> f64 {
        self.0 as f64 / Self::ONE
    }

    /// The zoom level this scale is, if it is one.
    pub fn level(self) -> Option<i32> {
        let l = (self.get().log2() * 2.0).round() as i32;
        (Self::from_level(l) == self).then_some(l)
    }

    pub fn raw(self) -> u32 {
        self.0
    }

    pub fn from_raw(raw: u32) -> Self {
        Self(raw.max(1))
    }
}

/// One tile of a page bitmap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileKey {
    pub doc: DocId,
    pub page: u32,
    /// Extra clockwise rotation on top of the page's `/Rotate`.
    pub rotation: Rotation,
    /// The part of the page shown, if not all of it.
    pub crop: Option<Crop>,
    pub content: Content,
    pub scale: TileScale,
    pub tx: u32,
    pub ty: u32,
}

impl TileKey {
    /// The tile's pixel rectangle within the full page bitmap at its scale, or `None` if the tile
    /// lies outside the page.
    ///
    /// `geometry` is the page as shown, cropped but without the extra rotation.
    pub fn region(&self, geometry: &PageGeometry) -> Option<DeviceIntRect> {
        let full = geometry.rotated(self.rotation).pixel_size(self.scale.get());
        let (x, y) = (self.tx * TILE_SIZE, self.ty * TILE_SIZE);
        if x >= full.width || y >= full.height {
            return None;
        }
        Some(DeviceIntRect::new(
            (x, y).into(),
            (
                (x + TILE_SIZE).min(full.width),
                (y + TILE_SIZE).min(full.height),
            )
                .into(),
        ))
    }
}

/// Tile indices `(tx, ty)` covering `area` (pixels of the full page bitmap at `scale`), clipped to
/// the page. `geometry` is the page as displayed.
pub fn tiles_covering(
    geometry: &PageGeometry,
    scale: TileScale,
    area: DeviceIntRect,
) -> impl Iterator<Item = (u32, u32)> {
    let full = geometry.pixel_size(scale.get());
    let x1 = area.max.x.min(full.width);
    let y1 = area.max.y.min(full.height);
    let (x0, y0) = (area.min.x, area.min.y);
    let empty = x0 >= x1 || y0 >= y1;
    let (tx0, ty0) = (x0 / TILE_SIZE, y0 / TILE_SIZE);
    let (tx1, ty1) = if empty {
        (tx0, ty0)
    } else {
        ((x1 - 1) / TILE_SIZE + 1, (y1 - 1) / TILE_SIZE + 1)
    };
    (ty0..ty1).flat_map(move |ty| (tx0..tx1).map(move |tx| (tx, ty)))
}

struct Entry<T> {
    value: T,
    bytes: usize,
    last_used: u64,
}

/// LRU cache of rendered tiles in whatever form the frontend draws them (GDK textures or Android
/// bitmaps, for example), bounded by a byte budget.
///
/// Call [`begin_frame`](Self::begin_frame) before drawing. Tiles used in the current frame are
/// never evicted. Everything else can be, so the budget always holds. `TileCache<T>` is `Send` when
/// `T` is.
pub struct TileCache<T> {
    map: HashMap<TileKey, Entry<T>>,
    bytes: usize,
    budget: usize,
    frame: u64,
    evict_last: Box<dyn Fn(&TileKey) -> bool + Send + Sync>,
}

impl<T> TileCache<T> {
    pub fn new(budget_bytes: usize) -> Self {
        Self {
            map: HashMap::new(),
            bytes: 0,
            budget: budget_bytes,
            frame: 0,
            evict_last: Box::new(|_| false),
        }
    }

    /// Tiles matching `evict_last` (such as low-resolution placeholders) are evicted only after all
    /// others, oldest first.
    ///
    /// They are not pinned: at 0.5 pixels per point an A4 placeholder is about 0.5 MB, so a
    /// 1,000-page document would otherwise outgrow any budget.
    pub fn with_evict_last(
        mut self,
        evict_last: impl Fn(&TileKey) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.evict_last = Box::new(evict_last);
        self
    }

    pub fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Returns the tile and marks it as used in this frame.
    pub fn get(&mut self, key: &TileKey) -> Option<&T> {
        let frame = self.frame;
        self.map.get_mut(key).map(|e| {
            e.last_used = frame;
            &e.value
        })
    }

    pub fn contains(&self, key: &TileKey) -> bool {
        self.map.contains_key(key)
    }

    pub fn insert(&mut self, key: TileKey, value: T, bytes: usize) {
        let entry = Entry {
            value,
            bytes,
            last_used: self.frame,
        };
        self.bytes += bytes;
        if let Some(old) = self.map.insert(key, entry) {
            self.bytes -= old.bytes;
        }
        if self.bytes > self.budget {
            self.evict(self.budget * 3 / 4);
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Evicts least recently used tiles until at most `target` bytes remain: ordinary tiles first,
    /// then `evict_last` ones, never those used in the current frame.
    fn evict(&mut self, target: usize) {
        let mut candidates: Vec<(bool, u64, TileKey)> = self
            .map
            .iter()
            .filter(|(_, e)| e.last_used < self.frame)
            .map(|(k, e)| ((self.evict_last)(k), e.last_used, *k))
            .collect();
        candidates.sort_unstable();
        for (_, _, key) in candidates {
            if self.bytes <= target {
                break;
            }
            if let Some(e) = self.map.remove(&key) {
                self.bytes -= e.bytes;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use specto_geom::{PdfBox, PdfPoint};

    fn letter() -> PageGeometry {
        PageGeometry::new(
            PdfBox::new(PdfPoint::new(0.0, 0.0), PdfPoint::new(612.0, 792.0)),
            None,
            Rotation::None,
        )
    }

    fn key(page: u32, level: i32) -> TileKey {
        let scale = TileScale::from_level(level);
        TileKey {
            doc: DocId(0),
            page,
            rotation: Rotation::None,
            crop: None,
            content: Content::PageOnly,
            scale,
            tx: 0,
            ty: 0,
        }
    }

    #[test]
    fn levels_round_up() {
        assert_eq!(level_for_scale(1.0), 0);
        assert_eq!(level_for_scale(2.0), 2);
        assert_eq!(level_for_scale(2.01), 3);
        assert_eq!(level_for_scale(1.5), 2); // 1.5 rounds up to 2.0, not down to 1.41
        assert_eq!(level_for_scale(0.25), -4);
        for s in [0.3, 0.77, 1.0, 1.25, 1.5, 2.0, 2.625, 3.3] {
            let l = level_for_scale(s);
            assert!(level_scale(l) >= s - 1e-9, "{s}");
            assert!(level_scale(l - 1) < s, "{s}");
        }
    }

    #[test]
    fn tile_scales() {
        for l in -12..=12 {
            let s = TileScale::from_level(l);
            assert_eq!(s.level(), Some(l));
            assert_eq!(level_for_scale(s.get()), l, "level {l}");
        }
        assert_eq!(TileScale::exact(2.0), TileScale::from_level(2));
        assert_eq!(TileScale::exact(1.81).level(), None);
        assert!((TileScale::exact(1.81).get() - 1.81).abs() < 1e-5);
    }

    #[test]
    fn tile_grid_covers_page() {
        let g = letter();
        // Level 2 is scale 2, so the page is 1224 by 1584 pixels, or 3 by 4 tiles.
        let all: Vec<_> = tiles_covering(
            &g,
            TileScale::from_level(2),
            DeviceIntRect::new((0, 0).into(), (9999, 9999).into()),
        )
        .collect();
        assert_eq!(all.len(), 12);
        let last = TileKey {
            tx: 2,
            ty: 3,
            ..key(0, 2)
        };
        let r = last.region(&g).unwrap();
        assert_eq!(
            (r.min.x, r.min.y, r.max.x, r.max.y),
            (1024, 1536, 1224, 1584)
        );
        assert!(TileKey { tx: 3, ..key(0, 2) }.region(&g).is_none());
        // A small area in the middle touches exactly the tiles under it.
        let some: Vec<_> = tiles_covering(
            &g,
            TileScale::from_level(2),
            DeviceIntRect::new((500, 500).into(), (530, 1100).into()),
        )
        .collect();
        assert_eq!(some, vec![(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)]);
        assert_eq!(
            tiles_covering(
                &g,
                TileScale::from_level(2),
                DeviceIntRect::new((5, 5).into(), (5, 9).into())
            )
            .count(),
            0
        );
    }

    #[test]
    fn cache_evicts_lru_ordinary_tiles_first_and_spares_current_frame() {
        let mut c = TileCache::new(100).with_evict_last(|k| k.scale.level() == Some(-4));
        c.begin_frame();
        c.insert(key(0, -4), "thumb", 20); // evicted last
        c.insert(key(1, 0), "old", 40);
        c.begin_frame();
        // 110 is over the budget of 100, so it evicts down to 75: the ordinary tile goes, even
        // though the placeholder is just as old.
        c.insert(key(2, 0), "new", 50);
        assert!(c.contains(&key(0, -4)));
        assert!(!c.contains(&key(1, 0)));
        assert!(c.contains(&key(2, 0)));
        assert_eq!(c.bytes(), 70);

        // Placeholders go too once only they are left, so the budget holds.
        for p in 3..8 {
            c.begin_frame();
            c.insert(key(p, -4), "thumb", 40);
        }
        assert!(c.bytes() <= 100, "{}", c.bytes());
        assert!(c.contains(&key(7, -4))); // the newest
        assert!(!c.contains(&key(0, -4))); // the oldest placeholder went
    }

    #[test]
    fn cache_is_send() {
        fn assert_send<S: Send>() {}
        assert_send::<TileCache<Vec<u8>>>();
    }
}
