use std::collections::HashSet;

use specto_geom::{
    PageGeometry, PagePoint, PdfBox, PdfPoint, Rotation, ViewPoint, ViewSize, ViewVector,
};
use std::time::Instant;

use specto_geom::{DevicePoint, DeviceRect};
use specto_render::tile::{DocId, TileKey, TileScale};

use crate::{LayoutMode, Settings, ViewPage, Viewer, ZoomMode, placeholder_level};

fn page(i: u32, w: f64, h: f64) -> ViewPage {
    ViewPage {
        doc: DocId::from_raw(0),
        page: i,
        geometry: PageGeometry::new(
            PdfBox::new(PdfPoint::new(0.0, 0.0), PdfPoint::new(w, h)),
            None,
            Rotation::None,
        ),
        crop: None,
        rotation: Rotation::None,
    }
}

fn letters(n: u32) -> Vec<ViewPage> {
    (0..n).map(|i| page(i, 612.0, 792.0)).collect()
}

/// 1000 by 800 logical pixels at scale 1, gap and margin 10.
fn viewer(pages: Vec<ViewPage>) -> Viewer {
    let mut v = Viewer::new(
        pages,
        Settings {
            gap: 10.0,
            margin: 10.0,
            ..Settings::default()
        },
    );
    v.set_viewport(ViewSize::new(1000.0, 800.0), 1.0);
    v
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn fit_width_and_fit_page() {
    let mut v = viewer(letters(3));
    assert_eq!(v.zoom_mode(), ZoomMode::FitWidth);
    assert!(close(v.zoom(), (1000.0 - 20.0) / 612.0));
    v.set_zoom_mode(ZoomMode::FitPage);
    assert!(close(v.zoom(), (800.0 - 20.0) / 792.0));
    // Two-page: two pages plus one gap must fit the width.
    v.set_zoom_mode(ZoomMode::FitWidth);
    v.set_layout_mode(LayoutMode::TwoPageContinuous);
    assert!(close(v.zoom(), (1000.0 - 20.0 - 10.0) / 1224.0));
}

#[test]
fn continuous_scrolling_and_current_page() {
    let mut v = viewer(letters(10));
    v.set_zoom(0.5, ViewPoint::origin()); // pages of 306 by 396, plus a gap of 10
    assert_eq!(v.current_page(), 0);
    let r0 = v.page_rect(0).unwrap();
    assert!(close(r0.origin.y, 10.0));
    assert!(close(r0.origin.x, (1000.0 - 306.0) / 2.0));

    v.scroll_by(ViewVector::new(0.0, 406.0 * 3.0));
    assert_eq!(v.current_page(), 3);
    assert!(close(v.page_rect(3).unwrap().origin.y, 10.0));

    // Clamped at the end: content is 10 times 406, plus 10, tall.
    v.scroll_by(ViewVector::new(0.0, 1e6));
    assert!(close(v.scroll_offset().y, 10.0 * 406.0 + 10.0 - 800.0));
    v.go_to_page(4);
    assert!(close(v.page_rect(4).unwrap().origin.y, 10.0));
    v.next_page();
    assert_eq!(v.current_page(), 5);
}

#[test]
fn zooms_and_scrolls_that_are_not_numbers_are_ignored() {
    let mut v = viewer(letters(5));
    v.set_zoom(1.0, ViewPoint::origin());
    v.scroll_by(ViewVector::new(0.0, 500.0));
    for zoom in [f64::NAN, f64::INFINITY, 0.0, -2.0] {
        v.set_zoom(zoom, ViewPoint::origin());
        v.zoom_by(zoom, ViewPoint::origin());
    }
    v.scroll_by(ViewVector::new(f64::NAN, 1.0));
    v.scroll_in_page_by(ViewVector::new(0.0, f64::INFINITY));
    assert!(close(v.zoom(), 1.0));
    assert!(close(v.scroll_offset().y, 500.0));
    // Still works afterwards.
    v.scroll_by(ViewVector::new(0.0, 10.0));
    assert!(close(v.scroll_offset().y, 510.0));
}

#[test]
fn a_viewport_smaller_than_its_margins_keeps_the_zoom() {
    let mut v = viewer(letters(3));
    v.set_viewport(ViewSize::new(10.0, 10.0), 1.0);
    assert!(v.zoom() > 0.0);
    // What a double tap zooms from.
    assert!(v.zoom_relative_to(ZoomMode::FitWidth) > 0.0);
}

#[test]
fn zoom_keeps_the_anchor_point_in_place() {
    let mut v = viewer(letters(5));
    v.set_zoom(1.0, ViewPoint::origin());
    v.scroll_by(ViewVector::new(0.0, 500.0));
    let anchor = ViewPoint::new(600.0, 300.0);
    let (page, before) = v.hit_test(anchor).unwrap();
    v.zoom_by(2.0, anchor);
    assert!(close(v.zoom(), 2.0));
    let after = v.page_to_view(page, before).unwrap();
    assert!(
        close(after.x, anchor.x) && close(after.y, anchor.y),
        "{after:?}"
    );
    assert_eq!(v.zoom_mode(), ZoomMode::Fixed);
}

#[test]
fn paged_modes_lay_out_one_spread() {
    let mut v = viewer(letters(5));
    v.set_layout_mode(LayoutMode::TwoPage);
    v.set_cover_alone(true);
    // Spreads: [0] [1 2] [3 4]
    assert!(v.page_rect(0).is_some() && v.page_rect(1).is_none());
    v.next_page();
    assert_eq!(v.current_page(), 1);
    let (a, b) = (v.page_rect(1).unwrap(), v.page_rect(2).unwrap());
    assert!(close(a.origin.y, b.origin.y));
    assert!(close(b.origin.x - a.max_x(), 10.0));
    v.go_to_page(4);
    assert_eq!(v.current_page(), 3);
    v.previous_page();
    assert_eq!(v.current_page(), 1);
    // Switching to continuous keeps the page.
    v.set_layout_mode(LayoutMode::Continuous);
    assert_eq!(v.current_page(), 1);
}

#[test]
fn short_content_is_centered() {
    let mut v = viewer(letters(1));
    v.set_zoom(0.5, ViewPoint::origin());
    let r = v.page_rect(0).unwrap();
    assert!(
        close(r.center().x, 500.0) && close(r.center().y, 400.0),
        "{r:?}"
    );
}

#[test]
fn hit_test_round_trips_and_misses_gaps() {
    let mut v = viewer(letters(3));
    v.set_zoom(1.0, ViewPoint::origin());
    let (i, p) = v.hit_test(ViewPoint::new(500.0, 300.0)).unwrap();
    assert_eq!(i, 0);
    let back = v.page_to_view(i, p).unwrap();
    assert!(close(back.x, 500.0) && close(back.y, 300.0));
    // The gap between page 0 and 1.
    let gap_y = v.page_rect(0).unwrap().max_y() + 5.0;
    assert!(v.hit_test(ViewPoint::new(500.0, gap_y)).is_none());
    assert!(v.hit_test(ViewPoint::new(5.0, 300.0)).is_none());
    let _ = PagePoint::origin();
}

#[test]
fn rotated_pages_use_displayed_size() {
    let mut pages = letters(2);
    pages[1].rotation = Rotation::Cw90;
    let mut v = viewer(pages);
    v.set_zoom(1.0, ViewPoint::origin());
    let r = v.page_rect(1).unwrap();
    assert!(close(r.size.width, 792.0) && close(r.size.height, 612.0));
}

// Frame planning

fn all_cached(_: &TileKey) -> bool {
    true
}

fn level(k: &TileKey) -> Option<i32> {
    k.scale.level()
}

#[test]
fn frame_requests_placeholders_first_then_sharp_tiles() {
    let mut v = viewer(letters(20));
    v.set_viewport(ViewSize::new(1000.0, 800.0), 2.0);
    v.set_zoom(1.0, ViewPoint::origin());
    let f = v.frame(Instant::now(), |_| false);
    assert!(!f.complete);
    // Page 0 spans y 10..802, and page 1 starts below the 800-pixel viewport.
    let visible: Vec<usize> = f.pages.iter().map(|p| p.page).collect();
    assert_eq!(visible, vec![0]);
    let pl = placeholder_level(&v.pages()[0].displayed());
    // The visible page's placeholder comes first, then its sharp tiles at zoom 1 at scale 2, which
    // is level 2 (1224 by 1584 pixels, or 3 by 4 tiles).
    assert_eq!((f.requests[0].page, level(&f.requests[0])), (0, pl));
    let sharp = f
        .requests
        .iter()
        .skip(1)
        .take_while(|k| k.page == 0 && level(k) == Some(2))
        .count();
    assert_eq!(sharp, 12);
    // Then neighbors' placeholders, then prefetch (page 1's top tiles).
    let rest = &f.requests[1 + sharp..];
    assert_eq!((rest[0].page, level(&rest[0])), (1, pl));
    assert!(rest.iter().any(|k| k.page == 1 && level(k) == Some(2)));
    // No duplicates.
    let unique: HashSet<_> = f.requests.iter().collect();
    assert_eq!(unique.len(), f.requests.len());
}

#[test]
fn frame_with_everything_cached_is_complete_and_exact() {
    let mut v = viewer(letters(3));
    v.set_viewport(ViewSize::new(1000.0, 800.0), 2.0);
    v.set_zoom(1.0, ViewPoint::origin());
    let f = v.frame(Instant::now(), all_cached);
    assert!(f.complete);
    assert_eq!(f.settles_at, None); // 100% at scale 2 *is* level 2
    assert!(f.pages.iter().all(|p| p.complete));
    for p in &f.pages {
        // Page rects sit on whole device pixels.
        assert_eq!(p.rect.origin.x.fract(), 0.0);
        assert_eq!(p.rect.origin.y.fract(), 0.0);
        let sharp: Vec<_> = p
            .layers
            .iter()
            .filter(|t| level(&t.key) == Some(2))
            .collect();
        assert!(!sharp.is_empty());
        // Device scale 2 times zoom 1 is level 2 exactly: tiles map one to one.
        assert!(sharp.iter().all(|t| t.exact));
        for t in &sharp {
            assert_eq!(t.dest.origin.x.fract(), 0.0);
            assert_eq!(t.dest.origin.y.fract(), 0.0);
        }
    }
    // Requests only contain things outside the cache, so nothing.
    assert!(f.requests.is_empty());
    // Fully covered pages don't list the placeholder underneath (overdraw).
    for p in &f.pages {
        let pl = v.placeholder_key(p.page).unwrap();
        assert!(p.layers.iter().all(|t| t.key != pl));
    }
}

/// Checks the sharpest layer of every visible page: never upscaled, and covering the visible part
/// of the page. Returns those layers' scales.
fn check_coverage(v: &Viewer, f: &crate::Frame, what: &str) -> HashSet<TileScale> {
    let (vw, vh, scale) = (v.viewport().width, v.viewport().height, v.scale_factor());
    let viewport = DeviceRect::new(DevicePoint::origin(), (vw * scale, vh * scale).into());
    let mut scales = HashSet::new();
    for p in &f.pages {
        let Some(visible) = p.rect.intersection(&viewport) else {
            continue;
        };
        let best = p.layers.iter().map(|t| t.key.scale).max().unwrap();
        scales.insert(best);
        let sharp: Vec<_> = p.layers.iter().filter(|t| t.key.scale == best).collect();
        for t in &sharp {
            let r = t.key.region(&v.pages()[p.page].geometry).unwrap();
            assert!(
                t.dest.size.width <= r.width() as f64 + 1e-9,
                "upscaled ({what})"
            );
        }
        let covered: f64 = sharp
            .iter()
            .filter_map(|t| t.dest.intersection(&visible))
            .map(|r| r.area())
            .sum();
        assert!(covered + 1e-6 >= visible.area(), "gap ({what})");
    }
    scales
}

#[test]
fn zooming_uses_levels_and_settled_views_use_exact_tiles() {
    let mut v = viewer(letters(3));
    let t0 = Instant::now();
    let delay = v.settings().settle_delay;
    v.frame(t0, all_cached); // first frame: settled immediately
    for (zoom, scale) in [(1.3, 1.25), (0.77, 2.0), (3.1, 1.5), (0.906, 2.0)] {
        let what = format!("{zoom}×{scale}");
        v.set_viewport(ViewSize::new(1000.0, 800.0), scale);
        v.set_zoom(zoom, ViewPoint::origin());
        // Right after the change: level tiles, and a time to settle.
        let f = v.frame(t0, all_cached);
        let scales = check_coverage(&v, &f, &what);
        assert!(
            scales.iter().all(|s| s.level().is_some()),
            "{what}: {scales:?}"
        );
        assert_eq!(f.settles_at, Some(t0 + delay), "{what}");
        assert!(!v.is_settled(t0));

        // Settled: exact tiles at the device scale, drawn one to one.
        let later = t0 + delay;
        let f = v.frame(later, all_cached);
        assert!(v.is_settled(later));
        assert_eq!(f.settles_at, None);
        let scales = check_coverage(&v, &f, &what);
        assert_eq!(scales, HashSet::from([v.device_scale()]), "{what}");
        for p in &f.pages {
            for t in p.layers.iter().filter(|t| t.key.scale == v.device_scale()) {
                assert!(t.exact, "{what}: {t:?}");
                assert_eq!(t.dest.origin.x.fract(), 0.0);
                assert_eq!(t.dest.origin.y.fract(), 0.0);
            }
        }
        // Reset the clock for the next round.
        v.set_zoom(1.0, ViewPoint::origin());
        v.frame(later, all_cached);
    }
}

#[test]
fn settled_view_requests_exact_tiles_and_falls_back_to_levels() {
    let mut v = viewer(letters(1));
    v.set_viewport(ViewSize::new(1000.0, 800.0), 2.0);
    v.set_zoom(0.906, ViewPoint::origin()); // device scale 1.812, so level 2 drawn at 90.6%
    let t0 = Instant::now();
    v.frame(t0, |_| false);
    let later = t0 + v.settings().settle_delay;
    // Only level tiles are cached (from while zooming).
    let f = v.frame(later, |k| level(k).is_some());
    assert!(!f.complete);
    let device = v.device_scale();
    assert_eq!(device.level(), None);
    assert!(f.pages[0].layers.iter().all(|t| level(&t.key).is_some()));
    assert!(f.pages[0].layers.iter().any(|t| level(&t.key) == Some(2)));
    assert!(f.requests.iter().any(|k| k.scale == device));
    assert!(!f.requests.iter().any(|k| level(k) == Some(2)));
}

#[test]
fn no_exact_tiles_during_a_zoom_gesture() {
    let mut v = viewer(letters(1));
    v.set_viewport(ViewSize::new(1000.0, 800.0), 2.0);
    let t0 = Instant::now();
    v.frame(t0, |_| false);
    let delay = v.settings().settle_delay;
    v.begin_zoom_gesture();
    v.zoom_by(1.1, ViewPoint::new(500.0, 400.0));
    v.frame(t0, |_| false);
    // Paused mid-pinch for a long time: still only level tiles, no deadline.
    let much_later = t0 + delay * 20;
    let f = v.frame(much_later, |_| false);
    assert!(!v.is_settled(much_later));
    assert_eq!(f.settles_at, None);
    assert!(f.requests.iter().all(|k| level(k).is_some()));
    // Released: settles after the delay, measured from the release.
    v.end_zoom_gesture();
    let f = v.frame(much_later, |_| false);
    assert_eq!(f.settles_at, Some(much_later + delay));
    let f = v.frame(much_later + delay, |_| false);
    assert!(f.requests.iter().any(|k| k.scale == v.device_scale()));
}

#[test]
fn scrolling_does_not_unsettle() {
    let mut v = viewer(letters(5));
    v.set_viewport(ViewSize::new(1000.0, 800.0), 2.0);
    v.set_zoom(0.906, ViewPoint::origin());
    let t0 = Instant::now();
    v.frame(t0, |_| false);
    let later = t0 + v.settings().settle_delay;
    v.frame(later, |_| false);
    v.scroll_by(ViewVector::new(0.0, 300.0));
    v.go_to_page(3);
    assert!(v.is_settled(later));
}

#[test]
fn missing_tiles_fall_back_to_neighboring_levels() {
    let mut v = viewer(letters(1));
    v.set_viewport(ViewSize::new(1000.0, 800.0), 1.0);
    v.set_zoom(2.0, ViewPoint::origin()); // level 2
    // Only level-1 tiles (the previous zoom) are cached.
    let f = v.frame(Instant::now(), |k| level(k) == Some(1));
    assert!(!f.complete);
    assert!(!f.pages[0].complete);
    let levels: HashSet<Option<i32>> = f.pages[0].layers.iter().map(|t| level(&t.key)).collect();
    assert_eq!(levels, HashSet::from([Some(1)]));
    assert!(f.requests.iter().any(|k| level(k) == Some(2)));
}

#[test]
fn placeholder_fits_one_tile() {
    let poster = page(0, 3000.0, 5000.0);
    let l = placeholder_level(&poster.displayed()).unwrap();
    let px = poster
        .displayed()
        .pixel_size(specto_render::tile::level_scale(l));
    assert!(px.width <= 512 && px.height <= 512);
    assert_eq!(
        placeholder_level(&page(0, 612.0, 792.0).displayed()),
        Some(-2)
    );
}

#[test]
fn go_to_pdf_position_puts_the_target_at_the_top() {
    let mut v = viewer(letters(5));
    v.set_zoom(1.0, ViewPoint::origin());
    // Page 3 (index 2), 200 pt below its top edge (PDF y = 792 - 200).
    v.go_to_pdf_position(2, None, Some(592.0));
    let r = v.page_rect(2).unwrap();
    assert!(close(r.origin.y + 200.0, 10.0), "{r:?}"); // margin 10
    assert_eq!(v.current_page(), 2);
    // No coordinates: the page top.
    v.go_to_pdf_position(3, None, None);
    assert!(close(v.page_rect(3).unwrap().origin.y, 10.0));
    // Out-of-page positions are clamped, and the last page clamps the scroll.
    v.go_to_pdf_position(4, None, Some(-5000.0));
    assert_eq!(v.current_page(), 4);
}

#[test]
fn go_to_pdf_position_respects_rotation() {
    let mut pages = letters(4);
    pages[1].rotation = Rotation::Cw90; // displayed 792 by 612, not the last page
    let mut v = viewer(pages);
    v.set_zoom(1.0, ViewPoint::origin());
    // PDF x = 300 on a page turned clockwise becomes a page-space y of 300.
    v.go_to_pdf_position(1, Some(300.0), Some(792.0));
    let r = v.page_rect(1).unwrap();
    assert!(close(r.origin.y + 300.0, 10.0), "{r:?}");
}

#[test]
fn view_state_round_trips_across_viewers_and_sizes() {
    let mut a = viewer(letters(10));
    a.set_layout_mode(LayoutMode::TwoPageContinuous);
    a.set_cover_alone(true);
    a.set_zoom(0.8, ViewPoint::origin());
    a.go_to_pdf_position(5, None, Some(792.0 - 300.0)); // 300 pt into page 6
    let state = a.view_state();
    assert_eq!(state.page, 5);
    assert!(
        (state.page_offset - 300.0 / 792.0).abs() < 0.02,
        "{state:?}"
    );
    assert_eq!(state.zoom_mode, ZoomMode::Fixed);

    // A new viewer, as after reopening the file, in a different window size.
    let mut b = viewer(letters(10));
    b.restore_view_state(&state);
    b.set_viewport(ViewSize::new(700.0, 500.0), 2.0);
    assert_eq!(b.layout_mode(), LayoutMode::TwoPageContinuous);
    assert!(b.settings().cover_alone);
    assert!(close(b.zoom(), 0.8));
    let again = b.view_state();
    assert_eq!(again.page, 5);
    assert!(
        (again.page_offset - state.page_offset).abs() < 0.02,
        "{again:?}"
    );

    // Fewer pages now: clamped instead of failing, and the last page is in view (the scroll is
    // clamped too, so it needn't be at the top).
    let mut c = viewer(letters(3));
    c.restore_view_state(&state);
    let last = c.page_rect(2).unwrap();
    assert!(
        last.min_y() < c.viewport().height && last.max_y() > 0.0,
        "{last:?}"
    );
}

#[test]
fn back_and_forward_after_jumps() {
    let mut v = viewer(letters(20));
    assert!(!v.can_go_back() && !v.can_go_forward());
    assert!(!v.go_back() && !v.go_forward());

    // Scrolling isn't a jump, and neither is a jump to where the reader is.
    v.scroll_by(ViewVector::new(0.0, 300.0));
    let start = v.view_state();
    assert!(!v.jump(|v| v.restore_view_state(&start)));
    assert!(!v.can_go_back());

    assert!(v.jump(|v| v.go_to_page(7)));
    assert!(v.jump(|v| v.go_to_pdf_position(12, None, Some(400.0))));
    let far = v.view_state();
    assert_eq!(far.page, 12);

    // Back returns to the page and the place on it, and Forward to where Back was taken from.
    assert!(v.go_back());
    assert_eq!(v.current_page(), 7);
    assert!(v.go_back());
    let back = v.view_state();
    assert_eq!(back.page, start.page);
    assert!(close(back.page_offset, start.page_offset));
    assert!(!v.can_go_back() && v.can_go_forward());
    assert!(v.go_forward() && v.go_forward());
    let again = v.view_state();
    assert_eq!(again.page, 12);
    assert!(close(again.page_offset, far.page_offset));
    assert!(!v.can_go_forward());

    // A jump after going back drops what was ahead.
    assert!(v.go_back());
    assert!(v.jump(|v| v.go_to_page(3)));
    assert!(!v.can_go_forward());

    // A place on a page that is gone becomes the last page.
    assert!(v.jump(|v| v.go_to_page(19)));
    assert!(v.jump(|v| v.go_to_page(0)));
    v.set_pages(letters(5));
    assert!(v.go_back());
    assert_eq!(v.current_page(), 4);
}

#[test]
fn back_remembers_only_so_many_places() {
    let mut v = viewer(letters(3));
    for i in 0..250 {
        assert!(v.jump(|v| v.go_to_page(1 + i % 2)));
    }
    let mut steps = 0;
    while v.go_back() {
        steps += 1;
    }
    assert_eq!(steps, 100);
}

#[test]
fn mode_names_round_trip() {
    for m in LayoutMode::ALL {
        assert_eq!(LayoutMode::from_name(m.name()), Some(m));
    }
    for z in [ZoomMode::Fixed, ZoomMode::FitWidth, ZoomMode::FitPage] {
        assert_eq!(ZoomMode::from_name(z.name()), Some(z));
    }
    assert_eq!(LayoutMode::from_name("nonsense"), None);
}

#[test]
fn view_state_at_the_document_start_keeps_the_margin() {
    let mut a = viewer(letters(10));
    a.set_zoom(1.0, ViewPoint::origin());
    let state = a.view_state();
    assert_eq!(state.page, 0);
    assert!(state.page_offset < 0.0, "{state:?}");
    let mut b = viewer(letters(10));
    b.restore_view_state(&state);
    assert!(close(b.scroll_offset().y, 0.0), "{:?}", b.scroll_offset());
}

#[test]
fn the_end_of_a_continuous_document_is_its_last_page() {
    let mut v = viewer(letters(10));
    v.set_zoom(0.5, ViewPoint::origin()); // two pages per viewport
    v.scroll_by(ViewVector::new(0.0, 1e6));
    // Page 8 shows as much as page 9, but the user has reached the end.
    assert_eq!(v.current_page(), 9);
    v.scroll_by(ViewVector::new(0.0, -100.0));
    assert_eq!(v.current_page(), 8);
}

/// Letter portrait, letter landscape, and a small page, swiped one at a time in a 1000 by 800
/// viewport (margin and gap 10).
fn swiper() -> Viewer {
    let mut v = viewer(vec![
        page(0, 612.0, 792.0),
        page(1, 792.0, 612.0),
        page(2, 300.0, 400.0),
    ]);
    v.set_layout_mode(LayoutMode::SinglePage);
    v.set_zoom_mode(ZoomMode::FitPage);
    v.set_swipe(true);
    v
}

#[test]
fn swipe_fits_each_page_on_its_own() {
    let mut v = swiper();
    // Portrait: limited by the height.
    let z0 = 780.0 / 792.0;
    assert!(close(v.zoom(), z0));
    let r0 = v.page_rect(0).unwrap();
    assert!(close(r0.origin.x, (1000.0 - 612.0 * z0) / 2.0) && close(r0.origin.y, 10.0));
    // The next page waits just beyond the right edge, already at its own fit.
    let z1 = 980.0 / 792.0;
    let r1 = v.page_rect(1).unwrap();
    assert!(close(r1.origin.x, 1000.0 + 10.0 + 10.0), "{r1:?}");
    assert!(close(r1.size.width, 792.0 * z1));

    // Landscape: limited by the width, centered vertically.
    v.go_to_page(1);
    assert_eq!(v.current_page(), 1);
    assert!(close(v.zoom(), z1));
    let r1 = v.page_rect(1).unwrap();
    assert!(close(r1.origin.x, 10.0) && close(r1.origin.y, (800.0 - 612.0 * z1) / 2.0));
    assert!(v.page_rect(0).unwrap().max_x() < 0.0);

    // A small page is enlarged to fit, too.
    v.next_page();
    assert_eq!(v.current_page(), 2);
    assert!(close(v.zoom(), 780.0 / 400.0));
    v.previous_page();
    assert_eq!(v.current_page(), 1);

    // Rotating the device refits every page and stays on the current one.
    v.set_viewport(ViewSize::new(800.0, 1000.0), 1.0);
    assert_eq!(v.current_page(), 1);
    assert!(close(v.zoom(), 780.0 / 792.0));
    assert!(close(v.page_rect(1).unwrap().origin.x, 10.0));
    assert_eq!(v.snap_delta(ViewVector::zero()), None);
}

#[test]
fn swiping_slides_between_pages_and_snaps() {
    let mut v = swiper();
    let still = ViewVector::zero();
    assert_eq!(v.snap_delta(still), None);

    // A short drag: both pages move with the finger, and letting go returns.
    let before = v.page_rect(0).unwrap().origin.x;
    v.scroll_by(ViewVector::new(300.0, 0.0));
    assert!(close(v.page_rect(0).unwrap().origin.x, before - 300.0));
    assert!(close(v.page_rect(1).unwrap().origin.x, 1020.0 - 300.0));
    assert_eq!(v.current_page(), 0);
    assert_eq!(v.snap_delta(still), Some(ViewVector::new(-300.0, 0.0)));
    // The neighbor can be hit while it is partly on screen.
    assert_eq!(
        v.hit_test(ViewPoint::new(900.0, 400.0)).map(|h| h.0),
        Some(1)
    );

    // The same drag ending in a flick turns the page. Slots are a viewport wide plus the gap.
    let flick = ViewVector::new(400.0, 0.0);
    assert_eq!(v.snap_delta(flick), Some(ViewVector::new(710.0, 0.0)));
    // So does dragging more than half way.
    v.scroll_by(ViewVector::new(250.0, 0.0));
    assert_eq!(v.current_page(), 1);
    let rest = v.snap_delta(still).unwrap();
    assert!(close(rest.x, 460.0));
    // ... unless the finger was moving back.
    assert_eq!(v.snap_delta(-flick), Some(ViewVector::new(-550.0, 0.0)));

    // Arriving: page 1 is current and at rest.
    v.scroll_by(rest);
    assert_eq!(v.current_page(), 1);
    assert_eq!(v.snap_delta(still), None);
    assert!(close(v.zoom(), 980.0 / 792.0));
    assert!(close(v.page_rect(1).unwrap().origin.x, 10.0));

    // Backwards, with a flick.
    v.scroll_by(ViewVector::new(-40.0, 0.0));
    let back = v.snap_delta(-flick).unwrap();
    assert!(close(back.x, -970.0));
    v.scroll_by(back);
    assert_eq!(v.current_page(), 0);

    // There is nothing before the first or after the last page.
    v.scroll_by(ViewVector::new(-500.0, 0.0));
    assert!(close(v.scroll_offset().x, 0.0));
    assert_eq!(v.snap_delta(-flick), None);
    v.go_to_page(2);
    v.scroll_by(ViewVector::new(500.0, 0.0));
    assert_eq!(v.snap_delta(flick), None);
    assert_eq!(v.current_page(), 2);
}

#[test]
fn swipe_zoom_pans_within_the_page() {
    let mut v = swiper();
    let fit = v.zoom();
    let anchor = ViewPoint::new(600.0, 300.0);
    let (page, before) = v.hit_test(anchor).unwrap();
    v.zoom_by(2.0, anchor);
    assert_eq!(v.zoom_mode(), ZoomMode::Fixed);
    assert!(close(v.zoom(), 2.0 * fit));
    let after = v.page_to_view(page, before).unwrap();
    assert!(
        close(after.x, anchor.x) && close(after.y, anchor.y),
        "{after:?}"
    );
    // The zoomed page is larger than the viewport, so it rests anywhere within its slot. Its
    // neighbors stay fitted.
    assert_eq!(v.snap_delta(ViewVector::zero()), None);
    let z1 = 980.0 / 792.0;
    assert!(close(v.page_rect(1).unwrap().size.width, 980.0));

    // A fling stops at the page's edge instead of turning the page ...
    v.scroll_in_page_by(ViewVector::new(1e6, 1e6));
    assert_eq!(v.current_page(), 0);
    assert_eq!(v.snap_delta(ViewVector::zero()), None);
    let r0 = v.page_rect(0).unwrap();
    assert!(
        close(r0.max_x(), 1000.0 - 10.0) && close(r0.max_y(), 800.0 - 10.0),
        "{r0:?}"
    );
    // ... while a drag beyond it starts sliding to the next page, which comes in fitted and
    // centered, however far down this one was scrolled.
    v.scroll_by(ViewVector::new(600.0, 0.0));
    let next = v.page_rect(1).unwrap();
    assert!(close(next.origin.y, (800.0 - 612.0 * z1) / 2.0), "{next:?}");
    let on_screen = next.origin.x;
    let rest = v.snap_delta(ViewVector::zero()).unwrap();
    v.scroll_by(rest);
    // Arriving turns the page: the view is fitted again, and the page that slid in is where the
    // slide left it.
    assert_eq!(v.current_page(), 1);
    assert_eq!(v.zoom_mode(), ZoomMode::FitPage);
    assert!(close(v.zoom(), z1));
    assert!(close(v.scroll_offset().y, 0.0));
    let arrived = v.page_rect(1).unwrap();
    assert!(close(arrived.origin.x, on_screen - rest.x) && close(arrived.origin.x, 10.0));
    assert_eq!(v.snap_delta(ViewVector::zero()), None);
    // The page left behind is fitted again, too.
    v.scroll_by(ViewVector::new(-20.0, 0.0));
    let back = v.snap_delta(ViewVector::new(-400.0, 0.0)).unwrap();
    v.scroll_by(back);
    assert_eq!(v.current_page(), 0);
    assert!(close(v.zoom(), fit));
    assert!(close(
        v.page_rect(0).unwrap().origin.x,
        (1000.0 - 612.0 * fit) / 2.0
    ));

    // So does going to a page any other way.
    v.zoom_by(2.0, anchor);
    v.next_page();
    assert_eq!((v.current_page(), v.zoom_mode()), (1, ZoomMode::FitPage));
    v.zoom_by(2.0, anchor);
    v.go_to_page(1);
    assert_eq!(
        v.zoom_mode(),
        ZoomMode::Fixed,
        "staying on the page keeps the zoom"
    );
    v.go_to_page(0);
    assert_eq!(v.zoom_mode(), ZoomMode::FitPage);

    // Pages can't be made smaller than fitting the viewport.
    v.zoom_by(0.01, anchor);
    assert!(close(v.zoom(), fit));
    if let Some(center) = v.snap_delta(ViewVector::zero()) {
        // Pinching out around a point off-center can leave the page slightly aside. Letting go
        // centers it again.
        assert!(center.x.abs() < 20.0, "{center:?}");
        v.scroll_by(center);
    }
    assert_eq!(v.current_page(), 0);
    assert!(close(
        v.page_rect(0).unwrap().origin.x,
        (1000.0 - 612.0 * fit) / 2.0
    ));
    assert_eq!(v.snap_delta(ViewVector::zero()), None);

    // Leaving swipe mode keeps the page and its zoom.
    v.go_to_page(1);
    v.zoom_by(1.5, anchor);
    let zoom = v.zoom();
    v.set_swipe(false);
    assert_eq!(v.current_page(), 1);
    assert!(close(v.zoom(), zoom));
    assert!(v.page_rect(0).is_none());
    v.set_swipe(true);
    assert!(close(v.zoom(), zoom));
}

#[test]
fn swipe_frames_use_each_pages_own_scale_and_prepare_the_neighbors() {
    let mut v = swiper();
    v.set_viewport(ViewSize::new(1000.0, 800.0), 2.0);
    v.go_to_page(1);
    let scale = |v: &Viewer, i: usize| {
        TileScale::exact(
            v.page_rect(i).unwrap().size.width / v.pages()[i].displayed().size().width * 2.0,
        )
    };
    let (s0, s1, s2) = (scale(&v, 0), scale(&v, 1), scale(&v, 2));
    assert!(s0 != s1 && s1 != s2);

    // At rest only the current page is drawn, but both neighbors' tiles are requested, each at its
    // own exact scale.
    let f = v.frame(Instant::now(), |_| false);
    assert_eq!(f.pages.iter().map(|p| p.page).collect::<Vec<_>>(), vec![1]);
    let sharp = |page: u32, s: TileScale| {
        f.requests
            .iter()
            .filter(|k| k.page == page && k.scale == s)
            .count()
    };
    assert!(sharp(1, s1) > 0 && sharp(0, s0) > 0 && sharp(2, s2) > 0);
    // The visible page's tiles come before the neighbors'.
    let first_neighbor = f.requests.iter().position(|k| k.page != 1).unwrap();
    assert!(f.requests[..first_neighbor].iter().any(|k| k.scale == s1));

    // Turning the page changes the scale, but nothing is zooming: the view stays settled and draws
    // exact tiles one to one right away.
    let now = Instant::now();
    v.frame(now, all_cached);
    v.go_to_page(2);
    let f = v.frame(now, all_cached);
    assert!(f.complete && f.settles_at.is_none());
    assert!(v.is_settled(now));
    assert_eq!(f.pages.len(), 1);
    assert!(
        f.pages[0]
            .layers
            .iter()
            .all(|l| l.exact && l.key.scale == s2)
    );

    // While sliding, both pages are drawn, each from its own tiles.
    v.scroll_by(ViewVector::new(-400.0, 0.0));
    let f = v.frame(now, all_cached);
    let drawn: Vec<(usize, TileScale)> = f
        .pages
        .iter()
        .map(|p| (p.page, p.layers[0].key.scale))
        .collect();
    assert_eq!(drawn, vec![(1, s1), (2, s2)]);
    assert!(f.pages.iter().all(|p| p.layers.iter().all(|l| l.exact)));

    // Zooming still goes through levels and settles afterwards.
    let rest = v.snap_delta(ViewVector::zero()).unwrap();
    v.scroll_by(rest);
    v.zoom_by(1.3, ViewPoint::new(500.0, 400.0));
    let f = v.frame(now, all_cached);
    assert!(f.settles_at.is_some());
    // Turning the page from a zoomed-in one returns to the fitted view, but nothing zooms on the
    // page that appears: it is sharp at once.
    let later = now + std::time::Duration::from_secs(1);
    assert!(v.frame(later, all_cached).settles_at.is_none());
    v.previous_page();
    let f = v.frame(later, all_cached);
    assert_eq!(v.zoom_mode(), ZoomMode::FitPage);
    assert!(f.complete && f.settles_at.is_none());
    assert!(f.pages[0].layers.iter().all(|l| l.exact));
    v.next_page();
    v.zoom_by(1.3, ViewPoint::new(500.0, 400.0));
    let f = v.frame(later, all_cached);
    assert!(
        f.pages[0]
            .layers
            .iter()
            .all(|l| l.key.scale.level().is_some())
    );
}

#[test]
fn destinations_in_swipe_mode_stay_on_their_page() {
    let mut v = swiper();
    // A fitted page has nothing to scroll: the destination is the page.
    v.go_to_pdf_position(1, Some(100.0), Some(300.0));
    assert_eq!(v.current_page(), 1);
    assert_eq!(v.snap_delta(ViewVector::zero()), None);
    assert!(close(v.page_rect(1).unwrap().origin.x, 10.0));

    // Zoomed in, a position on the same page is honored: PDF y 300 of the 612 pt tall page is 312
    // pt from its top.
    v.zoom_by(3.0, ViewPoint::new(500.0, 400.0));
    v.go_to_pdf_position(1, Some(50.0), Some(300.0));
    assert_eq!((v.current_page(), v.zoom_mode()), (1, ZoomMode::Fixed));
    assert_eq!(v.snap_delta(ViewVector::zero()), None);
    let on_page = v.hit_test(ViewPoint::new(10.0, 10.0)).unwrap();
    assert_eq!(on_page.0, 1);
    assert!(
        close(on_page.1.x, 50.0) && close(on_page.1.y, 312.0),
        "{on_page:?}"
    );

    // A destination on another page turns the page, which is shown fitted.
    v.go_to_pdf_position(2, Some(50.0), Some(300.0));
    assert_eq!((v.current_page(), v.zoom_mode()), (2, ZoomMode::FitPage));
    assert_eq!(v.snap_delta(ViewVector::zero()), None);
}

#[test]
fn view_state_round_trips_in_swipe_mode() {
    let mut a = swiper();
    a.go_to_page(1);
    a.zoom_by(2.0, ViewPoint::new(500.0, 400.0));
    a.scroll_by(ViewVector::new(0.0, 150.0));
    let state = a.view_state();
    assert_eq!((state.page, state.zoom_mode), (1, ZoomMode::Fixed));
    assert!(state.page_offset > 0.0, "{state:?}");

    // Restored in a viewer that was on another page at another zoom.
    let mut b = swiper();
    b.go_to_page(2);
    b.restore_view_state(&state);
    assert_eq!(b.current_page(), 1);
    assert!(close(b.zoom(), a.zoom()));
    assert!(close(b.scroll_offset().y, a.scroll_offset().y));
    assert_eq!(b.snap_delta(ViewVector::zero()), None);

    // A fitted page comes back fitted.
    a.set_zoom_mode(ZoomMode::FitPage);
    let state = a.view_state();
    b.restore_view_state(&state);
    assert_eq!((b.current_page(), b.zoom_mode()), (1, ZoomMode::FitPage));
    assert!(close(b.zoom(), 980.0 / 792.0));
}

#[test]
fn swiping_through_two_page_spreads() {
    // Five letter pages, two side by side, in a wide 1400 by 800 viewport.
    let mut v = viewer(letters(5));
    v.set_viewport(ViewSize::new(1400.0, 800.0), 1.0);
    v.set_layout_mode(LayoutMode::TwoPage);
    v.set_zoom_mode(ZoomMode::FitPage);
    v.set_swipe(true);

    // The spread is fitted as a whole: two pages and the gap between them.
    let fit = f64::min((1400.0 - 20.0 - 10.0) / 1224.0, 780.0 / 792.0);
    assert!(close(v.zoom(), fit));
    let (a, b) = (v.page_rect(0).unwrap(), v.page_rect(1).unwrap());
    assert!(close(b.origin.x - a.max_x(), 10.0) && close(a.origin.y, b.origin.y));
    // Centered as a pair.
    assert!(close(a.origin.x, 1400.0 - b.max_x()), "{a:?} {b:?}");
    assert_eq!(v.current_page(), 0);

    assert_eq!(v.current_spread(), [0, 1]);

    // A flick turns to the next spread, not the next page.
    v.scroll_by(ViewVector::new(100.0, 0.0));
    let rest = v.snap_delta(ViewVector::new(500.0, 0.0)).unwrap();
    v.scroll_by(rest);
    assert_eq!(v.current_page(), 2);
    assert_eq!(v.snap_delta(ViewVector::zero()), None);
    // Both of its pages are drawn, and nothing of the neighbors.
    let frame = v.frame(Instant::now(), all_cached);
    assert_eq!(
        frame.pages.iter().map(|p| p.page).collect::<Vec<_>>(),
        vec![2, 3]
    );

    // The last page is alone in its spread and centered.
    v.go_to_page(4);
    assert_eq!(v.current_page(), 4);
    assert_eq!(v.current_spread(), [4]);
    let last = v.page_rect(4).unwrap();
    assert!(close(last.origin.x, 1400.0 - last.max_x()));
    // A page in the middle of a spread shows that spread.
    v.go_to_page(3);
    assert_eq!(v.current_page(), 2);

    // With the cover alone, pairs shift by one: [0] [1 2] [3 4]. The page the reader was on (2) is
    // now the right-hand one of its spread.
    v.set_cover_alone(true);
    assert_eq!(v.current_page(), 1);
    assert!(v.page_rect(2).is_some_and(|r| r.origin.x > 700.0));
    v.go_to_page(0);
    let cover = v.page_rect(0).unwrap();
    assert!(close(cover.origin.x, 1400.0 - cover.max_x()));
    v.next_page();
    assert_eq!(v.current_page(), 1);
    assert_eq!(v.snap_delta(ViewVector::zero()), None);

    // Back to single pages keeps the page.
    v.set_layout_mode(LayoutMode::SinglePage);
    assert_eq!(v.current_page(), 1);
    assert!(close(v.zoom(), 780.0 / 792.0));
}

// Text selection

mod selection {
    use std::collections::HashMap;
    use std::sync::Arc;

    use specto_geom::{PdfPoint, PdfQuad, Rotation, ViewPoint};
    use specto_text::{TextChar, TextPage};

    use super::{letters, page, viewer};
    use crate::{Granularity, Selection, TextPages, TextPos, text_char, text_position};

    /// Characters 6 pt wide and 10 pt tall on baseline `y`, from x = 100.
    fn line(text: &str, y: f64) -> Vec<TextChar> {
        text.chars()
            .enumerate()
            .map(|(i, c)| {
                let x = 100.0 + 6.0 * i as f64;
                let p = |x: f64, y: f64| PdfPoint::new(x, y);
                TextChar {
                    text: c.to_string(),
                    quad: PdfQuad::new([
                        p(x, y - 2.0),
                        p(x + 6.0, y - 2.0),
                        p(x + 6.0, y + 8.0),
                        p(x, y + 8.0),
                    ]),
                    size: 10.0,
                }
            })
            .collect()
    }

    /// Page `i`: "page i one" on y = 700 and "second line" on y = 680.
    struct Pages(HashMap<usize, Arc<TextPage>>);

    impl Pages {
        fn new(n: usize) -> Self {
            Self(
                (0..n)
                    .map(|i| {
                        let mut chars = line(&format!("page {i} one"), 700.0);
                        chars.extend(line("second line", 680.0));
                        (i, Arc::new(TextPage::from_chars(chars)))
                    })
                    .collect(),
            )
        }
    }

    impl TextPages for Pages {
        fn text_page(&mut self, page: usize) -> Option<Arc<TextPage>> {
            self.0.get(&page).cloned()
        }
    }

    #[test]
    fn drag_by_characters_both_ways() {
        let mut pages = Pages::new(1);
        let mut s = Selection::new(&mut pages, TextPos::new(0, 5), Granularity::Char);
        assert!(s.is_empty());
        s.extend(&mut pages, TextPos::new(0, 10));
        assert_eq!(s.text(&mut pages), "0 one");
        // Back past the anchor: the anchor becomes the end. ("ge " in the page. A selected trailing
        // space isn't copied.)
        s.extend(&mut pages, TextPos::new(0, 2));
        assert_eq!(s.range(), (TextPos::new(0, 2), TextPos::new(0, 5)));
        assert_eq!(s.text(&mut pages), "ge");
        // Into the next line.
        s.extend(&mut pages, TextPos::new(0, 16));
        assert_eq!(s.text(&mut pages), "0 one\nsecond");
    }

    #[test]
    fn words_and_lines() {
        let mut pages = Pages::new(1);
        // Double-click on "one" (characters 7..10).
        let mut s = Selection::new(&mut pages, TextPos::new(0, 8), Granularity::Word);
        assert_eq!(s.text(&mut pages), "one");
        // Dragging forwards into "second" takes the whole word.
        s.extend(&mut pages, TextPos::new(0, 12));
        assert_eq!(s.text(&mut pages), "one\nsecond");
        // Backwards into "page" takes it whole, and keeps "one".
        s.extend(&mut pages, TextPos::new(0, 2));
        assert_eq!(s.text(&mut pages), "page 0 one");
        // Triple-click selects the line.
        let s = Selection::new(&mut pages, TextPos::new(0, 13), Granularity::Line);
        assert_eq!(s.text(&mut pages), "second line");
    }

    #[test]
    fn across_pages_and_all() {
        let mut pages = Pages::new(3);
        let mut s = Selection::new(&mut pages, TextPos::new(0, 21), Granularity::Char);
        s.extend(&mut pages, TextPos::new(2, 4));
        assert_eq!(
            s.page_range(0, 21),
            None,
            "starts at the very end of page 0"
        );
        assert_eq!(s.page_range(1, 21), Some(0..21));
        assert_eq!(s.page_range(2, 21), Some(0..4));
        assert_eq!(s.text(&mut pages), "page 1 one\nsecond line\npage");
        let all = Selection::all(3);
        assert_eq!(all.page_range(2, 21), Some(0..21));
        assert_eq!(all.text(&mut pages).lines().count(), 6);
    }

    #[test]
    fn positions_from_the_screen() {
        let mut v = viewer(letters(2));
        v.set_zoom(1.0, ViewPoint::origin());
        let mut pages = Pages::new(2);
        // Page 0's "o" of "one" (character 7, x = 142..148 at y = 700).
        let at = v.pdf_to_view(0, PdfPoint::new(143.0, 703.0)).unwrap();
        assert_eq!(text_position(&v, &mut pages, at), Some(TextPos::new(0, 7)));
        assert_eq!(text_char(&v, &mut pages, at), Some(TextPos::new(0, 7)));
        // In the gap below page 0: still page 0, at the end of its last line.
        let r = v.page_rect(0).unwrap();
        let gap = ViewPoint::new(at.x + 200.0, r.max_y() + 4.0);
        assert_eq!(text_position(&v, &mut pages, gap).map(|p| p.page), Some(0));
        // The highlight lands on the characters.
        let mut s = Selection::new(&mut pages, TextPos::new(0, 7), Granularity::Char);
        s.extend(&mut pages, TextPos::new(0, 10));
        let q = s.view_quads(&v, &mut pages, 0);
        assert_eq!(q.len(), 1);
        assert!((q[0][0].x - at.x).abs() < 1.5, "{:?} vs {at:?}", q[0][0]);
        assert!(s.view_quads(&v, &mut pages, 1).is_empty());
    }

    #[test]
    fn rotated_page_maps_both_ways() {
        let mut p = page(0, 612.0, 792.0);
        p.rotation = Rotation::Cw90;
        let mut v = viewer(vec![p]);
        v.set_zoom(1.0, ViewPoint::origin());
        let pdf = PdfPoint::new(100.0, 700.0);
        let view = v.pdf_to_view(0, pdf).unwrap();
        let (i, back) = v.nearest_pdf_point(view).unwrap();
        assert_eq!(i, 0);
        assert!((back.x - pdf.x).abs() < 1e-6 && (back.y - pdf.y).abs() < 1e-6);
    }
}

// Search

mod search {
    use specto_geom::{PdfBox, PdfPoint, ViewPoint};

    use super::{letters, viewer};
    use crate::{LayoutMode, SearchResults};

    #[test]
    fn results_in_order_while_pages_arrive() {
        let mut r = SearchResults::new();
        // Indexing starts at the current page (5), then wraps around.
        r.add_page(5, vec![3..6, 10..13]);
        assert_eq!(
            r.select_from(5).map(|m| (m.page, m.chars.clone())),
            Some((5, 3..6))
        );
        let one = |r: std::ops::Range<usize>| vec![r];
        r.add_page(8, one(0..3));
        r.add_page(1, one(7..9));
        // The earlier page went in front, and the current match stayed.
        let (i, m) = r.current().unwrap();
        assert_eq!((i, m.page), (1, 5));
        assert_eq!(r.len(), 4);
        assert_eq!(r.select_next().map(|m| m.page), Some(5));
        assert_eq!(r.select_next().map(|m| m.page), Some(8));
        assert_eq!(
            r.select_next().map(|m| m.page),
            Some(1),
            "wraps to the start"
        );
        assert_eq!(r.select_previous().map(|m| m.page), Some(8), "and back");
        let on_5: Vec<bool> = r.on_page(5).map(|(_, current)| current).collect();
        assert_eq!(on_5, [false, false]);
        assert_eq!(r.on_page(2).count(), 0);
        // After the last page with matches, select_from wraps to the first.
        assert_eq!(r.select_from(9).map(|m| m.page), Some(1));
        assert!(SearchResults::new().select_next().is_none());
    }

    #[test]
    fn reveal_scrolls_only_when_needed() {
        let mut v = viewer(letters(10));
        v.set_zoom(1.0, ViewPoint::origin());
        let near_top = PdfBox::new(PdfPoint::new(72.0, 700.0), PdfPoint::new(140.0, 712.0));
        // Already visible on page 0: nothing moves.
        let before = v.scroll_offset();
        v.reveal(0, near_top);
        assert_eq!(v.scroll_offset(), before);
        // Page 6: scrolled there, the match a third of the way down.
        v.reveal(6, near_top);
        let at = v.pdf_to_view(6, PdfPoint::new(72.0, 712.0)).unwrap();
        assert!((at.y - 800.0 / 3.0).abs() < 1.0, "{at:?}");
        assert_eq!(v.current_page(), 6);
        // Single-page mode goes to the page first.
        v.set_layout_mode(LayoutMode::SinglePage);
        v.reveal(2, near_top);
        assert_eq!(v.current_page(), 2);
        let at = v.pdf_to_view(2, PdfPoint::new(72.0, 712.0)).unwrap();
        assert!(at.y >= 0.0 && at.y <= 800.0, "{at:?}");
    }
}
