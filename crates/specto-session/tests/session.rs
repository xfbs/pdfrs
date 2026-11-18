//! A session through its public interface: text, search and links across page edits, pages of a
//! second file, and saving.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use specto_doc::Rotation;
use specto_geom::{PdfPoint, ViewSize};
use specto_render::scheduler::Scheduler;
use specto_session::{Indexed, MAIN, Options, Session};
use specto_source::Target;
use specto_view::{LayoutMode, Selection, TextPages};

/// A file of `pages` pages of 200 by 300 points.
///
/// Page n (from 1) reads "`word` page n" and, below that, "common text". The first two pages are
/// numbered i and ii, the rest from 1. A link on the first page leads to the third, if there is
/// one.
fn book(pages: usize, word: &str) -> Vec<u8> {
    book_with(pages, word, "/Fit")
}

/// [`book`] with the link's destination on the third page as given, such as `/XYZ 0 265 0`.
fn book_with(pages: usize, word: &str, destination: &str) -> Vec<u8> {
    let mut objects: Vec<(usize, String)> = Vec::new();
    let kids: String = (0..pages).map(|i| format!("{} 0 R ", 10 + 2 * i)).collect();
    objects.push((
        1,
        "<< /Type /Catalog /Pages 2 0 R \
         /PageLabels << /Nums [0 << /S /r >> 2 << /S /D >>] >> >>"
            .into(),
    ));
    objects.push((
        2,
        format!(
            "<< /Type /Pages /Kids [{kids}] /Count {pages} /MediaBox [0 0 200 300] \
             /Resources << /Font << /F1 3 0 R >> >> >>"
        ),
    ));
    objects.push((
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
    ));
    for i in 0..pages {
        let annots = if i == 0 && pages > 2 {
            " /Annots [5 0 R]"
        } else {
            ""
        };
        objects.push((
            10 + 2 * i,
            format!(
                "<< /Type /Page /Parent 2 0 R /Contents {} 0 R{annots} >>",
                11 + 2 * i
            ),
        ));
        let content = format!(
            "BT /F1 16 Tf 20 250 Td ({word} page {}) Tj 0 -30 Td (common text) Tj ET",
            i + 1
        );
        objects.push((
            11 + 2 * i,
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        ));
    }
    if pages > 2 {
        objects.push((
            5,
            format!(
                "<< /Type /Annot /Subtype /Link /Rect [20 100 180 140] \
                 /Dest [14 0 R {destination}] >>"
            ),
        ));
    }
    objects.sort_by_key(|(number, _)| *number);

    // With a proper cross-reference table: saving reads the file strictly.
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

/// The Android app keeps its session behind a mutex and calls it from several threads.
#[test]
fn a_session_can_move_between_threads() {
    fn is_send<T: Send>() {}
    is_send::<Session>();
}

fn open(scheduler: &Scheduler, data: Vec<u8>) -> Session {
    let mut session = Session::open(scheduler, data.into(), None, Options::default()).unwrap();
    session
        .viewer
        .set_viewport(ViewSize::new(400.0, 600.0), 1.0);
    session
}

/// The text of every page, in the order the pages are in now.
fn all_text(session: &mut Session) -> String {
    let pages = session.viewer.page_count();
    session.selection = Some(Selection::all(pages));
    let text = session.selected_text().unwrap_or_default();
    session.selection = None;
    text
}

/// Runs the indexer until it has nothing more to hand over, as a frontend would: its pages come
/// over a channel and go into the session here. Returns how many pages it indexed.
fn index(session: &mut Session, from: usize) -> usize {
    let (tx, rx) = mpsc::channel::<Indexed>();
    if !session.start_indexing(from, move |page| tx.send(page).is_ok()) {
        return 0;
    }
    let mut pages = 0;
    // The channel closes when the thread is done.
    while let Ok(page) = rx.recv_timeout(Duration::from_secs(30)) {
        session.indexed(page);
        pages += 1;
    }
    pages
}

#[test]
fn text_follows_the_pages() {
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(3, "alpha"));
    let text = all_text(&mut session);
    assert!(text.starts_with("alpha page 1\ncommon text\n"), "{text:?}");
    assert!(text.contains("alpha page 3"));

    // A selection holds positions, so an edit drops it.
    session.selection = Some(Selection::all(3));
    assert_eq!(session.move_pages(&[0], 3), Some(2..3));
    assert!(session.selection.is_none());
    let moved = all_text(&mut session);
    assert!(moved.starts_with("alpha page 2\n"), "{moved:?}");
    assert!(moved.trim_end().ends_with("alpha page 1\ncommon text"));

    // The text layer goes by position too.
    let (_, mut texts, _) = session.text();
    let last = texts.text_page(2).unwrap();
    assert!(last.text().starts_with("alpha page 1"));
    assert!(texts.text_page(3).is_none());
    session.close(&scheduler);
}

#[test]
fn pages_keep_their_labels() {
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(4, "alpha"));
    assert!(session.labels().is_some());
    assert_eq!(session.page_label(0), "i");
    assert_eq!(session.page_label(2), "1");
    assert_eq!(session.find_page("ii"), Some(1));
    // "1" is the page labeled 1, not the first page.
    assert_eq!(session.find_page("1"), Some(2));

    // Turned, deleted or moved, pages keep their labels.
    assert!(session.rotate(&[0], Rotation::Cw90));
    assert_eq!(session.page_label(0), "i");
    assert!(session.delete(&[1]));
    let labels: Vec<String> = (0..3).map(|i| session.page_label(i)).collect();
    assert_eq!(labels, ["i", "1", "2"]);
    assert_eq!(session.find_page("1"), Some(1));
    assert_eq!(session.find_page("ii"), None);
    assert_eq!(session.move_pages(&[0], 3), Some(2..3));
    let labels: Vec<String> = (0..3).map(|i| session.page_label(i)).collect();
    assert_eq!(labels, ["1", "2", "i"]);
    // Numbers that aren't labels are positions.
    assert_eq!(session.find_page("3"), Some(2));

    assert!(session.undo(&scheduler).is_some());
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(session.page_label(1), "ii");

    // Back in the file's order, the labels are the file's again. A file whose labels are 1, 2, 3
    // and so on has none.
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(session.labels(), session.main().page_labels().as_ref());
    session.close(&scheduler);
}

#[test]
fn links_name_pages_of_their_file() {
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(4, "alpha"));
    let on_link = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(100.0, 120.0))
        .unwrap();
    let beside = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(100.0, 200.0))
        .unwrap();
    assert!(session.link_at(beside).is_none());
    let (source, target) = session.link_at(on_link).unwrap();
    assert_eq!(source, MAIN);
    let Target::Destination(destination) = target else {
        panic!("not a destination: {target:?}");
    };
    assert_eq!(destination.page, 2);
    assert_eq!(session.position_of(source, destination.page), Some(2));

    // The page it leads to moves up, and then goes.
    assert!(session.delete(&[1]));
    assert_eq!(session.position_of(MAIN, 2), Some(1));
    assert!(session.delete(&[1]));
    assert_eq!(session.position_of(MAIN, 2), None);
    // The link itself is still on the first page.
    let on_link = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(100.0, 120.0))
        .unwrap();
    assert!(session.link_at(on_link).is_some());
    session.close(&scheduler);
}

#[test]
fn a_link_shows_where_it_leads() {
    let scheduler = Scheduler::new(1, || {});
    // A link to a page as a whole has nothing to show.
    let mut session = open(&scheduler, book(4, "alpha"));
    let on_link = |session: &Session| {
        let at = PdfPoint::new(100.0, 120.0);
        session.viewer.pdf_to_view(0, at).unwrap()
    };
    assert!(session.link_preview(on_link(&session)).is_none());
    session.close(&scheduler);

    // One to a place 35 points down the third page.
    let mut session = open(&scheduler, book_with(4, "alpha", "/XYZ 0 265 0"));
    let p = on_link(&session);
    let beside = session.viewer.pdf_to_view(0, PdfPoint::new(100.0, 200.0));
    assert!(session.link_preview(beside.unwrap()).is_none());
    let preview = session.link_preview(p).unwrap();
    assert!(preview.link.contains(p));
    let zoom = session.viewer.zoom();
    let (w, h) = (preview.link.size.width, preview.link.size.height);
    assert!((w - 160.0 * zoom).abs() < 0.01 && (h - 40.0 * zoom).abs() < 0.01);
    assert!(preview.same_target(&session.link_preview(p).unwrap()));

    // At two pixels a point, from a little above the target: the page's first line of text, whose
    // baseline is 50 points down the page, is in rows 34 to 58.
    let bitmap = preview.render(400, 120).unwrap();
    assert_eq!((bitmap.width, bitmap.height), (400, 120));
    let dark = |rows: std::ops::Range<usize>| {
        let pixels = bitmap.data[rows.start * 1600..rows.end * 1600].chunks(4);
        pixels.filter(|p| p[0] < 100).count()
    };
    assert_eq!(dark(0..30), 0);
    assert!(dark(30..60) > 200, "{}", dark(30..60));
    // No taller than the page is below the target.
    assert_eq!(preview.render(400, 5000).unwrap().height, 600);

    // The page it leads to turned: shown as turned, across its width.
    assert!(session.rotate(&[2], Rotation::Cw90));
    let turned = session.link_preview(on_link(&session)).unwrap();
    assert!(!turned.same_target(&preview));
    let bitmap = turned.render(300, 5000).unwrap();
    assert_eq!((bitmap.width, bitmap.height), (300, 200));
    // And gone: nothing to show.
    assert!(session.delete(&[2]));
    assert!(session.link_preview(on_link(&session)).is_none());
    session.close(&scheduler);
}

#[test]
fn search_follows_indexing_and_edits() {
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(4, "alpha"));

    // Nothing is indexed yet, so nothing is found yet.
    assert!(!session.search("ALPHA"));
    let status = session.search_status().unwrap();
    assert_eq!(
        (status.matches, status.pages_indexed, status.pages),
        (0, 0, 4)
    );
    assert_eq!(index(&mut session, 0), 4);
    let status = session.search_status().unwrap();
    assert_eq!((status.matches, status.current), (4, Some(0)));
    assert_eq!(status.pages_indexed, 4);
    // All done: no second run.
    assert_eq!(index(&mut session, 0), 0);

    // Going to matches remembers where the search began, once.
    assert!(!session.viewer.can_go_back());
    session.reveal_match();
    assert!(!session.viewer.can_go_back(), "the match is in view");
    assert!(session.search_step(true));
    assert_eq!(session.current_match().unwrap().page, 1);
    session.reveal_match();
    assert!(session.search_step(true));
    session.reveal_match();
    assert_eq!(session.viewer.current_page(), 2);
    assert!(session.viewer.go_back());
    assert_eq!(session.viewer.current_page(), 0);
    assert!(!session.viewer.can_go_back());
    assert!(session.search_step(false));
    assert_eq!(session.current_match().unwrap().page, 1);
    assert_eq!(session.matches_on(1).len(), 1);
    assert!(session.matches_on(1)[0].1, "the current one");
    assert!(!session.matches_on(0)[0].1);

    // Results hold positions: after an edit the search runs again.
    assert!(session.delete(&[0]));
    let status = session.search_status().unwrap();
    assert_eq!((status.matches, status.pages), (3, 3));
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(session.search_status().unwrap().matches, 4);

    // What is found on a page is found wherever the page goes.
    assert!(session.search("page 4"));
    assert_eq!(session.current_match().unwrap().page, 3);
    session.move_pages(&[3], 0);
    assert_eq!(session.current_match().unwrap().page, 0);

    session.end_search();
    assert!(session.search_status().is_none());
    assert!(session.matches_on(0).is_empty());
    session.close(&scheduler);
}

#[test]
fn pages_of_another_file() {
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(2, "alpha"));
    assert_eq!(index(&mut session, 0), 2);
    assert!(session.search("page"));
    assert_eq!(session.search_status().unwrap().matches, 2);

    let (other, count) = session
        .add_source(&scheduler, book(3, "beta").into(), None)
        .unwrap();
    assert_eq!(count, 3);
    // Opened, but not in the document until inserted.
    assert_eq!(session.viewer.page_count(), 2);
    assert_eq!(session.insert(1, &[other]), Some(1..4));
    assert_eq!(session.viewer.page_count(), 5);
    let text = all_text(&mut session);
    let order: Vec<&str> = text.lines().filter(|l| l.contains("page")).collect();
    assert_eq!(
        order,
        [
            "alpha page 1",
            "beta page 1",
            "beta page 2",
            "beta page 3",
            "alpha page 2"
        ]
    );

    // The new pages are searched once they are indexed.
    let status = session.search_status().unwrap();
    assert_eq!(
        (status.matches, status.pages_indexed, status.pages),
        (2, 2, 5)
    );
    assert_eq!(index(&mut session, 0), 3);
    let status = session.search_status().unwrap();
    assert_eq!((status.matches, status.pages_indexed), (5, 5));
    assert!(session.search("beta"));
    assert_eq!(session.current_match().unwrap().page, 1);

    // Its links and labels are its own: the pages keep their labels, and the first file's link
    // still finds its page.
    let labels: Vec<String> = (0..5).map(|i| session.page_label(i)).collect();
    assert_eq!(labels, ["i", "i", "ii", "1", "ii"]);
    assert_eq!(session.position_of(other, 2), Some(3));

    // Undone, the file stays known, for redo.
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(session.viewer.page_count(), 2);
    assert!(session.redo(&scheduler).is_some());
    assert_eq!(session.viewer.page_count(), 5);
    session.close(&scheduler);
}

#[test]
fn pages_from_one_session_into_another() {
    let scheduler = Scheduler::new(1, || {});
    let mut from = open(&scheduler, book(3, "alpha"));
    let (beta, _) = from
        .add_source(&scheduler, book(2, "beta").into(), None)
        .unwrap();
    assert_eq!(from.insert(3, &[beta]), Some(3..5));
    assert!(from.rotate(&[2], Rotation::Cw90));
    // Pages of both files, in document order whatever the order asked.
    let bundle = from.bundle(&[4, 2, 0, 2]);
    assert_eq!(bundle.page_count(), 3);

    let mut to = open(&scheduler, book(2, "gamma"));
    assert_eq!(to.insert_bundle(&scheduler, 1, &bundle), Ok(Some(1..4)));
    let text = all_text(&mut to);
    let order: Vec<&str> = text.lines().filter(|l| l.contains("page")).collect();
    assert_eq!(
        order,
        [
            "gamma page 1",
            "alpha page 1",
            "alpha page 3",
            "beta page 2",
            "gamma page 2"
        ]
    );
    // The rotation comes along, and it's one edit.
    assert_eq!(to.edit().pages()[2].rotation, Rotation::Cw90);
    assert_eq!(
        to.edit().undo_edit(),
        Some(specto_doc::Edit::Insert { pages: 3 })
    );

    // The same files again aren't opened a second time.
    let again = from.bundle(&[1]);
    assert_eq!(to.insert_bundle(&scheduler, 0, &again), Ok(Some(0..1)));
    let sources: std::collections::HashSet<_> =
        to.edit().pages().iter().map(|p| p.source).collect();
    assert_eq!(sources.len(), 3);

    // Within one session, a bundle copies pages.
    let own = to.bundle(&[0]);
    assert_eq!(to.insert_bundle(&scheduler, 6, &own), Ok(Some(6..7)));
    assert_eq!(to.edit().pages()[6], to.edit().pages()[0]);

    // Saved, the copied pages are in the file.
    let mut saved = open(&scheduler, to.save_job().write().unwrap());
    assert_eq!(saved.viewer.page_count(), 7);
    assert!(all_text(&mut saved).contains("beta page 2"));
    for s in [&mut from, &mut to, &mut saved] {
        s.close(&scheduler);
    }
}

#[test]
fn saving_writes_the_pages_as_edited() {
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(3, "alpha"));
    assert!(!session.edit().is_modified());
    assert!(session.rotate(&[0], Rotation::Cw90));
    assert!(session.delete(&[1]));
    let (other, _) = session
        .add_source(&scheduler, book(1, "beta").into(), None)
        .unwrap();
    assert_eq!(session.insert(2, &[other]), Some(2..3));
    assert!(session.edit().is_modified());

    let job = session.save_job();
    let written = job.write().unwrap();
    // An edit made while the file was being written stays unsaved.
    assert!(session.rotate(&[1], Rotation::Cw90));
    session.mark_saved(job.version);
    assert!(session.edit().is_modified());
    assert!(session.undo(&scheduler).is_some());
    assert!(!session.edit().is_modified());

    let mut saved = open(&scheduler, written);
    assert_eq!(saved.viewer.page_count(), 3);
    let first = saved.viewer.pages()[0].displayed().size();
    assert_eq!((first.width, first.height), (300.0, 200.0));
    let second = saved.viewer.pages()[1].displayed().size();
    assert_eq!((second.width, second.height), (200.0, 300.0));
    let text = all_text(&mut saved);
    let order: Vec<&str> = text.lines().filter(|l| l.contains("page")).collect();
    assert_eq!(order, ["alpha page 1", "alpha page 3", "beta page 1"]);

    // Some of the pages only, as when exporting a selection.
    let pages = vec![session.edit().pages()[2]];
    let exported = session.save_pages(pages).write().unwrap();
    let mut exported = open(&scheduler, exported);
    assert_eq!(all_text(&mut exported).lines().next(), Some("beta page 1"));

    saved.close(&scheduler);
    exported.close(&scheduler);
    session.close(&scheduler);
}

#[test]
fn the_indexer_stops_when_told() {
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(40, "alpha"));

    // A sink that takes one page and no more.
    let (tx, rx) = mpsc::channel::<Indexed>();
    assert!(session.start_indexing(0, move |page| {
        let _ = tx.send(page);
        false
    }));
    let first = rx.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(30)).is_err(),
        "the thread ended"
    );
    session.search("alpha");
    assert!(session.indexed(first), "the first match is there now");
    assert_eq!(session.search_status().unwrap().pages_indexed, 1);

    // It can be started again for the rest, once the thread has gone.
    let deadline = Instant::now() + Duration::from_secs(30);
    let rx = loop {
        let (tx, rx) = mpsc::channel::<Indexed>();
        if session.start_indexing(5, move |page| tx.send(page).is_ok()) {
            break rx;
        }
        assert!(Instant::now() < deadline, "the first thread never finished");
        std::thread::sleep(Duration::from_millis(5));
    };
    // From the page asked for on, then from the start, but not the one done.
    let mut pages = 0;
    let mut first_match = None;
    while let Ok(page) = rx.recv_timeout(Duration::from_secs(30)) {
        session.indexed(page);
        pages += 1;
        if pages == 1 {
            first_match = Some(session.search_status().unwrap().matches);
        }
    }
    assert_eq!(pages, 39);
    assert_eq!(first_match, Some(2));
    assert_eq!(session.search_status().unwrap().matches, 40);

    // Dropping the session stops a running indexer: its sink is let go.
    let (tx, rx) = mpsc::channel::<()>();
    let mut session = open(&scheduler, book(40, "alpha"));
    assert!(session.start_indexing(0, move |_| {
        let _ = &tx;
        std::thread::sleep(Duration::from_millis(20));
        true
    }));
    session.close(&scheduler);
    drop(session);
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(30)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
}

#[test]
fn a_page_handed_over_twice_counts_once() {
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(3, "alpha"));
    session.search("alpha");
    // Two indexers whose pages arrive after both ran, as when a file is inserted while a search
    // is indexing: the second lists the pages whose text the session doesn't have yet.
    let pages_of = |session: &mut Session| {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let (tx, rx) = mpsc::channel::<Indexed>();
            if session.start_indexing(0, move |page| tx.send(page).is_ok()) {
                return rx.iter().collect::<Vec<_>>();
            }
            assert!(Instant::now() < deadline, "the first thread never finished");
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let first = pages_of(&mut session);
    let second = pages_of(&mut session);
    assert_eq!((first.len(), second.len()), (3, 3));
    for page in first.into_iter().chain(second) {
        session.indexed(page);
    }
    let status = session.search_status().unwrap();
    assert_eq!((status.matches, status.pages_indexed), (3, 3));
    session.close(&scheduler);
}

#[test]
fn marking_text_and_saving_it() {
    use specto_annot::{AnnotKind, AnnotType, Color, MarkupStyle};
    use specto_doc::{AnnotId, Edit};
    use specto_session::ViewShape;
    use specto_view::{Granularity, text_char};

    let yellow = Color::new(1.0, 0.9, 0.0);
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(3, "alpha"));
    // Nothing selected: nothing to mark.
    assert!(!session.mark_selection(MarkupStyle::Highlight, yellow));
    assert!(session.overlay(0).is_empty());

    // Everything on pages 1 to 3: one annotation a page, as one edit.
    session.selection = Some(Selection::all(3));
    assert!(session.mark_selection(MarkupStyle::Highlight, yellow));
    assert!(session.selection.is_none());
    assert_eq!(
        session.edit().undo_edit(),
        Some(Edit::AddAnnotation(AnnotType::Highlight))
    );
    assert!(session.edit().is_modified());
    for position in 0..3 {
        let annots = session.annotations(position);
        assert_eq!(annots.len(), 1, "page {position}");
        let a = &annots[0].1;
        let AnnotKind::TextMarkup { quads, .. } = &a.kind else {
            panic!("not text markup");
        };
        assert_eq!(quads.len(), 2, "one quad a line");
        assert_eq!(
            a.contents.as_deref(),
            Some(format!("alpha page {}\ncommon text", position + 1).as_str())
        );
        assert!(a.modified.is_some_and(|d| d.year >= 2026));
    }
    // Drawn where the text is: the first line of page 1 starts at x 20, baseline y 250 of a page
    // 300 high.
    let shapes = session.overlay(0);
    assert_eq!(shapes.len(), 2);
    let ViewShape::Fill { quad, multiply, .. } = shapes[0] else {
        panic!("a highlight is filled");
    };
    assert!(multiply);
    let origin = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(20.0, 250.0))
        .unwrap();
    let left = quad.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let bottom = quad.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    assert!((left - origin.x).abs() < 2.0, "{left} vs {}", origin.x);
    assert!(bottom > origin.y && bottom - origin.y < 8.0);

    // Undone and redone as one.
    assert_eq!(
        session.undo(&scheduler),
        Some(Edit::AddAnnotation(AnnotType::Highlight))
    );
    assert!((0..3).all(|p| session.annotations(p).is_empty()));
    assert!(session.redo(&scheduler).is_some());

    // Underlining a word on page 2, then picking and deleting it. The page is scrolled to first, as
    // only pages near the view can be pointed at.
    session.viewer.go_to_page(1);
    let word = session
        .viewer
        .pdf_to_view(1, PdfPoint::new(25.0, 255.0))
        .unwrap();
    {
        let (viewer, mut texts, selection) = session.text();
        let at = text_char(viewer, &mut texts, word).expect("text under the point");
        *selection = Some(Selection::new(&mut texts, at, Granularity::Word));
    }
    assert_eq!(session.selected_text().as_deref(), Some("alpha"));
    assert!(session.mark_selection(MarkupStyle::Underline, Color::new(1.0, 0.0, 0.0)));
    let picked = session
        .annotation_at(word)
        .expect("an annotation under the point");
    // The underline lies on top of the highlight.
    assert_eq!(picked, (1, session.annotations(1)[1].0));
    session.picked = Some(picked);
    let (position, corners) = session.picked_outline().unwrap();
    assert_eq!(position, 1);
    assert!(corners.iter().any(|c| (c.x - word.x).abs() < 30.0));
    assert!(session.remove_picked_annotation(&scheduler).unwrap());
    assert_eq!(
        session.edit().undo_edit(),
        Some(Edit::RemoveAnnotation(AnnotType::Underline))
    );
    assert!(session.picked.is_none());
    assert!(!session.remove_picked_annotation(&scheduler).unwrap());
    assert_eq!(session.annotations(1).len(), 1);

    // Saved: the highlights are in the file, with the page they are on moved to the front and a
    // page deleted.
    assert_eq!(session.move_pages(&[2], 0), Some(0..1));
    assert!(session.delete(&[1]));
    let written = session.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    assert_eq!(saved.viewer.page_count(), 2);
    for (position, text) in [(0, "alpha page 3"), (1, "alpha page 2")] {
        let annots = saved.annotations(position);
        assert_eq!(annots.len(), 1);
        assert!(matches!(annots[0].0, AnnotId::File { .. }));
        assert_eq!(annots[0].1.annot_type(), AnnotType::Highlight);
        assert!(annots[0].1.contents.as_deref().unwrap().starts_with(text));
    }
    // They are part of the page there, not drawn on top.
    assert!(saved.overlay(0).is_empty());
    saved.close(&scheduler);
    session.close(&scheduler);
}

#[test]
fn deleting_an_annotation_the_file_has() {
    use specto_annot::{Color, MarkupStyle};
    use specto_doc::AnnotId;

    // A file with a highlight on each of its two pages.
    let scheduler = Scheduler::new(1, || {});
    let mut first = open(&scheduler, book(2, "alpha"));
    first.selection = Some(Selection::all(2));
    assert!(first.mark_selection(MarkupStyle::Highlight, Color::new(1.0, 0.9, 0.0)));
    let file = first.save_job().write().unwrap();
    // Only something new: appended to the file, however often it is saved.
    let original = book(2, "alpha");
    assert!(file.starts_with(&original) && file.len() > original.len());
    assert_eq!(first.save_job().write().unwrap(), file);
    first.close(&scheduler);

    let mut session = open(&scheduler, file);
    let original = session.viewer.pages().to_vec();
    assert_eq!(session.annotations(0).len(), 1);
    // The renderer draws it with the page: nothing for the frontend to do.
    assert!(session.overlay(0).is_empty());

    // Picked like any other, and deleted: the page is then rendered from a copy of its own, without
    // it. The other page comes from the file as before.
    let on_it = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(25.0, 255.0))
        .unwrap();
    let picked = session
        .annotation_at(on_it)
        .expect("the highlight is there");
    assert!(matches!(picked.1, AnnotId::File { page: 0, .. }));
    session.picked = Some(picked);
    assert!(session.picked_outline().is_some());
    let scroll = session.viewer.scroll_offset();
    assert!(session.remove_picked_annotation(&scheduler).unwrap());
    assert!(session.annotations(0).is_empty());
    assert!(session.edit().is_modified());
    let now = session.viewer.pages().to_vec();
    assert_ne!(now[0].doc, original[0].doc);
    assert_eq!(now[0].page, 0);
    assert_eq!(now[0].geometry, original[0].geometry);
    assert_eq!((now[1].doc, now[1].page), (original[1].doc, 1));
    assert_eq!(session.viewer.scroll_offset(), scroll, "the view moved");
    // Its text is still the file's.
    assert!(all_text(&mut session).starts_with("alpha page 1"));

    // Undone: from the file again. Redone: a copy again.
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(session.viewer.pages()[0].doc, original[0].doc);
    assert_eq!(session.annotations(0).len(), 1);
    assert!(session.redo(&scheduler).is_some());
    let copy = session.viewer.pages()[0].doc;
    assert_ne!(copy, original[0].doc);

    // The page moved and turned is still shown from its copy.
    assert_eq!(session.move_pages(&[0], 2), Some(1..2));
    assert!(session.rotate(&[1], Rotation::Cw90));
    let copy = session.viewer.pages()[1];
    assert!(copy.doc != original[0].doc && copy.page == 0);
    assert_eq!(copy.rotation, Rotation::Cw90);
    assert_eq!(session.viewer.pages()[0].doc, original[1].doc);

    // Saved: the highlight is gone from that page only.
    let written = session.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    assert_eq!(saved.annotations(0).len(), 1);
    assert!(saved.annotations(1).is_empty());
    saved.close(&scheduler);
    session.close(&scheduler);
}

#[test]
fn annotations_go_with_pages_to_another_session() {
    use specto_annot::{Color, MarkupStyle};

    let scheduler = Scheduler::new(1, || {});
    let mut from = open(&scheduler, book(3, "alpha"));
    from.selection = Some(Selection::all(1));
    assert!(from.mark_selection(MarkupStyle::Highlight, Color::new(1.0, 0.9, 0.0)));
    let bundle = from.bundle(&[0, 1]);
    // For printing: the marked page as a file with the highlight in it, the other one straight from
    // its file.
    let copy = bundle
        .annotated_page(0)
        .expect("page 1 was marked")
        .unwrap();
    let mut printed = open(&scheduler, copy);
    assert_eq!(printed.viewer.page_count(), 1);
    assert_eq!(printed.annotations(0).len(), 1);
    printed.close(&scheduler);
    assert!(bundle.annotated_page(1).is_none());

    // Into another document: the highlight comes along, as part of the one edit.
    let mut to = open(&scheduler, book(1, "beta"));
    assert_eq!(
        to.insert_bundle(&scheduler, 1, &bundle).unwrap(),
        Some(1..3)
    );
    assert!(to.annotations(0).is_empty());
    assert_eq!(to.annotations(1).len(), 1);
    assert_eq!(to.overlay(1).len(), 2);
    assert!(to.annotations(2).is_empty());
    assert!(to.undo(&scheduler).is_some());
    assert_eq!(to.viewer.page_count(), 1);
    assert!(to.redo(&scheduler).is_some());
    assert_eq!(to.annotations(1).len(), 1);
    // Saved from there, it is in the file.
    let written = to.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    assert_eq!(saved.annotations(1).len(), 1);
    saved.close(&scheduler);

    // The same pages again: the page has its annotations here already.
    assert_eq!(
        to.insert_bundle(&scheduler, 0, &bundle).unwrap(),
        Some(0..2)
    );
    assert_eq!(to.annotations(0).len(), 1);
    from.close(&scheduler);
    to.close(&scheduler);
}

#[test]
fn drawing_with_the_pointer() {
    use specto_annot::{AnnotKind, AnnotType, Color};
    use specto_doc::Edit;
    use specto_geom::{ViewPoint, ViewVector};
    use specto_session::{DraftKind, ViewShape};

    let red = Color::new(1.0, 0.0, 0.0);
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(2, "alpha"));
    let at = |s: &Session, x, y| s.viewer.pdf_to_view(0, PdfPoint::new(x, y)).unwrap();

    // Not beside the pages.
    assert!(!session.begin_draft(DraftKind::Ink, red, 2.0, ViewPoint::new(-50.0, -50.0)));
    assert!(!session.is_drafting() && !session.finish_draft());

    // A stroke: shown while it is drawn, an annotation once it is done.
    let start = at(&session, 50.0, 100.0);
    assert!(session.begin_draft(DraftKind::Ink, red, 2.0, start));
    for i in 1..=200 {
        // A wave, a tenth of a pixel at a time: far more points than needed.
        let x = f64::from(i) * 0.5;
        session.extend_draft(start + ViewVector::new(x, (x / 10.0).sin() * 10.0));
    }
    assert!(session.is_drafting() && session.annotations(0).is_empty());
    assert!(matches!(session.overlay(0)[..], [ViewShape::Path { .. }]));
    assert!(session.overlay(1).is_empty());
    assert!(session.finish_draft());
    assert!(!session.is_drafting());
    assert_eq!(
        session.edit().undo_edit(),
        Some(Edit::AddAnnotation(AnnotType::Ink))
    );
    let stroke = session.annotations(0)[0].1.clone();
    let AnnotKind::Ink { strokes } = &stroke.kind else {
        panic!("not ink");
    };
    // Thinned out, but still the wave: it starts and ends where the pointer did, and goes up and
    // down in between.
    let points = &strokes[0];
    assert!((8..80).contains(&points.len()), "{} points", points.len());
    assert!((points[0] - PdfPoint::new(50.0, 100.0)).length() < 0.01);
    let ys: Vec<f64> = points.iter().map(|p| p.y).collect();
    let (low, high) = (
        ys.iter().copied().fold(f64::INFINITY, f64::min),
        ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
    );
    assert!(high - low > 10.0, "flattened: {low} to {high}");
    assert_eq!(stroke.color, Some(red));
    assert!(stroke.modified.is_some());
    // The area it takes up holds the whole line.
    assert!(points.iter().all(|p| stroke.rect.contains_inclusive(*p)));
    // Picked on the line, not in the area around it.
    let on_it = session
        .viewer
        .pdf_to_view(0, points[points.len() / 2])
        .unwrap();
    assert!(session.annotation_at(on_it).is_some());
    let corner = session.viewer.pdf_to_view(0, stroke.rect.min).unwrap();
    assert!(session.annotation_at(corner).is_none());

    // A rectangle from corner to corner. It stays on its page when the pointer leaves it.
    assert!(session.begin_draft(DraftKind::Square, red, 2.0, at(&session, 20.0, 280.0)));
    session.extend_draft(at(&session, 100.0, 200.0));
    session.extend_draft(ViewPoint::new(-500.0, -500.0));
    assert!(session.finish_draft());
    let square = session.annotations(0)[1].1.clone();
    assert_eq!(square.annot_type(), AnnotType::Square);
    // Up to the page's left and top edge, and its line around that.
    assert!(square.rect.min.x <= 0.0 && square.rect.max.y >= 300.0);
    assert!((square.rect.max.x - 21.0).abs() < 0.01);

    // A click with a shape tool draws nothing, and Escape gives a drawing up.
    assert!(session.begin_draft(DraftKind::Circle, red, 2.0, at(&session, 50.0, 50.0)));
    assert!(!session.finish_draft());
    assert!(session.begin_draft(DraftKind::Line, red, 2.0, at(&session, 50.0, 50.0)));
    session.extend_draft(at(&session, 150.0, 50.0));
    session.cancel_draft();
    assert!(!session.finish_draft());
    assert_eq!(session.annotations(0).len(), 2);
    // A click with the pen is a dot.
    assert!(session.begin_draft(DraftKind::Ink, red, 2.0, at(&session, 150.0, 150.0)));
    assert!(session.finish_draft());
    assert_eq!(session.annotations(0).len(), 3);

    // Saved, they are in the file.
    let written = session.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    let types: Vec<AnnotType> = saved
        .annotations(0)
        .iter()
        .map(|(_, a)| a.annot_type())
        .collect();
    assert_eq!(types, [AnnotType::Ink, AnnotType::Square, AnnotType::Ink]);
    saved.close(&scheduler);
    session.close(&scheduler);
}

#[test]
fn moving_and_resizing() {
    use specto_annot::{AnnotKind, AnnotType, Appearance, Color};
    use specto_doc::{AnnotId, Edit};
    use specto_geom::ViewVector;
    use specto_session::{DraftKind, Grip};

    let red = Color::new(1.0, 0.0, 0.0);
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(1, "alpha"));
    let at = |s: &Session, x, y| s.viewer.pdf_to_view(0, PdfPoint::new(x, y)).unwrap();
    // How far a point of the page is on screen.
    let unit = (at(&session, 1.0, 0.0) - at(&session, 0.0, 0.0)).length();
    let draw = |s: &mut Session, kind, from: (f64, f64), to: (f64, f64)| {
        assert!(s.begin_draft(kind, red, 2.0, at(s, from.0, from.1)));
        s.extend_draft(at(s, to.0, to.1));
        assert!(s.finish_draft());
    };
    draw(
        &mut session,
        DraftKind::Square,
        (50.0, 50.0),
        (100.0, 100.0),
    );
    draw(&mut session, DraftKind::Line, (20.0, 150.0), (120.0, 150.0));
    let frame = |s: &mut Session, i: usize| s.annotations(0)[i].1.frame();

    // Nothing to take hold of on the empty page. The rectangle is taken by its inside, and moved by
    // as far as the pointer goes.
    assert_eq!(session.grip_at(at(&session, 150.0, 250.0)), None);
    let inside = at(&session, 75.0, 75.0);
    assert_eq!(session.grip_at(inside), Some(Grip::Move));
    assert!(session.begin_adjust(&scheduler, inside).unwrap());
    assert!(session.is_adjusting() && session.picked.is_some());
    session.adjust_to(inside + ViewVector::new(10.0 * unit, 0.0));
    // Shown where it is going, while the document has it where it was.
    assert_eq!(frame(&mut session, 0).min.x, 50.0);
    assert_eq!(session.picked_handles().len(), 4);
    assert!(session.finish_adjust(&scheduler));
    assert_eq!(
        session.edit().undo_edit(),
        Some(Edit::ChangeAnnotation(AnnotType::Square))
    );
    let f = frame(&mut session, 0);
    assert!((f.min.x - 60.0).abs() < 1e-6 && (f.max.x - 110.0).abs() < 1e-6);
    assert!((f.min.y - 50.0).abs() < 1e-6);

    // Its top right corner dragged out: the corner across stays.
    let corner = at(&session, 110.0, 100.0);
    assert_eq!(session.grip_at(corner), Some(Grip::Corner(2)));
    assert!(session.begin_adjust(&scheduler, corner).unwrap());
    session.adjust_to(at(&session, 140.0, 130.0));
    assert!(session.finish_adjust(&scheduler));
    let f = frame(&mut session, 0);
    assert!((f.min.x - 60.0).abs() < 1e-6 && (f.min.y - 50.0).abs() < 1e-6);
    assert!((f.max.x - 140.0).abs() < 1e-6 && (f.max.y - 130.0).abs() < 1e-6);

    // A drag that ends where it began changes nothing, and Escape gives one up.
    assert!(
        session
            .begin_adjust(&scheduler, at(&session, 100.0, 90.0))
            .unwrap()
    );
    assert!(!session.finish_adjust(&scheduler));
    assert!(
        session
            .begin_adjust(&scheduler, at(&session, 100.0, 90.0))
            .unwrap()
    );
    session.adjust_to(at(&session, 10.0, 10.0));
    session.cancel_adjust(&scheduler);
    assert!(!session.is_adjusting());
    assert!((frame(&mut session, 0).min.x - 60.0).abs() < 1e-6);

    // The line: taken on the line (it gets picked by that), and then by an end, which goes where
    // the pointer does.
    let on_line = at(&session, 70.0, 150.0);
    assert_eq!(session.grip_at(on_line), Some(Grip::Move));
    assert!(session.begin_adjust(&scheduler, on_line).unwrap());
    assert!(!session.finish_adjust(&scheduler));
    assert_eq!(session.picked_handles().len(), 2);
    let end = at(&session, 120.0, 150.0);
    assert_eq!(session.grip_at(end), Some(Grip::End(1)));
    assert!(session.begin_adjust(&scheduler, end).unwrap());
    session.adjust_to(at(&session, 150.0, 200.0));
    assert!(session.finish_adjust(&scheduler));
    let AnnotKind::Line { from, to } = session.annotations(0)[1].1.kind else {
        panic!("not a line");
    };
    assert!((from - PdfPoint::new(20.0, 150.0)).length() < 1e-6);
    assert!((to - PdfPoint::new(150.0, 200.0)).length() < 1e-6);
    // Not beyond the page.
    assert!(
        session
            .begin_adjust(&scheduler, at(&session, 150.0, 200.0))
            .unwrap()
    );
    session.adjust_to(at(&session, 900.0, 900.0));
    assert!(session.finish_adjust(&scheduler));
    let AnnotKind::Line { to, .. } = session.annotations(0)[1].1.kind else {
        panic!("not a line");
    };
    assert!((to - PdfPoint::new(200.0, 300.0)).length() < 1e-6, "{to:?}");

    // Saved and opened again, they are the file's, drawn with the page, and still to be moved: the
    // look in the file is known to be ours.
    let written = session.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    let (id, square) = saved.annotations(0)[0].clone();
    assert!(matches!(id, AnnotId::File { .. }));
    assert_eq!(square.appearance, Appearance::Own);
    assert!(saved.overlay(0).is_empty());
    let original = saved.viewer.pages()[0].doc;
    let inside = saved
        .viewer
        .pdf_to_view(0, PdfPoint::new(100.0, 90.0))
        .unwrap();
    assert!(saved.begin_adjust(&scheduler, inside).unwrap());
    // Out of the page while it is dragged, and drawn over it instead.
    assert_ne!(saved.viewer.pages()[0].doc, original);
    assert_eq!(saved.overlay(0).len(), 1);
    saved.adjust_to(inside + ViewVector::new(0.0, 20.0 * unit));
    assert!(saved.finish_adjust(&scheduler));
    assert_ne!(saved.viewer.pages()[0].doc, original);
    assert!((saved.annotations(0)[0].1.frame().min.y - 30.0).abs() < 1e-3);
    // Given up, it is back in the page as the file has it.
    assert!(saved.undo(&scheduler).is_some());
    assert_eq!(saved.viewer.pages()[0].doc, original);
    let inside = saved
        .viewer
        .pdf_to_view(0, PdfPoint::new(100.0, 90.0))
        .unwrap();
    assert!(saved.begin_adjust(&scheduler, inside).unwrap());
    saved.cancel_adjust(&scheduler);
    assert_eq!(saved.viewer.pages()[0].doc, original);
    assert!(saved.overlay(0).is_empty());
    saved.close(&scheduler);
    session.close(&scheduler);
}

#[test]
fn notes_and_text_boxes() {
    use specto_annot::{AnnotType, Color, TextFont};
    use specto_doc::Edit;
    use specto_geom::ViewPoint;

    let black = Color::new(0.0, 0.0, 0.0);
    let yellow = Color::new(1.0, 0.9, 0.0);
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(2, "alpha"));
    let file = session.viewer.pages()[0].doc;
    let at = |s: &Session, x, y| s.viewer.pdf_to_view(0, PdfPoint::new(x, y)).unwrap();

    // A note: picked when made, its text set afterwards. The frontend draws its mark, and the page
    // is still the file's.
    assert!(!session.add_note(ViewPoint::new(-50.0, -50.0), yellow));
    assert!(session.add_note(at(&session, 150.0, 100.0), yellow));
    assert_eq!(
        session.edit().undo_edit(),
        Some(Edit::AddAnnotation(AnnotType::Note))
    );
    assert_eq!(session.picked_text().as_deref(), Some(""));
    assert!(!session.picked_is_text_box());
    assert!(session.set_picked_text(&scheduler, "Check this").unwrap());
    assert!(!session.set_picked_text(&scheduler, "Check this").unwrap());
    assert_eq!(session.picked_text().as_deref(), Some("Check this"));
    assert!(!session.overlay(0).is_empty());
    assert_eq!(session.viewer.pages()[0].doc, file);
    // It moves, but has no handles to resize it by.
    assert!(session.picked_handles().is_empty());
    assert!(session.grip_at(at(&session, 155.0, 95.0)).is_some());

    // A text box: no text, no box. With text, the renderer draws it, from a copy of the page with
    // it in.
    let place = at(&session, 20.0, 200.0);
    assert!(!session.add_text_box(&scheduler, place, "  ", (12.0, TextFont::default()), black));
    assert!(session.add_text_box(
        &scheduler,
        place,
        "Some words to wrap",
        (12.0, TextFont::default()),
        black
    ));
    assert!(session.picked_is_text_box());
    assert_eq!(session.annotations(0).len(), 2);
    let with_box = session.viewer.pages()[0].doc;
    assert_ne!(with_box, file);
    assert_eq!(session.viewer.pages()[1].doc, file);
    // No more than the note for the frontend to draw.
    let note_only = session.overlay(0).len();
    assert_eq!(session.overlay_in(0, 100.0, 150.0).len(), note_only);
    let one_line = session.annotations(0)[1].1.rect;
    // More text: a higher box, with its top where it was, and another copy.
    assert!(
        session
            .set_picked_text(&scheduler, "Some words to wrap\nand a second line")
            .unwrap()
    );
    let two_lines = session.annotations(0)[1].1.rect;
    assert_eq!(two_lines.max, one_line.max);
    assert!(two_lines.height() > one_line.height() + 10.0);
    assert_ne!(session.viewer.pages()[0].doc, with_box);
    // Narrower by a handle: more lines.
    let corner = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(two_lines.max.x, two_lines.min.y))
        .unwrap();
    assert!(session.begin_adjust(&scheduler, corner).unwrap());
    session.adjust_to(at(&session, two_lines.min.x + 60.0, two_lines.min.y));
    assert!(session.finish_adjust(&scheduler));
    let narrow = session.annotations(0)[1].1.rect;
    assert!(narrow.width() < 70.0 && narrow.height() > two_lines.height());

    // Undo takes the page back through its copies to the file.
    assert!(session.undo(&scheduler).is_some());
    assert!(session.undo(&scheduler).is_some());
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(session.viewer.pages()[0].doc, file);
    assert!(session.redo(&scheduler).is_some());
    assert_ne!(session.viewer.pages()[0].doc, file);

    // Saved and opened again: both are the file's, and their text can still be changed. Emptied, a
    // text box is deleted.
    let written = session.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    let types: Vec<AnnotType> = saved
        .annotations(0)
        .iter()
        .map(|(_, a)| a.annot_type())
        .collect();
    assert_eq!(types, [AnnotType::Note, AnnotType::FreeText]);
    let on_box = saved
        .viewer
        .pdf_to_view(0, PdfPoint::new(30.0, 195.0))
        .unwrap();
    saved.picked = saved.annotation_at(on_box);
    assert_eq!(saved.picked_text().as_deref(), Some("Some words to wrap"));
    assert!(saved.set_picked_text(&scheduler, "Other words").unwrap());
    assert_eq!(
        saved.annotations(0)[1].1.contents.as_deref(),
        Some("Other words")
    );
    assert!(saved.set_picked_text(&scheduler, "").unwrap());
    assert_eq!(saved.annotations(0).len(), 1);
    assert_eq!(
        saved.edit().undo_edit(),
        Some(Edit::RemoveAnnotation(AnnotType::FreeText))
    );
    saved.close(&scheduler);
    session.close(&scheduler);
}

#[test]
fn where_a_text_box_is_typed_into() {
    use specto_annot::{Color, FontFamily, TextFont};

    let serif = TextFont {
        family: FontFamily::Serif,
        ..TextFont::default()
    };

    let black = Color::new(0.0, 0.0, 0.0);
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(2, "alpha"));
    let file = session.viewer.pages()[0].doc;
    // The page is 200 points wide in a view of 400: two pixels a point, less the margins around it.
    let unit = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(1.0, 0.0))
        .unwrap()
        - session
            .viewer
            .pdf_to_view(0, PdfPoint::new(0.0, 0.0))
            .unwrap();
    let click = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(20.0, 200.0))
        .unwrap();
    let place = session
        .new_text_box_place(click, (12.0, TextFont::default()), black)
        .expect("on a page");
    assert_eq!(place.position, 0);
    assert!((place.at - PdfPoint::new(20.0, 200.0)).length() < 1e-6);
    assert!(
        (place.zoom - unit.length()).abs() < 1e-9,
        "{} vs {}",
        place.zoom,
        unit.length()
    );
    assert!(place.zoom > 1.5, "{}", place.zoom);
    // As wide as there is room for to the page's edge.
    assert!((place.width - 176.0).abs() < 1e-6, "{}", place.width);
    assert_eq!(session.text_box_corner(&place).map(|(c, _)| c), Some(click));

    // Made there, and typed into again: out of the page meanwhile.
    assert!(session.add_text_box_on(
        &scheduler,
        0,
        place.at,
        place.width,
        "Words",
        (12.0, TextFont::default()),
        black
    ));
    let with_box = session.viewer.pages()[0].doc;
    assert_ne!(with_box, file);
    let (again, text) = session
        .begin_text_edit(&scheduler)
        .expect("a text box is picked");
    assert_eq!(text, "Words");
    assert!((again.at - place.at).length() < 1e-6 && (again.width - place.width).abs() < 1e-6);
    assert_eq!(session.viewer.pages()[0].doc, file);
    // Given up: back as it was. With other text: changed.
    assert!(!session.end_text_edit(&scheduler, None).unwrap());
    assert_ne!(session.viewer.pages()[0].doc, file);
    assert!(session.begin_text_edit(&scheduler).is_some());
    assert!(
        session
            .end_text_edit(&scheduler, Some(("Other words", (14.0, serif))))
            .unwrap()
    );
    assert_eq!(
        session.annotations(0)[0].1.contents.as_deref(),
        Some("Other words")
    );
    assert_ne!(session.viewer.pages()[0].doc, file);
    // In another size and font with it, as one edit, and those alone.
    assert_eq!(session.picked_text_style(), Some((14.0, serif)));
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(
        session.picked_text_style(),
        None,
        "nothing is picked after undo"
    );
    assert_eq!(
        session.annotations(0)[0].1.contents.as_deref(),
        Some("Words")
    );
    assert!(session.redo(&scheduler).is_some());
    let on_box = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(30.0, 195.0))
        .unwrap();
    session.picked = session.annotation_at(on_box);
    let height = session.annotations(0)[0].1.rect.height();
    let big = (
        28.0,
        TextFont {
            bold: true,
            ..serif
        },
    );
    assert!(session.restyle_picked_text_box(&scheduler, big).unwrap());
    assert!(!session.restyle_picked_text_box(&scheduler, big).unwrap());
    assert_eq!(session.picked_text_style(), Some(big));
    assert!(session.annotations(0)[0].1.rect.height() > height + 10.0);
    // Saved and read again, it is what it was.
    let written = session.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    let on_box = saved
        .viewer
        .pdf_to_view(0, PdfPoint::new(30.0, 190.0))
        .unwrap();
    saved.picked = saved.annotation_at(on_box);
    assert_eq!(saved.picked_text_style(), Some(big));
    saved.close(&scheduler);
    session.close(&scheduler);
}

#[test]
fn typed_text_goes_to_the_box_it_was_typed_into() {
    use specto_annot::{Color, TextFont};

    let black = Color::new(0.0, 0.0, 0.0);
    let style = (12.0, TextFont::default());
    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(2, "alpha"));
    let a = PdfPoint::new(20.0, 200.0);
    let b = PdfPoint::new(20.0, 100.0);
    assert!(session.add_text_box_on(&scheduler, 0, a, 150.0, "First", style, black));
    assert!(session.add_text_box_on(&scheduler, 0, b, 150.0, "Second", style, black));
    let texts = |s: &mut Session| -> Vec<String> {
        s.annotations(0)
            .iter()
            .map(|(_, a)| a.contents.clone().unwrap_or_default())
            .collect()
    };
    let on = |s: &Session, at: PdfPoint| {
        s.viewer
            .pdf_to_view(0, PdfPoint::new(at.x + 10.0, at.y - 5.0))
            .unwrap()
    };

    // Typing into the first, the second gets picked meanwhile: what was
    // typed is the first one's text all the same.
    session.picked = session.annotation_at(on(&session, a));
    assert!(session.begin_text_edit(&scheduler).is_some());
    session.picked = session.annotation_at(on(&session, b));
    assert!(
        session
            .end_text_edit(&scheduler, Some(("Changed", style)))
            .unwrap()
    );
    assert_eq!(texts(&mut session), ["Changed", "Second"]);

    // The same with nothing picked by the end, and with the box emptied:
    // it is the one that goes.
    session.picked = session.annotation_at(on(&session, a));
    assert!(session.begin_text_edit(&scheduler).is_some());
    session.picked = None;
    assert!(
        session
            .end_text_edit(&scheduler, Some(("Again", style)))
            .unwrap()
    );
    assert_eq!(texts(&mut session), ["Again", "Second"]);
    session.picked = session.annotation_at(on(&session, a));
    assert!(session.begin_text_edit(&scheduler).is_some());
    session.picked = session.annotation_at(on(&session, b));
    assert!(
        session
            .end_text_edit(&scheduler, Some(("", style)))
            .unwrap()
    );
    assert_eq!(texts(&mut session), ["Second"]);
    // The second stays picked: it wasn't the one deleted.
    assert!(session.picked.is_some());

    // Ended twice, or without having begun: nothing happens.
    assert!(
        !session
            .end_text_edit(&scheduler, Some(("Stray", style)))
            .unwrap()
    );
    assert_eq!(texts(&mut session), ["Second"]);
    session.close(&scheduler);
}

/// A PDF of the numbered object bodies, with a proper cross-reference table.
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

/// Two pages of 200 by 300 with a form on the first: a text field "name" at x 20 to 180, y 240
/// to 260, and a checkbox "agree" at x 20 to 40, y 180 to 200. The second page has a read-only
/// text field.
fn form() -> Vec<u8> {
    let stream = |dict: &str, content: &str| {
        format!(
            "<< {dict} /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        )
    };
    let look = "/Type /XObject /Subtype /Form /BBox [0 0 20 20]";
    let objects =
        vec![
        (
            1,
            "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [6 0 R 7 0 R 10 0 R] \
             /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 3 0 R >> >> >> >>"
                .to_owned(),
        ),
        (
            2,
            "<< /Type /Pages /Kids [4 0 R 5 0 R] /Count 2 /MediaBox [0 0 200 300] \
             /Resources << >> >>"
                .to_owned(),
        ),
        (
            3,
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
                .to_owned(),
        ),
        (4, "<< /Type /Page /Parent 2 0 R /Annots [6 0 R 7 0 R] >>".to_owned()),
        (5, "<< /Type /Page /Parent 2 0 R /Annots [10 0 R] >>".to_owned()),
        (
            6,
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (name) /Rect [20 240 180 260] /P 4 0 R >>"
                .to_owned(),
        ),
        (
            7,
            "<< /Type /Annot /Subtype /Widget /FT /Btn /T (agree) /Rect [20 180 40 200] /P 4 0 R \
             /V /Off /AS /Off /AP << /N << /Yes 8 0 R /Off 9 0 R >> >> >>"
                .to_owned(),
        ),
        (8, stream(look, "0 1 0 rg 0 0 20 20 re f")),
        (9, stream(look, "")),
        (
            10,
            "<< /Type /Annot /Subtype /Widget /FT /Tx /T (fixed) /Ff 1 /V (Set) \
             /Rect [20 240 180 260] /P 5 0 R >>"
                .to_owned(),
        ),
    ];
    assemble(objects)
}

#[test]
fn filling_in_a_form() {
    use specto_doc::{Edit, FieldValue};
    use specto_forms::FieldKind;
    use specto_session::{FieldClick, FieldHover};

    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, form());
    let file = session.viewer.pages()[0].doc;
    let fields = session.form_fields();
    assert_eq!(fields.len(), 3);
    let (name, agree) = (fields[0].id, fields[1].id);
    assert!(matches!(fields[0].kind, FieldKind::Text { .. }));
    let at =
        |s: &Session, page: usize, x, y| s.viewer.pdf_to_view(page, PdfPoint::new(x, y)).unwrap();

    // What the pointer is over: a field to type into, a box to tick, nothing.
    assert_eq!(
        session.field_hover(at(&session, 0, 100.0, 250.0)),
        Some(FieldHover::Text)
    );
    assert_eq!(
        session.field_hover(at(&session, 0, 30.0, 190.0)),
        Some(FieldHover::Button)
    );
    assert_eq!(session.field_hover(at(&session, 0, 100.0, 100.0)), None);

    // A click ticks the box: the page is then shown from a copy with the tick in it.
    let on_box = at(&session, 0, 30.0, 190.0);
    assert_eq!(
        session.click_field(&scheduler, on_box),
        Some(FieldClick::Changed)
    );
    assert_eq!(session.field_value(agree), Some(FieldValue::Check(true)));
    assert_eq!(session.edit().undo_edit(), Some(Edit::FillField));
    assert!(session.edit().is_modified());
    let ticked = session.viewer.pages()[0].doc;
    assert_ne!(ticked, file);
    assert_eq!(session.viewer.pages()[1].doc, file);
    // Again unticks it: as in the file, and from the file.
    assert_eq!(
        session.click_field(&scheduler, on_box),
        Some(FieldClick::Changed)
    );
    assert_eq!(session.field_value(agree), Some(FieldValue::Check(false)));
    assert_eq!(session.viewer.pages()[0].doc, file);
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(session.field_value(agree), Some(FieldValue::Check(true)));
    assert_ne!(session.viewer.pages()[0].doc, file);

    // A click on the text field says where it is, to type into.
    let on_name = at(&session, 0, 100.0, 250.0);
    let Some(FieldClick::Text {
        field,
        place,
        height,
        multiline,
        text,
        ..
    }) = session.click_field(&scheduler, on_name)
    else {
        panic!("a text field is there");
    };
    assert_eq!(
        (field, height, multiline, text.as_str()),
        (name, 20.0, false, "")
    );
    assert!((place.at - PdfPoint::new(20.0, 260.0)).length() < 1e-9);
    assert_eq!(place.width, 160.0);
    assert!((4.0..=12.0).contains(&place.size), "{}", place.size);
    // Typed into, and what was typed kept, or not.
    session.begin_field_text(&scheduler, name);
    assert!(!session.end_field_text(&scheduler, None));
    assert_eq!(
        session.field_value(name),
        Some(FieldValue::Text(String::new()))
    );
    session.begin_field_text(&scheduler, name);
    assert!(session.end_field_text(&scheduler, Some("Ada")));
    assert_eq!(
        session.field_value(name),
        Some(FieldValue::Text("Ada".to_owned()))
    );
    assert!(!session.end_field_text(&scheduler, Some("Stray")));

    // Tab goes through the fields that can be filled in, in the order of the page, and round.
    // The keyboard is on the name, which was clicked last.
    assert_eq!(session.field_focus(), Some(name));
    assert_eq!(
        session.focused_field_outline().map(|(page, _)| page),
        Some(0)
    );
    assert_eq!(
        session.tab_field(&scheduler, true),
        Some(FieldClick::Focused)
    );
    assert_eq!(session.field_focus(), Some(agree));
    // Space unticks the box, and ticks it again.
    assert_eq!(
        session.press_focused_field(&scheduler),
        Some(FieldClick::Changed)
    );
    assert_eq!(session.field_value(agree), Some(FieldValue::Check(false)));
    assert_eq!(
        session.press_focused_field(&scheduler),
        Some(FieldClick::Changed)
    );
    // On to the first again, which is typed into, and back.
    assert!(matches!(
        session.tab_field(&scheduler, true),
        Some(FieldClick::Text { field, .. }) if field == name
    ));
    assert_eq!(
        session.tab_field(&scheduler, false),
        Some(FieldClick::Focused)
    );
    assert_eq!(session.field_focus(), Some(agree));
    assert!(session.clear_field_focus() && !session.clear_field_focus());
    assert_eq!(session.press_focused_field(&scheduler), None);
    // Without a field that has the keyboard, Tab starts at the first and Shift+Tab at the last.
    assert_eq!(
        session.tab_field(&scheduler, false),
        Some(FieldClick::Focused)
    );
    assert_eq!(session.field_focus(), Some(agree));
    session.clear_field_focus();

    // Text set directly, cut to one line. On a page shown turned, a click on the text field asks
    // for that instead of typing in place.
    assert!(session.set_field_text(&scheduler, name, "Ada\nL"));
    assert_eq!(
        session.field_value(name),
        Some(FieldValue::Text("Ada L".to_owned()))
    );
    assert!(session.set_field_text(&scheduler, name, "Ada"));
    session.rotate(&[0], Rotation::Cw90);
    let on_name = at(&session, 0, 100.0, 250.0);
    assert!(matches!(
        session.click_field(&scheduler, on_name),
        Some(FieldClick::Edit { field, multiline: false, password: false, ref text, .. })
            if field == name && text == "Ada"
    ));
    assert!(session.undo(&scheduler).is_some());
    session.clear_field_focus();

    // Nothing to click on the page without fields to fill in: its one is read-only.
    session.viewer.go_to_page(1);
    assert_eq!(
        session.click_field(&scheduler, at(&session, 1, 100.0, 250.0)),
        None
    );

    // For printing, the page comes filled in.
    let bundle = session.bundle(&[0, 1]);
    assert!(bundle.annotated_page(0).is_some());

    // Saved and opened again: the file has what was filled in.
    let written = session.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    let fields = saved.form_fields();
    assert_eq!(fields[0].value, Some(FieldValue::Text("Ada".to_owned())));
    assert_eq!(fields[1].value, Some(FieldValue::Check(true)));
    assert!(!saved.edit().is_modified());
    saved.close(&scheduler);
    session.close(&scheduler);
}

/// A page of 200 by 300 with a menu "country" at x 20 to 180, y 240 to 260, which holds "de"
/// without a look for it. `form` goes into the form's dictionary, and `flags` are the field's.
fn menu(form: &str, flags: u32) -> Vec<u8> {
    let objects = vec![
        (
            1,
            format!(
                "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R] \
             /DA (/Helv 0 Tf 0 g) {form} >> >>"
            ),
        ),
        (
            2,
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] \
             /Resources << >> >>"
                .to_owned(),
        ),
        (
            3,
            "<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>".to_owned(),
        ),
        (
            4,
            format!(
                "<< /Type /Annot /Subtype /Widget /FT /Ch /Ff {flags} /T (country) /V (de) \
                 /Opt [[(de) (Germany)] [(fr) (France)] (Other)] /Rect [20 240 180 260] \
                 /P 3 0 R >>"
            ),
        ),
    ];
    assemble(objects)
}

#[test]
fn choosing_in_a_form() {
    use specto_doc::FieldValue;
    use specto_session::{FieldClick, FieldHover};

    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, menu("", 1 << 17));
    let file = session.viewer.pages()[0].doc;
    let country = session.form_fields()[0].id;
    let on_menu = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(100.0, 250.0))
        .unwrap();
    assert_eq!(session.field_hover(on_menu), Some(FieldHover::Button));

    // A click says what there is to choose from, and what is chosen.
    let Some(FieldClick::Choose {
        field,
        labels,
        chosen,
        typed,
        corners,
    }) = session.click_field(&scheduler, on_menu)
    else {
        panic!("a menu is there");
    };
    assert_eq!((field, chosen), (country, Some(0)));
    assert_eq!(labels, ["Germany", "France", "Other"]);
    // It takes no text of one's own.
    assert_eq!(typed, None);
    assert!(!session.set_field_text(&scheduler, country, "Peru"));
    let low = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(20.0, 240.0))
        .unwrap();
    assert!((corners[0] - low).length() < 1e-9);
    assert!(!session.edit().is_modified());

    // Choosing what it holds changes nothing, and another is filled in.
    assert!(!session.choose_option(&scheduler, country, 0));
    assert!(!session.choose_option(&scheduler, country, 7));
    assert!(session.choose_option(&scheduler, country, 1));
    assert_eq!(
        session.field_value(country),
        Some(FieldValue::Text("fr".to_owned()))
    );
    assert_ne!(session.viewer.pages()[0].doc, file);
    // Space on it asks again, with the new one chosen.
    assert!(matches!(
        session.press_focused_field(&scheduler),
        Some(FieldClick::Choose {
            chosen: Some(1),
            ..
        })
    ));
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(
        session.field_value(country),
        Some(FieldValue::Text("de".to_owned()))
    );
    assert_eq!(session.viewer.pages()[0].doc, file);
    assert!(session.redo(&scheduler).is_some());

    let written = session.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    assert_eq!(
        saved.form_fields()[0].value,
        Some(FieldValue::Text("fr".to_owned()))
    );
    saved.close(&scheduler);
    session.close(&scheduler);
}

#[test]
fn a_form_that_leaves_looks_to_the_reader() {
    use specto_doc::FieldValue;

    let scheduler = Scheduler::new(1, || {});
    let mut plain = open(&scheduler, menu("", 1 << 17));
    assert!(!plain.form_fields()[0].unshown);
    plain.close(&scheduler);

    // What the menu holds has no look in the file, so the page is shown from a copy with one.
    let mut session = open(&scheduler, menu("/NeedAppearances true", 1 << 17));
    let country = session.form_fields()[0].id;
    assert!(session.form_fields()[0].unshown);
    let shown = session.viewer.pages()[0].doc;
    assert!(!session.edit().is_modified());

    // Choosing something else and then what the file has again makes a new copy each time: the
    // page never goes back to the file's own, which is what a form with looks does.
    assert!(!session.choose_option(&scheduler, country, 0));
    assert!(session.choose_option(&scheduler, country, 1));
    let other = session.viewer.pages()[0].doc;
    assert_ne!(other, shown);
    assert!(session.choose_option(&scheduler, country, 0));
    assert_eq!(
        session.field_value(country),
        Some(FieldValue::Text("de".to_owned()))
    );
    let again = session.viewer.pages()[0].doc;
    assert!(again != shown && again != other);
    session.close(&scheduler);
}

#[test]
fn typing_a_choice_of_ones_own() {
    use specto_doc::FieldValue;
    use specto_session::FieldClick;

    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, menu("", 1 << 17 | 1 << 18));
    let country = session.form_fields()[0].id;
    let on_menu = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(100.0, 250.0))
        .unwrap();
    // What it holds is said as the option shows it.
    let typed = |session: &mut Session| match session.click_field(&scheduler, on_menu) {
        Some(FieldClick::Choose { chosen, typed, .. }) => (chosen, typed),
        _ => panic!("a menu is there"),
    };
    assert_eq!(typed(&mut session), (Some(0), Some("Germany".to_owned())));

    // Text of one's own is held as it is, on one line.
    assert!(session.set_field_text(&scheduler, country, "Peru\nSouth"));
    assert_eq!(
        session.field_value(country),
        Some(FieldValue::Text("Peru South".to_owned()))
    );
    assert_eq!(typed(&mut session), (None, Some("Peru South".to_owned())));
    // What an option shows chooses that option.
    assert!(session.set_field_text(&scheduler, country, "France"));
    assert_eq!(
        session.field_value(country),
        Some(FieldValue::Text("fr".to_owned()))
    );
    assert!(session.set_field_text(&scheduler, country, "Peru"));

    let written = session.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    assert_eq!(
        saved.form_fields()[0].value,
        Some(FieldValue::Text("Peru".to_owned()))
    );
    saved.close(&scheduler);
    session.close(&scheduler);
}

#[test]
fn choosing_several_in_a_list() {
    use specto_doc::FieldValue;
    use specto_session::FieldClick;

    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, menu("", 1 << 21));
    let country = session.form_fields()[0].id;
    let on_list = session
        .viewer
        .pdf_to_view(0, PdfPoint::new(100.0, 250.0))
        .unwrap();
    let chosen = |session: &mut Session| match session.click_field(&scheduler, on_list) {
        Some(FieldClick::ChooseSeveral { labels, chosen, .. }) => {
            assert_eq!(labels, ["Germany", "France", "Other"]);
            chosen
        }
        other => panic!("a list of several is there: {other:?}"),
    };
    let several =
        |values: &[&str]| FieldValue::Several(values.iter().map(|v| (*v).to_owned()).collect());
    assert_eq!(chosen(&mut session), [0]);

    // It isn't chosen from as a menu is, and holds what is chosen in the order of the list.
    assert!(!session.choose_option(&scheduler, country, 1));
    assert!(!session.choose_options(&scheduler, country, &[0]));
    assert!(session.choose_options(&scheduler, country, &[2, 0, 7]));
    assert_eq!(
        session.field_value(country),
        Some(several(&["de", "Other"]))
    );
    assert_eq!(chosen(&mut session), [0, 2]);
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(session.field_value(country), Some(several(&["de"])));
    assert!(session.redo(&scheduler).is_some());

    let written = session.save_job().write().unwrap();
    let mut saved = open(&scheduler, written);
    assert_eq!(
        saved.form_fields()[0].value,
        Some(several(&["de", "Other"]))
    );
    saved.close(&scheduler);
    assert!(session.choose_options(&scheduler, country, &[]));
    assert_eq!(session.field_value(country), Some(several(&[])));
    session.close(&scheduler);
}

#[test]
fn tab_goes_by_rows_or_columns_where_a_page_asks() {
    // Four text fields in two rows and two columns, listed from the bottom right backwards. The
    // right one of the top row stands a little lower than the left one.
    let form = |tabs: &str| {
        let field = |name: &str, x: u32, y: u32| {
            format!(
                "<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) \
                 /Rect [{x} {y} {} {}] /P 3 0 R >>",
                x + 60,
                y + 20
            )
        };
        assemble(vec![
            (
                1,
                "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [4 0 R 5 0 R 6 0 R 7 0 R] \
                 /DA (/Helv 0 Tf 0 g) >> >>"
                    .to_owned(),
            ),
            (
                2,
                "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] \
                 /Resources << >> >>"
                    .to_owned(),
            ),
            (
                3,
                format!("<< /Type /Page /Parent 2 0 R /Annots [7 0 R 6 0 R 5 0 R 4 0 R] {tabs} >>"),
            ),
            (4, field("top left", 20, 240)),
            (5, field("top right", 110, 235)),
            (6, field("bottom left", 20, 100)),
            (7, field("bottom right", 110, 100)),
        ])
    };
    let scheduler = Scheduler::new(1, || {});
    let order = |tabs: &str| {
        let mut session = open(&scheduler, form(tabs));
        let fields = session.form_fields();
        let mut names = Vec::new();
        for _ in 0..4 {
            session.tab_field(&scheduler, true);
            let at = session.field_focus();
            names.push(
                fields
                    .iter()
                    .find(|f| Some(f.id) == at)
                    .unwrap()
                    .name
                    .clone(),
            );
            session.end_field_text(&scheduler, None);
        }
        session.close(&scheduler);
        names
    };
    assert_eq!(
        order(""),
        ["bottom right", "bottom left", "top right", "top left"]
    );
    assert_eq!(
        order("/Tabs /R"),
        ["top left", "top right", "bottom left", "bottom right"]
    );
    assert_eq!(
        order("/Tabs /C"),
        ["top left", "bottom left", "top right", "bottom right"]
    );
}

#[test]
fn resetting_a_form() {
    use specto_doc::{Edit, FieldValue};

    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, form());
    assert!(session.has_form());
    let fields = session.form_fields();
    // As the file has it, a reset changes what the file holds other than at first.
    let before: Vec<_> = fields.iter().map(|f| session.field_value(f.id)).collect();
    let name = fields[0].id;
    assert!(session.set_field_text(&scheduler, name, "Ada"));
    assert!(session.reset_form(&scheduler));
    assert_eq!(session.edit().undo_edit(), Some(Edit::ResetForm));
    for field in fields.iter().filter(|f| f.is_fillable()) {
        assert_eq!(session.field_value(field.id), field.default.clone());
    }
    assert_eq!(
        session.field_value(name),
        Some(FieldValue::Text(String::new()))
    );
    assert!(!session.reset_form(&scheduler));
    // One step back is the form as it was filled in, and one more as it was opened.
    assert!(session.undo(&scheduler).is_some());
    assert_eq!(
        session.field_value(name),
        Some(FieldValue::Text("Ada".to_owned()))
    );
    assert!(session.undo(&scheduler).is_some());
    let after: Vec<_> = fields.iter().map(|f| session.field_value(f.id)).collect();
    assert_eq!(before, after);
    session.close(&scheduler);

    let mut plain = open(&scheduler, book(1, "plain"));
    assert!(!plain.has_form());
    assert!(!plain.reset_form(&scheduler));
    plain.close(&scheduler);
}

#[test]
fn a_draft_brings_the_edits_back() {
    use specto_annot::{Color, MarkupStyle};
    use specto_session::RestoreError;

    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(3, "alpha"));
    let (other, _) = session
        .add_source(&scheduler, book(2, "beta").into(), None)
        .unwrap();
    // A file that is opened but no longer has pages in the document isn't kept.
    let (unused, _) = session
        .add_source(&scheduler, book(1, "gamma").into(), None)
        .unwrap();
    session.insert(3, &[unused]);
    session.delete(&[3]);
    session.insert(1, &[other]);
    session.rotate(&[0], Rotation::Cw90);
    session.selection = Some(Selection::all(1));
    assert!(session.mark_selection(MarkupStyle::Highlight, Color::new(1.0, 0.9, 0.0)));
    let draft = session.draft().unwrap();
    assert_eq!(draft.files.len(), 1);

    let mut again = open(&scheduler, book(3, "alpha"));
    again.restore(&scheduler, draft.clone()).unwrap();
    assert_eq!(all_text(&mut again), all_text(&mut session));
    assert_eq!(again.edit().pages()[0].rotation, Rotation::Cw90);
    assert_eq!(again.annotations(0).len(), 1);
    assert!(again.edit().is_modified());
    assert_eq!(again.edit().undo_edit(), None);

    // Not into a file that lacks the pages.
    let mut short = open(&scheduler, book(1, "alpha"));
    assert!(matches!(
        short.restore(&scheduler, draft),
        Err(RestoreError::Mismatch)
    ));
    assert_eq!(short.viewer.page_count(), 1);
    assert!(!short.edit().is_modified());

    for s in [&mut session, &mut again, &mut short] {
        s.close(&scheduler);
    }
}

#[test]
fn trimming_margins() {
    use specto_annot::Color;
    use specto_geom::{PageSize, ViewVector};
    use specto_session::DraftKind;

    let scheduler = Scheduler::new(1, || {});
    let mut session = open(&scheduler, book(6, "alpha"));
    session.viewer.set_layout_mode(LayoutMode::SinglePage);
    let whole = PageSize::new(200.0, 300.0);
    let size = |s: &Session, i: usize| s.viewer.pages()[i].displayed().size();
    let crop = |s: &Session, i: usize| s.viewer.pages()[i].crop.map(|c| c.to_box());

    // A stroke at the bottom right of the second page, far from its text.
    session.viewer.go_to_page(1);
    let at = |s: &Session, x, y| s.viewer.pdf_to_view(1, PdfPoint::new(x, y)).unwrap();
    let start = at(&session, 170.0, 20.0);
    assert!(session.begin_draft(DraftKind::Ink, Color::new(1.0, 0.0, 0.0), 2.0, start));
    session.extend_draft(start + ViewVector::new(10.0, 10.0));
    assert!(session.finish_draft());
    session.viewer.go_to_page(0);
    assert!(!session.trim_shown(), "not trimmed yet");

    // The page shown and the next two are measured and trimmed at once, the rest when they come.
    assert!(session.set_trim_margins(true));
    assert!(session.trim_margins());
    let first = crop(&session, 0).unwrap();
    // The text, at 20 points in from the left and up to 266 points up, is all shown.
    assert!(
        first.min.x < 20.0 && first.max.y > 266.0 && first.min.y > 100.0,
        "{first:?}"
    );
    assert!(size(&session, 0).width < 200.0 && size(&session, 0).height < 300.0);
    assert!(crop(&session, 2).is_some() && crop(&session, 3).is_none());
    session.viewer.go_to_page(3);
    assert!(session.trim_shown());
    assert!(!session.trim_shown(), "measured already");
    assert!(crop(&session, 3).is_some() && crop(&session, 5).is_some());
    // The stroke is shown too.
    let second = crop(&session, 1).unwrap();
    assert!(second.min.y < 20.0 && second.max.x > 180.0, "{second:?}");
    // Text is where it shows.
    session.viewer.go_to_page(0);
    let text = (session.viewer)
        .pdf_to_view(0, PdfPoint::new(30.0, 255.0))
        .unwrap();
    assert_eq!(session.viewer.hit_test(text).map(|(page, _)| page), Some(0));

    // The continuous layouts show pages whole, and measure nothing.
    session.viewer.set_layout_mode(LayoutMode::Continuous);
    assert!((0..6).all(|i| crop(&session, i).is_none() && size(&session, i) == whole));
    let (other, _) = (session.add_source(&scheduler, book(2, "beta").into(), None)).unwrap();
    assert_eq!(session.insert(6, &[other]), Some(6..8));
    session.viewer.go_to_page(7);
    assert!(!session.trim_shown());
    // The others show them trimmed again, pages of another file too.
    session.viewer.set_layout_mode(LayoutMode::TwoPage);
    assert_eq!(crop(&session, 0).unwrap(), first);
    session.viewer.go_to_page(7);
    assert!(session.trim_shown());
    assert!(crop(&session, 7).is_some());
    // A thumbnail shows the page whole.
    let thumbnail = session.viewer.page_whole(7).unwrap();
    assert_eq!(
        (thumbnail.crop, thumbnail.displayed().size()),
        (None, whole)
    );

    // And shown whole again.
    assert!(session.set_trim_margins(false));
    assert!(!session.set_trim_margins(false));
    assert!((0..8).all(|i| crop(&session, i).is_none() && size(&session, i) == whole));
    session.close(&scheduler);
}
