//! Software compositor for the front end: draws a stack of screens into one 640x480 RGBA image,
//! blending in screen (gamma) space the way the retail front end does.
//!
//! The GPU UI path blends in linear light, which makes every translucent piece (shadows, gloss,
//! disabled buttons, dimmed icons) lighter than retail: a half-opacity icon over black came out at
//! `#BB6A6B` where retail shows about `#7F4C5E`. Composing here on the stored 8-bit values gives
//! the retail result (`src * a + dst * (1 - a)` per channel) and pixel positions at the layout's
//! own resolution. See `docs/provenance/fe.md`.

use crate::menu::{rasterise_gradient, rasterise_text, Draw, Rect, Shape, Show};
use nfspu_formats::ffn::Font;
use std::collections::HashMap;
use std::path::PathBuf;

/// An opaque RGBA8 image, top row first.
#[derive(Debug, Clone, PartialEq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// While set, only pixels marked true are drawn (a movie's oval stencil).
    mask: Option<Vec<bool>>,
}

/// Where the compositor gets images and fonts. `ArtCache` reads them from the game folder.
pub trait Art {
    fn shape(&mut self, file: &str, index: usize) -> Option<&Shape>;
    fn font(&mut self, name: &str) -> Option<&Font>;
    /// The current frame of a movie source as (width, height, RGBA8), if it plays.
    fn movie(&mut self, _source: &str) -> Option<(u32, u32, &[u8])> {
        None
    }
    /// The animated pieces drawn over a movie circle this frame (lines, circles, lettering),
    /// already in screen positions.
    fn overlay(&mut self, _source: &str, _rect: Rect) -> Vec<Draw> {
        Vec::new()
    }
}

/// Decoded front-end art and fonts, loaded on first use and kept (a miss is remembered too, and
/// logged once through `on_miss`).
pub struct ArtCache {
    game_dir: PathBuf,
    shapes: HashMap<(String, usize), Option<Shape>>,
    fonts: HashMap<String, Option<Font>>,
    movies: HashMap<String, Option<crate::menu_movie::MoviePlayer>>,
    overlays: HashMap<String, crate::menu_movie::Overlay>,
    /// The selected player's career progress, which decides the demo loop's clips.
    progress: Option<crate::menu_movie::CareerProgress>,
    /// Messages about art or fonts that could not be loaded, for the caller to log.
    pub misses: Vec<String>,
}

impl ArtCache {
    pub fn new(game_dir: PathBuf) -> ArtCache {
        ArtCache { game_dir, shapes: HashMap::new(), fonts: HashMap::new(), movies: HashMap::new(), overlays: HashMap::new(), progress: None, misses: Vec::new() }
    }

    /// Sets the selected player's progress; a change restarts the movies with the new playlist.
    pub fn set_progress(&mut self, progress: Option<crate::menu_movie::CareerProgress>) {
        if self.progress != progress {
            self.progress = progress;
            self.movies.clear();
        }
    }

    /// Moves every movie that has been shown on by `elapsed_ms`. True when a picture changed.
    pub fn advance_movies(&mut self, elapsed_ms: f32) -> bool {
        let mut changed = false;
        for player in self.movies.values_mut().flatten() {
            changed |= player.advance(elapsed_ms);
            self.misses.append(&mut player.problems);
        }
        // The overlay moves continuously, so any overlay on screen means a new picture.
        for overlay in self.overlays.values_mut() {
            overlay.advance(elapsed_ms);
            changed = true;
        }
        changed
    }
}

impl Art for ArtCache {
    fn shape(&mut self, file: &str, index: usize) -> Option<&Shape> {
        let key = (file.to_ascii_lowercase(), index);
        if !self.shapes.contains_key(&key) {
            let loaded = crate::menu::load_art(&self.game_dir, file).and_then(|d| crate::menu::decode_shape(&d, index));
            if let Err(e) = &loaded {
                self.misses.push(format!("menu art {file} #{index}: not drawn ({e:?})"));
            }
            self.shapes.insert(key.clone(), loaded.ok());
        }
        self.shapes[&key].as_ref()
    }

    fn font(&mut self, name: &str) -> Option<&Font> {
        let key = name.to_ascii_lowercase();
        if !self.fonts.contains_key(&key) {
            let loaded = crate::menu::load_font(&self.game_dir, name);
            if loaded.is_none() {
                self.misses.push(format!("menu font {name}: not found, text not drawn"));
            }
            self.fonts.insert(key.clone(), loaded);
        }
        self.fonts[&key].as_ref()
    }

    fn movie(&mut self, source: &str) -> Option<(u32, u32, &[u8])> {
        if !self.movies.contains_key(source) {
            let player = crate::menu_movie::MoviePlayer::new(&self.game_dir, source, self.progress);
            if player.is_none() {
                self.misses.push(format!("movie source '{source}': no clips known, not drawn"));
            }
            self.movies.insert(source.to_string(), player);
        }
        let p = self.movies[source].as_ref()?;
        (p.width > 0).then_some((p.width, p.height, p.rgba.as_slice()))
    }

    fn overlay(&mut self, source: &str, rect: Rect) -> Vec<Draw> {
        // Make sure both overlay fonts are loaded, then lay out with them.
        for name in crate::menu_movie::OVERLAY_FONTS {
            self.font(name);
        }
        let fonts: Vec<Option<&Font>> = crate::menu_movie::OVERLAY_FONTS.iter().map(|n| self.fonts.get(&n.to_ascii_lowercase()).and_then(|f| f.as_ref())).collect();
        let overlay = self.overlays.entry(source.to_string()).or_insert_with(|| crate::menu_movie::Overlay::new(rect, 0x5EED));
        overlay.draws(&fonts)
    }
}

fn byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn inside([x, y, w, h]: Rect, p: [f32; 2]) -> bool {
    p[0] >= x && p[0] < x + w && p[1] >= y && p[1] < y + h
}

impl Canvas {
    /// A canvas filled with an opaque colour, or fully transparent when `clear`'s alpha is 0 (a
    /// page with a 3D room behind it: the room shows wherever nothing is drawn).
    pub fn new(width: u32, height: u32, clear: [f32; 4]) -> Canvas {
        let px = [byte(clear[0]), byte(clear[1]), byte(clear[2]), if clear[3] <= 0.0 { 0 } else { 255 }];
        Canvas { width, height, rgba: px.repeat((width * height) as usize), mask: None }
    }

    /// Blends one pixel: `src * a + dst * (1 - a)` on the stored (gamma-encoded) values. Over a
    /// see-through pixel the colours are weighted by coverage and the alpha grows ("over" with
    /// straight alpha); on an opaque canvas this is the plain formula above.
    fn blend(&mut self, x: i32, y: i32, rgb: [u8; 3], alpha: f32) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height || alpha <= 0.0 {
            return;
        }
        let index = (y as u32 * self.width + x as u32) as usize;
        if let Some(mask) = &self.mask {
            if !mask.get(index).copied().unwrap_or(false) {
                return;
            }
        }
        let at = index * 4;
        let a = alpha.min(1.0);
        let below = self.rgba[at + 3] as f32 / 255.0;
        let out = a + below * (1.0 - a);
        for c in 0..3 {
            let dst = self.rgba[at + c] as f32;
            self.rgba[at + c] = ((rgb[c] as f32 * a + dst * below * (1.0 - a)) / out).round() as u8;
        }
        self.rgba[at + 3] = (out * 255.0).round() as u8;
    }

    /// Fills a rectangle; edges are rounded to whole pixels.
    pub fn fill_rect(&mut self, [x, y, w, h]: Rect, colour: [f32; 4]) {
        let rgb = [byte(colour[0]), byte(colour[1]), byte(colour[2])];
        let (x0, y0, x1, y1) = (x.round() as i32, y.round() as i32, (x + w).round() as i32, (y + h).round() as i32);
        for py in y0..y1 {
            for px in x0..x1 {
                self.blend(px, py, rgb, colour[3]);
            }
        }
    }

    /// Draws an opaque RGBA8 image scaled (bilinear) to fill `rect`.
    pub fn draw_scaled(&mut self, [x, y, w, h]: Rect, src_w: u32, src_h: u32, rgba: &[u8]) {
        let (x0, y0, x1, y1) = (x.round() as i32, y.round() as i32, (x + w).round() as i32, (y + h).round() as i32);
        let sample = |sx: i32, sy: i32, c: usize| rgba[((sy.clamp(0, src_h as i32 - 1) as u32 * src_w + sx.clamp(0, src_w as i32 - 1) as u32) * 4) as usize + c] as f32;
        for py in y0.max(0)..y1.min(self.height as i32) {
            for px in x0.max(0)..x1.min(self.width as i32) {
                // Source position of this pixel's centre.
                let u = ((px as f32 + 0.5 - x) / w * src_w as f32 - 0.5).max(0.0);
                let v = ((py as f32 + 0.5 - y) / h * src_h as f32 - 0.5).max(0.0);
                let (iu, iv) = (u.floor() as i32, v.floor() as i32);
                let (fu, fv) = (u - iu as f32, v - iv as f32);
                let mut rgb = [0u8; 3];
                for (c, out) in rgb.iter_mut().enumerate() {
                    let top = sample(iu, iv, c) * (1.0 - fu) + sample(iu + 1, iv, c) * fu;
                    let bottom = sample(iu, iv + 1, c) * (1.0 - fu) + sample(iu + 1, iv + 1, c) * fu;
                    *out = (top * (1.0 - fv) + bottom * fv).round() as u8;
                }
                self.blend(px, py, rgb, 1.0);
            }
        }
    }

    /// Blends an RGBA8 image with its top-left at `(x, y)`, mirrored by `flip` (bit 1 left-right,
    /// bit 2 top-bottom) and with its alpha scaled by `alpha`.
    pub fn draw_image(&mut self, x: i32, y: i32, width: u32, height: u32, rgba: &[u8], flip: u8, alpha: f32) {
        for sy in 0..height {
            for sx in 0..width {
                let at = ((sy * width + sx) * 4) as usize;
                let Some(p) = rgba.get(at..at + 4) else { return };
                let dx = if flip & 1 != 0 { width - 1 - sx } else { sx };
                let dy = if flip & 2 != 0 { height - 1 - sy } else { sy };
                self.blend(x + dx as i32, y + dy as i32, [p[0], p[1], p[2]], p[3] as f32 / 255.0 * alpha);
            }
        }
    }
}

/// The pixels a movie may cover: the union of its stencil rectangles and the opaque pixels of its
/// stencil pictures. `None` (no stencil) means everywhere.
fn stencil_mask(stencil: &[Draw], art: &mut dyn Art) -> Option<Vec<bool>> {
    if stencil.is_empty() {
        return None;
    }
    let mut mask = vec![false; 640 * 480];
    let mut set = |x: i32, y: i32| {
        if (0..640).contains(&x) && (0..480).contains(&y) {
            mask[(y * 640 + x) as usize] = true;
        }
    };
    for piece in stencil {
        match piece {
            Draw::Quad { rect: [x, y, w, h], .. } => {
                for py in y.round() as i32..(y + h).round() as i32 {
                    for px in x.round() as i32..(x + w).round() as i32 {
                        set(px, py);
                    }
                }
            }
            Draw::Picture { file, index, pos, flip, .. } => {
                let Some(shape) = art.shape(file, *index) else { continue };
                let (w, h) = (shape.width, shape.height);
                for sy in 0..h {
                    for sx in 0..w {
                        if shape.rgba[((sy * w + sx) * 4 + 3) as usize] < 128 {
                            continue;
                        }
                        let dx = if flip & 1 != 0 { w - 1 - sx } else { sx };
                        let dy = if flip & 2 != 0 { h - 1 - sy } else { sy };
                        set(pos[0].round() as i32 + dx as i32, pos[1].round() as i32 + dy as i32);
                    }
                }
            }
            _ => {}
        }
    }
    Some(mask)
}

/// Draws `layers` (bottom first) over `clear`. Each layer's draw list is topmost first, so it is
/// painted from its end. `live` marks the layers that react to the pointer (hover fills, gloss and
/// bevel); `pointer` is in layout units.
pub fn compose(layers: &[(&crate::menu::Screen, bool)], pointer: Option<[f32; 2]>, clear: [f32; 4], art: &mut dyn Art) -> Canvas {
    let mut canvas = Canvas::new(640, 480, clear);
    let owner = list_under(layers, pointer);
    for (l, (screen, live)) in layers.iter().enumerate() {
        for (i, draw) in screen.draw.iter().enumerate().rev() {
            draw_one(&mut canvas, draw, *live, seen_pointer(owner, l, i, pointer), art);
        }
    }
    canvas
}

/// The open dropdown list under the pointer, as (layer, draw index) of its clipped group: the
/// topmost live layer's clipped group containing the pointer.
fn list_under(layers: &[(&crate::menu::Screen, bool)], pointer: Option<[f32; 2]>) -> Option<(usize, usize)> {
    let p = pointer?;
    layers.iter().enumerate().rev().filter(|(_, (_, live))| *live).find_map(|(l, (screen, _))| {
        screen.draw.iter().position(|d| matches!(d, Draw::Clipped { rect, .. } if inside(*rect, p))).map(|i| (l, i))
    })
}

/// The pointer as one draw sees it: while it is over an open list, everything in that layer and
/// below except the list itself is treated as not under the pointer (no hover look through the
/// list; layers above stay as they are).
fn seen_pointer(owner: Option<(usize, usize)>, layer: usize, index: usize, pointer: Option<[f32; 2]>) -> Option<[f32; 2]> {
    match owner {
        Some((ol, oi)) if layer < ol || (layer == ol && index != oi) => None,
        _ => pointer,
    }
}

/// Paints one draw command.
fn draw_one(canvas: &mut Canvas, draw: &Draw, live: bool, pointer: Option<[f32; 2]>, art: &mut dyn Art) {
    let over = |rect: Rect| live && pointer.map(|p| inside(rect, p)).unwrap_or(false);
    match draw {
        Draw::Quad { rect, colour, hover, .. } => {
            let fill = match hover {
                Some(h) if over(*rect) => *h,
                _ => *colour,
            };
            canvas.fill_rect(*rect, fill);
        }
        Draw::Gradient { corners, colours, when } => {
            let visible = match when {
                Show::Always => true,
                Show::Idle(r) => !over(*r),
                Show::Hover(r) => over(*r),
            };
            if visible {
                let (origin, [w, h], rgba) = rasterise_gradient(corners, colours, 1.0);
                canvas.draw_image(origin[0].round() as i32, origin[1].round() as i32, w, h, &rgba, 0, 1.0);
            }
        }
        Draw::Text { text, pos, colour, font, .. } => {
            let Some(f) = art.font(font) else { return };
            if let Some((origin, [w, h], rgba)) = rasterise_text(f, text, *colour) {
                canvas.draw_image((pos[0] + origin[0]).round() as i32, (pos[1] + origin[1]).round() as i32, w, h, &rgba, 0, 1.0);
            }
        }
        Draw::Picture { file, index, pos, flip, alpha } => {
            let Some(shape) = art.shape(file, *index) else { return };
            canvas.draw_image(pos[0].round() as i32, pos[1].round() as i32, shape.width, shape.height, &shape.rgba, *flip, *alpha);
        }
        Draw::Target { .. } => {}
        Draw::Clipped { rect: [x, y, w, h], draws } => {
            let mut mask = vec![false; (canvas.width * canvas.height) as usize];
            for py in (y.round() as i32).max(0)..((y + h).round() as i32).min(canvas.height as i32) {
                for px in (x.round() as i32).max(0)..((x + w).round() as i32).min(canvas.width as i32) {
                    mask[(py as u32 * canvas.width + px as u32) as usize] = true;
                }
            }
            let previous = canvas.mask.replace(mask);
            for piece in draws.iter().rev() {
                draw_one(canvas, piece, live, pointer, art);
            }
            canvas.mask = previous;
        }
        Draw::Shown { when, draw } => {
            let visible = match when {
                Show::Always => true,
                Show::Idle(r) => !over(*r),
                Show::Hover(r) => over(*r),
            };
            if visible {
                draw_one(canvas, draw, live, pointer, art);
            }
        }
        // The movie and its overlay are drawn only inside the stencil, like retail's stencil pass.
        Draw::Movie { rect, source, stencil } => {
            canvas.mask = stencil_mask(stencil, art);
            if let Some((w, h, rgba)) = art.movie(source) {
                canvas.draw_scaled(*rect, w, h, rgba);
            }
            for piece in art.overlay(source, *rect) {
                draw_one(canvas, &piece, false, None, art);
            }
            canvas.mask = None;
        }
    }
}
/// What the pointer is over that changes the picture: hover fills and hover-dependent gradient
/// pieces, as (layer, draw index). The front end recomposes only when this changes.
pub fn hover_key(layers: &[(&crate::menu::Screen, bool)], pointer: Option<[f32; 2]>) -> Vec<(usize, usize)> {
    let Some(p) = pointer else { return Vec::new() };
    let mut key = Vec::new();
    let owner = list_under(layers, pointer);
    for (l, (screen, live)) in layers.iter().enumerate() {
        if !live {
            continue;
        }
        // Pieces inside clipped groups count too (an open dropdown's rows); under an open list
        // only the list's own pieces do.
        // Keep each flattened draw's identity even when pointer ownership hides it.
        // Filtering before enumeration can give a different control the same cache key.
        let flat: Vec<(bool, &Draw)> = screen
            .draw
            .iter()
            .enumerate()
            .flat_map(|(i, d)| {
                let visible = seen_pointer(owner, l, i, pointer).is_some();
                match d {
                    Draw::Clipped { draws, .. } => draws.iter().map(|d| (visible, d)).collect::<Vec<_>>(),
                    other => vec![(visible, other)],
                }
            })
            .collect();
        for (i, (visible, draw)) in flat.into_iter().enumerate() {
            if !visible { continue; }
            let rect = match draw {
                Draw::Quad { rect, hover: Some(_), .. } => Some(*rect),
                Draw::Gradient { when: Show::Idle(r) | Show::Hover(r), .. } => Some(*r),
                Draw::Shown { when: Show::Idle(r) | Show::Hover(r), .. } => Some(*r),
                _ => None,
            };
            if rect.map(|r| inside(r, p)).unwrap_or(false) {
                key.push((l, i));
            }
        }
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::Screen;

    struct NoArt;
    impl Art for NoArt {
        fn shape(&mut self, _: &str, _: usize) -> Option<&Shape> {
            None
        }
        fn font(&mut self, _: &str) -> Option<&Font> {
            None
        }
    }

    fn pixel(c: &Canvas, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * c.width + x) * 4) as usize;
        [c.rgba[at], c.rgba[at + 1], c.rgba[at + 2], c.rgba[at + 3]]
    }

    #[test]
    fn half_alpha_blends_in_screen_space() {
        // Retail-style: 50% black over 255 gives 128, not the linear-light ~188.
        let mut c = Canvas::new(2, 1, [1.0, 1.0, 1.0, 1.0]);
        c.fill_rect([0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 0.0, 0.5]);
        assert_eq!(pixel(&c, 0, 0), [128, 128, 128, 255]);
        assert_eq!(pixel(&c, 1, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn transparent_canvas_keeps_coverage_for_the_room() {
        let mut c = Canvas::new(3, 1, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(pixel(&c, 2, 0)[3], 0);
        // half-covered red keeps its colour and gets alpha 128
        c.fill_rect([0.0, 0.0, 1.0, 1.0], [1.0, 0.0, 0.0, 0.5]);
        assert_eq!(pixel(&c, 0, 0), [255, 0, 0, 128]);
        // two half layers: alpha 0.75 (192 after the first layer is stored as 128), colour 2:1 to the top
        c.fill_rect([0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 1.0, 0.5]);
        assert_eq!(pixel(&c, 0, 0), [85, 0, 170, 192]);
        // opaque paint is plain
        c.fill_rect([1.0, 0.0, 1.0, 1.0], [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(pixel(&c, 1, 0), [0, 255, 0, 255]);
    }

    #[test]
    fn images_flip_and_scale_alpha() {
        let mut c = Canvas::new(2, 1, [0.0, 0.0, 0.0, 1.0]);
        // A 2x1 image: red then green, both opaque, drawn mirrored at half opacity.
        c.draw_image(0, 0, 2, 1, &[255, 0, 0, 255, 0, 255, 0, 255], 1, 0.5);
        assert_eq!(pixel(&c, 0, 0), [0, 128, 0, 255]);
        assert_eq!(pixel(&c, 1, 0), [128, 0, 0, 255]);
    }

    #[test]
    fn earlier_sections_paint_over_later_ones_and_hover_applies_to_live_layers() {
        let screen = Screen {
            draw: vec![
                // Topmost first: a small hoverable red box over a big blue one.
                Draw::Quad { rect: [10.0, 10.0, 4.0, 4.0], colour: [1.0, 0.0, 0.0, 1.0], hover: Some([1.0, 1.0, 0.0, 1.0]), link: None, back: false },
                Draw::Quad { rect: [0.0, 0.0, 20.0, 20.0], colour: [0.0, 0.0, 1.0, 1.0], hover: None, link: None, back: false },
            ],
            ..Default::default()
        };
        let idle = compose(&[(&screen, true)], None, [0.0, 0.0, 0.0, 1.0], &mut NoArt);
        assert_eq!(pixel(&idle, 11, 11), [255, 0, 0, 255], "the first section is on top");
        assert_eq!(pixel(&idle, 1, 1), [0, 0, 255, 255]);
        let hovered = compose(&[(&screen, true)], Some([11.0, 11.0]), [0.0, 0.0, 0.0, 1.0], &mut NoArt);
        assert_eq!(pixel(&hovered, 11, 11), [255, 255, 0, 255]);
        let inert = compose(&[(&screen, false)], Some([11.0, 11.0]), [0.0, 0.0, 0.0, 1.0], &mut NoArt);
        assert_eq!(pixel(&inert, 11, 11), [255, 0, 0, 255], "layers that take no input do not hover");
        assert_eq!(hover_key(&[(&screen, true)], Some([11.0, 11.0])), vec![(0, 0)]);
        assert!(hover_key(&[(&screen, false)], Some([11.0, 11.0])).is_empty());
    }

    /// Art with one 2x1 movie frame (red, blue) and one 2x2 stencil picture opaque on its left.
    struct MovieArt(Shape);
    impl Art for MovieArt {
        fn shape(&mut self, _: &str, _: usize) -> Option<&Shape> {
            Some(&self.0)
        }
        fn font(&mut self, _: &str) -> Option<&Font> {
            None
        }
        fn movie(&mut self, _: &str) -> Option<(u32, u32, &[u8])> {
            Some((2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]))
        }
    }

    #[test]
    fn a_movie_fills_its_rectangle_inside_the_stencil() {
        let half = Shape { width: 2, height: 2, rgba: vec![9, 9, 9, 255, 9, 9, 9, 0, 9, 9, 9, 255, 9, 9, 9, 0] };
        let screen = Screen {
            draw: vec![Draw::Movie {
                rect: [0.0, 0.0, 4.0, 2.0],
                source: "x".into(),
                stencil: vec![
                    Draw::Quad { rect: [3.0, 0.0, 1.0, 2.0], colour: [0.0; 4], hover: None, link: None, back: false },
                    Draw::Picture { file: "s".into(), index: 0, pos: [0.0, 0.0], flip: 0, alpha: 1.0 },
                ],
            }],
            ..Default::default()
        };
        let c = compose(&[(&screen, false)], None, [0.0, 0.0, 0.0, 1.0], &mut MovieArt(half));
        // (MovieArt has no overlay, so only the frame is drawn.)
        assert_eq!(pixel(&c, 0, 0), [255, 0, 0, 255], "left edge shows the frame's red");
        assert_eq!(pixel(&c, 1, 1), [0, 0, 0, 255], "outside the stencil stays clear");
        assert_eq!(pixel(&c, 3, 0), [0, 0, 255, 255], "stencil rectangle shows the frame's blue");
    }

    /// Under an open dropdown list nothing else lights up (playtest 2026-10-20).
    #[test]
    fn an_open_list_hides_hover_underneath() {
        let red = [1.0, 0.0, 0.0, 1.0];
        let yellow = [1.0, 1.0, 0.0, 1.0];
        let button = Draw::Quad { rect: [0.0, 0.0, 20.0, 20.0], colour: red, hover: Some(yellow), link: None, back: false };
        // A list (topmost, first) covering the button's right half, with a hoverable row.
        let list = Draw::Clipped { rect: [10.0, 0.0, 10.0, 20.0], draws: vec![Draw::Quad { rect: [10.0, 0.0, 10.0, 5.0], colour: red, hover: Some(yellow), link: None, back: false }] };
        let screen = Screen { draw: vec![list, button], ..Default::default() };
        let left = compose(&[(&screen, true)], Some([5.0, 10.0]), [0.0, 0.0, 0.0, 1.0], &mut NoArt);
        assert_eq!(pixel(&left, 5, 10), [255, 255, 0, 255], "the button lights up beside the list");
        let under = compose(&[(&screen, true)], Some([15.0, 10.0]), [0.0, 0.0, 0.0, 1.0], &mut NoArt);
        assert_eq!(pixel(&under, 5, 10), [255, 0, 0, 255], "but not through the list");
        let row = compose(&[(&screen, true)], Some([15.0, 2.0]), [0.0, 0.0, 0.0, 1.0], &mut NoArt);
        assert_eq!(pixel(&row, 15, 2), [255, 255, 0, 255], "the list's own row does");
        assert_eq!(hover_key(&[(&screen, true)], Some([15.0, 10.0])), Vec::<(usize, usize)>::new());
    }

    #[test]
    fn entering_a_list_changes_the_hover_cache_key() {
        let button = Draw::Quad { rect: [0.0, 0.0, 10.0, 10.0], colour: [1.0; 4], hover: Some([0.5; 4]), link: None, back: false };
        let row = Draw::Quad { rect: [10.0, 0.0, 10.0, 10.0], colour: [1.0; 4], hover: Some([0.5; 4]), link: None, back: false };
        let list = Draw::Clipped { rect: [10.0, 0.0, 10.0, 10.0], draws: vec![row] };
        let screen = Screen { draw: vec![button, list], ..Default::default() };
        let layers = [(&screen, true)];
        assert_ne!(hover_key(&layers, Some([5.0, 5.0])), hover_key(&layers, Some([15.0, 5.0])), "moving from a button to a list row must redraw the canvas");
    }

    #[test]
    fn hover_pieces_swap_with_the_pointer() {
        let r = [0.0, 0.0, 4.0, 4.0];
        let solid = |c: f32| [[c, c, c, 1.0]; 4];
        let corners = [[0.0, 0.0], [4.0, 0.0], [0.0, 4.0], [4.0, 4.0]];
        let screen = Screen {
            draw: vec![
                Draw::Gradient { corners, colours: solid(1.0), when: Show::Hover(r) },
                Draw::Gradient { corners, colours: solid(0.5), when: Show::Idle(r) },
            ],
            ..Default::default()
        };
        let idle = compose(&[(&screen, true)], Some([100.0, 100.0]), [0.0, 0.0, 0.0, 1.0], &mut NoArt);
        assert_eq!(pixel(&idle, 1, 1)[0], 128);
        let hovered = compose(&[(&screen, true)], Some([1.0, 1.0]), [0.0, 0.0, 0.0, 1.0], &mut NoArt);
        assert_eq!(pixel(&hovered, 1, 1)[0], 255);
    }
}
