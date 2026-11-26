//! The style environment (`tex::Environment`), unit conversion
//! (`SpaceAtom::getFactor`) and length parsing (`SpaceAtom::getLength`).

use crate::enums::Unit;
use crate::font::{GeneralSettings, PIXELS_PER_POINT, TeXFont, TexStyle};
use crate::tables;

/// `TeXFont::NO_FONT`.
pub const NO_FONT: i32 = -1;

/// The style, font and text settings a part of a formula is laid out with.
///
/// The C++ shares one font object between an environment and the ones derived
/// from it, and the atoms that change it in place restore it before they
/// return, so a copy per environment behaves the same. The general settings
/// are global in the C++; here every environment carries the ones of the
/// formula being laid out.
#[derive(Clone, Debug, PartialEq)]
pub struct Environment {
    pub style: TexStyle,
    pub font: TeXFont,
    pub settings: GeneralSettings,
    pub text_style: &'static str,
    pub small_cap: bool,
    pub scale_factor: f32,
    last_font_id: i32,
    text_width: f32,
    interline_unit: Unit,
    interline: f32,
}

impl Environment {
    pub fn new(style: TexStyle, font: TeXFont, settings: GeneralSettings) -> Self {
        Self::derived(style, 1.0, font, settings, "", false)
    }

    /// The constructor that takes a text width, converted with this
    /// environment's units.
    pub fn with_width(
        style: TexStyle,
        font: TeXFont,
        settings: GeneralSettings,
        unit: Unit,
        width: f32,
    ) -> Self {
        let mut result = Self::new(style, font, settings);
        result.text_width = width * unit_factor(unit, &result);
        result
    }

    /// The private constructor every derived environment goes through: the
    /// last font, the text width and the interline start over.
    fn derived(
        style: TexStyle,
        scale_factor: f32,
        font: TeXFont,
        settings: GeneralSettings,
        text_style: &'static str,
        small_cap: bool,
    ) -> Self {
        Self {
            style,
            font,
            settings,
            text_style,
            small_cap,
            scale_factor,
            last_font_id: NO_FONT,
            text_width: f32::INFINITY,
            interline_unit: Unit::Ex,
            interline: 1.0,
        }
    }

    fn with_style(&self, style: TexStyle) -> Self {
        Self::derived(
            style,
            self.scale_factor,
            self.font,
            self.settings,
            self.text_style,
            self.small_cap,
        )
    }

    fn style_index(&self) -> i32 {
        self.style as i32
    }

    pub fn set_interline(&mut self, unit: Unit, length: f32) {
        self.interline = length;
        self.interline_unit = unit;
    }

    pub fn interline(&self) -> f32 {
        self.interline * unit_factor(self.interline_unit, self)
    }

    pub fn set_text_width(&mut self, unit: Unit, width: f32) {
        self.text_width = width * unit_factor(unit, self);
    }

    pub fn text_width(&self) -> f32 {
        self.text_width
    }

    /// `copy()`: drops the last font, the text width and the interline.
    pub fn copy(&self) -> Self {
        self.with_style(self.style)
    }

    /// `copy(tf)`: keeps the text width and the interline.
    pub fn copy_with_font(&self, font: TeXFont) -> Self {
        let mut result = Self::derived(
            self.style,
            self.scale_factor,
            font,
            self.settings,
            self.text_style,
            self.small_cap,
        );
        result.text_width = self.text_width;
        result.interline = self.interline;
        result.interline_unit = self.interline_unit;
        result
    }

    pub fn cramp_style(&self) -> Self {
        let s = self.style_index();
        self.with_style(style_of(if s % 2 == 1 { s } else { s + 1 }))
    }

    pub fn dnom_style(&self) -> Self {
        let s = self.style_index();
        self.with_style(style_of(2 * (s / 2) + 1 + 2 - 2 * (s / 6)))
    }

    pub fn num_style(&self) -> Self {
        let s = self.style_index();
        self.with_style(style_of(s + 2 - 2 * (s / 6)))
    }

    pub fn root_style(&self) -> Self {
        self.with_style(TexStyle::ScriptScript)
    }

    pub fn sub_style(&self) -> Self {
        let s = self.style_index();
        self.with_style(style_of(2 * (s / 4) + 4 + 1))
    }

    pub fn sup_style(&self) -> Self {
        let s = self.style_index();
        self.with_style(style_of(2 * (s / 4) + 4 + (s % 2)))
    }

    pub fn size(&self) -> f32 {
        self.font.size
    }

    pub fn space(&self) -> f32 {
        self.font.space(&self.settings, self.style) * self.font.factor
    }

    pub fn set_last_font_id(&mut self, id: i32) {
        self.last_font_id = id;
    }

    pub fn last_font_id(&self) -> i32 {
        if self.last_font_id == NO_FONT {
            self.font.mu_font_id(&self.settings)
        } else {
            self.last_font_id
        }
    }
}

/// The style rules above only produce values in range.
fn style_of(index: i32) -> TexStyle {
    TexStyle::from_index(index & 7).unwrap_or(TexStyle::Display)
}

/// `SpaceAtom::getFactor()`: what one of `unit` measures in `env`.
#[allow(clippy::excessive_precision)]
pub fn unit_factor(unit: Unit, env: &Environment) -> f32 {
    let size = env.size();
    let settings = &env.settings;
    let font = &env.font;
    match unit {
        Unit::None => 0.0,
        Unit::Em => font.em(settings, env.style),
        Unit::Ex => font.x_height(settings, env.style, env.last_font_id()),
        Unit::Pixel => 1.0 / size,
        Unit::Point => PIXELS_PER_POINT / size,
        Unit::Pica => (12.0 * PIXELS_PER_POINT) / size,
        Unit::Mu => font.quad(settings, env.style, font.mu_font_id(settings)) / 18.0,
        Unit::Cm => (28.346456693 * PIXELS_PER_POINT) / size,
        Unit::Mm => (2.8346456693 * PIXELS_PER_POINT) / size,
        Unit::In => (72.0 * PIXELS_PER_POINT) / size,
        Unit::Sp => (65536.0 * PIXELS_PER_POINT) / size,
        Unit::Pt => (0.9962640099 * PIXELS_PER_POINT) / size,
        Unit::Dd => (1.0660349422 * PIXELS_PER_POINT) / size,
        Unit::Cc => (12.7924193070 * PIXELS_PER_POINT) / size,
        Unit::X8 => font.style_param(settings, "defaultrulethickness", env.style),
    }
}

/// `SpaceAtom::getSize()`.
pub fn unit_size(unit: Unit, size: f32, env: &Environment) -> f32 {
    unit_factor(unit, env) * size
}

/// `tex::wide2utf8()`, quirks included: it stops at the first NUL, and a low
/// surrogate without a high one before it becomes the code point of its low
/// ten bits, so U+DC70 turns into 'p'.
pub fn wide_to_utf8(text: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut codepoint: u32 = 0;
    for &unit in text {
        if unit == 0 {
            break;
        }
        let unit = u32::from(unit);
        if (0xD800..=0xDBFF).contains(&unit) {
            codepoint = ((unit - 0xD800) << 10) + 0x10000;
            continue;
        }
        if (0xDC00..=0xDFFF).contains(&unit) {
            codepoint |= unit - 0xDC00;
        } else {
            codepoint = unit;
        }
        let c = codepoint;
        if c <= 0x7F {
            out.push(c as u8);
        } else if c <= 0x7FF {
            out.push((0xC0 | ((c >> 6) & 0x1F)) as u8);
            out.push((0x80 | (c & 0x3F)) as u8);
        } else if c <= 0xFFFF {
            out.push((0xE0 | ((c >> 12) & 0x0F)) as u8);
            out.push((0x80 | ((c >> 6) & 0x3F)) as u8);
            out.push((0x80 | (c & 0x3F)) as u8);
        } else {
            out.push((0xF0 | ((c >> 18) & 0x07)) as u8);
            out.push((0x80 | ((c >> 12) & 0x3F)) as u8);
            out.push((0x80 | ((c >> 6) & 0x3F)) as u8);
            out.push((0x80 | (c & 0x3F)) as u8);
        }
        codepoint = 0;
    }
    out
}

/// `tex::binIndexOf()` without the closest match.
fn bin_index_of(count: usize, compare: impl Fn(usize) -> std::cmp::Ordering) -> Option<usize> {
    let mut low = 0i64;
    let mut high = count as i64 - 1;
    while low <= high {
        let middle = low + ((high - low) >> 1);
        match compare(middle as usize) {
            std::cmp::Ordering::Equal => return Some(middle as usize),
            std::cmp::Ordering::Less => high = middle - 1,
            std::cmp::Ordering::Greater => low = middle + 1,
        }
    }
    None
}

/// `SpaceAtom::getUnit()`: names compare as C strings, so the name ends at
/// the first NUL; an unknown name means pixels.
pub fn unit_of(name: &[u8]) -> Unit {
    let name = name.split(|&b| b == 0).next().unwrap_or_default();
    bin_index_of(tables::UNITS.len(), |i| {
        name.cmp(tables::UNITS[i].0.as_bytes())
    })
    .map_or(Unit::Pixel, |i| {
        Unit::from_index(i32::from(tables::UNITS[i].1))
    })
}

/// `std::stringstream >> float` with MSVC's parser, for text that holds no
/// ASCII letter, so no exponent and no hex prefix: classic-locale white
/// space, an optional sign, digits with an optional decimal point. Without a
/// digit it fails and gives 0. Otherwise the value is the correctly rounded
/// `strtof` of what was gathered, overflow to infinity included.
pub fn parse_float_prefix(text: &[u8]) -> f32 {
    let mut i = 0;
    while i < text.len() && matches!(text[i], b' ' | b'\t'..=b'\r') {
        i += 1;
    }
    let mut number = String::new();
    if i < text.len() && (text[i] == b'+' || text[i] == b'-') {
        number.push(text[i] as char);
        i += 1;
    }
    number.push('0');
    let mut digits = 0;
    while i < text.len() && text[i].is_ascii_digit() {
        number.push(text[i] as char);
        digits += 1;
        i += 1;
    }
    if i < text.len() && text[i] == b'.' {
        number.push('.');
        i += 1;
        while i < text.len() && text[i].is_ascii_digit() {
            number.push(text[i] as char);
            digits += 1;
            i += 1;
        }
    }
    if digits == 0 {
        return 0.0;
    }
    number.parse::<f32>().unwrap_or(0.0)
}

/// `SpaceAtom::getLength(const std::string&)`: a number, then a unit.
pub fn parse_length_bytes(text: &[u8]) -> (Unit, f32) {
    if text.is_empty() {
        return (Unit::Pixel, 0.0);
    }
    let split = text
        .iter()
        .position(|b| b.is_ascii_alphabetic())
        .unwrap_or(text.len());
    let mut value = parse_float_prefix(&text[..split]);
    // A literal too large for a float would spread infinity through every
    // box built from the length.
    if !value.is_finite() {
        value = 0.0;
    }
    let unit = if split != text.len() {
        unit_of(&text[split..].to_ascii_lowercase())
    } else {
        Unit::Pixel
    };
    (unit, value)
}

/// `SpaceAtom::getLength(const std::wstring&)`.
pub fn parse_length(text: &[u16]) -> (Unit, f32) {
    parse_length_bytes(&wide_to_utf8(text))
}
