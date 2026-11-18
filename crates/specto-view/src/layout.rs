//! Arranging pages into spreads and spreads into a document layout.

use specto_geom::{LayoutPoint, LayoutRect, LayoutSize, PageSize};

/// How pages are arranged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LayoutMode {
    /// One page at a time. Other pages are reached by paging.
    SinglePage,
    /// All pages in one vertical column.
    #[default]
    Continuous,
    /// Two pages side by side, one spread at a time.
    TwoPage,
    /// Two pages side by side, all spreads in one vertical column.
    TwoPageContinuous,
}

impl LayoutMode {
    pub const ALL: [Self; 4] = [
        Self::SinglePage,
        Self::Continuous,
        Self::TwoPage,
        Self::TwoPageContinuous,
    ];

    /// A stable name, for settings files and actions.
    pub fn name(self) -> &'static str {
        match self {
            Self::SinglePage => "single",
            Self::Continuous => "continuous",
            Self::TwoPage => "two-page",
            Self::TwoPageContinuous => "two-page-continuous",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }

    pub fn is_continuous(self) -> bool {
        matches!(self, Self::Continuous | Self::TwoPageContinuous)
    }

    pub fn is_two_page(self) -> bool {
        matches!(self, Self::TwoPage | Self::TwoPageContinuous)
    }
}

/// Groups page indices into spreads.
///
/// With `cover_alone`, the first page stands on its own (like a book cover), so that even pages end
/// up on the left.
pub fn group_spreads(page_count: usize, two_page: bool, cover_alone: bool) -> Vec<Vec<usize>> {
    if !two_page {
        return (0..page_count).map(|i| vec![i]).collect();
    }
    let mut spreads = Vec::new();
    let mut i = 0;
    if cover_alone && page_count > 0 {
        spreads.push(vec![0]);
        i = 1;
    }
    while i < page_count {
        spreads.push((i..(i + 2).min(page_count)).collect());
        i += 2;
    }
    spreads
}

/// Size in points of a spread (pages side by side, no gaps).
pub(crate) fn spread_points(spread: &[usize], sizes: &[PageSize]) -> (f64, f64) {
    spread.iter().fold((0.0, 0.0), |(w, h), &i| {
        (w + sizes[i].width, f64::max(h, sizes[i].height))
    })
}

#[derive(Clone, Debug, Default)]
pub(crate) struct LaidSpread {
    pub pages: Vec<usize>,
    pub rect: LayoutRect,
    /// In a strip ([`layout_strip`]): the cell this spread is centered in, at least as large as the
    /// viewport. Otherwise the same as `rect`.
    pub slot: LayoutRect,
}

/// The document laid out at a particular zoom, in logical pixels.
#[derive(Clone, Debug, Default)]
pub(crate) struct Layout {
    /// Spreads in this layout. In paged modes only the current one.
    pub spreads: Vec<LaidSpread>,
    /// Index into the full spread list of `spreads[0]`.
    pub first_spread: usize,
    /// Page rectangles, or `None` for pages not in this layout (paged modes).
    pub page_rects: Vec<Option<LayoutRect>>,
    pub size: LayoutSize,
}

pub(crate) struct LayoutParams<'a> {
    pub sizes: &'a [PageSize],
    pub spreads: &'a [Vec<usize>],
    /// Which spreads to include (all for continuous, one for paged).
    pub range: std::ops::Range<usize>,
    pub zoom: f64,
    /// Gap between spreads and between the pages of a spread, logical pixels.
    pub gap: f64,
    /// Space around the content, logical pixels.
    pub margin: f64,
    /// Viewport width: narrower content is centered within it.
    pub viewport_width: f64,
}

pub(crate) fn layout(p: &LayoutParams<'_>) -> Layout {
    let spread_size = |s: &[usize]| {
        let (w, h) = spread_points(s, p.sizes);
        (
            w * p.zoom + p.gap * (s.len().saturating_sub(1)) as f64,
            h * p.zoom,
        )
    };
    let widest = p.spreads[p.range.clone()]
        .iter()
        .map(|s| spread_size(s).0)
        .fold(0.0, f64::max);
    let content_width = f64::max(widest + 2.0 * p.margin, p.viewport_width);

    let mut page_rects = vec![None; p.sizes.len()];
    let mut spreads = Vec::with_capacity(p.range.len());
    let mut y = p.margin;
    for pages in &p.spreads[p.range.clone()] {
        let (w, h) = spread_size(pages);
        let mut x = ((content_width - w) / 2.0).max(p.margin);
        for &i in pages {
            let (pw, ph) = (p.sizes[i].width * p.zoom, p.sizes[i].height * p.zoom);
            // Pages of different heights are centered vertically within the spread.
            page_rects[i] = Some(LayoutRect::new(
                LayoutPoint::new(x, y + (h - ph) / 2.0),
                LayoutSize::new(pw, ph),
            ));
            x += pw + p.gap;
        }
        let rect = LayoutRect::new(
            LayoutPoint::new(((content_width - w) / 2.0).max(p.margin), y),
            LayoutSize::new(w, h),
        );
        spreads.push(LaidSpread {
            pages: pages.clone(),
            rect,
            slot: rect,
        });
        y += h + p.gap;
    }
    let height = if spreads.is_empty() {
        2.0 * p.margin
    } else {
        y - p.gap + p.margin
    };
    Layout {
        spreads,
        first_spread: p.range.start,
        page_rects,
        size: LayoutSize::new(content_width, height),
    }
}

pub(crate) struct StripParams<'a> {
    pub sizes: &'a [PageSize],
    pub spreads: &'a [Vec<usize>],
    /// Logical pixels per point for each spread: every spread has its own zoom, so pages of
    /// different sizes can each fit the viewport.
    pub zooms: &'a [f64],
    /// Gap between slots and between the pages of a spread, logical pixels.
    pub gap: f64,
    /// Space around a spread within its slot, logical pixels.
    pub margin: f64,
    pub viewport: (f64, f64),
}

/// Lays all spreads out side by side for swiping between them.
///
/// Each spread is centered in a slot that is at least as large as the viewport (larger when the
/// spread is zoomed in), so a viewport aligned with a slot shows that spread and nothing else.
pub(crate) fn layout_strip(p: &StripParams<'_>) -> Layout {
    let (vw, vh) = p.viewport;
    let mut page_rects = vec![None; p.sizes.len()];
    let mut spreads = Vec::with_capacity(p.spreads.len());
    let mut x = 0.0;
    let mut height: f64 = vh;
    for (pages, &zoom) in p.spreads.iter().zip(p.zooms) {
        let (pw, ph) = spread_points(pages, p.sizes);
        let w = pw * zoom + p.gap * (pages.len().saturating_sub(1)) as f64;
        let h = ph * zoom;
        let slot = LayoutRect::new(
            LayoutPoint::new(x, 0.0),
            LayoutSize::new(
                f64::max(vw, w + 2.0 * p.margin),
                f64::max(vh, h + 2.0 * p.margin),
            ),
        );
        let origin = LayoutPoint::new(
            x + (slot.size.width - w) / 2.0,
            (slot.size.height - h) / 2.0,
        );
        let mut px = origin.x;
        for &i in pages {
            let (iw, ih) = (p.sizes[i].width * zoom, p.sizes[i].height * zoom);
            page_rects[i] = Some(LayoutRect::new(
                LayoutPoint::new(px, origin.y + (h - ih) / 2.0),
                LayoutSize::new(iw, ih),
            ));
            px += iw + p.gap;
        }
        spreads.push(LaidSpread {
            pages: pages.clone(),
            rect: LayoutRect::new(origin, LayoutSize::new(w, h)),
            slot,
        });
        x += slot.size.width + p.gap;
        height = height.max(slot.size.height);
    }
    let width = if spreads.is_empty() { vw } else { x - p.gap };
    Layout {
        spreads,
        first_spread: 0,
        page_rects,
        size: LayoutSize::new(width.max(vw), height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping() {
        assert_eq!(
            group_spreads(3, false, false),
            vec![vec![0], vec![1], vec![2]]
        );
        assert_eq!(
            group_spreads(5, true, false),
            vec![vec![0, 1], vec![2, 3], vec![4]]
        );
        assert_eq!(
            group_spreads(5, true, true),
            vec![vec![0], vec![1, 2], vec![3, 4]]
        );
        assert_eq!(group_spreads(0, true, true), Vec::<Vec<usize>>::new());
    }

    #[test]
    fn strip_centers_each_spread_in_a_viewport_sized_slot() {
        // A portrait page, and a wide one zoomed past the viewport.
        let sizes = [PageSize::new(200.0, 300.0), PageSize::new(400.0, 100.0)];
        let spreads = group_spreads(2, false, false);
        let l = layout_strip(&StripParams {
            sizes: &sizes,
            spreads: &spreads,
            zooms: &[1.0, 3.0],
            gap: 10.0,
            margin: 20.0,
            viewport: (500.0, 400.0),
        });
        // Slot 0 is the viewport, and the page is centered in it.
        assert_eq!(
            l.spreads[0].slot,
            LayoutRect::new(LayoutPoint::new(0.0, 0.0), LayoutSize::new(500.0, 400.0))
        );
        let a = l.page_rects[0].unwrap();
        assert_eq!((a.origin.x, a.origin.y), (150.0, 50.0));
        // Slot 1 starts after a gap and grows with its 1200 by 300 page.
        let slot = l.spreads[1].slot;
        assert_eq!(
            (slot.origin.x, slot.size.width, slot.size.height),
            (510.0, 1240.0, 400.0)
        );
        let b = l.page_rects[1].unwrap();
        assert_eq!((b.origin.x, b.origin.y), (530.0, 50.0));
        assert_eq!(l.size, LayoutSize::new(1750.0, 400.0));
    }

    #[test]
    fn continuous_two_page_layout() {
        // Letter and a shorter landscape-ish page.
        let sizes = [
            PageSize::new(612.0, 792.0),
            PageSize::new(612.0, 396.0),
            PageSize::new(612.0, 792.0),
        ];
        let spreads = group_spreads(3, true, false);
        let l = layout(&LayoutParams {
            sizes: &sizes,
            spreads: &spreads,
            range: 0..spreads.len(),
            zoom: 0.5,
            gap: 10.0,
            margin: 20.0,
            viewport_width: 800.0,
        });
        // Spread 0: 306 + 10 + 306 = 622 wide, 396 tall, centered in 800.
        let a = l.page_rects[0].unwrap();
        let b = l.page_rects[1].unwrap();
        assert_eq!((a.origin.x, a.origin.y), (89.0, 20.0));
        assert_eq!((b.origin.x, b.origin.y), (89.0 + 306.0 + 10.0, 20.0 + 99.0));
        // Spread 1 (page 2 alone) starts after spread 0 plus the gap.
        let c = l.page_rects[2].unwrap();
        assert_eq!((c.origin.x, c.origin.y), (247.0, 20.0 + 396.0 + 10.0));
        assert_eq!(
            l.size,
            LayoutSize::new(800.0, 20.0 + 396.0 + 10.0 + 396.0 + 20.0)
        );
    }
}
