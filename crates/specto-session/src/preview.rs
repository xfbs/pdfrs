//! A look at where a link leads, without going there.

use specto_doc::Rotation;
use specto_geom::{DeviceIntRect, PageGeometry, PagePoint, PdfPoint, ViewPoint, ViewRect};
use specto_render::{Bitmap, Document as _, HayroDocument, RenderRequest};
use specto_source::{PdfData, Target};

use crate::Session;

/// How much of the page above the target is shown, in points: the line a destination names often
/// starts a little above it.
const LEAD: f64 = 14.0;

/// What showing the target of a link takes: the link's place in the view, and the page and the
/// place on it that the link leads to.
///
/// It holds the file, not the session, so it can be rendered on another thread.
#[derive(Clone, Debug)]
pub struct LinkPreview {
    /// The link's area in the view.
    pub link: ViewRect,
    data: PdfData,
    password: Option<String>,
    /// The page of the file, how the reader turned it, and its geometry as the file has it.
    page: u32,
    rotation: Rotation,
    geometry: PageGeometry,
    /// The target on the page as shown, in points from its top left corner.
    at: PagePoint,
}

impl LinkPreview {
    /// Whether `other` shows the same place: for keeping what was rendered.
    pub fn same_target(&self, other: &LinkPreview) -> bool {
        self.data.same(&other.data)
            && (self.page, self.rotation, self.at) == (other.page, other.rotation, other.at)
    }

    /// The page from the target down, across its whole width, `width` device pixels wide and at
    /// most `height` high. The file's own annotations are drawn, the session's edits are not.
    ///
    /// Opens the file anew, so it is for a thread that may take a moment. `None` if the page can't
    /// be drawn.
    pub fn render(&self, width: u32, height: u32) -> Option<Bitmap> {
        let geometry = self.geometry.rotated(self.rotation);
        let scale = f64::from(width) / geometry.size().width;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let full = geometry.pixel_size(scale);
        let height = height.min(full.height);
        let top = (((self.at.y - LEAD) * scale).max(0.0) as u32).min(full.height - height);
        let doc = HayroDocument::open(self.data.clone(), self.password.as_deref()).ok()?;
        let request = RenderRequest {
            rotation: self.rotation,
            region: Some(DeviceIntRect::new(
                (0, top).into(),
                (full.width, top + height).into(),
            )),
            ..RenderRequest::page(self.page as usize, scale)
        };
        doc.render(&request).ok()
    }
}

impl Session {
    /// What a preview of the link under `p` (viewport coordinates) takes, if a link is there that
    /// leads to a place on a page of the document.
    ///
    /// Links that only name a page have none: the top of a page says little about what is meant.
    pub fn link_preview(&mut self, p: ViewPoint) -> Option<LinkPreview> {
        let (position, link) = self.link_under(p)?;
        let Target::Destination(destination) = link.target else {
            return None;
        };
        let top = destination.top?;
        let from = *self.edit.pages().get(position)?;
        // The page may have been moved, turned or deleted since.
        let target = self.position_of(from.source, destination.page)?;
        let rotation = self.edit.pages()[target].rotation;
        let source = self.sources.get(from.source.0 as usize)?;
        let geometry = *source.geometries.get(destination.page)?;
        let shown = geometry.rotated(rotation);
        let left = destination.left.unwrap_or(shown.visible_box().min.x);
        let at = (shown.pdf_to_page()).transform_point(PdfPoint::new(left, top));
        let corners =
            [link.rect.min, link.rect.max].map(|corner| self.viewer.pdf_to_view(position, corner));
        let [Some(a), Some(b)] = corners else {
            return None;
        };
        Some(LinkPreview {
            link: ViewRect::from_points([a, b]),
            data: source.data.clone(),
            password: source.password.clone(),
            page: destination.page as u32,
            rotation,
            geometry,
            at,
        })
    }
}
