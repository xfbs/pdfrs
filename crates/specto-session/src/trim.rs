//! Trimming the margins of pages in the view: the box of what each page draws, measured in a
//! background thread, and the part of each page shown.
//!
//! Pages are trimmed to steady boxes, so that they don't change size from page to page: one for
//! the pages at even positions and one for those at odd positions, since books have the wider
//! margin on alternate sides. The margins of most pages decide each box. A page that draws further
//! out, such as a cover or a wide table, gets as much more as it needs. Nothing a page draws is cut
//! off, and annotations count as drawn.

use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use specto_doc::SourceId;
use specto_geom::{Crop, PdfBox, PdfPoint};
use specto_render::ContentBounds;
use specto_source::PdfData;

use crate::{Session, view_pages};

/// Space left around what a page draws, in points.
const PADDING: f64 = 8.0;

/// The share of pages, those with the narrowest margins, that may need more than the steady box.
const OUTLIERS: f64 = 0.2;

/// Flags shared with the measuring thread.
#[derive(Default)]
pub(crate) struct Measurer {
    /// Set to stop it.
    stop: AtomicBool,
    /// Set by the thread when it's done.
    done: AtomicBool,
}

/// What the measuring thread hands over: the box of what each page of a file draws. Pass it to
/// [`Session::measured`].
pub struct Measured {
    source: SourceId,
    boxes: Vec<Option<PdfBox>>,
}

impl Session {
    /// Whether the pages are shown with their margins trimmed.
    pub fn trim_margins(&self) -> bool {
        self.trim
    }

    /// Shows the pages with their margins trimmed, or whole. Returns whether that changed.
    ///
    /// Pages are trimmed once they are measured: see [`start_measuring`](Self::start_measuring).
    pub fn set_trim_margins(&mut self, on: bool) -> bool {
        if self.trim == on {
            return false;
        }
        self.trim = on;
        if !on {
            self.stop_measuring();
        }
        self.update_page_areas();
        true
    }

    /// Starts measuring the pages of files not measured yet in a background thread, if the margins
    /// are trimmed and that isn't running already. Returns whether it started.
    ///
    /// The thread parses the files for itself. It calls `sink` once for each file: get that to
    /// whoever holds the session and pass it to [`measured`](Self::measured). The thread ends when
    /// `sink` returns `false`, when the session is dropped, or when trimming is turned off. Call
    /// this again after inserting pages of another file.
    pub fn start_measuring(
        &mut self,
        mut sink: impl FnMut(Measured) -> bool + Send + 'static,
    ) -> bool {
        if !self.trim || (self.measurer.as_ref()).is_some_and(|m| !m.done.load(Ordering::Relaxed)) {
            return false;
        }
        let used: HashSet<SourceId> = self.edit.pages().iter().map(|p| p.source).collect();
        let mut files: Vec<(SourceId, PdfData, Option<String>, usize)> = (self.sources.iter())
            .enumerate()
            .map(|(i, s)| (SourceId(i as u32), s))
            .filter(|(id, s)| s.content.is_none() && used.contains(id))
            .map(|(id, s)| (id, s.data.clone(), s.password.clone(), s.geometries.len()))
            .collect();
        if files.is_empty() {
            return false;
        }
        files.sort_by_key(|f| f.0);
        let measurer = Arc::new(Measurer::default());
        let flags = measurer.clone();
        let spawned = std::thread::Builder::new()
            .name("page margins".into())
            .spawn(move || {
                struct Done(Arc<Measurer>);
                impl Drop for Done {
                    fn drop(&mut self) {
                        self.0.done.store(true, Ordering::Relaxed);
                    }
                }
                let done = Done(flags);
                let stop = &done.0.stop;
                for (source, data, password, count) in files {
                    let Ok(pdf) = specto_source::open_pdf(data, password.as_deref()) else {
                        continue;
                    };
                    let mut bounds = ContentBounds::new(&pdf);
                    let mut boxes = Vec::with_capacity(count);
                    for page in 0..count {
                        if stop.load(Ordering::Relaxed) {
                            return;
                        }
                        match std::panic::catch_unwind(AssertUnwindSafe(|| bounds.page(page))) {
                            Ok(found) => boxes.push(found),
                            // Its caches may be broken: start over with new ones. The page counts
                            // as drawing everywhere, which trims nothing off it.
                            Err(_) => {
                                bounds = ContentBounds::new(&pdf);
                                boxes.push(Some(PdfBox::new(
                                    PdfPoint::new(f64::MIN, f64::MIN),
                                    PdfPoint::new(f64::MAX, f64::MAX),
                                )));
                            }
                        }
                    }
                    if !sink(Measured { source, boxes }) {
                        return;
                    }
                }
            });
        if spawned.is_err() {
            return false;
        }
        self.measurer = Some(measurer);
        true
    }

    /// Stops the measuring thread, if one is running.
    pub(crate) fn stop_measuring(&mut self) {
        if let Some(m) = self.measurer.take() {
            m.stop.store(true, Ordering::Relaxed);
        }
    }

    /// Takes in a file from the measuring thread. Returns whether the pages shown changed.
    pub fn measured(&mut self, Measured { source, boxes }: Measured) -> bool {
        let Some(s) = self.sources.get_mut(source.0 as usize) else {
            return false;
        };
        if boxes.len() != s.geometries.len() {
            return false;
        }
        s.content = Some(boxes);
        self.trim && self.update_page_areas()
    }

    /// Shows each page trimmed as it should be now. Returns whether the pages shown changed.
    pub(crate) fn update_page_areas(&mut self) -> bool {
        let crops = self.crops();
        let pages = view_pages(&self.edit, &self.sources, &self.variants, &crops);
        if pages == self.viewer.pages() {
            return false;
        }
        self.viewer.set_page_areas(pages);
        true
    }

    /// The part of the page at each position to show: all of it unless the margins are trimmed.
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
        let pages: Vec<_> = self.edit.pages().to_vec();
        let measured: Vec<Page> = (pages.iter().enumerate())
            .map(|(position, p)| {
                let source = &self.sources[p.source.0 as usize];
                let visible = source.geometries[p.page as usize].visible_box();
                let Some(content) = &source.content else {
                    return Page {
                        visible,
                        drawn: None,
                    };
                };
                let mut drawn = content[p.page as usize];
                if annotated.contains(&(p.source, p.page)) {
                    for (_, a) in self.annotations(position) {
                        if a.flags.is_visible() {
                            drawn = Some(drawn.map_or(a.rect, |d| d.union(&a.rect)));
                        }
                    }
                }
                Page {
                    visible,
                    drawn: Some(drawn),
                }
            })
            .collect();
        crops(&measured)
    }
}

/// A page to trim: its visible area, and the box of what it draws if it was measured (`None`
/// inside for a page that draws nothing).
#[derive(Clone, Copy, Debug)]
struct Page {
    visible: PdfBox,
    drawn: Option<Option<PdfBox>>,
}

/// How far in from each side of `visible` what is drawn begins: left, bottom, right, top.
fn margins(visible: PdfBox, drawn: PdfBox) -> [f64; 4] {
    [
        drawn.min.x - visible.min.x,
        drawn.min.y - visible.min.y,
        visible.max.x - drawn.max.x,
        visible.max.y - drawn.max.y,
    ]
    .map(|m| m.max(0.0))
}

/// The part of each of `pages` to show, in order. `None` for a page shown whole.
fn crops(pages: &[Page]) -> Vec<Option<Crop>> {
    // The steady margins of the pages at even and at odd positions: on each side, the narrowest
    // but for the outliers.
    let steady: [Option<[f64; 4]>; 2] = [0, 1].map(|parity| {
        let all: Vec<[f64; 4]> = (pages.iter().skip(parity).step_by(2))
            .filter_map(|p| Some(margins(p.visible, p.drawn??)))
            .collect();
        if all.is_empty() {
            return None;
        }
        let at = (all.len() as f64 * OUTLIERS) as usize;
        Some(std::array::from_fn(|side| {
            let mut values: Vec<f64> = all.iter().map(|m| m[side]).collect();
            values.sort_by(f64::total_cmp);
            values[at]
        }))
    });
    (pages.iter().enumerate())
        .map(|(position, page)| {
            let steady = steady[position % 2]?;
            let margins = match page.drawn? {
                Some(drawn) => {
                    let own = margins(page.visible, drawn);
                    std::array::from_fn(|side| steady[side].min(own[side]))
                }
                // A blank page.
                None => steady,
            };
            let [left, bottom, right, top] = margins.map(|m| (m - PADDING).max(0.0));
            if left + bottom + right + top < 1.0 {
                return None;
            }
            let v = page.visible;
            Some(Crop::new(PdfBox::new(
                PdfPoint::new(v.min.x + left, v.min.y + bottom),
                PdfPoint::new(v.max.x - right, v.max.y - top),
            )))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use specto_geom::{Crop, PdfBox, PdfPoint};

    use super::{Page, crops};

    fn pdf_box(x0: f64, y0: f64, x1: f64, y1: f64) -> PdfBox {
        PdfBox::new(PdfPoint::new(x0, y0), PdfPoint::new(x1, y1))
    }

    fn letter(drawn: Option<Option<PdfBox>>) -> Page {
        Page {
            visible: pdf_box(0.0, 0.0, 612.0, 792.0),
            drawn,
        }
    }

    fn crop(x0: f64, y0: f64, x1: f64, y1: f64) -> Option<Crop> {
        Some(Crop::new(pdf_box(x0, y0, x1, y1)))
    }

    #[test]
    fn pages_are_trimmed_to_steady_boxes() {
        // Text one inch in from each side, the wider margin on alternate sides.
        let left = Some(pdf_box(90.0, 72.0, 530.0, 720.0));
        let right = Some(pdf_box(82.0, 72.0, 522.0, 720.0));
        let mut pages: Vec<Page> = (0..20)
            .map(|i| letter(Some(if i % 2 == 0 { left } else { right })))
            .collect();
        // A short last line and a page with only a heading are inside the steady box.
        pages[2].drawn = Some(Some(pdf_box(90.0, 400.0, 530.0, 720.0)));
        pages[4].drawn = Some(Some(pdf_box(200.0, 650.0, 400.0, 700.0)));
        // A blank page.
        pages[6].drawn = Some(None);
        // A cover that fills the page.
        pages[0].drawn = Some(Some(pdf_box(0.0, 0.0, 612.0, 792.0)));
        // A wide table.
        pages[5].drawn = Some(Some(pdf_box(40.0, 72.0, 560.0, 720.0)));

        let found = crops(&pages);
        assert_eq!(found[0], None, "the cover is shown whole");
        for i in [2, 4, 6, 8] {
            assert_eq!(found[i], crop(82.0, 64.0, 538.0, 728.0), "page {i}");
        }
        for i in [1, 3, 7, 9] {
            assert_eq!(found[i], crop(74.0, 64.0, 530.0, 728.0), "page {i}");
        }
        assert_eq!(
            found[5],
            crop(32.0, 64.0, 568.0, 728.0),
            "the table isn't cut off"
        );
    }

    #[test]
    fn pages_not_measured_are_shown_whole() {
        let text = Some(Some(pdf_box(72.0, 72.0, 540.0, 720.0)));
        let found = crops(&[letter(text), letter(None), letter(text)]);
        assert_eq!(
            found,
            [
                crop(64.0, 64.0, 548.0, 728.0),
                None,
                crop(64.0, 64.0, 548.0, 728.0)
            ]
        );
        // Nothing measured at all, and nothing drawn on any page.
        assert_eq!(crops(&[letter(None)]), [None]);
        assert_eq!(crops(&[letter(Some(None))]), [None]);
        // Content up to the edges leaves nothing to trim.
        let full = Some(Some(pdf_box(0.0, 0.0, 612.0, 792.0)));
        assert_eq!(crops(&[letter(full)]), [None]);
    }
}
