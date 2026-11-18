//! The per-frame path, without UniFFI: the same operations as on
//! [`ViewSession`](crate::ViewSession), addressed by session id and with results written into a
//! caller-provided byte buffer.
//!
//! `jni.rs` exposes these to Kotlin, which reads the buffer with a `ByteBuffer` in native byte
//! order.
//!
//! Frame layout (written by `encode_frame`):
//!
//! ```text
//! u32 flags            bit 0: complete, bit 1: settled, bit 2: paged,
//!                      bit 3: two pages, bit 4: first page alone,
//!                      bit 5: text is selected,
//!                      bit 6: an annotation is picked, or a form field is
//!                      being filled in, and it is on screen
//! u32 zoom_mode        0 fixed, 1 fit width, 2 fit page
//! u32 current_page
//! i32 settles_in_ms    -1: none
//! u32 last_page        with two pages, the second of the current pair
//! u32 unused
//! f64 zoom
//! f64 device_scale
//! f64 zoom_factor
//! u32 evicted_count
//! u32 page_count
//! evicted_count × { u64 id }
//! page_count × {
//!     u32 page, u32 tile_count, f32 x, y, width, height
//!     u32 flags        bit 0: busy (slow to render and not sharp yet)
//!     u32 unused
//!     tile_count × { u64 id, f32 x, y, width, height, u32 exact, u32 unused }
//! }
//! u32 thumbnail_first
//! u32 thumbnail_count
//! thumbnail_count × { u64 id }    0: not rendered yet
//! u32 highlight_count
//! u32 edges            bit 0: the selection's start is on screen, bit 1: its end
//! highlight_count × {
//!     u32 kind (0: selection, 1: search match, 2: the current match), u32 unused
//!     4 × { f32 x, y }  start bottom, end bottom, end top, start top
//! }
//! 2 × { f32 bottom x, y, top x, y }    the selection's start and end edge,
//!                                      zeros if not on screen
//! u32 shape_count      annotations to draw over the pages, under the
//! u32 unused           highlights (see `ShapeDraw`)
//! shape_count × {
//!     u32 page         index into the pages above, to clip to
//!     u32 flags        bit 0: filled, bit 1: stroked, bit 2: the fill
//!                      darkens what is under it (Multiply), bit 3: closed,
//!                      bit 4: round pen
//!     u32 fill, u32 stroke             0xAARRGGBB
//!     f32 stroke_width, u32 segment_count
//!     f32 x, y         where the outline starts
//!     segment_count × {
//!         u32 kind (0: line, 1: curve), u32 unused
//!         3 × { f32 x, y }   line: its end, twice unused;
//!                            curve: two control points and its end
//!     }
//! }
//! 4 × { f32 x, y }     the corners of the box around the picked
//!                      annotation or the form field being filled in,
//!                      zeros if flag bit 6 isn't set
//! u32 shape_count      annotations to draw over the thumbnails: as the
//! u32 unused           shapes above, with `page` an index into the
//! shape_count × { .. } thumbnails, in units of which 1000 make the width
//!                      of that page, from its top left corner
//! ```
//!
//! Finished tiles ([`take_finished`]): `u32 count`, `u32 unused`, then `count × { u64 id, u32
//! width, u32 height }`.

use crate::{
    FinishedTile, Frame, HighlightKind, Point, Rect, SegmentDraw, ShapeDraw, ZoomMode, find_session,
};

/// Sets the view size in logical pixels (dp), and the device pixels per logical pixel (the display
/// density, such as 2.625).
///
/// Here because the app animates its bars, which resizes the view on every frame. `false` if the
/// session is closed.
pub fn set_viewport(session: u64, width: f64, height: f64, scale: f64) -> bool {
    let Some(session) = find_session(session) else {
        return false;
    };
    session
        .lock()
        .session
        .viewer
        .set_viewport(specto_geom::ViewSize::new(width, height), scale);
    true
}

/// Scrolls by logical pixels. `false` if the session is closed.
pub fn scroll_by(session: u64, dx: f64, dy: f64) -> bool {
    let Some(session) = find_session(session) else {
        return false;
    };
    session
        .lock()
        .session
        .viewer
        .scroll_by(specto_geom::ViewVector::new(dx, dy));
    true
}

/// Sets the pages whose thumbnails are on screen (`count` may be 0).
///
/// Frames report their tiles in [`Frame::thumbnails`], keep them cached, and request the ones
/// still missing. `false` if the session is closed.
pub fn set_thumbnail_range(session: u64, first: u32, count: u32) -> bool {
    let Some(session) = find_session(session) else {
        return false;
    };
    let mut state = session.lock();
    let pages = state.session.viewer.page_count() as u32;
    let first = first.min(pages);
    state.thumbnails = (first, count.min(pages - first));
    true
}

/// Scrolls like a fling: in paged mode it stops at the edges of the current page instead of sliding
/// on to the next. `false` if the session is closed.
pub fn scroll_in_page_by(session: u64, dx: f64, dy: f64) -> bool {
    let Some(session) = find_session(session) else {
        return false;
    };
    session
        .lock()
        .session
        .viewer
        .scroll_in_page_by(specto_geom::ViewVector::new(dx, dy));
    true
}

/// Multiplies the zoom around (`x`, `y`). `false` if the session is closed.
pub fn zoom_by(session: u64, factor: f64, x: f64, y: f64) -> bool {
    let Some(session) = find_session(session) else {
        return false;
    };
    session
        .lock()
        .session
        .viewer
        .zoom_by(factor, specto_geom::ViewPoint::new(x, y));
    true
}

/// Extends the selection to the text nearest to (`x`, `y`): from the word first selected, or
/// moving the end last grabbed.
///
/// `false` if the session is closed.
pub fn select_to(session: u64, x: f64, y: f64) -> bool {
    let Some(session) = find_session(session) else {
        return false;
    };
    session.lock().select_to(specto_geom::ViewPoint::new(x, y));
    true
}

/// Plans a frame and writes it to `out`.
///
/// Returns the number of bytes written. If `out` is too small, returns minus the size needed. Fetch
/// the same frame with [`last_frame`] and a larger buffer. Returns 0 for a closed session.
pub fn frame(session: u64, out: &mut [u8]) -> i64 {
    let Some(session) = find_session(session) else {
        return 0;
    };
    let mut state = session.lock();
    let frame = session.frame(&mut state);
    state.encoded.clear();
    encode_frame(&frame, &mut state.encoded);
    copy_out(&state.encoded, out)
}

/// The frame most recently planned by [`frame`], again.
pub fn last_frame(session: u64, out: &mut [u8]) -> i64 {
    let Some(session) = find_session(session) else {
        return 0;
    };
    copy_out(&session.lock().encoded, out)
}

/// Writes the tiles that finished rendering to `out`.
///
/// For each, the app creates a bitmap of the given size and passes it to `Native.copyTile`, then
/// plans a frame. Tiles not copied before the next call are discarded.
///
/// At most `(out.len() - 8) / 16` tiles are announced. The rest are dropped and rendered again, so
/// `out` is sized to fit them all. Returns the number of bytes written (0: closed or no room).
pub fn take_finished(session: u64, out: &mut [u8]) -> i64 {
    let Some(session) = find_session(session) else {
        return 0;
    };
    if out.len() < 8 {
        return 0;
    }
    let tiles = session.take_finished();
    let mut buf = Vec::with_capacity(8 + tiles.len() * 16);
    // Anything that doesn't fit is dropped by the next `add_finished` and simply rendered again.
    // Buffers are sized so this doesn't happen.
    let count = tiles.len().min((out.len() - 8) / 16);
    put_u32(&mut buf, count as u32);
    put_u32(&mut buf, 0);
    for FinishedTile { id, width, height } in tiles.into_iter().take(count) {
        buf.extend_from_slice(&id.to_ne_bytes());
        put_u32(&mut buf, width);
        put_u32(&mut buf, height);
    }
    copy_out(&buf, out)
}

fn copy_out(encoded: &[u8], out: &mut [u8]) -> i64 {
    if encoded.len() > out.len() {
        return -(encoded.len() as i64);
    }
    out[..encoded.len()].copy_from_slice(encoded);
    encoded.len() as i64
}

fn put_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_ne_bytes());
}

fn put_rect(buf: &mut Vec<u8>, r: &Rect) {
    for v in [r.x, r.y, r.width, r.height] {
        buf.extend_from_slice(&v.to_ne_bytes());
    }
}

pub(crate) fn encode_frame(frame: &Frame, buf: &mut Vec<u8>) {
    put_u32(
        buf,
        u32::from(frame.complete)
            | u32::from(frame.settled) << 1
            | u32::from(frame.paged) << 2
            | u32::from(frame.two_pages) << 3
            | u32::from(frame.cover_alone) << 4
            | u32::from(frame.selection) << 5
            | u32::from(frame.picked.len() == 4) << 6,
    );
    put_u32(
        buf,
        match frame.zoom_mode {
            ZoomMode::Fixed => 0,
            ZoomMode::FitWidth => 1,
            ZoomMode::FitPage => 2,
        },
    );
    put_u32(buf, frame.current_page);
    let settles = frame
        .settles_in_ms
        .map_or(-1, |ms| ms.min(i32::MAX as u32) as i32);
    buf.extend_from_slice(&settles.to_ne_bytes());
    put_u32(buf, frame.last_page);
    put_u32(buf, 0);
    buf.extend_from_slice(&frame.zoom.to_ne_bytes());
    buf.extend_from_slice(&frame.device_scale.to_ne_bytes());
    buf.extend_from_slice(&frame.zoom_factor.to_ne_bytes());
    put_u32(buf, frame.evicted.len() as u32);
    put_u32(buf, frame.pages.len() as u32);
    for id in &frame.evicted {
        buf.extend_from_slice(&id.to_ne_bytes());
    }
    for page in &frame.pages {
        put_u32(buf, page.page);
        put_u32(buf, page.tiles.len() as u32);
        put_rect(buf, &page.rect);
        put_u32(buf, u32::from(page.busy));
        put_u32(buf, 0);
        for tile in &page.tiles {
            buf.extend_from_slice(&tile.id.to_ne_bytes());
            put_rect(buf, &tile.dest);
            put_u32(buf, u32::from(tile.exact));
            put_u32(buf, 0);
        }
    }
    put_u32(buf, frame.thumbnail_first);
    put_u32(buf, frame.thumbnails.len() as u32);
    for id in &frame.thumbnails {
        buf.extend_from_slice(&id.to_ne_bytes());
    }
    put_u32(buf, frame.highlights.len() as u32);
    put_u32(
        buf,
        u32::from(frame.selection_start.is_some()) | u32::from(frame.selection_end.is_some()) << 1,
    );
    for h in &frame.highlights {
        put_u32(
            buf,
            match h.kind {
                HighlightKind::Selection => 0,
                HighlightKind::Match => 1,
                HighlightKind::CurrentMatch => 2,
            },
        );
        put_u32(buf, 0);
        for p in [h.start_bottom, h.end_bottom, h.end_top, h.start_top] {
            put_point(buf, p);
        }
    }
    for edge in [frame.selection_start, frame.selection_end] {
        let zero = Point { x: 0.0, y: 0.0 };
        let (bottom, top) = edge.map_or((zero, zero), |e| (e.bottom, e.top));
        put_point(buf, bottom);
        put_point(buf, top);
    }
    let zero = Point { x: 0.0, y: 0.0 };
    put_shapes(buf, &frame.shapes);
    let picked = (frame.picked.len() == 4).then_some(&frame.picked[..]);
    for i in 0..4 {
        put_point(buf, picked.map_or(zero, |p| p[i]));
    }
    put_shapes(buf, &frame.thumbnail_shapes);
}

fn put_shapes(buf: &mut Vec<u8>, shapes: &[ShapeDraw]) {
    let zero = Point { x: 0.0, y: 0.0 };
    put_u32(buf, shapes.len() as u32);
    put_u32(buf, 0);
    for shape in shapes {
        put_u32(buf, shape.page);
        put_u32(
            buf,
            u32::from(shape.fill.is_some())
                | u32::from(shape.stroke.is_some()) << 1
                | u32::from(shape.multiply) << 2
                | u32::from(shape.closed) << 3
                | u32::from(shape.round) << 4,
        );
        put_u32(buf, shape.fill.unwrap_or(0));
        put_u32(buf, shape.stroke.unwrap_or(0));
        buf.extend_from_slice(&shape.stroke_width.to_ne_bytes());
        put_u32(buf, shape.segments.len() as u32);
        put_point(buf, shape.start);
        for segment in &shape.segments {
            let (kind, points) = match *segment {
                SegmentDraw::Line { to } => (0, [to, zero, zero]),
                SegmentDraw::Curve { c1, c2, to } => (1, [c1, c2, to]),
            };
            put_u32(buf, kind);
            put_u32(buf, 0);
            for p in points {
                put_point(buf, p);
            }
        }
    }
}

fn put_point(buf: &mut Vec<u8>, p: Point) {
    buf.extend_from_slice(&p.x.to_ne_bytes());
    buf.extend_from_slice(&p.y.to_ne_bytes());
}
