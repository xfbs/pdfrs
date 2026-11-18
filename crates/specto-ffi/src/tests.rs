use std::sync::mpsc;
use std::time::Duration;

use super::*;

/// What the app does through JNI (`Native.kt`), called directly.
impl ViewSession {
    fn open(
        data: Vec<u8>,
        password: Option<String>,
        cache_bytes: u64,
        listener: Arc<dyn ViewListener>,
    ) -> Result<Arc<Self>, PdfError> {
        Self::open_data(data.into(), password, cache_bytes, listener)
    }

    fn set_thumbnail_range(&self, first: u32, count: u32) {
        hot::set_thumbnail_range(self.id, first, count);
    }

    fn set_viewport(&self, width: f64, height: f64, scale: f64) {
        hot::set_viewport(self.id, width, height, scale);
    }

    fn scroll_by(&self, dx: f64, dy: f64) {
        hot::scroll_by(self.id, dx, dy);
    }

    fn zoom_by(&self, factor: f64, x: f64, y: f64) {
        hot::zoom_by(self.id, factor, x, y);
    }

    fn select_to(&self, x: f64, y: f64) {
        hot::select_to(self.id, x, y);
    }

    fn frame(&self) -> Frame {
        self.shared.frame(&mut self.shared.lock())
    }

    fn take_finished(&self) -> Vec<FinishedTile> {
        self.shared.take_finished()
    }
}

/// `pages` pages of 200 by 300 points, each with a red triangle.
fn sample_pdf(pages: usize) -> Vec<u8> {
    let content = "1 0 0 rg 0 0 m 200 0 l 200 300 l f";
    let kids: String = (0..pages).map(|i| format!("{} 0 R ", 10 + i)).collect();
    let mut pdf = format!(
        "%PDF-1.7\n\
         1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
         2 0 obj << /Type /Pages /Kids [{kids}] /Count {pages} /MediaBox [0 0 200 300] >> endobj\n\
         4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\n",
        content.len(),
    );
    for i in 0..pages {
        pdf += &format!(
            "{} 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n",
            10 + i
        );
    }
    pdf += "trailer << /Root 1 0 R /Size 99 >>\n%%EOF\n";
    pdf.into_bytes()
}

struct Listener(Mutex<mpsc::Sender<()>>);

impl ViewListener for Listener {
    fn tiles_ready(&self) {
        let _ = self.0.lock().unwrap().send(());
    }

    // The same signal: tests that search wait for either.
    fn search_changed(&self) {
        let _ = self.0.lock().unwrap().send(());
    }
}

fn open(pages: usize, cache_bytes: u64) -> (Arc<ViewSession>, mpsc::Receiver<()>) {
    let (tx, rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(sample_pdf(pages), None, cache_bytes, listener).unwrap();
    (session, rx)
}

/// Plays the frontend: copies finished tiles until the view is settled and sharp.
///
/// Returns the final frame and all tile ids evicted on the way.
fn settle(session: &ViewSession, rx: &mpsc::Receiver<()>) -> (Frame, Vec<u64>) {
    let mut evicted = Vec::new();
    loop {
        let frame = session.frame();
        evicted.extend(&frame.evicted);
        if frame.complete && frame.settled {
            return (frame, evicted);
        }
        // Wake up for new tiles, or when the view is due to settle.
        let wait = match frame.settles_in_ms {
            Some(ms) => Duration::from_millis(u64::from(ms) + 1),
            None => Duration::from_secs(10),
        };
        if rx.recv_timeout(wait).is_err() {
            assert!(frame.settles_in_ms.is_some(), "tiles should arrive");
        }
        for tile in session.take_finished() {
            let copied = claim_tile(session.id(), tile.id, |b| {
                assert_eq!((b.width, b.height), (tile.width, tile.height));
                true
            });
            assert!(copied);
        }
    }
}

#[test]
fn rejects_garbage() {
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    assert!(matches!(
        ViewSession::open(b"nope".to_vec(), None, 1 << 20, listener),
        Err(PdfError::Invalid)
    ));
}

#[cfg(unix)]
#[test]
fn opens_from_a_file_descriptor() {
    use std::os::fd::IntoRawFd;
    let path = std::env::temp_dir().join(format!("specto-ffi-fd-{}.pdf", std::process::id()));
    std::fs::write(&path, sample_pdf(3)).unwrap();
    let fd = std::fs::File::open(&path).unwrap().into_raw_fd();
    std::fs::remove_file(&path).unwrap();
    let file = PdfFile::from_fd(fd).unwrap();
    assert!(!file.is_mapped(), "small files are read");
    assert_eq!(file.size(), sample_pdf(3).len() as u64);
    // Parts of it can be read back, clamped to what is there.
    assert_eq!(file.read(0, 8), b"%PDF-1.7");
    assert_eq!(file.read(file.size() - 6, 100), b"%%EOF\n");
    assert_eq!(file.read(file.size() + 5, 100), b"");
    assert_eq!(file.read(1, 0), b"");
    assert_eq!(file.read(0, u32::MAX).len() as u64, file.size());
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    // The same file serves several attempts (with passwords, for instance).
    for _ in 0..2 {
        let session =
            ViewSession::open_file(file.clone(), None, 1 << 20, listener.clone()).unwrap();
        assert_eq!(session.page_count(), 3);
    }
    assert!(matches!(PdfFile::from_fd(-1), Err(PdfError::Io { .. })));
}

#[test]
fn frame_becomes_sharp_and_covers_the_page() {
    let (session, rx) = open(3, 64 << 20);
    assert_eq!(session.page_count(), 3);
    // 200 dp wide at density 2: at fit width the page is 2 * (200 - 2 * 8) pixels wide.
    session.set_viewport(200.0, 300.0, 2.0);
    let first = session.frame();
    assert!(!first.complete);
    assert_eq!(first.zoom_mode, ZoomMode::FitWidth);

    let (frame, _) = settle(&session, &rx);
    assert_eq!(frame.current_page, 0);
    let page = &frame.pages[0];
    assert_eq!(
        (page.rect.x, page.rect.y, page.rect.width),
        (16.0, 16.0, 368.0)
    );
    assert!(!page.tiles.is_empty());
    // Sharp tiles tile the visible part of the page without gaps: their union spans the page's
    // width.
    let right = page
        .tiles
        .iter()
        .map(|t| t.dest.x + t.dest.width)
        .fold(0.0, f32::max);
    assert!((right - (page.rect.x + page.rect.width)).abs() < 1.0);
}

/// One page like [`sample_pdf`]'s, whose resources also name an image of 120 megapixels.
///
/// Nothing draws the image, so the page renders as quickly as any, but the scheduler goes by what a
/// page could draw and treats it as expensive.
fn heavy_pdf() -> Vec<u8> {
    let content = "1 0 0 rg 0 0 m 200 0 l 200 300 l f";
    format!(
        "%PDF-1.7\n\
         1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
         2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >> endobj\n\
         3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R \
         /Resources << /XObject << /Big 5 0 R >> >> >> endobj\n\
         4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\n\
         5 0 obj << /Type /XObject /Subtype /Image /Width 12000 /Height 10000 \
         /ColorSpace /DeviceGray /BitsPerComponent 1 /Length 0 >> stream\n\nendstream endobj\n\
         trailer << /Root 1 0 R /Size 6 >>\n%%EOF\n",
        content.len(),
    )
    .into_bytes()
}

#[test]
fn expensive_pages_are_busy_until_sharp() {
    let (tx, rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(heavy_pdf(), None, 64 << 20, listener).unwrap();
    session.set_viewport(400.0, 600.0, 2.0);
    let first = session.frame();
    assert!(!first.complete);
    assert!(first.pages[0].busy, "nothing of it is rendered yet");
    let (sharp, _) = settle(&session, &rx);
    assert!(sharp.pages.iter().all(|p| !p.busy));
    // Zoomed in, it has new tiles to wait for.
    session.zoom_by(3.0, 200.0, 300.0);
    assert!(session.frame().pages[0].busy);
    let (sharp, _) = settle(&session, &rx);
    assert!(sharp.pages.iter().all(|p| !p.busy));

    // An ordinary page never says so, sharp or not.
    let (session, rx) = open(1, 64 << 20);
    session.set_viewport(400.0, 600.0, 2.0);
    let first = session.frame();
    assert!(!first.complete);
    assert!(first.pages.iter().all(|p| !p.busy));
    let (sharp, _) = settle(&session, &rx);
    assert!(sharp.pages.iter().all(|p| !p.busy));
}

#[test]
fn scrolling_and_zooming_move_the_view() {
    let (session, rx) = open(5, 64 << 20);
    session.set_viewport(200.0, 300.0, 2.0);
    settle(&session, &rx);

    session.go_to_page(3);
    assert_eq!(session.frame().current_page, 3);
    session.scroll_by(0.0, -100_000.0);
    assert_eq!(session.frame().current_page, 0);

    let before = session.frame().zoom;
    session.zoom_by(2.0, 100.0, 150.0);
    let frame = session.frame();
    assert!((frame.zoom / before - 2.0).abs() < 1e-9);
    assert_eq!(frame.zoom_mode, ZoomMode::Fixed);
    session.fit();
    assert!((session.frame().zoom - before).abs() < 1e-9);
}

#[test]
fn settles_to_exact_tiles_after_a_zoom() {
    let (session, rx) = open(2, 64 << 20);
    // Density 2.625, as on many phones: fit width lands on no zoom level.
    session.set_viewport(200.0, 300.0, 2.625);
    let (frame, _) = settle(&session, &rx);
    assert_eq!(covered_exactly(&frame.pages[0]), 1.0);

    // During a pinch the view never settles, however long it pauses.
    session.begin_zoom_gesture();
    session.zoom_by(1.3, 100.0, 150.0);
    let zooming = session.frame();
    assert!(!zooming.settled);
    assert_eq!(zooming.settles_in_ms, None);

    // Afterwards it asks to be redrawn, and then sharpens.
    session.end_zoom_gesture();
    let ended = session.frame();
    assert!(!ended.settled);
    assert!(ended.settles_in_ms.is_some());
    let (frame, _) = settle(&session, &rx);
    assert_eq!(covered_exactly(&frame.pages[0]), 1.0);
}

/// Share of the page's on-screen area covered by one to one tiles (which sit on whole device
/// pixels). The viewport in these tests is 525 by 787.5 pixels.
fn covered_exactly(page: &PageDraw) -> f32 {
    let visible = |r: &Rect| {
        let w = (r.x + r.width).min(525.0) - r.x.max(0.0);
        let h = (r.y + r.height).min(787.5) - r.y.max(0.0);
        w.max(0.0) * h.max(0.0)
    };
    let mut exact = 0.0;
    for t in page.tiles.iter().filter(|t| t.exact) {
        assert_eq!((t.dest.x.fract(), t.dest.y.fract()), (0.0, 0.0));
        let clipped = Rect {
            x: t.dest.x.max(page.rect.x),
            y: t.dest.y.max(page.rect.y),
            width: (t.dest.x + t.dest.width).min(page.rect.x + page.rect.width)
                - t.dest.x.max(page.rect.x),
            height: (t.dest.y + t.dest.height).min(page.rect.y + page.rect.height)
                - t.dest.y.max(page.rect.y),
        };
        exact += visible(&clipped);
    }
    exact / visible(&page.rect)
}

#[test]
fn small_budget_evicts_tiles_that_left_the_screen() {
    // Room for roughly one screen of tiles.
    let (session, rx) = open(30, 400 * 600 * 4);
    session.set_viewport(200.0, 300.0, 2.0);
    let mut evicted = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for page in [0, 10, 20, 29] {
        session.go_to_page(page);
        let (frame, gone) = settle(&session, &rx);
        evicted.extend(gone);
        // Nothing the frame draws has been reported as evicted.
        for tile in frame.pages.iter().flat_map(|p| &p.tiles) {
            assert!(!evicted.contains(&tile.id));
            seen.insert(tile.id);
        }
    }
    evicted.extend(session.frame().evicted);
    assert!(
        !evicted.is_empty(),
        "the budget should have forced evictions"
    );

    // Trimming keeps exactly what the last frame used.
    let last = session.frame();
    session.trim_memory();
    let after = session.frame();
    let ids = |f: &Frame| -> Vec<u64> {
        f.pages
            .iter()
            .flat_map(|p| &p.tiles)
            .map(|t| t.id)
            .collect()
    };
    assert_eq!(ids(&last), ids(&after));
    assert!(after.complete);
}

#[test]
fn thumbnails_reuse_placeholders_and_survive_eviction_while_shown() {
    // Room for roughly one screen of tiles.
    let (session, rx) = open(30, 400 * 600 * 4);
    session.set_viewport(200.0, 300.0, 2.0);
    assert_eq!(session.page_sizes().len(), 30);
    assert_eq!(
        session.page_sizes()[0],
        PageSize {
            width: 200.0,
            height: 300.0
        }
    );

    // The grid shows pages 20 to 25, far from the page view at the top.
    session.set_thumbnail_range(20, 6);
    let first = session.frame();
    assert_eq!((first.thumbnail_first, first.thumbnails.len()), (20, 6));
    assert!(first.thumbnails.iter().all(|&id| id == 0));
    let ready = |session: &ViewSession| loop {
        let (frame, _) = settle(session, &rx);
        if frame.thumbnails.iter().all(|&id| id != 0) {
            return frame.thumbnails;
        }
        rx.recv_timeout(Duration::from_secs(10))
            .expect("thumbnails");
        for tile in session.take_finished() {
            claim_tile(session.id(), tile.id, |_| true);
        }
    };
    let shown = ready(&session);

    // Scrolling the page view through the document churns the small cache, but never evicts a
    // thumbnail that is on screen.
    let mut evicted = Vec::new();
    for page in [5, 12, 28] {
        session.go_to_page(page);
        evicted.extend(settle(&session, &rx).1);
    }
    assert!(!evicted.is_empty());
    assert!(shown.iter().all(|id| !evicted.contains(id)));
    assert_eq!(session.frame().thumbnails, shown);

    // A range past the end is clamped, and an empty one reports nothing.
    session.set_thumbnail_range(28, 10);
    assert_eq!(session.frame().thumbnails.len(), 2);
    session.set_thumbnail_range(0, 0);
    assert!(session.frame().thumbnails.is_empty());
}

#[test]
fn paged_mode_swipes_one_fitted_page_at_a_time() {
    let (session, rx) = open(5, 64 << 20);
    // Pages of 200 by 300 points in a view of 216 by 316 dp: fitted, 1 dp per point.
    session.set_viewport(216.0, 316.0, 2.0);
    session.go_to_page(2);
    session.set_view_mode(ViewMode::Paged);
    let (frame, _) = settle(&session, &rx);
    assert!(frame.paged);
    assert_eq!(
        (frame.current_page, frame.zoom, frame.zoom_factor),
        (2, 1.0, 1.0)
    );
    assert_eq!(frame.pages.len(), 1);
    let rect = frame.pages[0].rect;
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (16.0, 16.0, 400.0, 600.0)
    );
    assert_eq!(session.snap_delta(0.0, 0.0), None);

    // Dragging a third of the way and letting go returns to the page, and a flick turns it. Through
    // the per-frame path, as the app does.
    let id = session.id();
    assert!(hot::scroll_by(id, 72.0, 0.0));
    assert_eq!(session.frame().pages.len(), 2);
    assert_eq!(
        session.snap_delta(0.0, 0.0),
        Some(Offset { x: -72.0, y: 0.0 })
    );
    let turn = session.snap_delta(500.0, 0.0).unwrap();
    assert_eq!(
        turn,
        Offset {
            x: 216.0 + 8.0 - 72.0,
            y: 0.0
        }
    );
    hot::scroll_by(id, turn.x, turn.y);
    let (frame, _) = settle(&session, &rx);
    assert_eq!((frame.current_page, frame.pages.len()), (3, 1));
    assert_eq!(session.snap_delta(0.0, 0.0), None);

    // Zoomed in, a fling stays on the page, and `fit` returns to the page.
    session.zoom_by(2.0, 108.0, 158.0);
    assert_eq!(session.frame().zoom_factor, 2.0);
    assert!(hot::scroll_in_page_by(id, 10_000.0, 0.0));
    assert_eq!(session.frame().current_page, 3);
    assert_eq!(session.snap_delta(0.0, 0.0), None);
    session.fit();
    let frame = session.frame();
    assert_eq!(
        (frame.current_page, frame.zoom_factor, frame.zoom_mode),
        (3, 1.0, ZoomMode::FitPage)
    );
    assert_eq!(session.snap_delta(0.0, 0.0), None);

    // Back to scrolling: same page, fitted to the width.
    session.set_view_mode(ViewMode::Continuous);
    let frame = session.frame();
    assert!(!frame.paged);
    assert_eq!(
        (frame.current_page, frame.zoom_mode),
        (3, ZoomMode::FitWidth)
    );
}

/// Four pages of 200 by 300 points with an outline (Chapter 1 with Section 1.1 in it, Chapter 2)
/// and, on page 1, a link to page 3 and a link to a web page.
fn linked_pdf() -> Vec<u8> {
    let content = "1 0 0 rg 0 0 m 200 0 l 200 300 l f";
    format!(
        "%PDF-1.7\n\
         1 0 obj << /Type /Catalog /Pages 2 0 R /Outlines 20 0 R >> endobj\n\
         2 0 obj << /Type /Pages /Kids [10 0 R 11 0 R 12 0 R 13 0 R] /Count 4 /MediaBox [0 0 200 300] >> endobj\n\
         4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\n\
         10 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R /Annots [30 0 R 31 0 R] >> endobj\n\
         11 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n\
         12 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n\
         13 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n\
         20 0 obj << /Type /Outlines /First 21 0 R /Last 23 0 R /Count 3 >> endobj\n\
         21 0 obj << /Title (Chapter 1) /Parent 20 0 R /Next 23 0 R /First 22 0 R /Last 22 0 R /Count 1 /Dest [10 0 R /XYZ 0 300 0] >> endobj\n\
         22 0 obj << /Title (Section 1.1) /Parent 21 0 R /Dest [11 0 R /Fit] >> endobj\n\
         23 0 obj << /Title (Chapter 2) /Parent 20 0 R /Prev 21 0 R /Dest [13 0 R /XYZ 0 150 0] >> endobj\n\
         30 0 obj << /Type /Annot /Subtype /Link /Rect [20 200 180 240] /Dest [12 0 R /Fit] >> endobj\n\
         31 0 obj << /Type /Annot /Subtype /Link /Rect [20 100 180 140] /A << /S /URI /URI (https://example.com/) >> >> endobj\n\
         trailer << /Root 1 0 R /Size 99 >>\n%%EOF\n",
        content.len(),
    )
    .into_bytes()
}

#[test]
fn outline_and_links() {
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(linked_pdf(), None, 64 << 20, listener).unwrap();
    // Fit width: 1 dp per point, page 1 at (8, 8).
    session.set_viewport(216.0, 316.0, 2.0);

    let outline = session.outline();
    let summary: Vec<(&str, u32, Option<u32>, bool)> = outline
        .iter()
        .map(|e| (e.title.as_str(), e.depth, e.page, e.has_children))
        .collect();
    assert_eq!(
        summary,
        vec![
            ("Chapter 1", 0, Some(0), true),
            ("Section 1.1", 1, Some(1), false),
            ("Chapter 2", 0, Some(3), false),
        ]
    );
    assert_eq!(session.activate_outline(2), Activated::Moved);
    assert_eq!(session.frame().current_page, 3);
    assert_eq!(session.activate_outline(1), Activated::Moved);
    assert_eq!(session.frame().current_page, 1);
    assert_eq!(session.activate_outline(7), Activated::Nothing);

    // Back returns to where each entry was activated from, and Forward to where it led.
    assert!(session.can_go_back() && !session.can_go_forward());
    assert!(session.go_back());
    assert_eq!(session.frame().current_page, 3);
    assert!(session.go_back());
    assert_eq!(session.frame().current_page, 0);
    assert!(!session.go_back());
    assert!(session.go_forward());
    assert_eq!(session.frame().current_page, 3);
    // A page asked for by its number is a jump, and following the reader isn't.
    session.jump_to_page(2);
    assert!(!session.can_go_forward());
    session.go_to_page(1);
    assert!(session.go_back());
    assert_eq!(session.frame().current_page, 3);

    // The links on page 1: PDF y 200 to 240 is 60 to 100 points from the page's top.
    session.go_to_page(0);
    assert_eq!(session.tap(108.0, 8.0 + 20.0), Activated::Nothing);
    assert_eq!(
        session.tap(108.0, 8.0 + 180.0),
        Activated::Uri {
            uri: "https://example.com/".into()
        }
    );
    assert_eq!(session.frame().current_page, 0);
    assert_eq!(session.tap(108.0, 8.0 + 80.0), Activated::Moved);
    assert_eq!(session.frame().current_page, 2);

    // The same in paged mode, where the page is centered and fitted.
    session.set_view_mode(ViewMode::Paged);
    session.go_to_page(0);
    assert_eq!(session.tap(108.0, 8.0 + 80.0), Activated::Moved);
    let frame = session.frame();
    assert_eq!((frame.current_page, frame.pages.len()), (2, 1));
    assert_eq!(session.snap_delta(0.0, 0.0), None);
    // In the margin beside the page there is nothing to tap.
    assert_eq!(session.tap(2.0, 150.0), Activated::Nothing);
}

/// Four pages of 200 by 300 points reading "Page 1" to "Page 4", numbered i, ii, 1, 2, with an
/// outline entry for the second and for the fourth.
///
/// A well-formed file, cross-reference table and all: saving reads strictly.
fn book_pdf() -> Vec<u8> {
    let mut objects: Vec<(usize, String)> = vec![
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /Outlines 20 0 R \
             /PageLabels << /Nums [0 << /S /r >> 2 << /S /D >>] >> >>"
                .into(),
        ),
        (
            2,
            "<< /Type /Pages /Kids [10 0 R 12 0 R 14 0 R 16 0 R] /Count 4 \
             /MediaBox [0 0 200 300] /Resources << /Font << /F1 3 0 R >> >> >>"
                .into(),
        ),
        (
            3,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        ),
        (
            20,
            "<< /Type /Outlines /First 21 0 R /Last 22 0 R /Count 2 >>".into(),
        ),
        (
            21,
            "<< /Title (Second) /Parent 20 0 R /Next 22 0 R /Dest [12 0 R /Fit] >>".into(),
        ),
        (
            22,
            "<< /Title (Fourth) /Parent 20 0 R /Prev 21 0 R /Dest [16 0 R /Fit] >>".into(),
        ),
    ];
    for i in 0..4 {
        let content = format!("BT /F1 16 Tf 20 250 Td (Page {}) Tj ET", i + 1);
        objects.push((
            10 + 2 * i,
            format!(
                "<< /Type /Page /Parent 2 0 R /Contents {} 0 R >>",
                11 + 2 * i
            ),
        ));
        objects.push((
            11 + 2 * i,
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ));
    }
    assemble(objects)
}

/// A well-formed file of `objects`, each with its number.
fn assemble(mut objects: Vec<(usize, String)>) -> Vec<u8> {
    objects.sort_by_key(|(number, _)| *number);
    let size = objects.last().map_or(1, |(number, _)| number + 1);
    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = vec![None; size];
    for (number, body) in &objects {
        offsets[*number] = Some(out.len());
        out += &format!("{number} 0 obj\n{body}\nendobj\n");
    }
    let xref = out.len();
    out += &format!("xref\n0 {size}\n");
    for (number, offset) in offsets.iter().enumerate() {
        out += &match offset {
            Some(offset) => format!("{offset:010} 00000 n \n"),
            None if number == 0 => "0000000000 65535 f \n".into(),
            None => "0000000000 00000 f \n".into(),
        };
    }
    out += &format!("trailer\n<< /Size {size} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n");
    out.into_bytes()
}

/// A page of 200 by 300 points with a form: a text field "name" of at most 10 characters, which
/// the form calls "Your name", at x 20 to 180, y 240 to 260, a checkbox "agree" at x 20 to 40, y
/// 180 to 200, a menu "country" at x 20 to 180, y 120 to 140, which holds "de", a menu "color"
/// that takes text of one's own at y 80 to 100, and a list "days" that takes several choices at y
/// 20 to 60. Reset, "name" holds "Nobody".
fn form_pdf() -> Vec<u8> {
    let stream = |content: &str| {
        format!(
            "<< /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length {} >>\nstream\n{content}\n\
             endstream",
            content.len()
        )
    };
    assemble(vec![
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [6 0 R 7 0 R 10 0 R 11 0 R 12 0 R] \
             /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 3 0 R >> >> >> >>"
                .into(),
        ),
        (
            2,
            "<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 200 300] /Resources << >> >>"
                .into(),
        ),
        (
            3,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
                .into(),
        ),
        (
            4,
            "<< /Type /Page /Parent 2 0 R /Annots [6 0 R 7 0 R 10 0 R 11 0 R 12 0 R] >>".into(),
        ),
        (
            6,
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /TU (Your name) /MaxLen 10 /DV (Nobody) \
             /Rect [20 240 180 260] /P 4 0 R >>"
                .into(),
        ),
        (
            7,
            "<< /Type /Annot /Subtype /Widget /FT /Btn /T (agree) /Rect [20 180 40 200] /P 4 0 R \
             /V /Off /AS /Off /AP << /N << /Yes 8 0 R /Off 9 0 R >> >> >>"
                .into(),
        ),
        (8, stream("0 1 0 rg 0 0 20 20 re f")),
        (9, stream("")),
        (
            10,
            "<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (country) /V (de) \
             /Opt [[(de) (Germany)] [(fr) (France)] (Other)] /Rect [20 120 180 140] /P 4 0 R >>"
                .into(),
        ),
        (
            11,
            "<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 393216 /T (color) \
             /Opt [(Red) (Green)] /Rect [20 80 180 100] /P 4 0 R >>"
                .into(),
        ),
        (
            12,
            "<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 2097152 /T (days) \
             /Opt [(Mon) (Tue) (Wed)] /Rect [20 20 180 60] /P 4 0 R >>"
                .into(),
        ),
    ])
}

#[test]
fn filling_in_a_form() {
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(form_pdf(), None, 1 << 20, listener).unwrap();
    // PDF (x, y) is at (8 + x, 308 - y) in the view.
    session.set_viewport(216.0, 316.0, 2.0);
    session.set_view_mode(ViewMode::Paged);
    let tap = |x: f64, y: f64| session.tap_field(8.0 + x, 308.0 - y);

    // Blank paper is no field.
    assert_eq!(tap(100.0, 220.0), FieldTap::Nothing);
    assert!(!session.edit_state().modified);

    // A tap ticks the box, and that is all there is to it: nothing is outlined.
    assert_eq!(tap(30.0, 190.0), FieldTap::Changed);
    let state = session.edit_state();
    assert!(state.modified);
    assert_eq!(state.undo.unwrap().kind, EditKind::FillField);
    assert!(session.frame().picked.is_empty());
    assert!(!session.end_field());

    // A tap on the text field asks for its text, and outlines it meanwhile (in device pixels).
    let FieldTap::Text {
        field,
        label,
        multiline,
        password,
        max_length,
        text,
    } = tap(100.0, 250.0)
    else {
        panic!("a text field is there");
    };
    assert_eq!(label.as_deref(), Some("Your name"));
    assert_eq!((multiline, password, max_length), (false, false, Some(10)));
    assert_eq!(text, "");
    let outline = session.frame().picked;
    assert_eq!(outline.len(), 4);
    let (left, right) = (2.0 * (8.0 + 20.0), 2.0 * (8.0 + 180.0));
    assert!(
        outline
            .iter()
            .all(|p| p.x >= left - 1.0 && p.x <= right + 1.0)
    );
    // It holds what was typed, as far as it takes it.
    assert!(session.set_field_text(field, "Ada Lovelace".into()));
    assert!(session.end_field());
    assert!(session.frame().picked.is_empty());
    let FieldTap::Text { text, .. } = tap(100.0, 250.0) else {
        panic!("a text field is there");
    };
    assert_eq!(text, "Ada Lovela");
    // The same text again changes nothing.
    assert!(!session.set_field_text(field, "Ada Lovela".into()));
    session.end_field();

    // A tap on the menu says what it offers and holds.
    let FieldTap::Choose {
        field,
        label,
        options,
        typed,
        chosen,
    } = tap(100.0, 130.0)
    else {
        panic!("a menu is there");
    };
    assert_eq!((label, typed, chosen), (None, None, Some(0)));
    assert_eq!(options, ["Germany", "France", "Other"]);
    assert!(session.choose_option(field, 1));
    assert!(!session.choose_option(field, 1));
    assert!(!session.choose_option(field, 7));
    session.end_field();
    let FieldTap::Choose { chosen, .. } = tap(100.0, 130.0) else {
        panic!("a menu is there");
    };
    assert_eq!(chosen, Some(1));
    session.end_field();

    // A menu that takes text of one's own says what it shows, and holds what is typed.
    let FieldTap::Choose { field, typed, .. } = tap(100.0, 90.0) else {
        panic!("a menu is there");
    };
    assert_eq!(typed.as_deref(), Some(""));
    assert!(session.set_field_text(field, "Teal".into()));
    session.end_field();
    let FieldTap::Choose { typed, chosen, .. } = tap(100.0, 90.0) else {
        panic!("a menu is there");
    };
    assert_eq!((typed.as_deref(), chosen), (Some("Teal"), None));
    session.end_field();

    // A list that takes several holds any of what it offers.
    let FieldTap::ChooseSeveral {
        field,
        options,
        chosen,
        ..
    } = tap(100.0, 40.0)
    else {
        panic!("a list is there");
    };
    assert_eq!(options, ["Mon", "Tue", "Wed"]);
    assert!(chosen.is_empty());
    assert!(session.choose_options(field, vec![2, 0]));
    assert!(!session.choose_options(field, vec![0, 2]));
    session.end_field();
    let FieldTap::ChooseSeveral { chosen, .. } = tap(100.0, 40.0) else {
        panic!("a list is there");
    };
    assert_eq!(chosen, [0, 2]);
    session.end_field();

    // On request, every field that can be filled in is tinted.
    assert!(session.has_form());
    assert!(session.frame().shapes.is_empty());
    session.set_highlight_fields(true);
    let shapes = session.frame().shapes;
    assert_eq!(shapes.len(), 5);
    assert!(shapes.iter().all(|s| s.fill.is_some() && s.closed));
    session.set_highlight_fields(false);
    assert!(session.frame().shapes.is_empty());

    // Each was an edit, taken back one by one.
    for _ in 0..5 {
        assert_eq!(session.undo().unwrap().kind, EditKind::FillField);
    }
    assert!(!session.edit_state().modified);
    let FieldTap::Text { text, .. } = tap(100.0, 250.0) else {
        panic!("a text field is there");
    };
    assert_eq!(text, "");
    session.end_field();

    // Resetting sets every field back as one edit: the name has a text to start from.
    assert!(session.reset_form());
    assert_eq!(session.edit_state().undo.unwrap().kind, EditKind::ResetForm);
    let FieldTap::Text { text, .. } = tap(100.0, 250.0) else {
        panic!("a text field is there");
    };
    assert_eq!(text, "Nobody");
    assert!(!session.reset_form());
}

#[test]
fn editing_pages() {
    let (tx, rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(book_pdf(), None, 64 << 20, listener).unwrap();
    session.set_viewport(216.0, 316.0, 2.0);
    let sizes = |s: &ViewSession| -> Vec<(f32, f32)> {
        let sizes = s.page_sizes();
        sizes.iter().map(|p| (p.width, p.height)).collect()
    };
    let pages = |s: &ViewSession| -> Vec<Option<u32>> {
        let outline = s.outline();
        outline.iter().map(|e| e.page).collect()
    };
    let untouched = EditState {
        modified: false,
        undo: None,
        redo: None,
    };
    assert_eq!(session.edit_state(), untouched);
    assert_eq!(session.undo(), None);
    assert_eq!(pages(&session), [Some(1), Some(3)]);

    // Turned, a page is as wide as it was high, and is drawn that way.
    assert!(session.rotate_pages(vec![0], true));
    assert_eq!(sizes(&session)[..2], [(300.0, 200.0), (200.0, 300.0)]);
    let rotate = EditStep {
        kind: EditKind::Rotate,
        pages: 1,
    };
    assert_eq!(
        session.edit_state(),
        EditState {
            modified: true,
            undo: Some(rotate),
            redo: None
        }
    );
    let (frame, _) = settle(&session, &rx);
    let first = &frame.pages[0];
    assert_eq!(first.page, 0);
    assert!(first.rect.width > first.rect.height);
    assert!((covered_exactly(first) - 1.0).abs() < 1e-3);
    // The other way round twice: upside down, counted as one more each.
    assert!(session.rotate_pages(vec![0, 1], false));
    assert_eq!(sizes(&session)[..2], [(200.0, 300.0), (300.0, 200.0)]);
    assert_eq!(session.undo().map(|e| e.pages), Some(2));

    // Without a page, the others move up. They keep their printed numbers, and the outline knows
    // where its pages are now.
    assert!(session.delete_pages(vec![1]));
    assert_eq!(session.page_count(), 3);
    assert_eq!(session.page_labels().unwrap(), ["i", "1", "2"]);
    assert_eq!(pages(&session), [None, Some(2)]);
    assert_eq!(session.activate_outline(0), Activated::Nothing);
    assert_eq!(session.activate_outline(1), Activated::Moved);
    assert_eq!(session.frame().current_page, 2);
    assert_eq!(session.find_page("2".into()), Some(2));
    // A document keeps at least one page.
    assert!(!session.delete_pages(vec![0, 1, 2]));
    assert!(!session.delete_pages(vec![7]));

    // To the end, and back to the front.
    assert_eq!(session.move_pages(vec![0], 3), Some(2));
    assert_eq!(session.page_labels().unwrap(), ["1", "2", "i"]);
    assert_eq!(pages(&session), [None, Some(1)]);
    assert_eq!(session.move_pages(vec![2], 0), Some(0));
    assert_eq!(session.move_pages(vec![0], 0), None);

    // Selecting everything means the pages there are now.
    session.select_all();
    let text = session.selected_text().unwrap();
    assert_eq!(
        text.lines().collect::<Vec<_>>(),
        ["Page 1", "Page 3", "Page 4"]
    );
    // An edit lets go of the selection.
    assert!(session.rotate_pages(vec![2], true));
    assert!(session.selected_text().is_none());

    // Everything can be taken back and done again.
    let mut undone = 0;
    while session.undo().is_some() {
        undone += 1;
    }
    assert_eq!(undone, 5);
    assert_eq!(session.page_count(), 4);
    assert_eq!(pages(&session), [Some(1), Some(3)]);
    assert_eq!(sizes(&session), [(200.0, 300.0); 4]);
    let state = session.edit_state();
    assert!(!state.modified && state.undo.is_none());
    assert_eq!(state.redo, Some(rotate));
    assert_eq!(session.redo(), Some(rotate));
    assert!(session.edit_state().modified);
    let (frame, _) = settle(&session, &rx);
    assert!(frame.complete);
}

#[cfg(unix)]
#[test]
fn saving_what_was_edited() {
    use std::os::fd::IntoRawFd;
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(book_pdf(), None, 64 << 20, listener.clone()).unwrap();
    assert!(session.rotate_pages(vec![0], true));
    assert!(session.delete_pages(vec![1]));
    assert!(session.edit_state().modified);

    // Onto a file that was longer before: nothing of it is left behind.
    let path = std::env::temp_dir().join(format!("specto-ffi-save-{}.pdf", std::process::id()));
    std::fs::write(&path, vec![b'x'; 100_000]).unwrap();
    let fd = std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .into_raw_fd();
    // A copy on the side first: the document stays unsaved.
    let copy = std::env::temp_dir().join(format!("specto-ffi-copy-{}.pdf", std::process::id()));
    let copy_fd = std::fs::File::create(&copy).unwrap().into_raw_fd();
    session.save_to_fd(copy_fd, false).unwrap();
    assert!(session.edit_state().modified);
    // Unless the copy is put in the document's place: then what it holds is saved, and an edit made
    // since is not.
    assert!(session.rotate_pages(vec![0], true));
    session.written_is_saved();
    assert!(session.edit_state().modified);
    assert!(session.undo().is_some());
    assert!(!session.edit_state().modified);
    assert!(session.redo().is_some());
    assert!(session.undo().is_some());
    assert!(session.delete_pages(vec![0]));
    assert!(session.undo().is_some());
    // Said once, it is used up.
    assert!(session.rotate_pages(vec![1], true));
    session.written_is_saved();
    assert!(session.edit_state().modified);
    assert!(session.undo().is_some());
    assert!(!session.edit_state().modified);
    assert!(session.undo().is_some());
    assert!(session.edit_state().modified);
    assert!(session.redo().is_some());
    session.save_to_fd(fd, true).unwrap();
    let written = std::fs::read(&path).unwrap();
    assert_eq!(std::fs::read(&copy).unwrap(), written);
    std::fs::remove_file(&copy).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(written.starts_with(b"%PDF-"));
    assert!(written.len() < 100_000 && !written.ends_with(b"x"));

    // Saved, but the edits can still be taken back, which is a change again.
    let state = session.edit_state();
    assert!(!state.modified && state.undo.is_some());
    assert!(session.undo().is_some());
    assert!(session.edit_state().modified);

    let saved = ViewSession::open(written, None, 64 << 20, listener.clone()).unwrap();
    assert_eq!(saved.page_count(), 3);
    let first = saved.page_sizes()[0];
    assert_eq!((first.width, first.height), (300.0, 200.0));
    assert_eq!(saved.page_labels().unwrap(), ["i", "1", "2"]);
    saved.select_all();
    let text = saved.selected_text().unwrap();
    assert_eq!(
        text.lines().collect::<Vec<_>>(),
        ["Page 1", "Page 3", "Page 4"]
    );
    assert!(!saved.edit_state().modified);

    // For printing: the same document, and it counts for nothing.
    let print = std::env::temp_dir().join(format!("specto-ffi-print-{}.pdf", std::process::id()));
    let print_fd = std::fs::File::create(&print).unwrap().into_raw_fd();
    session.print_to_fd(print_fd).unwrap();
    let printed = std::fs::read(&print).unwrap();
    std::fs::remove_file(&print).unwrap();
    let printed = ViewSession::open(printed, None, 64 << 20, listener.clone()).unwrap();
    assert_eq!(printed.page_count(), session.page_count());
    session.written_is_saved();
    assert!(session.edit_state().modified);

    // A descriptor that can't be written to: an error, and still unsaved.
    let read_only = std::fs::File::open("/dev/null").unwrap().into_raw_fd();
    assert!(matches!(
        session.save_to_fd(read_only, true),
        Err(PdfError::Save { .. })
    ));
    assert!(session.edit_state().modified);
    assert!(matches!(
        session.save_to_fd(-1, true),
        Err(PdfError::Save { .. })
    ));

    // An untidy file (no cross-reference table) is saved as a tidy one.
    let loose = ViewSession::open(sample_pdf(2), None, 64 << 20, listener.clone()).unwrap();
    assert!(loose.rotate_pages(vec![0], true));
    let path = std::env::temp_dir().join(format!("specto-ffi-loose-{}.pdf", std::process::id()));
    let fd = std::fs::File::create(&path).unwrap().into_raw_fd();
    loose.save_to_fd(fd, true).unwrap();
    let written = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let tidy = ViewSession::open(written, None, 64 << 20, listener).unwrap();
    let first = tidy.page_sizes()[0];
    assert_eq!(
        (tidy.page_count(), first.width, first.height),
        (2, 300.0, 200.0)
    );
}

#[test]
fn printed_page_numbers() {
    // Front matter i to ii, then 1, 2 and so on: the page printed "1" is the third.
    let pdf = String::from_utf8(sample_pdf(5)).unwrap().replace(
        "/Type /Catalog",
        "/Type /Catalog /PageLabels << /Nums [0 << /S /r >> 2 << /S /D >>] >>",
    );
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(pdf.into_bytes(), None, 1 << 20, listener).unwrap();
    assert_eq!(
        session.page_labels(),
        Some(["i", "ii", "1", "2", "3"].map(String::from).to_vec())
    );
    // Labels win over positions, and case doesn't matter. Positions still work where no label says
    // otherwise.
    assert_eq!(session.find_page("ii".into()), Some(1));
    assert_eq!(session.find_page(" II ".into()), Some(1));
    assert_eq!(session.find_page("1".into()), Some(2));
    assert_eq!(session.find_page("5".into()), Some(4));
    assert_eq!(session.find_page("6".into()), None);
    assert_eq!(session.find_page("x".into()), None);

    // Without labels (or with plain ones) there is nothing to show, and text is a position.
    let (plain, _rx) = open(5, 1 << 20);
    assert_eq!(plain.page_labels(), None);
    assert_eq!(plain.find_page("3".into()), Some(2));
    assert_eq!(plain.find_page("0".into()), None);
}

#[test]
fn saved_view_continues_where_the_reader_left_off() {
    let (a, _rx) = open(8, 1 << 20);
    a.set_viewport(216.0, 316.0, 2.0);
    // Fresh documents are continuous and fitted, at the top.
    let fresh = a.saved_view();
    assert_eq!((fresh.page, fresh.paged, fresh.zoom), (0, false, None));
    assert!(!fresh.two_pages && !fresh.cover_alone);

    // Paged, on page 6, zoomed in and scrolled down a little.
    a.set_view_mode(ViewMode::Paged);
    a.go_to_page(5);
    a.zoom_by(2.0, 108.0, 158.0);
    hot::scroll_in_page_by(a.id(), 0.0, 40.0);
    let saved = a.saved_view();
    assert_eq!((saved.page, saved.paged, saved.zoom), (5, true, Some(2.0)));
    assert!(saved.page_offset > 0.0);

    let (b, _rx) = open(8, 1 << 20);
    b.set_viewport(216.0, 316.0, 2.0);
    b.restore_view(saved);
    let frame = b.frame();
    assert!(frame.paged);
    assert_eq!(
        (frame.current_page, frame.zoom, frame.zoom_factor),
        (5, 2.0, 2.0)
    );
    assert_eq!(b.saved_view(), saved);
    assert_eq!(b.snap_delta(0.0, 0.0), None);

    // Continuous, half way down page 3.
    a.set_view_mode(ViewMode::Continuous);
    a.go_to_page(2);
    a.scroll_by(0.0, 150.0);
    let saved = a.saved_view();
    assert_eq!((saved.page, saved.paged, saved.zoom), (2, false, None));
    b.restore_view(saved);
    let frame = b.frame();
    assert_eq!((frame.paged, frame.zoom_mode), (false, ZoomMode::FitWidth));
    assert!((b.saved_view().page_offset - saved.page_offset).abs() < 1e-9);

    // A page that no longer exists is clamped to the last one.
    let (short, _rx) = open(2, 1 << 20);
    short.set_viewport(216.0, 316.0, 2.0);
    short.restore_view(SavedView { page: 7, ..saved });
    assert_eq!(short.frame().current_page, 1);
}

#[test]
fn fit_page_or_fit_width() {
    // Pages of 200 by 300 points in a wide view: fitted whole they are 300 high, fitted to the
    // width three times as large.
    let (a, _rx) = open(4, 1 << 20);
    a.set_viewport(616.0, 316.0, 2.0);
    a.set_view_mode(ViewMode::Paged);
    let whole = a.frame();
    assert_eq!((whole.zoom, whole.zoom_factor), (1.0, 1.0));
    assert!(!a.saved_view().fit_width);

    a.set_fit_width(true);
    let wide = a.frame();
    assert_eq!((wide.zoom, wide.zoom_factor), (3.0, 1.0));
    assert_eq!(wide.zoom_mode, ZoomMode::FitWidth);
    // The rest of the page is reached by scrolling within it.
    hot::scroll_in_page_by(a.id(), 0.0, 100.0);
    assert!(a.frame().pages[0].rect.y < -90.0);

    // Turning the page and zooming come back to the chosen fit.
    a.turn_page(true);
    let next = a.frame();
    assert_eq!(
        (next.current_page, next.zoom, next.zoom_factor),
        (1, 3.0, 1.0)
    );
    a.zoom_by(2.0, 308.0, 158.0);
    assert_eq!(a.frame().zoom_factor, 2.0);
    a.fit();
    assert_eq!(a.frame().zoom, 3.0);

    // It is saved with the view, zoomed in or not.
    a.zoom_by(2.0, 308.0, 158.0);
    let saved = a.saved_view();
    assert!(saved.fit_width && saved.zoom == Some(6.0));
    let (b, _rx) = open(4, 1 << 20);
    b.set_viewport(616.0, 316.0, 2.0);
    b.restore_view(saved);
    assert_eq!(b.saved_view(), saved);
    assert_eq!(b.frame().zoom_factor, 2.0);
    b.fit();
    assert_eq!(b.frame().zoom, 3.0);

    // Each view mode starts with its own fit. Choosing the same mode again changes nothing.
    a.set_view_mode(ViewMode::Paged);
    assert!(a.saved_view().fit_width);
    a.set_view_mode(ViewMode::Continuous);
    assert!(a.saved_view().fit_width);
    a.set_fit_width(false);
    let whole = a.frame();
    assert_eq!((whole.zoom, whole.zoom_factor), (1.0, 1.0));
    a.set_view_mode(ViewMode::Paged);
    assert!(!a.saved_view().fit_width);
    a.set_view_mode(ViewMode::Continuous);
    assert!(a.saved_view().fit_width);
}

/// Two pages of 200 by 300 points with text in Helvetica at 16 points: "Hello brave world" (from x
/// 20, with "brave" from about 61, "world" from about 105) with "second line here" below it, and
/// "Other page".
fn text_pdf() -> Vec<u8> {
    let first = "BT /F1 16 Tf 20 250 Td (Hello brave world) Tj 0 -30 Td (second line here) Tj ET";
    let second = "BT /F1 16 Tf 20 250 Td (Other page) Tj ET";
    format!(
        "%PDF-1.7\n\
         1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
         2 0 obj << /Type /Pages /Kids [10 0 R 11 0 R] /Count 2 /MediaBox [0 0 200 300]\
           /Resources << /Font << /F1 3 0 R >> >> >> endobj\n\
         3 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n\
         4 0 obj << /Length {} >> stream\n{first}\nendstream endobj\n\
         5 0 obj << /Length {} >> stream\n{second}\nendstream endobj\n\
         10 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n\
         11 0 obj << /Type /Page /Parent 2 0 R /Contents 5 0 R >> endobj\n\
         trailer << /Root 1 0 R /Size 99 >>\n%%EOF\n",
        first.len(),
        second.len(),
    )
    .into_bytes()
}

#[test]
fn selecting_text() {
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(text_pdf(), None, 1 << 20, listener).unwrap();
    // Fitted, a page is shown one to one with its top-left corner at (8, 8), so PDF (x, y) is at (8
    // + x, 8 + 300 - y) in the view.
    session.set_viewport(216.0, 316.0, 2.0);
    session.set_view_mode(ViewMode::Paged);
    let at = |x: f64, y: f64| (8.0 + x, 308.0 - y);
    let text = |s: &ViewSession| s.selected_text().unwrap_or_default();
    let frame = session.frame();
    assert!(!frame.selection && frame.highlights.is_empty());
    assert_eq!(session.selected_text(), None);

    // A long press away from any text selects nothing. On a word, it selects the word.
    let (x, y) = at(100.0, 100.0);
    assert!(!session.select_word_at(x, y));
    assert!(!session.frame().selection);
    let (x, y) = at(30.0, 255.0);
    assert!(session.select_word_at(x, y));
    assert_eq!(text(&session), "Hello");

    // It is highlighted where the word is, in device pixels (two per logical one), and both ends
    // are on screen for their handles.
    let frame = session.frame();
    assert!(frame.selection);
    let [h] = frame.highlights[..] else {
        panic!("one line is selected: {:?}", frame.highlights);
    };
    assert!((h.start_bottom.x - 56.0).abs() < 4.0, "{h:?}");
    assert!((h.end_bottom.x - 129.0).abs() < 6.0, "{h:?}");
    assert!(h.start_top.y < h.start_bottom.y && h.start_bottom.y < 2.0 * 70.0);
    let (start, end) = (frame.selection_start.unwrap(), frame.selection_end.unwrap());
    assert_eq!((start.bottom, start.top), (h.start_bottom, h.start_top));
    assert_eq!((end.bottom, end.top), (h.end_bottom, h.end_top));

    // Moving on after the long press selects word by word.
    let (x, y) = at(80.0, 255.0);
    session.select_to(x, y);
    assert_eq!(text(&session), "Hello brave");

    // Taking the end by its handle moves it character by character, here on to the next line.
    session.grab_selection(true);
    let (x, y) = at(72.0, 225.0);
    session.select_to(x, y);
    assert_eq!(text(&session), "Hello brave world\nsecond");
    assert_eq!(session.frame().highlights.len(), 2);

    // The start likewise, and the end stays.
    session.grab_selection(false);
    let (x, y) = at(62.0, 255.0);
    session.select_to(x, y);
    assert_eq!(text(&session), "brave world\nsecond");

    // An end dragged onto the other doesn't leave an empty selection.
    session.grab_selection(true);
    let (x, y) = at(62.0, 255.0);
    session.select_to(x, y);
    assert_eq!(text(&session), "brave world\nsecond");

    // The frame's compact form carries the same.
    let mut buf = vec![0u8; 1 << 16];
    let n = hot::frame(session.id(), &mut buf);
    let hot_frame = decode_frame(&buf[..n as usize]);
    let frame = session.frame();
    assert!(hot_frame.selection);
    assert_eq!(hot_frame.highlights, frame.highlights);
    assert_eq!(hot_frame.selection_start, frame.selection_start);
    assert_eq!(hot_frame.selection_end, frame.selection_end);

    // Everything, across pages. An end on a page that isn't shown has no place for a handle.
    session.select_all();
    assert_eq!(
        text(&session),
        "Hello brave world\nsecond line here\nOther page"
    );
    let frame = session.frame();
    assert!(frame.selection && frame.selection_start.is_some());
    if frame.pages.iter().all(|p| p.page != 1) {
        assert_eq!(frame.selection_end, None);
    }

    // The selection stays with the text when the view changes.
    session.zoom_by(2.0, 108.0, 158.0);
    assert_eq!(
        text(&session),
        "Hello brave world\nsecond line here\nOther page"
    );
    assert!(!session.frame().highlights.is_empty());

    session.clear_selection();
    let frame = session.frame();
    assert!(!frame.selection && frame.highlights.is_empty());
    assert_eq!((frame.selection_start, frame.selection_end), (None, None));
    assert_eq!(session.selected_text(), None);
}

#[test]
fn searching() {
    let (tx, rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(text_pdf(), None, 1 << 20, listener).unwrap();
    session.set_viewport(216.0, 316.0, 2.0);
    session.set_view_mode(ViewMode::Paged);
    assert_eq!(session.search_status(), None);
    let kinds = |s: &ViewSession| -> Vec<HighlightKind> {
        s.frame().highlights.iter().map(|h| h.kind).collect()
    };

    // The first search has the document read in the background. The listener hears when there is
    // more to show. "page" is only on the second page, which the view then goes to.
    session.search("page".into());
    let status = loop {
        let status = session.search_status().unwrap();
        if status.pages_searched == status.pages {
            break status;
        }
        rx.recv_timeout(Duration::from_secs(10))
            .expect("the indexer reports its progress");
    };
    assert_eq!(
        (status.matches, status.current, status.pages),
        (1, Some(0), 2)
    );
    assert_eq!(session.frame().current_page, 1);
    assert_eq!(kinds(&session), [HighlightKind::CurrentMatch]);

    // From now on searches are answered at once. Case and spacing don't matter. The view returns to
    // the first page for this one.
    session.search("HELLO   brave".into());
    let status = session.search_status().unwrap();
    assert_eq!((status.matches, status.current), (1, Some(0)));
    let frame = session.frame();
    assert_eq!(frame.current_page, 0);
    let [h] = frame.highlights[..] else {
        panic!("one match on one line: {:?}", frame.highlights);
    };
    // "Hello brave" runs from x 20 to about 101 pt: 56 to about 218 pixels.
    assert!((h.start_bottom.x - 56.0).abs() < 4.0, "{h:?}");
    assert!((h.end_bottom.x - 218.0).abs() < 8.0, "{h:?}");

    // Three matches on the first page and one on the second, starting with the first one on the
    // page being looked at.
    session.search("o".into());
    let status = session.search_status().unwrap();
    assert_eq!((status.matches, status.current), (4, Some(0)));
    assert_eq!(
        kinds(&session),
        [
            HighlightKind::CurrentMatch,
            HighlightKind::Match,
            HighlightKind::Match
        ]
    );
    // Stepping goes through them and around, turning pages as needed.
    session.search_step(true);
    assert_eq!(session.search_status().unwrap().current, Some(1));
    assert_eq!(
        kinds(&session),
        [
            HighlightKind::Match,
            HighlightKind::CurrentMatch,
            HighlightKind::Match
        ]
    );
    session.search_step(true);
    session.search_step(true);
    assert_eq!(session.search_status().unwrap().current, Some(3));
    assert_eq!(session.frame().current_page, 1);
    assert_eq!(kinds(&session), [HighlightKind::CurrentMatch]);
    session.search_step(true);
    assert_eq!(session.search_status().unwrap().current, Some(0));
    assert_eq!(session.frame().current_page, 0);
    session.search_step(false);
    assert_eq!(session.search_status().unwrap().current, Some(3));
    assert_eq!(session.frame().current_page, 1);

    // A search started on the second page begins with the match there.
    session.search("o".into());
    assert_eq!(session.search_status().unwrap().current, Some(3));

    // Matches and the selection are highlighted side by side.
    session.go_to_page(0);
    session.select_all();
    let both = kinds(&session);
    assert!(both.contains(&HighlightKind::Selection) && both.contains(&HighlightKind::Match));
    assert_eq!(
        both.first(),
        Some(&HighlightKind::Match),
        "the selection is drawn on top"
    );
    session.clear_selection();

    // Nothing found, and no search at all.
    session.search("zebra".into());
    let status = session.search_status().unwrap();
    assert_eq!((status.matches, status.current), (0, None));
    assert!(session.frame().highlights.is_empty());
    session.search("  ".into());
    assert_eq!(session.search_status(), None);
    session.search("o".into());
    session.end_search();
    assert_eq!(session.search_status(), None);
    assert!(session.frame().highlights.is_empty());
}

#[test]
fn what_a_document_says_about_itself() {
    let (plain, _rx) = open(1, 1 << 20);
    let info = plain.info();
    assert_eq!(info.pdf_version, "1.7");
    assert_eq!((info.title, info.author, info.created), (None, None, None));

    let mut pdf = String::from_utf8(sample_pdf(1)).unwrap();
    pdf = pdf.replace(
        "trailer << /Root 1 0 R",
        "5 0 obj << /Title (Notes) /Creator (ed) /CreationDate (D:20240305142210+01'00') >> endobj\n\
         trailer << /Info 5 0 R /Root 1 0 R",
    );
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(pdf.into_bytes(), None, 1 << 20, listener).unwrap();
    let info = session.info();
    assert_eq!(info.title.as_deref(), Some("Notes"));
    assert_eq!(info.creator.as_deref(), Some("ed"));
    assert_eq!(info.created.as_deref(), Some("2024-03-05T14:22:10+01:00"));
    assert_eq!((info.author, info.modified), (None, None));
}

#[test]
fn two_pages_side_by_side() {
    let (session, rx) = open(5, 64 << 20);
    // Wide enough for two pages of 200 by 300 points at 1 dp per point: 8 + 200 + 8 + 200 + 8 by 8
    // + 300 + 8.
    session.set_viewport(424.0, 316.0, 2.0);
    session.set_view_mode(ViewMode::Paged);
    session.go_to_page(2);
    session.set_spread(true, false);
    let (frame, _) = settle(&session, &rx);
    assert!(frame.two_pages && !frame.cover_alone && frame.paged);
    // Pages 3 and 4 as a pair, fitted. The reader's page stays on screen.
    assert_eq!(
        (frame.current_page, frame.zoom, frame.zoom_factor),
        (2, 1.0, 1.0)
    );
    let pages: Vec<(u32, f32)> = frame.pages.iter().map(|p| (p.page, p.rect.x)).collect();
    assert_eq!(pages, vec![(2, 16.0), (3, 432.0)]);
    assert_eq!((frame.current_page, frame.last_page), (2, 3));

    // A flick turns to the next pair (here the last page, alone).
    hot::scroll_by(session.id(), 50.0, 0.0);
    let turn = session.snap_delta(500.0, 0.0).unwrap();
    hot::scroll_by(session.id(), turn.x, turn.y);
    let frame = session.frame();
    assert_eq!((frame.current_page, frame.pages.len()), (4, 1));
    assert_eq!(frame.last_page, 4);

    // The first page alone shifts the pairs: [1] [2 3] [4 5].
    session.set_spread(true, true);
    let frame = session.frame();
    assert!(frame.cover_alone);
    assert_eq!(
        frame.pages.iter().map(|p| p.page).collect::<Vec<_>>(),
        vec![3, 4]
    );

    // Scrolling instead of swiping keeps the pairs, fitted to the width.
    session.set_view_mode(ViewMode::Continuous);
    let frame = session.frame();
    assert!(frame.two_pages && !frame.paged);
    assert_eq!(frame.zoom_mode, ZoomMode::FitWidth);

    // Saved and restored with the rest of the view.
    let saved = session.saved_view();
    assert!(saved.two_pages && saved.cover_alone);
    let (other, _rx) = open(5, 1 << 20);
    other.set_viewport(424.0, 316.0, 2.0);
    other.restore_view(saved);
    let frame = other.frame();
    assert!(frame.two_pages && frame.cover_alone && !frame.paged);
    assert_eq!(other.saved_view(), saved);

    // And back to one page at a time.
    session.set_spread(false, false);
    assert!(!session.frame().two_pages);
}

#[test]
fn turning_pages_without_gestures() {
    let (session, _rx) = open(5, 1 << 20);
    session.set_viewport(424.0, 316.0, 2.0);
    let page = |s: &ViewSession| s.frame().current_page;

    // Continuous: page by page, stopping at both ends.
    session.turn_page(false);
    assert_eq!(page(&session), 0);
    session.turn_page(true);
    session.turn_page(true);
    assert_eq!(page(&session), 2);

    // Paged, also while zoomed in (where a swipe would only pan).
    session.set_view_mode(ViewMode::Paged);
    session.zoom_by(3.0, 100.0, 100.0);
    session.turn_page(true);
    assert_eq!(page(&session), 3);
    assert_eq!(session.snap_delta(0.0, 0.0), None);
    session.turn_page(true);
    session.turn_page(true);
    assert_eq!(page(&session), 4);

    // Two pages: pair by pair.
    session.fit();
    session.set_spread(true, false);
    session.go_to_page(0);
    session.turn_page(true);
    assert_eq!(page(&session), 2);
    session.turn_page(false);
    assert_eq!(page(&session), 0);
}

#[test]
fn closed_sessions_are_not_found() {
    let (session, _rx) = open(1, 1 << 20);
    let id = session.id();
    assert!(find_session(id).is_some());
    drop(session);
    assert!(find_session(id).is_none());
    assert!(!claim_tile(id, 1, |_| true));
}

/// Reads the frame layout documented in `hot.rs`, like the Kotlin side does.
fn decode_frame(mut b: &[u8]) -> Frame {
    fn take<const N: usize>(b: &mut &[u8]) -> [u8; N] {
        let (head, rest) = b.split_at(N);
        *b = rest;
        head.try_into().unwrap()
    }
    let u32_ = |b: &mut &[u8]| u32::from_ne_bytes(take(b));
    let rect = |b: &mut &[u8]| Rect {
        x: f32::from_ne_bytes(take(b)),
        y: f32::from_ne_bytes(take(b)),
        width: f32::from_ne_bytes(take(b)),
        height: f32::from_ne_bytes(take(b)),
    };
    let flags = u32_(&mut b);
    let zoom_mode = [ZoomMode::Fixed, ZoomMode::FitWidth, ZoomMode::FitPage][u32_(&mut b) as usize];
    let current_page = u32_(&mut b);
    let settles = i32::from_ne_bytes(take(&mut b));
    let last_page = u32_(&mut b);
    u32_(&mut b);
    let zoom = f64::from_ne_bytes(take(&mut b));
    let device_scale = f64::from_ne_bytes(take(&mut b));
    let zoom_factor = f64::from_ne_bytes(take(&mut b));
    let evicted_count = u32_(&mut b);
    let page_count = u32_(&mut b);
    let evicted = (0..evicted_count)
        .map(|_| u64::from_ne_bytes(take(&mut b)))
        .collect();
    let pages = (0..page_count)
        .map(|_| {
            let page = u32_(&mut b);
            let tile_count = u32_(&mut b);
            let page_rect = rect(&mut b);
            let busy = u32_(&mut b) & 1 != 0;
            u32_(&mut b);
            let tiles = (0..tile_count)
                .map(|_| {
                    let id = u64::from_ne_bytes(take(&mut b));
                    let dest = rect(&mut b);
                    let exact = u32_(&mut b) != 0;
                    u32_(&mut b);
                    TileDraw { id, dest, exact }
                })
                .collect();
            PageDraw {
                page,
                rect: page_rect,
                tiles,
                busy,
            }
        })
        .collect();
    let thumbnail_first = u32_(&mut b);
    let thumbnail_count = u32_(&mut b);
    let thumbnails = (0..thumbnail_count)
        .map(|_| u64::from_ne_bytes(take(&mut b)))
        .collect();
    let point = |b: &mut &[u8]| Point {
        x: f32::from_ne_bytes(take(b)),
        y: f32::from_ne_bytes(take(b)),
    };
    let highlight_count = u32_(&mut b);
    let edges = u32_(&mut b);
    let highlights = (0..highlight_count)
        .map(|_| {
            let kind = [
                HighlightKind::Selection,
                HighlightKind::Match,
                HighlightKind::CurrentMatch,
            ][u32_(&mut b) as usize];
            assert_eq!(u32_(&mut b), 0);
            Highlight {
                kind,
                start_bottom: point(&mut b),
                end_bottom: point(&mut b),
                end_top: point(&mut b),
                start_top: point(&mut b),
            }
        })
        .collect();
    let mut edge = |present: bool| {
        let e = SelectionEdge {
            bottom: point(&mut b),
            top: point(&mut b),
        };
        present.then_some(e)
    };
    let selection_start = edge(edges & 1 != 0);
    let selection_end = edge(edges & 2 != 0);
    let shapes = |b: &mut &[u8]| -> Vec<ShapeDraw> {
        let shape_count = u32_(b);
        assert_eq!(u32_(b), 0);
        (0..shape_count)
            .map(|_| {
                let page = u32_(b);
                let flags = u32_(b);
                let (fill, stroke) = (u32_(b), u32_(b));
                let stroke_width = f32::from_ne_bytes(u32_(b).to_ne_bytes());
                let segments = u32_(b);
                ShapeDraw {
                    page,
                    fill: (flags & 1 != 0).then_some(fill),
                    stroke: (flags & 2 != 0).then_some(stroke),
                    multiply: flags & 4 != 0,
                    closed: flags & 8 != 0,
                    round: flags & 16 != 0,
                    stroke_width,
                    start: point(b),
                    segments: (0..segments)
                        .map(|_| {
                            let kind = u32_(b);
                            assert_eq!(u32_(b), 0);
                            let points = [point(b), point(b), point(b)];
                            match kind {
                                0 => SegmentDraw::Line { to: points[0] },
                                1 => SegmentDraw::Curve {
                                    c1: points[0],
                                    c2: points[1],
                                    to: points[2],
                                },
                                other => panic!("segment kind {other}"),
                            }
                        })
                        .collect(),
                }
            })
            .collect()
    };
    let page_shapes = shapes(&mut b);
    let outline: Vec<Point> = (0..4).map(|_| point(&mut b)).collect();
    let picked = if flags & 64 != 0 { outline } else { Vec::new() };
    let thumbnail_shapes = shapes(&mut b);
    assert!(b.is_empty(), "trailing bytes");
    Frame {
        shapes: page_shapes,
        picked,
        thumbnail_shapes,
        selection: flags & 32 != 0,
        highlights,
        selection_start,
        selection_end,
        thumbnail_first,
        thumbnails,
        pages,
        evicted,
        complete: flags & 1 != 0,
        settled: flags & 2 != 0,
        paged: flags & 4 != 0,
        two_pages: flags & 8 != 0,
        cover_alone: flags & 16 != 0,
        zoom_factor,
        settles_in_ms: u32::try_from(settles).ok(),
        device_scale,
        current_page,
        last_page,
        zoom,
        zoom_mode,
    }
}

/// A frame with something in every field.
///
/// The Android app's `ViewFrameTest` decodes the encoded form of exactly this.
fn fixture_frame() -> Frame {
    let rect = |x, y, width, height| Rect {
        x,
        y,
        width,
        height,
    };
    Frame {
        pages: vec![
            PageDraw {
                page: 4,
                rect: rect(21.0, -300.0, 1038.0, 1469.0),
                tiles: vec![
                    TileDraw {
                        id: 7,
                        dest: rect(21.0, -300.0, 512.0, 512.0),
                        exact: true,
                    },
                    TileDraw {
                        id: 1 << 40,
                        dest: rect(533.0, -300.0, 463.5, 463.25),
                        exact: false,
                    },
                ],
                busy: false,
            },
            PageDraw {
                page: 5,
                rect: rect(21.0, 1190.0, 1038.0, 734.0),
                tiles: vec![],
                busy: true,
            },
        ],
        evicted: vec![3, 9_000_000_000],
        complete: false,
        settled: true,
        settles_in_ms: Some(180),
        device_scale: 1.744537353515625,
        current_page: 4,
        last_page: 5,
        zoom: 0.6645856618881226,
        zoom_mode: ZoomMode::FitPage,
        zoom_factor: 1.5,
        paged: true,
        two_pages: true,
        cover_alone: false,
        thumbnail_first: 2,
        thumbnails: vec![0, 11, 12],
        selection: true,
        highlights: vec![
            Highlight {
                kind: HighlightKind::Match,
                start_bottom: Point { x: 400.0, y: 300.0 },
                end_bottom: Point { x: 480.0, y: 300.0 },
                end_top: Point { x: 480.0, y: 270.0 },
                start_top: Point { x: 400.0, y: 270.0 },
            },
            Highlight {
                kind: HighlightKind::CurrentMatch,
                start_bottom: Point { x: 400.0, y: 500.0 },
                end_bottom: Point { x: 480.0, y: 500.0 },
                end_top: Point { x: 480.0, y: 470.0 },
                start_top: Point { x: 400.0, y: 470.0 },
            },
            Highlight {
                kind: HighlightKind::Selection,
                start_bottom: Point { x: 100.0, y: 80.5 },
                end_bottom: Point { x: 900.0, y: 80.5 },
                end_top: Point { x: 900.0, y: 50.25 },
                start_top: Point { x: 100.0, y: 50.25 },
            },
            Highlight {
                kind: HighlightKind::Selection,
                start_bottom: Point { x: 60.0, y: 120.0 },
                end_bottom: Point { x: 300.0, y: 121.0 },
                end_top: Point { x: 300.5, y: 91.0 },
                start_top: Point { x: 60.5, y: 90.0 },
            },
        ],
        // The selection starts above the view.
        selection_start: None,
        selection_end: Some(SelectionEdge {
            bottom: Point { x: 300.0, y: 121.0 },
            top: Point { x: 300.5, y: 91.0 },
        }),
        shapes: vec![
            // A highlight, a line under some text, and a drawing.
            ShapeDraw {
                page: 0,
                fill: Some(0xFFFF_EB3B),
                multiply: true,
                stroke: None,
                stroke_width: 0.0,
                round: false,
                closed: true,
                start: Point { x: 100.0, y: 200.0 },
                segments: vec![
                    SegmentDraw::Line {
                        to: Point { x: 300.0, y: 200.0 },
                    },
                    SegmentDraw::Line {
                        to: Point { x: 300.0, y: 170.5 },
                    },
                    SegmentDraw::Line {
                        to: Point { x: 100.0, y: 170.5 },
                    },
                ],
            },
            ShapeDraw {
                page: 1,
                fill: None,
                multiply: false,
                stroke: Some(0xFFE0_1C24),
                stroke_width: 2.5,
                round: false,
                closed: false,
                start: Point { x: 60.0, y: 1300.0 },
                segments: vec![SegmentDraw::Line {
                    to: Point {
                        x: 400.0,
                        y: 1300.0,
                    },
                }],
            },
            ShapeDraw {
                page: 1,
                fill: Some(0x8000_FF00),
                multiply: false,
                stroke: Some(0x80E0_1C24),
                stroke_width: 4.0,
                round: true,
                closed: true,
                start: Point {
                    x: 500.0,
                    y: 1400.0,
                },
                segments: vec![
                    SegmentDraw::Curve {
                        c1: Point {
                            x: 520.0,
                            y: 1380.0,
                        },
                        c2: Point {
                            x: 560.0,
                            y: 1380.0,
                        },
                        to: Point {
                            x: 580.0,
                            y: 1400.0,
                        },
                    },
                    SegmentDraw::Line {
                        to: Point {
                            x: 540.0,
                            y: 1450.0,
                        },
                    },
                ],
            },
        ],
        picked: vec![
            Point { x: 98.0, y: 202.0 },
            Point { x: 302.0, y: 202.0 },
            Point { x: 302.0, y: 168.5 },
            Point { x: 98.0, y: 168.5 },
        ],
        thumbnail_shapes: vec![ShapeDraw {
            page: 1,
            fill: Some(0xFFFF_EB3B),
            multiply: true,
            stroke: None,
            stroke_width: 0.0,
            round: false,
            closed: true,
            start: Point { x: 120.0, y: 250.0 },
            segments: vec![
                SegmentDraw::Line {
                    to: Point { x: 480.5, y: 250.0 },
                },
                SegmentDraw::Line {
                    to: Point { x: 480.5, y: 215.0 },
                },
                SegmentDraw::Line {
                    to: Point { x: 120.0, y: 215.0 },
                },
            ],
        }],
    }
}

#[test]
fn marking_text() {
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(text_pdf(), None, 1 << 20, listener.clone()).unwrap();
    // As in `selecting_text`: PDF (x, y) is at (8 + x, 308 - y) in the view.
    session.set_viewport(216.0, 316.0, 2.0);
    session.set_view_mode(ViewMode::Paged);
    let at = |x: f64, y: f64| (8.0 + x, 308.0 - y);
    let (hello, blank) = (at(30.0, 255.0), at(100.0, 100.0));

    // Nothing to mark without a selection, and nothing to pick.
    assert!(!session.mark_selection(Markup::Highlight));
    assert!(!session.pick_annotation_at(hello.0, hello.1));
    assert!(session.frame().shapes.is_empty());

    assert!(session.select_word_at(hello.0, hello.1));
    let selected = session.frame().highlights[0];
    assert!(session.mark_selection(Markup::Highlight));
    // The selection became the highlight: a fill that darkens, where the word is, in device pixels.
    let frame = session.frame();
    assert!(!frame.selection && frame.highlights.is_empty());
    let [shape] = &frame.shapes[..] else {
        panic!("one highlight: {:?}", frame.shapes);
    };
    assert_eq!((shape.page, shape.multiply, shape.closed), (0, true, true));
    assert!(shape.fill.is_some() && shape.stroke.is_none());
    assert!(
        (shape.start.x - selected.start_bottom.x).abs() < 6.0,
        "{shape:?}"
    );
    assert_eq!(shape.segments.len(), 3);
    let state = session.edit_state();
    assert!(state.modified);
    assert_eq!(state.undo.unwrap().kind, EditKind::AddAnnotation);

    // A line under the next word is a stroke.
    let brave = at(80.0, 255.0);
    assert!(session.select_word_at(brave.0, brave.1));
    assert!(session.mark_selection(Markup::Underline));
    let frame = session.frame();
    assert_eq!(frame.shapes.len(), 2);
    assert!(
        frame
            .shapes
            .iter()
            .any(|s| s.stroke.is_some() && s.fill.is_none())
    );

    // The thumbnail of the page shows them too, where they are on the page: the page is 200
    // by 300 points, and the word starts 20 from the left and ends 53 from the top.
    assert!(frame.thumbnail_shapes.is_empty());
    session.set_thumbnail_range(0, 1);
    let small = session.frame().thumbnail_shapes;
    assert_eq!(small.len(), 2);
    let mark = small.iter().find(|s| s.multiply).unwrap();
    assert_eq!(mark.page, 0);
    assert!((mark.start.x - 100.0).abs() < 5.0, "{mark:?}");
    assert!((mark.start.y - 265.0).abs() < 5.0, "{mark:?}");
    session.set_thumbnail_range(0, 0);

    // A tap on a mark picks it, a tap elsewhere lets go of it.
    assert!(frame.picked.is_empty());
    assert!(session.pick_annotation_at(hello.0, hello.1));
    let outline = session.frame().picked;
    assert_eq!(outline.len(), 4);
    assert!(outline.iter().any(|p| p.x <= shape.start.x + 1.0));
    assert!(!session.pick_annotation_at(blank.0, blank.1));
    assert!(session.frame().picked.is_empty());
    assert!(!session.clear_picked_annotation());

    // The picked one can be deleted, and that taken back.
    assert!(!session.remove_picked_annotation().unwrap());
    assert!(session.pick_annotation_at(hello.0, hello.1));
    assert!(session.remove_picked_annotation().unwrap());
    let frame = session.frame();
    assert!(frame.picked.is_empty());
    assert_eq!(frame.shapes.len(), 1);
    assert_eq!(session.undo().unwrap().kind, EditKind::RemoveAnnotation);
    assert_eq!(session.frame().shapes.len(), 2);

    // Saved, they are in the file, which then draws them itself.
    use std::os::fd::IntoRawFd;
    let path = std::env::temp_dir().join(format!("specto-ffi-marks-{}.pdf", std::process::id()));
    let fd = std::fs::File::create(&path).unwrap().into_raw_fd();
    session.save_to_fd(fd, true).unwrap();
    let written = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let (tx, rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let saved = ViewSession::open(written, None, 1 << 20, listener).unwrap();
    saved.set_viewport(216.0, 316.0, 2.0);
    saved.set_view_mode(ViewMode::Paged);
    let (frame, _) = settle(&saved, &rx);
    assert!(frame.shapes.is_empty());

    // One of the file's own can be deleted too: its page is then rendered without it, from a copy,
    // and with it again after undo.
    assert!(saved.pick_annotation_at(hello.0, hello.1));
    assert!(saved.remove_picked_annotation().unwrap());
    assert!(!saved.pick_annotation_at(hello.0, hello.1));
    let (frame, _) = settle(&saved, &rx);
    assert!(frame.shapes.is_empty() && !frame.pages[0].tiles.is_empty());
    assert_eq!(saved.undo().unwrap().kind, EditKind::RemoveAnnotation);
    settle(&saved, &rx);
    assert!(saved.pick_annotation_at(hello.0, hello.1));
}

/// `fixtures/frame.bin` is what Rust writes for [`fixture_frame`] and what the Kotlin decoder is
/// tested with, so the two can't drift apart unnoticed.
///
/// After a deliberate change to the layout, update `ViewFrame.decode` and rerun with
/// `SPECTO_UPDATE_FIXTURES=1`.
#[test]
#[cfg(target_endian = "little")]
fn frame_fixture_is_current() {
    let mut bytes = Vec::new();
    hot::encode_frame(&fixture_frame(), &mut bytes);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/frame.bin");
    if std::env::var_os("SPECTO_UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &bytes).unwrap();
    }
    let fixture = std::fs::read(&path).unwrap();
    assert!(
        fixture == bytes,
        "the frame layout changed: update ViewFrame.decode in apps/android, \
         then rerun with SPECTO_UPDATE_FIXTURES=1"
    );
    // And it reads back as what was written.
    assert_eq!(
        format!("{:?}", decode_frame(&bytes)),
        format!("{:?}", fixture_frame())
    );
}

#[test]
fn hot_path_matches_the_uniffi_one() {
    let (session, rx) = open(8, 64 << 20);
    session.set_viewport(200.0, 300.0, 2.625);
    settle(&session, &rx);
    let id = session.id();

    assert!(hot::scroll_by(id, 0.0, 450.0));
    assert!(hot::zoom_by(id, 1.25, 100.0, 150.0));
    let mut buf = vec![0u8; 1 << 16];
    let n = hot::frame(id, &mut buf);
    assert!(n > 0);
    let hot_frame = decode_frame(&buf[..n as usize]);
    // Nothing changed in between, so the UniFFI call plans the same frame (its eviction list is
    // empty: the hot call already took it).
    let frame = session.frame();
    assert_eq!(
        format!("{:?}", hot_frame.pages),
        format!("{:?}", frame.pages)
    );
    assert_eq!(hot_frame.current_page, frame.current_page);
    assert_eq!(hot_frame.zoom, frame.zoom);
    assert_eq!(hot_frame.zoom_mode, ZoomMode::Fixed);
    assert_eq!(hot_frame.device_scale, frame.device_scale);
    assert!(!hot_frame.settled && hot_frame.settles_in_ms.is_some());

    // A buffer that is too small reports the size needed. The frame can then be fetched again
    // without planning a new one.
    let mut small = [0u8; 16];
    let needed = hot::frame(id, &mut small);
    assert!(needed < -16);
    let mut exact = vec![0u8; (-needed) as usize];
    assert_eq!(hot::last_frame(id, &mut exact), -needed);
    decode_frame(&exact);

    // Finished tiles: count, then (id, width, height) each.
    let (finished, _) = (hot::take_finished(id, &mut buf), ());
    assert!(finished >= 8);
    let count = u32::from_ne_bytes(buf[..4].try_into().unwrap()) as i64;
    assert_eq!(finished, 8 + count * 16);

    // A closed session is not found.
    drop(session);
    assert!(!hot::scroll_by(id, 0.0, 1.0));
    assert_eq!(hot::frame(id, &mut buf), 0);
}

#[test]
fn a_draft_outlives_the_session() {
    let dir = std::env::temp_dir().join(format!("specto-ffi-drafts-{}", std::process::id()));
    let store = DraftStore::open(dir.to_string_lossy().into_owned()).unwrap();
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let open = || ViewSession::open(book_pdf(), None, 64 << 20, listener.clone()).unwrap();

    let session = open();
    // Nothing to keep yet.
    assert!(!session.keep_draft(&store, "book".into()).unwrap());
    assert!(session.rotate_pages(vec![0], true));
    assert!(session.delete_pages(vec![1]));
    assert!(session.keep_draft(&store, "book".into()).unwrap());

    let again = open();
    assert!(!again.restore_draft(&store, "other".into()).unwrap());
    assert!(again.restore_draft(&store, "book".into()).unwrap());
    assert_eq!(again.page_count(), 3);
    let first = again.page_sizes()[0];
    assert_eq!((first.width, first.height), (300.0, 200.0));
    let state = again.edit_state();
    assert!(state.modified && state.undo.is_none());

    // Undone, there is nothing to keep, and the draft goes.
    assert!(session.undo().is_some());
    assert!(session.undo().is_some());
    assert!(!session.keep_draft(&store, "book".into()).unwrap());
    assert!(!open().restore_draft(&store, "book".into()).unwrap());

    // One that doesn't fit the file is deleted.
    assert!(session.delete_pages(vec![0, 1, 2]));
    assert!(session.keep_draft(&store, "book".into()).unwrap());
    let short = ViewSession::open(sample_pdf(1), None, 1 << 20, listener.clone()).unwrap();
    assert!(short.restore_draft(&store, "book".into()).is_err());
    assert!(!open().restore_draft(&store, "book".into()).unwrap());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn trimming_margins() {
    let (tx, _rx) = mpsc::channel();
    let listener = Arc::new(Listener(Mutex::new(tx)));
    let session = ViewSession::open(text_pdf(), None, 1 << 20, listener).unwrap();
    session.set_viewport(216.0, 316.0, 2.0);
    session.set_view_mode(ViewMode::Paged);
    let shape = |s: &ViewSession| {
        let r = s.frame().pages[0].rect;
        r.height / r.width
    };
    let whole = shape(&session);
    assert!(!session.trim_margins());

    // The page is measured as it is shown. Its text is wider than tall, so the page becomes so
    // too. Thumbnails show it whole.
    session.set_trim_margins(true);
    assert!(session.trim_margins());
    assert!(shape(&session) < 1.0, "{}", shape(&session));
    let sizes = session.page_sizes();
    assert_eq!(sizes[0].height / sizes[0].width, whole);

    // Scrolling pages show them whole.
    session.set_view_mode(ViewMode::Continuous);
    assert_eq!(shape(&session), whole);
    session.set_view_mode(ViewMode::Paged);
    assert!(shape(&session) < 1.0);

    session.set_trim_margins(false);
    assert!(!session.trim_margins());
    assert_eq!(shape(&session), whole);
}
