//! Trimming the margins of pages in the view: each page is shown by the box of what it draws, with a
//! little room around it.
//!
//! A page is measured when it is about to be shown, in the layouts of single pages and pairs, which
//! fit each spread to the window on its own. Pages differ in size then, and that is fine. The
//! continuous layouts show pages whole: there one zoom fits the widest page to the window, and
//! measuring every page of a long document up front would take seconds.

use std::collections::HashSet;
use std::panic::AssertUnwindSafe;

use specto_geom::{Crop, PdfBox, PdfPoint};
use specto_render::ContentBounds;

use crate::{Session, view_pages};

/// Space left around what a page draws, in points.
const PADDING: f64 = 8.0;

/// How many pages before and after the current spread are measured too, so that turning the page
/// doesn't wait for it.
const AHEAD: usize = 2;

impl Session {
    /// Whether the pages are shown with their margins trimmed.
    pub fn trim_margins(&self) -> bool {
        self.trim
    }

    /// Shows the pages with their margins trimmed, or whole. Returns whether that changed.
    ///
    /// Only the layouts of single pages and pairs show pages trimmed (see
    /// [`Viewer::crops_pages`](specto_view::Viewer::crops_pages)), and only once
    /// [`trim_shown`](Self::trim_shown) has measured them.
    pub fn set_trim_margins(&mut self, on: bool) -> bool {
        if self.trim == on {
            return false;
        }
        self.trim = on;
        self.update_page_areas();
        self.trim_shown();
        true
    }

    /// Measures the pages about to be shown that aren't measured yet, if they are shown trimmed.
    /// Returns whether pages changed size.
    ///
    /// Call it before laying out a frame, after the view moved. A page takes a fraction of a
    /// millisecond for most documents, so it runs on the caller's thread, like the text layer.
    pub fn trim_shown(&mut self) -> bool {
        if !self.trim || !self.viewer.crops_pages() {
            return false;
        }
        let spread = self.viewer.current_spread();
        let (Some(&first), Some(&last)) = (spread.first(), spread.last()) else {
            return false;
        };
        let end = (last + AHEAD + 1).min(self.edit.page_count());
        let mut measured = false;
        for position in first.saturating_sub(AHEAD)..end {
            let p = self.edit.pages()[position];
            let source = &mut self.sources[p.source.0 as usize];
            if source.content.contains_key(&p.page) {
                continue;
            }
            let pdf = source.doc.pdf();
            let drawn = std::panic::catch_unwind(AssertUnwindSafe(|| {
                ContentBounds::new(pdf).page(p.page as usize)
            }))
            // A page hayro can't read is shown whole.
            .unwrap_or(None);
            source.content.insert(p.page, drawn);
            measured = true;
        }
        measured && self.update_page_areas()
    }

    /// Shows each page trimmed as it should be now. Returns whether the pages shown changed.
    pub(crate) fn update_page_areas(&mut self) -> bool {
        let crops = self.crops();
        let pages = view_pages(&self.edit, &self.sources, &self.variants, &crops);
        self.viewer.set_page_areas(pages)
    }

    /// The part of the page at each position to show where the layout crops: all of it unless the
    /// margins are trimmed and the page is measured.
    pub(crate) fn crops(&mut self) -> Vec<Option<Crop>> {
        if !self.trim {
            return Vec::new();
        }
        // What was drawn on pages since the file was opened counts too.
        let annotated: HashSet<_> = self
            .edit
            .pages_with_annotation_edits()
            .into_iter()
            .collect();
        let pages = self.edit.pages().to_vec();
        (pages.iter().enumerate())
            .map(|(position, p)| {
                let source = &self.sources[p.source.0 as usize];
                let visible = source.geometries[p.page as usize].visible_box();
                let mut drawn = *source.content.get(&p.page)?;
                if annotated.contains(&(p.source, p.page)) {
                    for (_, a) in self.annotations(position) {
                        if a.flags.is_visible() {
                            drawn = Some(drawn.map_or(a.rect, |d| d.union(&a.rect)));
                        }
                    }
                }
                crop(visible, drawn?)
            })
            .collect()
    }
}

/// The part of a page with `visible` as its visible area to show, for what it draws in `drawn`.
/// `None` where that leaves next to nothing to trim.
fn crop(visible: PdfBox, drawn: PdfBox) -> Option<Crop> {
    let padded = PdfBox::new(
        PdfPoint::new(drawn.min.x - PADDING, drawn.min.y - PADDING),
        PdfPoint::new(drawn.max.x + PADDING, drawn.max.y + PADDING),
    );
    let shown = padded.intersection(&visible)?;
    let trimmed = (shown.min.x - visible.min.x)
        + (shown.min.y - visible.min.y)
        + (visible.max.x - shown.max.x)
        + (visible.max.y - shown.max.y);
    (trimmed >= 1.0).then(|| Crop::new(shown))
}

#[cfg(test)]
mod tests {
    use specto_geom::{Crop, PdfBox, PdfPoint};

    use super::crop;

    fn pdf_box(x0: f64, y0: f64, x1: f64, y1: f64) -> PdfBox {
        PdfBox::new(PdfPoint::new(x0, y0), PdfPoint::new(x1, y1))
    }

    #[test]
    fn a_page_is_trimmed_to_what_it_draws() {
        let letter = pdf_box(0.0, 0.0, 612.0, 792.0);
        // Text one inch in from each side, with room around it.
        assert_eq!(
            crop(letter, pdf_box(72.0, 72.0, 540.0, 720.0)),
            Some(Crop::new(pdf_box(64.0, 64.0, 548.0, 728.0)))
        );
        // Near an edge, the room stops at the page.
        assert_eq!(
            crop(letter, pdf_box(3.0, 72.0, 540.0, 720.0)),
            Some(Crop::new(pdf_box(0.0, 64.0, 548.0, 728.0)))
        );
        // Up to the edges leaves nothing to trim.
        assert_eq!(crop(letter, pdf_box(2.0, 0.0, 612.0, 791.0)), None);
        // What is drawn beside the page isn't shown.
        assert_eq!(crop(letter, pdf_box(700.0, 0.0, 800.0, 100.0)), None);
    }
}
