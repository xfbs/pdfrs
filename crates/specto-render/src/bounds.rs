//! The box of what a page draws, for trimming its margins in the view.

use hayro::hayro_interpret::font::Glyph;
use hayro::hayro_interpret::font::GlyphRun;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, Paint, RenderContent, SoftMask, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{Affine, BezPath, Rect, Shape};
use specto_geom::{PdfBox, PdfPoint};

/// Glyph space is 1000 units per em (hayro's convention).
const EM: f64 = 1000.0;

/// Finds the box of what pages draw, one page after another.
///
/// Reads the content without rasterizing it, so a page takes about as long as extracting its text.
/// Keeps hayro's cache of fonts across pages.
pub struct ContentBounds<'a> {
    pdf: &'a Pdf,
    cache: InterpreterCache<'a>,
    settings: InterpreterSettings,
}

impl<'a> ContentBounds<'a> {
    pub fn new(pdf: &'a Pdf) -> Self {
        Self {
            pdf,
            cache: InterpreterCache::new(),
            settings: InterpreterSettings {
                render_content: RenderContent::all(),
                ..InterpreterSettings::default()
            },
        }
    }

    /// The box in PDF user space of what page `index` draws, within its crop box.
    ///
    /// `None` for a page that draws nothing, or no such page. What is drawn in white counts as
    /// nothing, since a white rectangle behind the page is common. So does what is clipped away,
    /// as far as the clip's bounds tell. A scan is all one image, so its box is the whole page.
    pub fn page(&mut self, index: usize) -> Option<PdfBox> {
        let page = self.pdf.pages().get(index)?;
        let crop = page.crop_box();
        let crop = Rect::new(crop.x0, crop.y0, crop.x1, crop.y1);
        let mut ctx = Context::new(
            // Identity: what is drawn comes out in PDF user space.
            Affine::IDENTITY,
            crop,
            &self.cache,
            page.xref(),
            self.settings.clone(),
        );
        let mut device = BoundsDevice {
            clips: vec![crop],
            drawn: None,
        };
        interpret_page(page, &mut ctx, &mut device);
        let drawn = device.drawn?.intersect(crop);
        (drawn.width() > 0.0 && drawn.height() > 0.0).then(|| {
            PdfBox::new(
                PdfPoint::new(drawn.x0, drawn.y0),
                PdfPoint::new(drawn.x1, drawn.y1),
            )
        })
    }
}

/// Collects the bounds of everything drawn, clipped by the bounds of the clips around it.
struct BoundsDevice {
    /// The bounds of the clips in effect, each within the one before. The first is the crop box.
    clips: Vec<Rect>,
    drawn: Option<Rect>,
}

impl BoundsDevice {
    /// Adds `bounds`, in PDF user space, to what is drawn.
    fn add(&mut self, bounds: Rect) {
        let clip = *self.clips.last().expect("the crop box is never popped");
        let visible = bounds.intersect(clip);
        if !(visible.width() >= 0.0 && visible.height() >= 0.0) || !visible.is_finite() {
            return;
        }
        // A point shows nothing. A straight line without width still counts.
        if visible.width() == 0.0 && visible.height() == 0.0 {
            return;
        }
        self.drawn = Some(self.drawn.map_or(visible, |d| d.union(visible)));
    }
}

/// Whether `paint` is white, which doesn't show on paper.
fn is_white(paint: &Paint<'_>) -> bool {
    match paint {
        Paint::Color(c) => {
            let [r, g, b, _] = c.to_rgba().to_rgba8();
            r >= 250 && g >= 250 && b >= 250
        }
        Paint::Pattern(_) => false,
    }
}

impl<'a> Device<'a> for BoundsDevice {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, mode: &DrawMode) {
        if is_white(&props.paint) {
            return;
        }
        let width = match mode {
            DrawMode::Fill(_) => 0.0,
            DrawMode::Stroke(s) | DrawMode::FillAndStroke(_, s) => f64::from(s.line_width),
            DrawMode::Invisible => return,
        };
        let bounds = (props.transform * path.clone()).bounding_box();
        // Half the line width on each side, in user space as far as the transform scales it.
        let half = width / 2.0 * props.transform.determinant().abs().sqrt();
        self.add(bounds.inflate(half, half));
    }

    fn push_clip_path(&mut self, clip: &ClipPath) {
        let last = *self.clips.last().expect("the crop box is never popped");
        self.clips.push(clip.path.bounding_box().intersect(last));
    }

    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        if matches!(mode, DrawMode::Invisible) || is_white(&props.paint) {
            return;
        }
        for g in run.glyphs() {
            let t = props.transform * g.transform();
            let glyph_box = match &**g {
                Glyph::Outline(o) => o.outline().bounding_box(),
                // The em square above the baseline.
                Glyph::Type3(_) => Rect::new(0.0, 0.0, EM * 0.5, EM),
            };
            if glyph_box.width() > 0.0 || glyph_box.height() > 0.0 {
                self.add(t.transform_rect_bbox(glyph_box));
            }
        }
    }

    fn draw_image(&mut self, _: Image<'a, '_>, props: ImageDrawProps<'a>) {
        // An image fills the unit square of its space.
        self.add(
            props
                .transform
                .transform_rect_bbox(Rect::new(0.0, 0.0, 1.0, 1.0)),
        );
    }

    fn pop_clip(&mut self) {
        if self.clips.len() > 1 {
            self.clips.pop();
        }
    }

    fn pop_transparency_group(&mut self) {}
}
