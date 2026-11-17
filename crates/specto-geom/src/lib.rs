//! Typed coordinate spaces for Specto.
//!
//! Every point, size, rectangle and transform carries the space it lives in, so mixing them up is a
//! compile error. A `PdfPoint` can't be used where a `PagePoint` is expected, and a transform
//! carries both its source and its destination space.
//!
//! | Space         | Unit                  | Origin and axes                                           |
//! |---------------|-----------------------|-----------------------------------------------------------|
//! | [`PdfSpace`]  | PDF points            | page user space: y up, as written in the file             |
//! | [`PageSpace`] | PDF points            | displayed page: top-left of the crop box after rotation, y down |
//! | [`LayoutSpace`] | logical pixels      | whole document as laid out by the viewer at the current zoom, y down |
//! | [`ViewSpace`] | logical pixels        | top-left of the viewport (what the UI toolkit uses)       |
//! | [`DeviceSpace`] | physical pixels     | top-left of the viewport or bitmap                        |
//!
//! Annotation rectangles, quad points and link targets are in [`PdfSpace`]. Hit-testing, selection
//! and drawing use [`PageSpace`] and upward.

mod rotation;

pub use euclid;
pub use rotation::Rotation;

use euclid::{Box2D, Point2D, Rect, Size2D, Transform2D, Vector2D};
use serde::{Deserialize, Serialize};

/// PDF user space of one page (points, y up).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PdfSpace {}

/// The page as displayed: origin at the top-left of the visible area, y down, points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageSpace {}

/// The whole document as laid out by the viewer at the current zoom (logical pixels, y down).
/// Without the scroll offset, this is [`ViewSpace`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LayoutSpace {}

/// Logical pixels relative to the viewport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ViewSpace {}

/// Physical pixels relative to the viewport or a bitmap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceSpace {}

pub type PdfPoint = Point2D<f64, PdfSpace>;
pub type PdfBox = Box2D<f64, PdfSpace>;
pub type PdfVector = Vector2D<f64, PdfSpace>;
pub type PagePoint = Point2D<f64, PageSpace>;
pub type PageSize = Size2D<f64, PageSpace>;
pub type PageRect = Rect<f64, PageSpace>;
pub type PageVector = Vector2D<f64, PageSpace>;
pub type LayoutPoint = Point2D<f64, LayoutSpace>;
pub type LayoutRect = Rect<f64, LayoutSpace>;
pub type LayoutSize = Size2D<f64, LayoutSpace>;
pub type LayoutVector = Vector2D<f64, LayoutSpace>;
pub type ViewVector = Vector2D<f64, ViewSpace>;
pub type ViewPoint = Point2D<f64, ViewSpace>;
pub type ViewRect = Rect<f64, ViewSpace>;
pub type ViewSize = Size2D<f64, ViewSpace>;
pub type DevicePoint = Point2D<f64, DeviceSpace>;
pub type DeviceRect = Rect<f64, DeviceSpace>;

/// Integer pixel rectangle, such as a tile within a page bitmap.
pub type DeviceIntRect = Box2D<u32, DeviceSpace>;
pub type DeviceIntSize = Size2D<u32, DeviceSpace>;

pub type PdfToPage = Transform2D<f64, PdfSpace, PageSpace>;
pub type PageToPdf = Transform2D<f64, PageSpace, PdfSpace>;

/// A quadrilateral, such as a glyph or a highlighted run of text that may be rotated.
///
/// Points go around in order. For text they are bottom-left, bottom-right, top-right and top-left,
/// in the text's own direction.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Quad<U> {
    pub points: [Point2D<f64, U>; 4],
}

impl<U> Quad<U> {
    pub fn new(points: [Point2D<f64, U>; 4]) -> Self {
        Self { points }
    }

    /// The axis-aligned bounding box.
    pub fn bounds(&self) -> Box2D<f64, U> {
        Box2D::from_points(self.points)
    }

    /// Whether `p` lies inside (convex quads, either winding).
    pub fn contains(&self, p: Point2D<f64, U>) -> bool {
        let mut sign = 0.0;
        for i in 0..4 {
            let (a, b) = (self.points[i], self.points[(i + 1) % 4]);
            let cross = (b - a).cross(p - a);
            if cross.abs() < 1e-12 {
                continue;
            }
            if sign == 0.0 {
                sign = cross.signum();
            } else if cross.signum() != sign {
                return false;
            }
        }
        true
    }

    /// Maps the quad into another space.
    pub fn transform<V>(&self, t: &Transform2D<f64, U, V>) -> Quad<V> {
        Quad {
            points: self.points.map(|p| t.transform_point(p)),
        }
    }
}

pub type PdfQuad = Quad<PdfSpace>;
pub type PageQuad = Quad<PageSpace>;

/// Fallback size for degenerate pages: A4 (210 by 297 mm), matching hayro.
const A4: (f64, f64) = (210.0 / 25.4 * 72.0, 297.0 / 25.4 * 72.0);

/// The geometry of one page: which part of user space is visible and how it is rotated.
///
/// Mirrors how renderers interpret the page dictionary: the visible area is the crop box clipped to
/// the media box, and `/Rotate` turns it clockwise for display.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeometry {
    /// The crop box clipped to the media box, with its corners in order.
    visible: PdfBox,
    rotation: Rotation,
}

impl PageGeometry {
    /// Creates the geometry of a page from its boxes and its `/Rotate` value.
    ///
    /// `crop_box` defaults to the media box. Boxes may be given with any corner order.
    pub fn new(media_box: PdfBox, crop_box: Option<PdfBox>, rotation: Rotation) -> Self {
        let media = normalize(media_box);
        let visible = match crop_box {
            Some(c) => normalize(c).intersection_unchecked(&media),
            None => media,
        };
        Self { visible, rotation }
    }

    /// The same page with an additional clockwise rotation applied on top of `/Rotate`, as the
    /// user's rotate command does.
    pub fn rotated(self, extra: Rotation) -> Self {
        Self {
            rotation: self.rotation + extra,
            ..self
        }
    }

    pub fn rotation(&self) -> Rotation {
        self.rotation
    }

    /// The same page with only what is within `crop` visible.
    ///
    /// Where the two don't overlap, the page stays as it is.
    pub fn cropped(self, crop: Crop) -> Self {
        match self.visible.intersection(&crop.to_box()) {
            Some(visible) if visible.width() > 0.0 && visible.height() > 0.0 => {
                Self { visible, ..self }
            }
            _ => self,
        }
    }

    /// The visible area in PDF user space.
    pub fn visible_box(&self) -> PdfBox {
        self.visible
    }

    /// Unrotated width and height of the visible area.
    ///
    /// Pages without an area are treated as A4, and each side is at least 1 point.
    fn base_size(&self) -> (f64, f64) {
        let (w, h) = (self.visible.width(), self.visible.height());
        if !(w > f64::EPSILON && h > f64::EPSILON) {
            A4
        } else {
            (w.max(1.0), h.max(1.0))
        }
    }

    /// Size of the page as displayed, in points.
    pub fn size(&self) -> PageSize {
        let (w, h) = self.base_size();
        if self.rotation.swaps_axes() {
            PageSize::new(h, w)
        } else {
            PageSize::new(w, h)
        }
    }

    /// Size of the full-page bitmap at `scale` device pixels per point.
    ///
    /// Renderers and layout must agree on this rounding, so it lives here.
    pub fn pixel_size(&self, scale: f64) -> DeviceIntSize {
        let s = self.size();
        DeviceIntSize::new(pixel_extent(s.width, scale), pixel_extent(s.height, scale))
    }

    /// Maps PDF user space to displayed page space.
    pub fn pdf_to_page(&self) -> PdfToPage {
        let (w, h) = self.base_size();
        // Flip y and move the visible area's top-left to the origin: u = x - x0, v = base_h - (y -
        // y0)
        let flip = Transform2D::<f64, PdfSpace, PageSpace>::new(
            1.0,
            0.0,
            0.0,
            -1.0,
            -self.visible.min.x,
            h + self.visible.min.y,
        );
        let rotate = match self.rotation {
            Rotation::None => Transform2D::identity(),
            // (u, v) becomes (h - v, u)
            Rotation::Cw90 => Transform2D::new(0.0, 1.0, -1.0, 0.0, h, 0.0),
            // (u, v) becomes (w - u, h - v)
            Rotation::Cw180 => Transform2D::new(-1.0, 0.0, 0.0, -1.0, w, h),
            // (u, v) becomes (v, w - u)
            Rotation::Cw270 => Transform2D::new(0.0, -1.0, 1.0, 0.0, 0.0, w),
        };
        flip.then(&rotate)
    }

    /// Maps displayed page space to PDF user space.
    pub fn page_to_pdf(&self) -> PageToPdf {
        self.pdf_to_page()
            .inverse()
            .expect("page transform is a rotation plus flip, always invertible")
    }
}

/// A part of a page that the view is limited to, such as the page without its margins.
///
/// In PDF user space, in hundredths of a point, so that it can be part of a key: hashable, and the
/// same box wherever it is turned back into points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Crop([i32; 4]);

impl Crop {
    pub fn new(area: PdfBox) -> Self {
        let b = normalize(area);
        let q = |v: f64| (v * 100.0).round().clamp(i32::MIN as f64, i32::MAX as f64) as i32;
        Self([q(b.min.x), q(b.min.y), q(b.max.x), q(b.max.y)])
    }

    pub fn to_box(self) -> PdfBox {
        let [x0, y0, x1, y1] = self.0.map(|v| f64::from(v) / 100.0);
        PdfBox::new(PdfPoint::new(x0, y0), PdfPoint::new(x1, y1))
    }
}

/// Number of pixels covering `points` at `scale` pixels per point, at least 1.
pub fn pixel_extent(points: f64, scale: f64) -> u32 {
    (points * scale).round().max(1.0) as u32
}

fn normalize(b: PdfBox) -> PdfBox {
    PdfBox::new(
        PdfPoint::new(b.min.x.min(b.max.x), b.min.y.min(b.max.y)),
        PdfPoint::new(b.min.x.max(b.max.x), b.min.y.max(b.max.y)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pdf_box(x0: f64, y0: f64, x1: f64, y1: f64) -> PdfBox {
        PdfBox::new(PdfPoint::new(x0, y0), PdfPoint::new(x1, y1))
    }

    fn assert_near(a: PagePoint, b: (f64, f64)) {
        assert!(
            (a.x - b.0).abs() < 1e-9 && (a.y - b.1).abs() < 1e-9,
            "{a:?} != {b:?}"
        );
    }

    /// Letter page with a crop box offset from the origin.
    fn geometry(rotation: Rotation) -> PageGeometry {
        PageGeometry::new(
            pdf_box(0.0, 0.0, 612.0, 792.0),
            Some(pdf_box(100.0, 200.0, 400.0, 600.0)), // 300 by 400
            rotation,
        )
    }

    #[test]
    fn unrotated_maps_top_left_of_crop_box_to_origin() {
        let g = geometry(Rotation::None);
        assert_eq!(g.size(), PageSize::new(300.0, 400.0));
        let t = g.pdf_to_page();
        assert_near(t.transform_point(PdfPoint::new(100.0, 600.0)), (0.0, 0.0));
        assert_near(
            t.transform_point(PdfPoint::new(400.0, 200.0)),
            (300.0, 400.0),
        );
    }

    #[test]
    fn rotations_move_the_top_left_corner_clockwise() {
        // The crop box's top-left corner in PDF space.
        let tl = PdfPoint::new(100.0, 600.0);
        let cases = [
            (Rotation::None, (300.0, 400.0), (0.0, 0.0)),
            (Rotation::Cw90, (400.0, 300.0), (400.0, 0.0)),
            (Rotation::Cw180, (300.0, 400.0), (300.0, 400.0)),
            (Rotation::Cw270, (400.0, 300.0), (0.0, 300.0)),
        ];
        for (rot, size, tl_page) in cases {
            let g = geometry(rot);
            assert_eq!(g.size(), PageSize::new(size.0, size.1), "{rot:?}");
            assert_near(g.pdf_to_page().transform_point(tl), tl_page);
        }
    }

    #[test]
    fn page_to_pdf_round_trips() {
        for rot in [
            Rotation::None,
            Rotation::Cw90,
            Rotation::Cw180,
            Rotation::Cw270,
        ] {
            let g = geometry(rot);
            let p = PdfPoint::new(123.0, 456.0);
            let back = g
                .page_to_pdf()
                .transform_point(g.pdf_to_page().transform_point(p));
            assert!((back - p).length() < 1e-9, "{rot:?}");
        }
    }

    #[test]
    fn extra_rotation_composes() {
        let g = geometry(Rotation::Cw270).rotated(Rotation::Cw180);
        assert_eq!(g.rotation(), Rotation::Cw90);
    }

    #[test]
    fn crop_box_is_clipped_to_media_box_and_normalized() {
        let g = PageGeometry::new(
            pdf_box(0.0, 0.0, 200.0, 100.0),
            Some(pdf_box(250.0, 150.0, -50.0, 50.0)), // reversed corners, sticks out
            Rotation::None,
        );
        assert_eq!(g.visible_box(), pdf_box(0.0, 50.0, 200.0, 100.0));
        assert_eq!(g.size(), PageSize::new(200.0, 50.0));
    }

    #[test]
    fn quads_contain_points_and_transform() {
        // A square rotated by 45 degrees, centered on (10, 10).
        let q = PdfQuad::new([
            PdfPoint::new(10.0, 5.0),
            PdfPoint::new(15.0, 10.0),
            PdfPoint::new(10.0, 15.0),
            PdfPoint::new(5.0, 10.0),
        ]);
        assert!(q.contains(PdfPoint::new(10.0, 10.0)));
        assert!(q.contains(PdfPoint::new(12.0, 10.0)));
        assert!(!q.contains(PdfPoint::new(6.0, 6.0))); // inside the bounds, outside the quad
        assert_eq!(q.bounds(), pdf_box(5.0, 5.0, 15.0, 15.0));
        let g = geometry(Rotation::None);
        let page = q.transform(&g.pdf_to_page());
        assert!(page.contains(g.pdf_to_page().transform_point(PdfPoint::new(10.0, 10.0))));
    }

    #[test]
    fn degenerate_pages_fall_back_to_a4() {
        let g = PageGeometry::new(pdf_box(0.0, 0.0, 0.0, 0.0), None, Rotation::None);
        assert!((g.size().width - 595.28).abs() < 0.01);
        assert!((g.size().height - 841.89).abs() < 0.01);
    }

    #[test]
    fn pixel_size_rounds_and_is_never_zero() {
        let g = geometry(Rotation::None);
        assert_eq!(g.pixel_size(1.5), DeviceIntSize::new(450, 600));
        assert_eq!(g.pixel_size(0.0001), DeviceIntSize::new(1, 1));
    }
}
