//! Paints a HUD draw list (`hud::HudDraw`) into a transparent RGBA8 image the size of the window.
//! Blending happens on the stored 8-bit values (`src * a + dst * (1 - a)`), like the game's.

use crate::hud::{Align, HudDraw};
use nfspu_formats::ffn::Font;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A decoded picture, RGBA8, top row first.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Where the painter gets art and fonts.
pub trait HudArt {
    /// `entry`: 4-letter entry name, `#n` for the n-th entry, or empty for the first.
    fn picture(&mut self, file: &str, entry: &str) -> Option<&Picture>;
    fn font(&mut self, name: &str) -> Option<&Font>;
}

/// Art and fonts from `GameData/DashHud`, loaded on first use and kept (misses too).
pub struct DashArt {
    dir: PathBuf,
    pictures: HashMap<(String, String), Option<Picture>>,
    fonts: HashMap<String, Option<Font>>,
    /// What could not be loaded, for the caller to log once.
    pub misses: Vec<String>,
}

impl DashArt {
    pub fn new(game_dir: &Path) -> DashArt {
        DashArt { dir: game_dir.join("GameData").join("DashHud"), pictures: HashMap::new(), fonts: HashMap::new(), misses: Vec::new() }
    }

    fn read(&self, file: &str) -> Option<Vec<u8>> {
        let exact = self.dir.join(file);
        let path = if exact.is_file() {
            exact
        } else {
            std::fs::read_dir(&self.dir).ok()?.flatten().map(|e| e.path()).find(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.eq_ignore_ascii_case(file)))?
        };
        let raw = std::fs::read(path).ok()?;
        if nfspu_formats::refpack::is_refpack(&raw) {
            nfspu_formats::refpack::decompress(&raw).ok()
        } else {
            Some(raw)
        }
    }

    fn load_picture(&self, file: &str, entry: &str) -> Option<Picture> {
        let data = self.read(file)?;
        let archive = nfspu_formats::shpi::parse(&data).ok()?;
        let e = if entry.is_empty() {
            archive.entries.first()?
        } else if let Some(n) = entry.strip_prefix('#') {
            archive.entries.get(n.parse::<usize>().ok()?)?
        } else {
            archive.find(entry)?
        };
        let rgba = nfspu_formats::shpi::decode_rgba8(&data, e)?;
        Some(Picture { width: e.width as u32, height: e.height as u32, rgba })
    }
}

impl HudArt for DashArt {
    fn picture(&mut self, file: &str, entry: &str) -> Option<&Picture> {
        let key = (file.to_ascii_lowercase(), entry.to_string());
        if !self.pictures.contains_key(&key) {
            let loaded = self.load_picture(file, entry);
            if loaded.is_none() {
                self.misses.push(format!("hud art {file} '{entry}': not found or not decodable"));
            }
            self.pictures.insert(key.clone(), loaded);
        }
        self.pictures[&key].as_ref()
    }

    fn font(&mut self, name: &str) -> Option<&Font> {
        let key = name.to_ascii_lowercase();
        if !self.fonts.contains_key(&key) {
            let loaded = self.read(name).and_then(|d| Font::parse(&d));
            if loaded.is_none() {
                self.misses.push(format!("hud font {name}: not found"));
            }
            self.fonts.insert(key.clone(), loaded);
        }
        self.fonts[&key].as_ref()
    }
}

/// A transparent RGBA8 image (straight alpha), top row first.
#[derive(Debug, Clone, PartialEq)]
pub struct HudCanvas {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl HudCanvas {
    pub fn new(width: u32, height: u32) -> HudCanvas {
        HudCanvas { width, height, rgba: vec![0; (width * height * 4) as usize] }
    }

    pub fn clear(&mut self) {
        self.rgba.fill(0);
    }

    /// "Over" blend of one straight-alpha pixel (0..=255 per channel).
    fn blend(&mut self, x: i32, y: i32, src: [f32; 4]) {
        if x < 0 || y < 0 || x as u32 >= self.width || y as u32 >= self.height || src[3] <= 0.0 {
            return;
        }
        let at = ((y as u32 * self.width + x as u32) * 4) as usize;
        let a = (src[3] / 255.0).min(1.0);
        let da = self.rgba[at + 3] as f32 / 255.0;
        let out_a = a + da * (1.0 - a);
        for c in 0..3 {
            let d = self.rgba[at + c] as f32;
            self.rgba[at + c] = ((src[c] * a + d * da * (1.0 - a)) / out_a.max(1e-6)).round().clamp(0.0, 255.0) as u8;
        }
        self.rgba[at + 3] = (out_a * 255.0).round() as u8;
    }

    pub fn fill_rect(&mut self, [x, y, w, h]: [f32; 4], colour: [f32; 4]) {
        let c = colour.map(|v| v.clamp(0.0, 1.0) * 255.0);
        for py in y.round() as i32..(y + h).round() as i32 {
            for px in x.round() as i32..(x + w).round() as i32 {
                self.blend(px, py, c);
            }
        }
    }

    /// Paints `pic` onto the parallelogram whose picture corners land on `tl`, `tr` and `bl`
    /// (the fourth follows), sampling bilinearly and multiplying by `tint`.
    pub fn draw_mapped(&mut self, pic: &Picture, tl: [f32; 2], tr: [f32; 2], bl: [f32; 2], tint: [f32; 4]) {
        let (ux, uy) = (tr[0] - tl[0], tr[1] - tl[1]);
        let (vx, vy) = (bl[0] - tl[0], bl[1] - tl[1]);
        let det = ux * vy - uy * vx;
        if det.abs() < 1e-6 || pic.width == 0 || pic.height == 0 {
            return;
        }
        let br = [tr[0] + vx, tr[1] + vy];
        let xs = [tl[0], tr[0], bl[0], br[0]];
        let ys = [tl[1], tr[1], bl[1], br[1]];
        let x0 = xs.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as i32;
        let x1 = xs.iter().cloned().fold(f32::MIN, f32::max).ceil().min(self.width as f32) as i32;
        let y0 = ys.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as i32;
        let y1 = ys.iter().cloned().fold(f32::MIN, f32::max).ceil().min(self.height as f32) as i32;
        let (w, h) = (pic.width as i32, pic.height as i32);
        let texel = |sx: i32, sy: i32| -> [f32; 4] {
            if sx < 0 || sy < 0 || sx >= w || sy >= h {
                return [0.0; 4];
            }
            let at = ((sy * w + sx) * 4) as usize;
            [pic.rgba[at] as f32, pic.rgba[at + 1] as f32, pic.rgba[at + 2] as f32, pic.rgba[at + 3] as f32]
        };
        for py in y0..y1 {
            for px in x0..x1 {
                // Solve p = tl + u * U + v * V for the pixel centre.
                let (dx, dy) = (px as f32 + 0.5 - tl[0], py as f32 + 0.5 - tl[1]);
                let u = (dx * vy - dy * vx) / det;
                let v = (ux * dy - uy * dx) / det;
                if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
                    continue;
                }
                let (sx, sy) = (u * w as f32 - 0.5, v * h as f32 - 0.5);
                let (ix, iy) = (sx.floor() as i32, sy.floor() as i32);
                let (fx, fy) = (sx - ix as f32, sy - iy as f32);
                // Bilinear on premultiplied values so transparent texels do not darken edges.
                let mut acc = [0.0f32; 4];
                for (ox, oy, wt) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
                    let t = texel((ix + ox).clamp(0, w - 1), (iy + oy).clamp(0, h - 1));
                    let a = t[3] / 255.0;
                    acc[0] += t[0] * a * wt;
                    acc[1] += t[1] * a * wt;
                    acc[2] += t[2] * a * wt;
                    acc[3] += t[3] * wt;
                }
                if acc[3] <= 0.0 {
                    continue;
                }
                let a = acc[3] / 255.0;
                let src = [acc[0] / a * tint[0], acc[1] / a * tint[1], acc[2] / a * tint[2], acc[3] * tint[3]];
                self.blend(px, py, src);
            }
        }
    }

    pub fn draw_image(&mut self, pic: &Picture, [x, y, w, h]: [f32; 4], tint: [f32; 4]) {
        self.draw_mapped(pic, [x, y], [x + w, y], [x, y + h], tint);
    }
}

/// Paints a draw list.
pub fn paint(canvas: &mut HudCanvas, draws: &[HudDraw], art: &mut dyn HudArt) {
    for d in draws {
        match d {
            HudDraw::Fill { rect, colour } => canvas.fill_rect(*rect, *colour),
            HudDraw::Image { file, entry, rect, tint } => {
                if let Some(p) = art.picture(file, entry) {
                    let p = p.clone();
                    canvas.draw_image(&p, *rect, *tint);
                }
            }
            HudDraw::Sprite { file, entry, centre, scale, tint } => {
                if let Some(p) = art.picture(file, entry) {
                    let p = p.clone();
                    let (w, h) = (p.width as f32 * scale, p.height as f32 * scale);
                    canvas.draw_image(&p, [(centre[0] - w / 2.0).floor(), (centre[1] - h / 2.0).floor(), w, h], *tint);
                }
            }
            HudDraw::Quad { file, entry, corners, tint } => {
                if let Some(p) = art.picture(file, entry) {
                    let p = p.clone();
                    canvas.draw_mapped(&p, corners[0], corners[1], corners[3], *tint);
                }
            }
            HudDraw::Text { font, text, pos, colour, align, scale } => {
                let Some(f) = art.font(font) else { continue };
                let width = f.measure(text) as f32 * scale;
                let Some((origin, [w, h], rgba)) = crate::menu::rasterise_text(f, text, *colour) else { continue };
                let left = match align {
                    Align::Left => pos[0],
                    Align::Right => pos[0] - width,
                    Align::Centre => pos[0] - width / 2.0,
                };
                let pic = Picture { width: w, height: h, rgba };
                let rect = [(left + origin[0] * scale).round(), (pos[1] + origin[1] * scale).round(), w as f32 * scale, h as f32 * scale];
                canvas.draw_image(&pic, rect, [1.0; 4]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OneArt(Picture);
    impl HudArt for OneArt {
        fn picture(&mut self, _: &str, _: &str) -> Option<&Picture> {
            Some(&self.0)
        }
        fn font(&mut self, _: &str) -> Option<&Font> {
            None
        }
    }

    fn px(c: &HudCanvas, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * c.width + x) * 4) as usize;
        [c.rgba[at], c.rgba[at + 1], c.rgba[at + 2], c.rgba[at + 3]]
    }

    #[test]
    fn half_fill_over_transparent_keeps_colour_and_alpha() {
        let mut c = HudCanvas::new(2, 1);
        c.fill_rect([0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 0.5, 0.5]);
        assert_eq!(px(&c, 0, 0), [0, 0, 128, 128]);
        assert_eq!(px(&c, 1, 0), [0, 0, 0, 0]);
        // A second half layer: alpha about 0.75, colour unchanged.
        c.fill_rect([0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 0.5, 0.5]);
        assert_eq!(px(&c, 0, 0), [0, 0, 128, 192]);
    }

    #[test]
    fn images_land_one_to_one_and_tint_multiplies() {
        // 2x1 picture: red, half-transparent green.
        let pic = Picture { width: 2, height: 1, rgba: vec![255, 0, 0, 255, 0, 255, 0, 128] };
        let mut c = HudCanvas::new(4, 2);
        paint(&mut c, &[HudDraw::Image { file: "x".into(), entry: String::new(), rect: [1.0, 1.0, 2.0, 1.0], tint: [1.0, 1.0, 1.0, 1.0] }], &mut OneArt(pic.clone()));
        assert_eq!(px(&c, 1, 1), [255, 0, 0, 255]);
        assert_eq!(px(&c, 2, 1), [0, 255, 0, 128]);
        assert_eq!(px(&c, 0, 0)[3], 0);
        let mut c = HudCanvas::new(2, 1);
        paint(&mut c, &[HudDraw::Image { file: "x".into(), entry: String::new(), rect: [0.0, 0.0, 2.0, 1.0], tint: [0.5, 1.0, 1.0, 0.5] }], &mut OneArt(pic));
        assert_eq!(px(&c, 0, 0), [128, 0, 0, 128]);
    }

    #[test]
    fn quads_rotate_pictures() {
        // A 1x2 picture (top white, bottom black) turned to lie left-to-right: tip to the right.
        let pic = Picture { width: 1, height: 2, rgba: vec![255, 255, 255, 255, 0, 0, 0, 255] };
        let mut c = HudCanvas::new(2, 1);
        // Picture top-left at (2,0), top-right at (2,1), bottom-left at (0,0).
        c.draw_mapped(&pic, [2.0, 0.0], [2.0, 1.0], [0.0, 0.0], [1.0; 4]);
        assert_eq!(px(&c, 1, 0), [255, 255, 255, 255]);
        assert_eq!(px(&c, 0, 0), [0, 0, 0, 255]);
    }
}
