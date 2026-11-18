//! A signature from a photo: ink on paper becomes ink on nothing.
//!
//! A phone's photo of a sheet of paper is never evenly lit: one side is darker, there is a shadow
//! of the hand. So "lighter than this is paper" doesn't work for the whole picture. The paper's
//! brightness is estimated around each spot instead (the lightest of what is near it), and a pixel
//! is ink by how much darker it is than the paper there.

use std::sync::Arc;

use crate::{Color, StampImage};

/// How to make a signature of a photo.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InkSettings {
    /// How much darker than the paper around it a pixel must be to count as ink, from 0 to 1.
    ///
    /// Lower keeps faint strokes and more of the paper's grain, higher keeps only what is dark.
    pub threshold: f32,
    /// The color the ink gets. `None` keeps the photo's own.
    pub color: Option<Color>,
}

impl Default for InkSettings {
    fn default() -> Self {
        Self {
            threshold: 0.3,
            color: None,
        }
    }
}

/// Signatures are made no wider or higher than this many pixels: plenty for print, and a small
/// file.
const MAX_SIDE: u32 = 1200;

/// The picture is divided into tiles of about this many pixels a side to estimate the paper's
/// brightness.
const TILE: u32 = 48;

/// The ink of `photo` (straight RGBA, 8 bits each, row by row) on a transparent background, cut
/// down to where there is ink.
///
/// `None` if there is none, or the picture's size and data don't go together.
pub fn ink_from_photo(photo: &StampImage, settings: InkSettings) -> Option<StampImage> {
    let photo = scaled_down(photo)?;
    let (w, h) = (photo.width as usize, photo.height as usize);
    let light: Vec<f32> = photo
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            // On white, where the photo is itself transparent.
            let a = f32::from(p[3]) / 255.0;
            let on_white = |c: u8| (f32::from(c) / 255.0) * a + (1.0 - a);
            0.299 * on_white(p[0]) + 0.587 * on_white(p[1]) + 0.114 * on_white(p[2])
        })
        .collect();
    let paper = paper_brightness(&light, w, h);

    // From no ink to full ink over this much of darkness: soft edges.
    let soft = 0.12;
    let threshold = settings.threshold.clamp(0.02, 0.95);
    let mut rgba = vec![0u8; w * h * 4];
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let darker = 1.0 - (light[i] / paper.at(x, y)).min(1.0);
            let alpha = ((darker - threshold + soft / 2.0) / soft).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }
            let [r, g, b] = match settings.color {
                Some(c) => [c.r, c.g, c.b].map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
                None => [
                    photo.rgba[i * 4],
                    photo.rgba[i * 4 + 1],
                    photo.rgba[i * 4 + 2],
                ],
            };
            rgba[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, (alpha * 255.0).round() as u8]);
            // Specks don't decide where the signature ends.
            if alpha > 0.5 {
                (x0, y0) = (x0.min(x), y0.min(y));
                (x1, y1) = (x1.max(x), y1.max(y));
            }
        }
    }
    if x1 < x0 || y1 < y0 {
        return None;
    }
    // A little room around the ink.
    let margin = (w.max(h) / 100).max(2);
    let (x0, y0) = (x0.saturating_sub(margin), y0.saturating_sub(margin));
    let (x1, y1) = ((x1 + margin).min(w - 1), (y1 + margin).min(h - 1));
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut cut = Vec::with_capacity(cw * ch * 4);
    for y in y0..=y1 {
        cut.extend_from_slice(&rgba[(y * w + x0) * 4..(y * w + x1 + 1) * 4]);
    }
    Some(StampImage {
        width: cw as u32,
        height: ch as u32,
        rgba: cut,
    })
}

/// `photo`, made smaller if it is larger than a signature needs (by averaging blocks of pixels,
/// which also takes out a photo's noise).
fn scaled_down(photo: &StampImage) -> Option<Arc<StampImage>> {
    let (w, h) = (photo.width, photo.height);
    if w == 0 || h == 0 || photo.rgba.len() != w as usize * h as usize * 4 {
        return None;
    }
    let by = w.max(h).div_ceil(MAX_SIDE).max(1);
    if by == 1 {
        return Some(Arc::new(photo.clone()));
    }
    let (nw, nh) = ((w / by).max(1), (h / by).max(1));
    let mut rgba = Vec::with_capacity((nw * nh * 4) as usize);
    for y in 0..nh {
        for x in 0..nw {
            let mut sum = [0u32; 4];
            for dy in 0..by {
                for dx in 0..by {
                    let at = (((y * by + dy) * w + x * by + dx) * 4) as usize;
                    for (s, v) in sum.iter_mut().zip(&photo.rgba[at..at + 4]) {
                        *s += u32::from(*v);
                    }
                }
            }
            rgba.extend(sum.map(|s| (s / (by * by)) as u8));
        }
    }
    Some(Arc::new(StampImage {
        width: nw,
        height: nh,
        rgba,
    }))
}

/// How bright the paper is around each pixel.
struct Paper {
    /// The brightness of the paper in each tile.
    tiles: Vec<f32>,
    across: usize,
    down: usize,
    tile: usize,
}

impl Paper {
    /// At a pixel: between the tiles around it.
    fn at(&self, x: usize, y: usize) -> f32 {
        // In units of tiles, from the center of the first.
        let place = |v: usize, count: usize| {
            let t = (v as f32 + 0.5) / self.tile as f32 - 0.5;
            let t = t.clamp(0.0, (count - 1) as f32);
            (t.floor() as usize, t.fract())
        };
        let (tx, fx) = place(x, self.across);
        let (ty, fy) = place(y, self.down);
        let get = |x: usize, y: usize| {
            self.tiles[y.min(self.down - 1) * self.across + x.min(self.across - 1)]
        };
        let top = get(tx, ty) * (1.0 - fx) + get(tx + 1, ty) * fx;
        let bottom = get(tx, ty + 1) * (1.0 - fx) + get(tx + 1, ty + 1) * fx;
        (top * (1.0 - fy) + bottom * fy).max(0.05)
    }
}

/// The paper's brightness in tiles: in each, the brightness nine tenths of its pixels are darker
/// than (ink is the lesser part of any tile, and the very brightest pixels are glare). A tile that
/// is mostly ink takes after its neighbors.
fn paper_brightness(light: &[f32], w: usize, h: usize) -> Paper {
    let tile = TILE as usize;
    let (across, down) = (w.div_ceil(tile).max(1), h.div_ceil(tile).max(1));
    let mut tiles = Vec::with_capacity(across * down);
    for ty in 0..down {
        for tx in 0..across {
            let mut values: Vec<f32> = (ty * tile..((ty + 1) * tile).min(h))
                .flat_map(|y| (tx * tile..((tx + 1) * tile).min(w)).map(move |x| light[y * w + x]))
                .collect();
            let at = (values.len() * 9 / 10).min(values.len() - 1);
            let (_, value, _) = values.select_nth_unstable_by(at, f32::total_cmp);
            tiles.push(*value);
        }
    }
    // The lightest of each tile and its neighbors: a tile under a thick stroke would otherwise call
    // the ink paper.
    let lightest: Vec<f32> = (0..down)
        .flat_map(|ty| (0..across).map(move |tx| (tx, ty)))
        .map(|(tx, ty)| {
            let mut best = 0.0f32;
            for y in ty.saturating_sub(1)..=(ty + 1).min(down - 1) {
                for x in tx.saturating_sub(1)..=(tx + 1).min(across - 1) {
                    best = best.max(tiles[y * across + x]);
                }
            }
            best
        })
        .collect();
    Paper {
        tiles: lightest,
        across,
        down,
        tile,
    }
}

#[cfg(test)]
mod tests {
    use super::{InkSettings, ink_from_photo};
    use crate::{Color, StampImage};

    /// A sheet of 400 by 300 lit from the left (bright there, half as bright on the right) with a
    /// dark bar across the middle and, on the bright side, a faint one.
    fn photo() -> StampImage {
        let (w, h) = (400u32, 300u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let paper = 240.0 - 120.0 * (x as f32 / w as f32);
                let ink = if (140..160).contains(&y) && (50..350).contains(&x) {
                    0.15
                } else if (200..210).contains(&y) && (50..150).contains(&x) {
                    0.8
                } else {
                    1.0
                };
                let v = (paper * ink) as u8;
                rgba.extend([v, v, (v as f32 * 0.9) as u8, 255]);
            }
        }
        StampImage {
            width: w,
            height: h,
            rgba,
        }
    }

    #[test]
    fn ink_stays_and_paper_goes_under_uneven_light() {
        let ink = ink_from_photo(&photo(), InkSettings::default()).expect("there is ink");
        // Cut down to the dark bar, with a little room.
        assert!((300..=312).contains(&ink.width), "{}", ink.width);
        assert!((20..=32).contains(&ink.height), "{}", ink.height);
        let alpha = |x: u32, y: u32| ink.rgba[((y * ink.width + x) * 4 + 3) as usize];
        // Ink through and through, on the bright side and the dark one, though the paper on the
        // dark side is darker than the faint bar.
        assert_eq!(alpha(10, ink.height / 2), 255);
        assert_eq!(alpha(ink.width - 10, ink.height / 2), 255);
        // Nothing in the corners, which are paper.
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(ink.width - 1, ink.height - 1), 0);
    }

    #[test]
    fn the_threshold_decides_about_faint_strokes() {
        let keep = InkSettings {
            threshold: 0.1,
            ..Default::default()
        };
        let faint = ink_from_photo(&photo(), keep).unwrap();
        // The faint bar is in now, so the signature reaches further down.
        assert!(faint.height > 60, "{}", faint.height);
        // Nothing is dark enough for this.
        let only_black = InkSettings {
            threshold: 0.95,
            ..Default::default()
        };
        assert!(ink_from_photo(&photo(), only_black).is_none());
    }

    #[test]
    fn ink_in_a_color_and_photos_too_large() {
        let blue = InkSettings {
            color: Some(Color::new(0.0, 0.0, 1.0)),
            ..Default::default()
        };
        let ink = ink_from_photo(&photo(), blue).unwrap();
        let middle = ((ink.height / 2 * ink.width + ink.width / 2) * 4) as usize;
        assert_eq!(ink.rgba[middle..middle + 4], [0, 0, 255, 255]);
        // Three times the size: brought down to a signature's.
        let big = photo();
        let mut rgba = Vec::new();
        for y in 0..big.height * 4 {
            for x in 0..big.width * 4 {
                let at = (((y / 4) * big.width + x / 4) * 4) as usize;
                rgba.extend_from_slice(&big.rgba[at..at + 4]);
            }
        }
        let big = StampImage {
            width: big.width * 4,
            height: big.height * 4,
            rgba,
        };
        let ink = ink_from_photo(&big, InkSettings::default()).unwrap();
        assert!(ink.width <= 1200 && ink.width > 500, "{}", ink.width);
        // A picture whose data is short is no picture.
        let broken = StampImage {
            width: 10,
            height: 10,
            rgba: vec![0; 5],
        };
        assert!(ink_from_photo(&broken, InkSettings::default()).is_none());
    }
}
