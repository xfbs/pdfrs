use hayro::hayro_interpret::{InterpreterSettings, RenderContent};
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::object::Rect as HayroRect;
use hayro::hayro_syntax::object::dict::keys::{HEIGHT, RESOURCES, SUBTYPE, WIDTH, XOBJECT};
use hayro::hayro_syntax::object::{Dict, Name, Stream};
use hayro::hayro_syntax::page::{Page, Rotation as HayroRotation};
use hayro::kurbo::Affine;
use hayro::vello_cpu::color::AlphaColor;
use hayro::vello_cpu::color::palette::css::TRANSPARENT;
use hayro::vello_cpu::{Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{RenderCache, RenderSettings, render_into};
use specto_geom::{DeviceIntRect, PageGeometry, PdfBox, PdfPoint, Rotation};
use specto_source::PdfData;

use crate::{Bitmap, Content, Document, OpenError, RenderError, RenderRequest};

/// vello_cpu addresses pixels with `u16`.
const MAX_DIMENSION: u32 = u16::MAX as u32;

self_cell::self_cell!(
    /// A parsed PDF together with hayro's render cache, which borrows it.
    struct PdfWithCache {
        owner: Pdf,
        #[not_covariant]
        dependent: RenderCache,
    }
);

/// A PDF opened with the pure-Rust hayro renderer.
///
/// Keeps hayro's cache of fonts, glyphs and images across renders, which makes tiles of the same
/// page 1.4 to 6 times faster. The cache only grows, so long-lived users should re-open the
/// document now and then. Not `Send`: the cache uses `Rc`.
pub struct HayroDocument {
    inner: PdfWithCache,
    settings: InterpreterSettings,
    geometries: Vec<PageGeometry>,
}

impl HayroDocument {
    pub fn open(data: impl Into<PdfData>, password: Option<&str>) -> Result<Self, OpenError> {
        let pdf = specto_source::open_pdf(data.into(), password)?;
        let geometries = pdf.pages().iter().map(geometry_of).collect();
        Ok(Self {
            inner: PdfWithCache::new(pdf, |_| RenderCache::new()),
            // Defaults use hayro's embedded standard fonts and CMaps. TODO: resolve fallback fonts
            // via fontconfig for better CJK coverage.
            settings: InterpreterSettings::default(),
            geometries,
        })
    }

    pub fn pdf(&self) -> &Pdf {
        self.inner.borrow_owner()
    }

    /// Total pixels of the images page `page` draws through XObjects, looking into form XObjects a
    /// few levels deep (inline images are ignored).
    ///
    /// Reads dictionaries only, so it's cheap. hayro decodes images whole, and at high zoom a big
    /// scan costs gigabytes per render (4.5 GB for one tile of a 4.4 by 5.9 m JPEG 2000 page), so
    /// the scheduler treats pages above a threshold specially.
    pub fn page_image_pixels(&self, page: usize) -> u64 {
        let Some(p) = self.pdf().pages().get(page) else {
            return 0;
        };
        let mut budget = 10_000; // XObjects visited, against pathological trees
        image_pixels(&p.resources().x_objects, 0, &mut budget)
    }
}

fn image_pixels(xobjects: &Dict<'_>, depth: u32, budget: &mut u32) -> u64 {
    if depth > 4 {
        return 0;
    }
    let mut total = 0u64;
    for name in xobjects.keys() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let Some(stream) = xobjects.get::<Stream<'_>>(&name) else {
            continue;
        };
        let dict = stream.dict();
        match dict.get::<Name<'_>>(SUBTYPE).as_deref() {
            Some(b"Image") => {
                let w = dict.get::<u32>(WIDTH).unwrap_or(0) as u64;
                let h = dict.get::<u32>(HEIGHT).unwrap_or(0) as u64;
                total = total.saturating_add(w.saturating_mul(h));
            }
            Some(b"Form") => {
                if let Some(inner) = dict
                    .get::<Dict<'_>>(RESOURCES)
                    .and_then(|r| r.get::<Dict<'_>>(XOBJECT))
                {
                    total = total.saturating_add(image_pixels(&inner, depth + 1, budget));
                }
            }
            _ => {}
        }
    }
    total
}

fn geometry_of(page: &Page<'_>) -> PageGeometry {
    let to_box = |r: HayroRect| PdfBox::new(PdfPoint::new(r.x0, r.y0), PdfPoint::new(r.x1, r.y1));
    let rotation = match page.rotation() {
        HayroRotation::None => Rotation::None,
        HayroRotation::Horizontal => Rotation::Cw90,
        HayroRotation::Flipped => Rotation::Cw180,
        HayroRotation::FlippedHorizontal => Rotation::Cw270,
    };
    PageGeometry::new(
        to_box(page.media_box()),
        Some(to_box(page.crop_box())),
        rotation,
    )
}

impl Document for HayroDocument {
    fn page_count(&self) -> usize {
        self.geometries.len()
    }

    fn page_geometry(&self, page: usize) -> Option<PageGeometry> {
        self.geometries.get(page).copied()
    }

    fn render(&self, req: &RenderRequest) -> Result<Bitmap, RenderError> {
        let mut geometry = self
            .page_geometry(req.page)
            .ok_or(RenderError::PageOutOfRange(req.page))?;
        if let Some(crop) = req.crop {
            geometry = geometry.cropped(crop);
        }
        let geometry = geometry.rotated(req.rotation);
        let full = geometry.pixel_size(req.scale);
        let region = req.region.unwrap_or_else(|| DeviceIntRect::from_size(full));
        if region.is_empty() || region.max.x > full.width || region.max.y > full.height {
            return Err(RenderError::RegionOutOfBounds);
        }
        let (w, h) = (region.width(), region.height());
        if w > MAX_DIMENSION || h > MAX_DIMENSION {
            return Err(RenderError::TooLarge(w, h));
        }

        // Scale non-uniformly by a hair so the page exactly fills the rounded pixel dimensions.
        // Tiles then line up seamlessly with full renders.
        let size = geometry.size();
        let sx = full.width as f64 / size.width;
        let sy = full.height as f64 / size.height;
        let page_t = geometry.pdf_to_page();
        let transform = Affine::translate((-(region.min.x as f64), -(region.min.y as f64)))
            * Affine::scale_non_uniform(sx, sy)
            * Affine::new([
                page_t.m11, page_t.m12, page_t.m21, page_t.m22, page_t.m31, page_t.m32,
            ]);

        let mut settings = self.settings.clone();
        settings.render_content = match req.content {
            Content::PageAndAnnotations => RenderContent::all(),
            Content::PageOnly => RenderContent::PAGE,
        };
        let background = req
            .background
            .map(|[r, g, b, a]| AlphaColor::from_rgba8(r, g, b, a))
            .unwrap_or(TRANSPARENT);

        let data = self.inner.with_dependent(|pdf, cache| {
            let page = &pdf.pages()[req.page];
            let mut ctx = RenderContext::new(w as u16, h as u16);
            render_into(
                page,
                cache,
                &settings,
                &RenderSettings::default(),
                &mut ctx,
                transform,
            );
            ctx.flush();
            let mut pixmap = Pixmap::new(w as u16, h as u16);
            ctx.render_with(
                &mut pixmap,
                &mut Resources::default(),
                RasterizerSettings {
                    target_init: TargetInit::Clear(background),
                    ..Default::default()
                },
            );
            pixmap.data_as_u8_slice().to_vec()
        });
        Ok(Bitmap {
            width: w,
            height: h,
            data,
        })
    }
}

/// Checks that our page transform agrees with hayro's own for every page.
///
/// Otherwise annotation positions, hit-testing and rendering would drift apart.
pub fn check_geometry_matches_hayro(doc: &HayroDocument) -> Result<(), String> {
    use hayro::hayro_interpret::util::TransformExt;
    for (i, page) in doc.pdf().pages().iter().enumerate() {
        let t = doc.geometries[i].pdf_to_page();
        let ours = [t.m11, t.m12, t.m21, t.m22, t.m31, t.m32];
        let theirs = page.initial_transform(true).to_kurbo().as_coeffs();
        let (rw, rh) = page.render_dimensions();
        let size = doc.geometries[i].size();
        let size_ok =
            (size.width - rw as f64).abs() < 1e-3 && (size.height - rh as f64).abs() < 1e-3;
        // hayro computes in f32 in places, so compare relative to page size.
        let tol = 1e-4 * (1.0 + size.width.max(size.height));
        let tf_ok = ours.iter().zip(theirs).all(|(a, b)| (a - b).abs() < tol);
        if !(size_ok && tf_ok) {
            return Err(format!(
                "page {i}: ours {ours:?} {size:?}, hayro {theirs:?} ({rw}×{rh})"
            ));
        }
    }
    Ok(())
}
