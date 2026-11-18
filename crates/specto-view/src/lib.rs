//! The UI-agnostic viewer core shared by the desktop and Android apps.
//!
//! [`Viewer`] owns layout (single page, continuous, two-page), zoom (fixed, fit width, fit page),
//! scrolling and hit-testing, and plans each [`Frame`]: which cached tiles to draw where (in device
//! pixels, snapped so tiles at the exact device scale are drawn one to one) and which tiles to
//! request from the `specto_render::scheduler`, in priority order.
//!
//! The frontend's job is small: forward input, keep a tile cache, and draw rectangles. It never
//! does layout math.

mod layout;
mod search;
mod selection;
mod viewer;

pub use layout::LayoutMode;
pub use search::{SearchMatch, SearchResults};
pub use selection::{
    Granularity, Selection, TextPages, TextPos, over_text, text_char, text_position, text_quads,
};
pub use viewer::{
    Frame, PageDraw, Settings, TileDraw, ViewPage, ViewState, Viewer, ZoomMode, placeholder_level,
};

#[cfg(test)]
mod tests;
