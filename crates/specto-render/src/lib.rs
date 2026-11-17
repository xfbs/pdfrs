//! Page rasterization for Specto.
//!
//! Everything engine-specific stays behind [`Document`] so that the rest of the code base never
//! sees hayro types. See `docs/ARCHITECTURE.md` ("Rendering pipeline").

mod bounds;
mod hayro_backend;
pub mod scheduler;
pub mod tile;

pub use bounds::ContentBounds;
pub use hayro_backend::HayroDocument;
#[doc(hidden)]
pub use hayro_backend::check_geometry_matches_hayro;
pub use specto_geom::{Crop, DeviceIntRect, DeviceIntSize, PageGeometry, Rotation};

/// Which parts of a page to draw.
///
/// The viewer draws a file's annotations with the page. Annotations made or changed in the session
/// are drawn by the apps as an overlay (see `docs/ARCHITECTURE.md`, "Annotations").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub enum Content {
    /// Page content stream plus the annotations stored in the file.
    #[default]
    PageAndAnnotations,
    /// Only the page content stream.
    PageOnly,
}

/// Parameters for rendering one page (or one tile of a page).
#[derive(Clone, Copy, Debug)]
pub struct RenderRequest {
    /// Zero-based page index.
    pub page: usize,
    /// Device pixels per PDF point (the zoom times the HiDPI scale factor).
    pub scale: f64,
    /// Clockwise rotation on top of the page's own `/Rotate`.
    pub rotation: Rotation,
    /// The part of the page to show, if not all of it.
    pub crop: Option<Crop>,
    pub content: Content,
    /// Background as straight (non-premultiplied) RGBA, or `None` for transparent.
    pub background: Option<[u8; 4]>,
    /// Only render this region of the full page bitmap (see [`PageGeometry::pixel_size`]), or
    /// `None` for the whole page.
    pub region: Option<DeviceIntRect>,
}

impl RenderRequest {
    /// Whole page, white background, with annotations.
    pub fn page(page: usize, scale: f64) -> Self {
        Self {
            page,
            scale,
            rotation: Rotation::None,
            crop: None,
            content: Content::default(),
            background: Some([255, 255, 255, 255]),
            region: None,
        }
    }
}

/// A rendered bitmap: premultiplied RGBA8, rows top to bottom, no padding.
#[derive(Clone, Debug)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub use specto_source::{OpenError, PdfData};

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    #[error("page index {0} out of range")]
    PageOutOfRange(usize),
    #[error("requested bitmap of {0}×{1} px exceeds the size limit")]
    TooLarge(u32, u32),
    #[error("region lies outside the page bitmap")]
    RegionOutOfBounds,
    #[error("the renderer panicked")]
    Panicked,
}

/// An opened, read-only PDF that can be rasterized.
///
/// Implementations may keep caches internally and need not be `Send`. The [`scheduler`] opens one
/// instance per worker thread.
pub trait Document {
    fn page_count(&self) -> usize;
    /// Geometry of the page as stored in the file (its own `/Rotate` included).
    fn page_geometry(&self, page: usize) -> Option<PageGeometry>;
    fn render(&self, request: &RenderRequest) -> Result<Bitmap, RenderError>;
}
