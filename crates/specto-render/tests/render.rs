use std::sync::mpsc;
use std::time::Duration;

use specto_geom::{PdfBox, PdfPoint};
use specto_render::scheduler::Scheduler;
use specto_render::tile::{TileKey, TileScale, level_for_scale, level_scale, tiles_covering};
use specto_render::{
    Bitmap, Content, ContentBounds, Crop, DeviceIntRect, Document, HayroDocument, OpenError,
    RenderRequest, Rotation, check_geometry_matches_hayro,
};

/// One page with a media box of 200 by 100 points: a red triangle covering the lower-right half
/// plus a blue 20 pt square annotation near the top-left.
///
/// `extra` is spliced into the page dictionary (such as `/Rotate 90`). Offsets in the xref are
/// deliberately wrong, and hayro recovers by scanning.
fn sample_pdf(extra: &str) -> Vec<u8> {
    sample_pdf_drawing("1 0 0 rg 0 0 m 200 0 l 200 100 l f", extra)
}

/// [`sample_pdf`] with `content` as what the page draws.
fn sample_pdf_drawing(content: &str, extra: &str) -> Vec<u8> {
    let ap = "0 0 1 rg 0 0 20 20 re f";
    format!(
        "%PDF-1.7\n\
         1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
         2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 100] >> endobj\n\
         3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R /Annots [5 0 R] {extra} >> endobj\n\
         4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\n\
         5 0 obj << /Type /Annot /Subtype /Square /Rect [10 70 30 90] /AP << /N 6 0 R >> >> endobj\n\
         6 0 obj << /Type /XObject /Subtype /Form /BBox [0 0 20 20] /Length {} >> stream\n{ap}\nendstream endobj\n\
         trailer << /Root 1 0 R /Size 7 >>\n%%EOF\n",
        content.len(),
        ap.len(),
    )
    .into_bytes()
}

fn open(extra: &str) -> HayroDocument {
    HayroDocument::open(sample_pdf(extra), None).unwrap()
}

fn pixel(b: &Bitmap, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * b.width + x) * 4) as usize;
    b.data[i..i + 4].try_into().unwrap()
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

#[test]
fn page_size_and_full_render() {
    let doc = open("");
    assert_eq!(doc.page_count(), 1);
    let g = doc.page_geometry(0).unwrap();
    assert_eq!((g.size().width, g.size().height), (200.0, 100.0));

    let bmp = doc.render(&RenderRequest::page(0, 2.0)).unwrap();
    assert_eq!((bmp.width, bmp.height), (400, 200));
    // The triangle covers the lower-right half: bottom-right is red, top-center is white.
    assert_eq!(pixel(&bmp, 390, 190), RED);
    assert_eq!(pixel(&bmp, 200, 10), WHITE);
}

#[test]
fn annotations_can_be_excluded() {
    let doc = open("");
    // The annotation covers x 10..30, y 70..90 in points, which is around pixel (40, 40) at scale
    // 2.
    let mut req = RenderRequest::page(0, 2.0);
    assert_eq!(pixel(&doc.render(&req).unwrap(), 40, 40), BLUE);
    req.content = Content::PageOnly;
    assert_eq!(pixel(&doc.render(&req).unwrap(), 40, 40), WHITE);
}

#[test]
fn tiles_match_full_render() {
    let doc = open("");
    let scale = 1.37; // deliberately odd
    let full = doc.render(&RenderRequest::page(0, scale)).unwrap();
    let g = doc.page_geometry(0).unwrap();
    let level_free_tiles = 64;
    for ty in (0..full.height).step_by(level_free_tiles) {
        for tx in (0..full.width).step_by(level_free_tiles) {
            let region = DeviceIntRect::new(
                (tx, ty).into(),
                (
                    (tx + level_free_tiles as u32).min(full.width),
                    (ty + level_free_tiles as u32).min(full.height),
                )
                    .into(),
            );
            let mut req = RenderRequest::page(0, scale);
            req.region = Some(region);
            let tile = doc.render(&req).unwrap();
            for y in 0..tile.height {
                for x in 0..tile.width {
                    let (a, b) = (pixel(&tile, x, y), pixel(&full, tx + x, ty + y));
                    let max = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap();
                    assert!(max <= 2, "tile ({tx},{ty}) pixel ({x},{y}): {a:?} vs {b:?}");
                }
            }
        }
    }
    assert_eq!(g.pixel_size(scale).width, full.width);
}

#[test]
fn rotate_and_crop_box_match_hayro() {
    for extra in [
        "",
        "/Rotate 90",
        "/Rotate 180",
        "/Rotate 270",
        "/Rotate -90",
        "/CropBox [20 10 150 90]",
        "/CropBox [20 10 150 90] /Rotate 90",
        "/CropBox [-50 -50 120 300] /Rotate 270", // sticks out of the media box
        "/CropBox [0 0 0 0]",                     // degenerate
    ] {
        check_geometry_matches_hayro(&open(extra)).unwrap_or_else(|e| panic!("{extra}: {e}"));
    }
}

#[test]
fn rotated_page_puts_content_where_expected() {
    // /Rotate 90 turns the page clockwise: 100 by 200 points, and the annotation (top-left of the
    // unrotated page) ends up top-right.
    let doc = open("/Rotate 90");
    let g = doc.page_geometry(0).unwrap();
    assert_eq!((g.size().width, g.size().height), (100.0, 200.0));
    let bmp = doc.render(&RenderRequest::page(0, 1.0)).unwrap();
    assert_eq!((bmp.width, bmp.height), (100, 200));
    // The annotation's center (20, 80) in PDF space is (100 - (100-80), 20) = (80, 20) in page
    // space.
    assert_eq!(pixel(&bmp, 80, 20), BLUE);
}

#[test]
fn a_cropped_page_is_that_part_of_the_whole() {
    let doc = open("");
    let crop = Crop::new(PdfBox::new(
        PdfPoint::new(5.0, 10.0),
        PdfPoint::new(105.0, 95.0),
    ));
    for rotation in [Rotation::None, Rotation::Cw90] {
        let mut req = RenderRequest::page(0, 2.0);
        req.rotation = rotation;
        let full = doc.render(&req).unwrap();
        req.crop = Some(crop);
        let part = doc.render(&req).unwrap();
        // 100 by 85 points, at 2 pixels a point. Its top-left is 5 points in and 5 down on the
        // page as it is, or 10 in and 5 down turned a quarter clockwise.
        let (size, at) = match rotation {
            Rotation::None => ((200, 170), (10, 10)),
            _ => ((170, 200), (20, 10)),
        };
        assert_eq!((part.width, part.height), size);
        for y in 0..part.height {
            for x in 0..part.width {
                let (a, b) = (pixel(&part, x, y), pixel(&full, at.0 + x, at.1 + y));
                let max = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap();
                assert!(max <= 2, "{rotation:?} pixel ({x},{y}): {a:?} vs {b:?}");
            }
        }
    }
}

#[test]
fn the_box_of_what_a_page_draws() {
    // A white background, a black rectangle, and a blue page clipped to a corner, besides the
    // blue square annotation at 10 to 30 by 70 to 90.
    let content = "1 1 1 rg 0 0 200 100 re f 0 0 0 rg 50 20 30 40 re f \
                   q 0 0 10 10 re W n 0 0 1 rg 0 0 200 100 re f Q";
    let doc = HayroDocument::open(sample_pdf_drawing(content, ""), None).unwrap();
    let found = ContentBounds::new(doc.pdf()).page(0).unwrap();
    assert_eq!(
        (found.min.x, found.min.y, found.max.x, found.max.y),
        (0.0, 0.0, 80.0, 90.0)
    );

    // Nothing drawn but in white, and no such page.
    let blank = HayroDocument::open(sample_pdf_drawing("1 1 1 rg 0 0 200 100 re f", ""), None);
    let blank = blank.unwrap();
    let mut bounds = ContentBounds::new(blank.pdf());
    let annotation = bounds.page(0).unwrap();
    assert_eq!(
        (annotation.min.x, annotation.max.y),
        (10.0, 90.0),
        "only the annotation"
    );
    assert_eq!(bounds.page(1), None);
}

#[test]
fn extra_rotation_equals_page_rotate() {
    let plain = open("");
    let rotated = open("/Rotate 270");
    let mut req = RenderRequest::page(0, 1.5);
    req.rotation = Rotation::Cw270;
    let a = plain.render(&req).unwrap();
    let b = rotated.render(&RenderRequest::page(0, 1.5)).unwrap();
    assert_eq!((a.width, a.height), (b.width, b.height));
    assert_eq!(a.data, b.data);
}

#[test]
fn rejects_garbage_and_bad_regions() {
    assert!(matches!(
        HayroDocument::open(b"not a pdf".to_vec(), None),
        Err(OpenError::Invalid)
    ));
    let doc = open("");
    let mut req = RenderRequest::page(0, 1.0);
    req.region = Some(DeviceIntRect::new((150, 0).into(), (250, 10).into()));
    assert!(doc.render(&req).is_err());
    assert!(doc.render(&RenderRequest::page(1, 1.0)).is_err());
}

// Scheduler

fn wait_for(
    sched: &Scheduler,
    rx: &mpsc::Receiver<()>,
    n: usize,
) -> Vec<specto_render::scheduler::Finished> {
    let mut out = Vec::new();
    while out.len() < n {
        rx.recv_timeout(Duration::from_secs(10))
            .expect("timed out waiting for tiles");
        out.extend(sched.take_finished());
    }
    out
}

#[test]
fn scheduler_renders_requested_tiles() {
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let sched = Scheduler::new(2, move || {
        let _ = tx.lock().unwrap().send(());
    });
    let opened = sched.open(sample_pdf(""), None).unwrap();
    let g = opened.pages[0];

    let level = level_for_scale(3.0); // level 4: 4 pixels per point, 800 by 400 pixels, 2 by 1 tiles
    assert_eq!(level_scale(level), 4.0);
    let scale = TileScale::from_level(level);
    let keys: Vec<TileKey> = tiles_covering(&g, scale, DeviceIntRect::from_size(g.pixel_size(4.0)))
        .map(|(tx, ty)| TileKey {
            doc: opened.id,
            page: 0,
            rotation: Rotation::None,
            crop: None,
            content: Content::PageAndAnnotations,
            scale,
            tx,
            ty,
        })
        .collect();
    assert_eq!(keys.len(), 2);
    sched.request(keys.clone());
    let done = wait_for(&sched, &rx, 2);

    let direct = HayroDocument::open(sample_pdf(""), None).unwrap();
    for f in done {
        let bmp = f.result.unwrap();
        let mut req = RenderRequest::page(0, 4.0);
        req.region = f.key.region(&g);
        assert_eq!(bmp.data, direct.render(&req).unwrap().data, "{:?}", f.key);
    }
    assert_eq!(sched.pending(), 0);
}

#[test]
fn scheduler_does_not_rerender_uncollected_tiles() {
    let sched = Scheduler::new(2, || {});
    let opened = sched.open(sample_pdf(""), None).unwrap();
    let key = TileKey {
        doc: opened.id,
        page: 0,
        rotation: Rotation::None,
        crop: None,
        content: Content::PageOnly,
        scale: TileScale::from_level(0),
        tx: 0,
        ty: 0,
    };
    sched.request([key]);
    let start = std::time::Instant::now();
    while sched.pending() > 0 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "render timed out"
        );
        std::thread::yield_now();
    }
    // Finished but not taken yet: asking again must not queue it.
    sched.request([key]);
    assert_eq!(sched.pending(), 0);
    let done = sched.take_finished();
    assert_eq!(done.len(), 1);
    // Once taken, it can be requested again (after cache eviction, for example).
    sched.request([key]);
    assert!(sched.pending() <= 1);
}

/// A page of 200 by 400 points with `n` tiny filled squares: slow to render.
fn slow_pdf(n: usize) -> Vec<u8> {
    let mut content = String::from("0 0 1 rg\n");
    for i in 0..n {
        let (x, y) = ((i % 400) as f64 * 0.5, ((i / 400) % 800) as f64 * 0.5);
        content.push_str(&format!("{x} {y} 0.4 0.4 re f\n"));
    }
    format!(
        "%PDF-1.7\n\
         1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n\
         2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 400] >> endobj\n\
         3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj\n\
         4 0 obj << /Length {} >> stream\n{content}\nendstream endobj\n\
         trailer << /Root 1 0 R /Size 5 >>\n%%EOF\n",
        content.len()
    )
    .into_bytes()
}

#[test]
fn slow_pages_get_an_extra_worker_and_render_one_tile_at_a_time() {
    use specto_render::scheduler::Limits;
    let sched = Scheduler::with_limits(
        1,
        Limits {
            slow_after: Duration::from_millis(10),
            stall_after: Duration::from_millis(20),
            heavy_image_pixels: u64::MAX,
            max_extra_workers: 2,
        },
        || {},
    );
    let slow = sched.open(slow_pdf(400_000), None).unwrap();
    let fast = sched.open(sample_pdf(""), None).unwrap();
    let key = |doc, scale: f64, tx, ty| TileKey {
        doc,
        page: 0,
        rotation: Rotation::None,
        crop: None,
        content: Content::PageOnly,
        scale: TileScale::exact(scale),
        tx,
        ty,
    };
    // Four slow tiles queued ahead of a fast one, with one regular worker.
    let slow_tiles: Vec<_> = (0..4).map(|ty| key(slow.id, 4.0, 0, ty)).collect();
    let fast_tile = key(fast.id, 1.0, 0, 0);
    sched.request(slow_tiles.iter().copied().chain([fast_tile]));

    let start = std::time::Instant::now();
    let (mut order, mut max_workers) = (Vec::new(), 1);
    while order.len() < 5 {
        assert!(start.elapsed() < Duration::from_secs(30), "timed out");
        max_workers = max_workers.max(sched.workers());
        order.extend(sched.take_finished().into_iter().map(|f| f.key));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(sched.is_expensive(slow.id, 0));
    assert!(!sched.is_expensive(fast.id, 0));
    assert!(max_workers >= 2, "no extra worker started");
    // The fast tile didn't wait for the whole slow page.
    let pos = |k| order.iter().position(|o| *o == k).unwrap();
    assert!(pos(fast_tile) < pos(slow_tiles[1]), "{order:?}");
    // Extra workers retire once nothing is stalled.
    let start = std::time::Instant::now();
    while sched.workers() > 1 {
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "extra worker didn't retire"
        );
        sched.request([]); // wakes idle workers
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn clients_have_separate_queues_and_results() {
    let sched = Scheduler::new(2, || {});
    let (a_tx, a_rx) = mpsc::channel();
    let a_tx = std::sync::Mutex::new(a_tx);
    let a = sched.client(move || {
        let _ = a_tx.lock().unwrap().send(());
    });
    let b = sched.client(|| {});
    let doc_a = sched.open(sample_pdf(""), None).unwrap().id;
    let doc_b = sched.open(sample_pdf("/Rotate 90"), None).unwrap().id;
    let key = |doc, tx| TileKey {
        doc,
        page: 0,
        rotation: Rotation::None,
        crop: None,
        content: Content::PageOnly,
        scale: TileScale::from_level(2),
        tx,
        ty: 0,
    };
    // B's request must not replace A's (as `Scheduler::request` would).
    a.request([key(doc_a, 0)]);
    b.request([key(doc_b, 0)]);
    a_rx.recv_timeout(Duration::from_secs(10))
        .expect("A was notified");
    let start = std::time::Instant::now();
    while a.pending() + b.pending() > 0 {
        assert!(start.elapsed() < Duration::from_secs(10), "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    let (ra, rb) = (a.take_finished(), b.take_finished());
    assert_eq!(
        ra.iter().map(|f| f.key).collect::<Vec<_>>(),
        [key(doc_a, 0)]
    );
    assert_eq!(
        rb.iter().map(|f| f.key).collect::<Vec<_>>(),
        [key(doc_b, 0)]
    );
    assert!(
        sched.take_finished().is_empty(),
        "nothing for the default client"
    );
    // A dropped client's requests disappear.
    b.request([key(doc_b, 1)]);
    drop(b);
    assert_eq!(sched.pending() + a.pending(), 0);
}

#[test]
fn scheduler_drops_closed_documents() {
    let sched = Scheduler::new(1, || {});
    let opened = sched.open(sample_pdf(""), None).unwrap();
    sched.close(opened.id);
    sched.request([TileKey {
        doc: opened.id,
        page: 0,
        rotation: Rotation::None,
        crop: None,
        content: Content::PageOnly,
        scale: TileScale::from_level(0),
        tx: 0,
        ty: 0,
    }]);
    assert_eq!(sched.pending(), 0);
    assert!(matches!(
        sched.open(b"garbage".to_vec(), None),
        Err(OpenError::Invalid)
    ));
}

/// Compares our page transform with hayro's on every page of the corpus.
///
/// Run with `cargo test -p specto-render -- --ignored` after `corpus/fetch.sh`.
#[test]
#[ignore = "needs corpus/files (run corpus/fetch.sh)"]
fn corpus_geometry_matches_hayro() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/files");
    let mut stack = vec![dir];
    let (mut checked, mut failures) = (0, Vec::new());
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "pdf")
                && let Ok(doc) = HayroDocument::open(std::fs::read(&p).unwrap(), None)
            {
                checked += 1;
                if let Err(e) = check_geometry_matches_hayro(&doc) {
                    failures.push(format!("{}: {e}", p.display()));
                }
            }
        }
    }
    assert!(checked > 0, "corpus is empty");
    assert!(
        failures.is_empty(),
        "{} of {checked} files differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Finds the box of what every page of the corpus draws, and how long that takes.
///
/// Run with `cargo test --release -p specto-render corpus_content_bounds -- --ignored --nocapture`.
#[test]
#[ignore = "needs corpus/files (run corpus/fetch.sh)"]
fn corpus_content_bounds() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/files");
    let mut stack = vec![dir];
    let (mut pages, mut empty, mut trimmed_area, mut full) = (0usize, 0usize, 0.0f64, 0usize);
    let mut slowest: Vec<(f64, String)> = Vec::new();
    let start = std::time::Instant::now();
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_none_or(|e| e != "pdf") {
                continue;
            }
            let Ok(doc) = HayroDocument::open(std::fs::read(&p).unwrap(), None) else {
                continue;
            };
            let mut bounds = specto_render::ContentBounds::new(doc.pdf());
            for i in 0..doc.page_count().min(50) {
                let t = std::time::Instant::now();
                let found =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| bounds.page(i)));
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                slowest.push((ms, format!("{} page {}", p.display(), i + 1)));
                pages += 1;
                let visible = doc.page_geometry(i).unwrap().visible_box();
                match found {
                    Ok(Some(b)) => {
                        let share = (b.width() * b.height()) / (visible.width() * visible.height());
                        trimmed_area += 1.0 - share;
                        if share > 0.98 {
                            full += 1;
                        }
                    }
                    _ => empty += 1,
                }
            }
        }
    }
    slowest.sort_by(|a, b| b.0.total_cmp(&a.0));
    let total: f64 = slowest.iter().map(|s| s.0).sum();
    let median = slowest[slowest.len() / 2].0;
    println!(
        "{pages} pages in {:.1} s, {total:.0} ms measuring, median {median:.2} ms",
        start.elapsed().as_secs_f64()
    );
    println!(
        "empty {empty}, nearly whole page {full}, area trimmed on average {:.0}%",
        trimmed_area / (pages - empty) as f64 * 100.0
    );
    for (ms, what) in slowest.iter().take(10) {
        println!("{ms:8.1} ms {what}");
    }
}
