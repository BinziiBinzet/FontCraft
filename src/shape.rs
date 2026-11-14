//! Text that needs more than one picture per letter: right-to-left scripts (Arabic, Hebrew,
//! Persian, Urdu, Syriac, Thaana, N'Ko…) and scripts whose letters join or combine (Devanagari and
//! the other Indic scripts, Khmer, Myanmar…).
//!
//! The text is put in visual order with the Unicode bidirectional rules (`unicode-bidi`), each run
//! is shaped with the font's own rules (`rustybuzz`, the Rust port of HarfBuzz: joined Arabic
//! letters, ligatures, marks), and the glyphs are drawn into a picture.

use rustybuzz::{Direction, UnicodeBuffer};
use unicode_bidi::BidiInfo;

/// Right-to-left letters.
pub fn is_rtl_char(c: char) -> bool {
    matches!(c as u32,
        0x0590..=0x08FF      // Hebrew, Arabic, Syriac, Thaana, N'Ko, Samaritan, Mandaic, Arabic Extended
        | 0xFB1D..=0xFDFF    // Hebrew and Arabic presentation forms
        | 0xFE70..=0xFEFF
        | 0x10800..=0x10FFF  // historic right-to-left scripts, Adlam's neighbours
        | 0x1E800..=0x1EFFF) // Mende Kikakui, Adlam, Arabic mathematical
}

/// Letters that only look right after shaping (besides right-to-left ones).
fn is_complex_char(c: char) -> bool {
    matches!(c as u32,
        0x0900..=0x0DFF      // Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam, Sinhala
        | 0x0F00..=0x0FFF    // Tibetan
        | 0x1000..=0x109F    // Myanmar
        | 0x1780..=0x17FF    // Khmer
        | 0x1B00..=0x1B7F    // Balinese
        | 0xA980..=0xA9DF)   // Javanese
}

/// Does this text have to be shaped to look right?
pub fn needs_shaping(text: &str) -> bool {
    text.chars().any(|c| is_rtl_char(c) || is_complex_char(c))
}

/// Is the text written right to left (its first letter with a direction decides)?
pub fn base_rtl(text: &str) -> bool {
    for c in text.chars() {
        if is_rtl_char(c) {
            return true;
        }
        if c.is_alphabetic() {
            return false;
        }
    }
    false
}

/// Right-to-left scripts, with the letters a font needs for them and a sample text.
pub const RTL_SCRIPTS: &[(&str, &str, &str)] = &[
    ("Arabic", "ابتثجحخ", "نص حكيم له سر قاطع وذو شأن عظيم مكتوب على ثوب أخضر ومغلف بجلد أزرق"),
    ("Hebrew", "אבגדהוז", "דג סקרן שט בים מאוכזב ולפתע מצא לו חברה"),
    ("Syriac", "ܐܒܓܕ", "ܫܠܡܐ ܥܠܝܟܘܢ ܐܚܝ ܘܚܒܝܒܝ"),
    ("Thaana", "ހށނރ", "ދިވެހިރާއްޖެ ދިވެހި ބަސް"),
    ("N'Ko", "ߊߋߌߍ", "ߒߞߏ ߞߊ߲ ߘߐ߫ ߞߎߘߊ"),
    ("Adlam", "𞤀𞤁𞤂𞤃", "𞤀𞤣𞤤𞤢𞤥 𞤆𞤵𞤤𞤢𞥄𞤪"),
];

/// The sample text for a right-to-left script (by name), if it is one.
pub fn rtl_sample(script: &str) -> Option<&'static str> {
    RTL_SCRIPTS.iter().find(|(n, _, _)| *n == script).map(|(_, _, s)| *s)
}

/// Four-letter script codes (as in a font's `meta` table or Google's list) that are right to left.
pub fn rtl_script_name(code: &str) -> Option<&'static str> {
    match code.trim().get(..4).unwrap_or("") {
        "Arab" => Some("Arabic"),
        "Hebr" => Some("Hebrew"),
        "Syrc" => Some("Syriac"),
        "Thaa" => Some("Thaana"),
        "Nkoo" => Some("N'Ko"),
        "Adlm" => Some("Adlam"),
        _ => None,
    }
}

/// How a picture of text is laid out.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

pub struct Picture {
    pub w: usize,
    pub h: usize,
    /// one byte of coverage (0..255) per pixel
    pub alpha: Vec<u8>,
    /// where the first line's baseline is, from the top (pixels)
    pub baseline: f32,
}

struct Shaper<'a> {
    face: rustybuzz::Face<'a>,
    font: ab_glyph::FontRef<'a>,
    /// font units to pixels
    k: f32,
    scale: ab_glyph::PxScale,
}

impl<'a> Shaper<'a> {
    fn new(data: &'a [u8], index: u32, px: f32) -> Option<Self> {
        use ab_glyph::Font;
        let face = rustybuzz::Face::from_slice(data, index)?;
        let font = ab_glyph::FontRef::try_from_slice_and_index(data, index).ok()?;
        let upem = face.units_per_em() as f32;
        if upem <= 0.0 {
            return None;
        }
        // the size means the em, like everywhere else in FontCraft
        let scale = ab_glyph::PxScale::from(px * font.height_unscaled() / upem);
        Some(Self { face, font, k: px / upem, scale })
    }

    /// The glyphs of one line in visual order: (glyph, x, y offset), and the line's width.
    fn line(&self, text: &str) -> (Vec<(u16, f32, f32)>, f32) {
        let mut out = vec![];
        let mut x = 0.0;
        if text.is_empty() {
            return (out, 0.0);
        }
        let bidi = BidiInfo::new(text, None);
        for para in &bidi.paragraphs {
            let (levels, runs) = bidi.visual_runs(para, para.range.clone());
            for run in runs {
                let rtl = levels[run.start].is_rtl();
                let mut buf = UnicodeBuffer::new();
                buf.push_str(&text[run.clone()]);
                buf.guess_segment_properties();
                buf.set_direction(if rtl { Direction::RightToLeft } else { Direction::LeftToRight });
                let shaped = rustybuzz::shape(&self.face, &[], buf);
                // the shaper hands the glyphs over left to right already
                for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                    out.push((info.glyph_id as u16, x + pos.x_offset as f32 * self.k, pos.y_offset as f32 * self.k));
                    x += pos.x_advance as f32 * self.k;
                }
            }
        }
        (out, x)
    }

    /// Break the text into lines no wider than `max_w` (at spaces; a long word gets a line of its own).
    fn wrap(&self, text: &str, max_w: f32) -> Vec<String> {
        let mut lines = vec![];
        for para in text.split('\n') {
            let mut line = String::new();
            for word in para.split(' ') {
                let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
                if line.is_empty() || self.line(&candidate).1 <= max_w {
                    line = candidate;
                } else {
                    lines.push(std::mem::replace(&mut line, word.to_string()));
                }
            }
            lines.push(line);
        }
        lines
    }
}

/// The lines `text` breaks into at `px` pixels in a width of `max_w` pixels (for printing).
pub fn wrap_lines(data: &[u8], index: u32, text: &str, px: f32, max_w: f32) -> Vec<String> {
    match Shaper::new(data, index, px) {
        Some(s) => s.wrap(text, max_w),
        None => vec![text.to_string()],
    }
}

/// Draw `text` in the font at `px` pixels. With `max_w` it is wrapped to that width and every line
/// is placed by `align` inside it (then `max_lines` limits how many lines are drawn); without it,
/// one line exactly as wide as the text.
pub fn render(data: &[u8], index: u32, text: &str, px: f32, max_w: Option<f32>, align: HAlign, max_lines: usize) -> Option<Picture> {
    use ab_glyph::{point, Font, GlyphId};
    let s = Shaper::new(data, index, px)?;
    let ascent = s.face.ascender() as f32 * s.k;
    let descent = s.face.descender() as f32 * s.k;
    let gap = (s.face.line_gap() as f32 * s.k).max(0.0);
    let line_h = (ascent - descent + gap).max(1.0);
    let texts = match max_w {
        Some(w) => s.wrap(text, w),
        None => text.split('\n').map(String::from).collect(),
    };
    let lines: Vec<(Vec<(u16, f32, f32)>, f32)> = texts.iter().take(max_lines.max(1)).map(|t| s.line(t)).collect();
    let widest = lines.iter().map(|l| l.1).fold(0.0f32, f32::max);
    let w = (max_w.unwrap_or(widest).max(widest).ceil() as usize + 4).clamp(1, 8000);
    let h = ((line_h * lines.len() as f32).ceil() as usize + 4).clamp(1, 8000);
    let mut alpha = vec![0u8; w * h];
    for (k, (glyphs, lw)) in lines.iter().enumerate() {
        let dx = match align {
            HAlign::Left => 2.0,
            HAlign::Center => (w as f32 - lw) / 2.0,
            HAlign::Right => w as f32 - 2.0 - lw,
        };
        let base = ascent + 2.0 + k as f32 * line_h;
        for (gid, x, yoff) in glyphs {
            let g = GlyphId(*gid).with_scale_and_position(s.scale, point(dx + x, base - yoff));
            if let Some(og) = s.font.outline_glyph(g) {
                let b = og.px_bounds();
                og.draw(|gx, gy, c| {
                    let px_ = b.min.x as i32 + gx as i32;
                    let py_ = b.min.y as i32 + gy as i32;
                    if px_ >= 0 && py_ >= 0 && (px_ as usize) < w && (py_ as usize) < h {
                        let i = py_ as usize * w + px_ as usize;
                        let a = (c.clamp(0.0, 1.0) * 255.0) as u8;
                        alpha[i] = alpha[i].max(a);
                    }
                });
            }
        }
    }
    Some(Picture { w, h, alpha, baseline: ascent + 2.0 })
}

/// The right-to-left script a font is made for, if any (see `library::parse_file`):
/// 1. the font says so itself: the design languages in its `meta` table ("Arab", "Hebr"…),
/// 2. it has letters of a right-to-left script but no basic Latin letters,
/// 3. it has them and its name says so ("… Arabic", "… Naskh", "… Hebrew"…).
pub fn rtl_script_of(face: &ttf_parser::Face, family: &str) -> String {
    if let Some(meta) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"meta")) {
        if let Some(langs) = meta_design_languages(meta) {
            if let Some(first) = langs.split(',').map(|l| l.trim()).find(|l| !l.is_empty()) {
                // "Arab", "ar-Arab", "Arab-PK"… — the four-letter script code
                let code = first.split('-').find(|p| p.len() == 4 && p.chars().next().map(|c| c.is_uppercase()).unwrap_or(false));
                if let Some(name) = code.and_then(rtl_script_name) {
                    return name.into();
                }
                if code.is_some() {
                    return String::new();
                }
            }
        }
    }
    let covered = RTL_SCRIPTS.iter().find(|(_, chars, _)| chars.chars().all(|c| face.glyph_index(c).is_some()));
    let Some((name, _, _)) = covered else { return String::new() };
    let latin = "abcdefghijklmnopqrstuvwxyz".chars().all(|c| face.glyph_index(c).is_some());
    if !latin {
        return name.to_string();
    }
    let f = family.to_lowercase();
    let words = [
        "arab", "hebr", "naskh", "kufi", "kufic", "nastaliq", "nastaleeq", "farsi", "persian", "urdu", "thuluth", "ruqaa", "diwani",
        "maghribi", "syriac", "thaana", "nko", "adlam", "ivrit",
    ];
    if words.iter().any(|w| f.contains(w)) {
        return name.to_string();
    }
    String::new()
}

/// The `dlng` (design languages) entry of a `meta` table, e.g. "Arab, Latn".
fn meta_design_languages(t: &[u8]) -> Option<String> {
    let u32_at = |i: usize| t.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let count = u32_at(12)? as usize;
    for k in 0..count.min(64) {
        let rec = 16 + k * 12;
        let tag = t.get(rec..rec + 4)?;
        if tag == b"dlng" {
            let off = u32_at(rec + 4)? as usize;
            let len = u32_at(rec + 8)? as usize;
            return t.get(off..off + len).map(|b| String::from_utf8_lossy(b).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn system_font(names: &[&str]) -> Option<Vec<u8>> {
        let win = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        names.iter().map(|n| win.join("Fonts").join(n)).find(|p| p.exists()).and_then(|p| std::fs::read(p).ok())
    }

    #[test]
    fn knows_directions() {
        assert!(base_rtl("نص حكيم"));
        assert!(base_rtl("  «שלום» world"));
        assert!(!base_rtl("Hello مرحبا"));
        assert!(needs_shaping("नमस्ते"));
        assert!(!needs_shaping("The quick brown fox"));
        assert_eq!(rtl_script_name("Arab"), Some("Arabic"));
        assert_eq!(rtl_script_name("Latn"), None);
    }

    /// Arabic letters are joined: the shaped word is narrower than its letters drawn one by one,
    /// and the glyphs come out of the shaper (not the stand-alone letters).
    #[test]
    fn shapes_arabic() {
        let Some(data) = system_font(&["arial.ttf", "segoeui.ttf", "tahoma.ttf"]) else { return };
        let s = Shaper::new(&data, 0, 40.0).unwrap();
        let word = "مرحبا";
        let (joined, w_joined) = s.line(word);
        let isolated: f32 = word.chars().map(|c| s.line(&c.to_string()).1).sum();
        assert!(!joined.is_empty());
        assert!(w_joined < isolated, "joined {w_joined} vs isolated {isolated}");
        let pic = render(&data, 0, "نص حكيم له سر قاطع", 40.0, Some(200.0), HAlign::Right, 10).unwrap();
        assert!(pic.h as f32 > 40.0 * 1.5, "should wrap to two lines or more, height {}", pic.h);
        assert!(pic.alpha.iter().any(|a| *a > 200));
    }
}
