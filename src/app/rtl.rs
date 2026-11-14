//! Previews in right-to-left and other complex scripts: drawn by `shape.rs` (joined letters,
//! right-to-left order) and kept as pictures, so scrolling stays smooth.

use super::*;
use crate::shape::{self, HAlign};
use egui::{pos2, vec2, Rect};
use std::path::Path;
use std::sync::Arc;

/// A drawn preview: its picture, size in points and where its first baseline is.
struct Drawn {
    tex: egui::TextureHandle,
    size: egui::Vec2,
    baseline: f32,
}

pub struct ShapedText {
    ctx: egui::Context,
    cache: HashMap<u64, Drawn>,
    fonts: HashMap<PathBuf, Arc<Vec<u8>>>,
    /// pictures that may still be drawn in this frame (the rest follow in the next ones)
    budget: usize,
}

/// For right-to-left text "left" means where the line starts: on the right.
pub fn mirror(a: config::TextAlign) -> config::TextAlign {
    use config::TextAlign::*;
    match a {
        Left => Right,
        Right => Left,
        JustifyLeft => JustifyRight,
        JustifyRight => JustifyLeft,
        x => x,
    }
}

/// The text to preview a font with: a right-to-left font gets a sample in its own script unless
/// the preview text is already written in a right-to-left script.
pub fn text_for(base: &str, rtl: &str) -> String {
    if !rtl.is_empty() && !base.chars().any(shape::is_rtl_char) {
        if let Some(s) = shape::rtl_sample(rtl) {
            return s.to_string();
        }
    }
    base.to_string()
}

impl ShapedText {
    pub fn new(ctx: egui::Context) -> Self {
        Self { ctx, cache: HashMap::new(), fonts: HashMap::new(), budget: 0 }
    }

    pub fn begin_frame(&mut self) {
        self.budget = 16;
        if self.cache.len() > 900 {
            self.cache.clear();
        }
        if self.fonts.len() > 48 {
            self.fonts.clear();
        }
    }

    fn font(&mut self, path: &Path) -> Option<Arc<Vec<u8>>> {
        if let Some(d) = self.fonts.get(path) {
            return Some(d.clone());
        }
        let d = Arc::new(std::fs::read(path).ok()?);
        self.fonts.insert(path.to_path_buf(), d.clone());
        Some(d)
    }

    /// Does the font file have every letter of the text (spaces and marks aside)?
    pub fn covers(&mut self, path: &Path, text: &str) -> bool {
        let Some(data) = self.font(path) else { return false };
        let Ok(face) = ttf_parser::Face::parse(&data, 0) else { return false };
        text.chars().filter(|c| c.is_alphabetic()).all(|c| face.glyph_index(c).is_some())
    }

    /// Draw `text` (which needs shaping) in the font file `path` into `area`, starting at the same
    /// baseline as the other previews. `wrap`: the width to wrap at (tiles). Returns false while the
    /// picture isn't ready yet (the caller shows a placeholder).
    #[allow(clippy::too_many_arguments)]
    pub fn paint(
        &mut self,
        painter: &egui::Painter,
        area: Rect,
        path: &Path,
        index: u32,
        text: &str,
        size: f32,
        color: Color32,
        align: config::TextAlign,
        wrap: Option<f32>,
    ) -> bool {
        let align = if shape::base_rtl(text) { mirror(align) } else { align };
        let h = match align.halign() {
            egui::Align::Min => HAlign::Left,
            egui::Align::Center => HAlign::Center,
            egui::Align::Max => HAlign::Right,
        };
        let ppp = self.ctx.pixels_per_point();
        let max_lines = match wrap {
            Some(_) => ((area.height() - size * 0.2) / (size * 1.3)).floor().max(1.0) as usize,
            None => 1,
        };
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hs = std::collections::hash_map::DefaultHasher::new();
            (path, index, text, size.to_bits(), color, wrap.map(|w| w.to_bits()), h, ppp.to_bits(), max_lines).hash(&mut hs);
            hs.finish()
        };
        if !self.cache.contains_key(&key) {
            if self.budget == 0 {
                self.ctx.request_repaint();
                return false;
            }
            self.budget -= 1;
            let Some(data) = self.font(path) else { return false };
            let Some(pic) = shape::render(&data, index, text, size * ppp, wrap.map(|w| w * ppp), h, max_lines) else { return false };
            let mut rgba = Vec::with_capacity(pic.w * pic.h * 4);
            for a in &pic.alpha {
                let a = ((*a as u32 * color.a() as u32) / 255) as u8;
                rgba.extend_from_slice(&[color.r(), color.g(), color.b(), a]);
            }
            let img = egui::ColorImage::from_rgba_unmultiplied([pic.w, pic.h], &rgba);
            let tex = self.ctx.load_texture(format!("shaped-{key}"), img, egui::TextureOptions::LINEAR);
            self.cache.insert(key, Drawn { tex, size: vec2(pic.w as f32 / ppp, pic.h as f32 / ppp), baseline: pic.baseline / ppp });
        }
        let d = &self.cache[&key];
        let x = match (wrap, h) {
            (Some(_), _) | (None, HAlign::Left) => area.left() - 2.0 / ppp,
            (None, HAlign::Center) => area.center().x - d.size.x / 2.0,
            (None, HAlign::Right) => area.right() - d.size.x + 2.0 / ppp,
        };
        let y = area.top() + size * 0.98 - d.baseline;
        let rect = Rect::from_min_size(pos2(x, y), d.size);
        painter.image(d.tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        true
    }
}
