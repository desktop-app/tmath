//! The TeX font engine: `tex::DefaultTeXFont` and `tex::FontInfo`.
//!
//! Lookups follow the C++ exactly, quirks included. Next-larger and extension
//! glyphs are measured without the font's scale factor, a mapped character
//! loses its bold font, and rows are found by the same binary search over the
//! same row order with the same key types.

use crate::tables::{self, CharFont, FontInfo};

/// `Formula::PIXELS_PER_POINT`. Nothing sets a DPI target, so it stays 1.
pub const PIXELS_PER_POINT: f32 = 1.0;

/// `PREC` in `utils/nums.h`.
pub const PREC: f32 = 0.000_000_1;

const NUMBERS: usize = 0;
const CAPITAL: usize = 1;
const SMALL: usize = 2;
const IS_UNICODE: usize = 3;
const NONE: i32 = -1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TexStyle {
    Display = 0,
    Display1 = 1,
    Text = 2,
    Text1 = 3,
    Script = 4,
    Script1 = 5,
    ScriptScript = 6,
    ScriptScript1 = 7,
}

impl TexStyle {
    pub fn from_index(index: i32) -> Option<Self> {
        Some(match index {
            0 => Self::Display,
            1 => Self::Display1,
            2 => Self::Text,
            3 => Self::Text1,
            4 => Self::Script,
            5 => Self::Script1,
            6 => Self::ScriptScript,
            7 => Self::ScriptScript1,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub italic: f32,
    pub size: f32,
}

/// A measured glyph, as `tex::Char`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Char {
    pub c: u16,
    pub font: i32,
    pub metrics: Metrics,
}

impl Char {
    /// `Char::getCharFont()`: the bold font is the font itself.
    pub fn char_font(&self) -> CharFont {
        CharFont {
            chr: self.c,
            font: self.font,
            bold_font: self.font,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Extension {
    pub top: Option<Char>,
    pub middle: Option<Char>,
    pub repeat: Option<Char>,
    pub bottom: Option<Char>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontError {
    SymbolMappingNotFound,
    TextStyleMappingNotFound,
}

/// `DefaultTeXFont::_generalSettings`. It is global in the C++, written by
/// `\DeclareMathSizes` and reset before each formula; here it belongs to the
/// formula being laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeneralSettings {
    pub mu_font_id: f32,
    pub script_factor: f32,
    pub script_script_factor: f32,
    pub space_font_id: f32,
    pub text_factor: f32,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        let get = |name: &str| {
            find_by_key(&tables::GENERAL_SETTINGS, name, |e| e.0)
                .map(|e| e.1)
                .unwrap_or(0.0)
        };
        Self {
            mu_font_id: get("mufontid"),
            script_factor: get("scriptfactor"),
            script_script_factor: get("scriptscriptfactor"),
            space_font_id: get("spacefontid"),
            text_factor: get("textfactor"),
        }
    }
}

impl GeneralSettings {
    /// `DefaultTeXFont::getSizeFactor()`.
    pub fn size_factor(&self, style: TexStyle) -> f32 {
        if style < TexStyle::Text {
            1.0
        } else if style < TexStyle::Script {
            self.text_factor
        } else if style < TexStyle::ScriptScript {
            self.script_factor
        } else {
            self.script_script_factor
        }
    }
}

/// `std::map::find` over a table generated in map order.
pub(crate) fn find_by_key<'a, T>(
    items: &'a [T],
    key: &str,
    name: impl Fn(&T) -> &str,
) -> Option<&'a T> {
    items
        .binary_search_by(|item| name(item).as_bytes().cmp(key.as_bytes()))
        .ok()
        .map(|index| &items[index])
}

/// `IndexedArray::operator()`: a binary search over the first `K` columns.
fn indexed_find<T: PartialOrd + Copy, const N: usize>(
    rows: &[[T; N]],
    keys: &[T],
) -> Option<[T; N]> {
    let mut low = 0i32;
    let mut high = rows.len() as i32 - 1;
    while low <= high {
        let middle = low + ((high - low) >> 1);
        let row = rows[middle as usize];
        let mut order = 0;
        for (column, key) in keys.iter().enumerate() {
            if *key < row[column] {
                order = -1;
                break;
            }
            if *key > row[column] {
                order = 1;
                break;
            }
        }
        if order == 0 {
            return Some(row);
        }
        if order < 0 {
            high = middle - 1;
        } else {
            low = middle + 1;
        }
    }
    None
}

pub fn info(id: i32) -> &'static FontInfo {
    &tables::FONTS[id as usize]
}

fn font_metrics(info: &FontInfo, c: u16) -> Option<[f32; 5]> {
    indexed_find(info.metrics, &[f32::from(c)])
}

fn font_extension(info: &FontInfo, c: u16) -> Option<[i32; 5]> {
    indexed_find(info.extensions, &[i32::from(c)])
}

fn font_next_larger(info: &FontInfo, c: u16) -> Option<CharFont> {
    indexed_find(info.largers, &[i32::from(c)]).map(|row| CharFont {
        chr: row[1] as u16,
        font: row[2],
        bold_font: row[2],
    })
}

/// `FontInfo::getKern()`.
fn font_kern(info: &FontInfo, left: u16, right: u16, factor: f32) -> f32 {
    match indexed_find(info.kerns, &[f32::from(left), f32::from(right)]) {
        Some(row) => row[2] * factor,
        None => 0.0,
    }
}

/// `DefaultTeXFont::getParameter()`.
pub fn parameter(name: &str) -> f32 {
    find_by_key(&tables::PARAMETERS, name, |e| e.0)
        .map(|e| e.1)
        .unwrap_or(0.0)
}

/// `DefaultTeXFont::getMetrics()`: a missing row measures as zeros.
pub fn metrics(cf: CharFont, size: f32) -> Metrics {
    let m = font_metrics(info(cf.font), cf.chr)
        .map_or([0.0; 4], |row| [row[1], row[2], row[3], row[4]]);
    let factor = size * PIXELS_PER_POINT;
    Metrics {
        width: m[0] * factor,
        height: m[1] * factor,
        depth: m[2] * factor,
        italic: m[3] * factor,
        size,
    }
}

/// `tex::DefaultTeXFont`: a size, a scale factor and the font flags.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TeXFont {
    pub size: f32,
    pub factor: f32,
    pub bold: bool,
    pub roman: bool,
    pub ss: bool,
    pub tt: bool,
    pub it: bool,
}

impl TeXFont {
    pub fn new(size: f32) -> Self {
        Self {
            size,
            factor: 1.0,
            bold: false,
            roman: false,
            ss: false,
            tt: false,
            it: false,
        }
    }

    /// `getChar(const CharFont&, TexStyle)`.
    pub fn char_of(&self, settings: &GeneralSettings, c: CharFont, style: TexStyle) -> Char {
        let mut cf = c;
        let size_factor = settings.size_factor(style);
        let mut id = if self.bold { cf.bold_font } else { cf.font };
        let mut font = info(id);
        fn switch(next: i32, cf: &mut CharFont, id: &mut i32, font: &mut &'static FontInfo) {
            *id = next;
            *font = info(next);
            *cf = CharFont {
                chr: cf.chr,
                font: next,
                bold_font: next,
            };
        }
        if self.bold && cf.font == cf.bold_font {
            switch(font.bold, &mut cf, &mut id, &mut font);
        }
        if self.roman {
            switch(font.roman, &mut cf, &mut id, &mut font);
        }
        if self.ss {
            switch(font.ss, &mut cf, &mut id, &mut font);
        }
        if self.tt {
            switch(font.tt, &mut cf, &mut id, &mut font);
        }
        if self.it {
            switch(font.it, &mut cf, &mut id, &mut font);
        }
        Char {
            c: cf.chr,
            font: id,
            metrics: metrics(cf, self.factor * size_factor),
        }
    }

    /// `getChar(const std::string& symbolName, TexStyle)`.
    pub fn char_of_symbol(
        &self,
        settings: &GeneralSettings,
        name: &str,
        style: TexStyle,
    ) -> Result<Char, FontError> {
        match find_by_key(&tables::SYMBOL_FONTS, name, |e| e.0) {
            Some((_, Some(cf))) => Ok(self.char_of(settings, *cf, style)),
            Some((_, None)) | None => Err(FontError::SymbolMappingNotFound),
        }
    }

    fn char_of_mappings(
        &self,
        settings: &GeneralSettings,
        c: u16,
        mappings: &[Option<CharFont>],
        style: TexStyle,
    ) -> Result<Char, FontError> {
        let (kind, offset) = match c {
            0x30..=0x39 => (NUMBERS, i32::from(c) - 0x30),
            0x61..=0x7A => (SMALL, i32::from(c) - 0x61),
            0x41..=0x5A => (CAPITAL, i32::from(c) - 0x41),
            _ => (IS_UNICODE, i32::from(c)),
        };
        match mappings.get(kind).copied().flatten() {
            None => self.default_char(settings, c, style),
            Some(x) => {
                let cf = CharFont {
                    chr: (i32::from(x.chr) + offset) as u16,
                    font: x.font,
                    bold_font: x.font,
                };
                Ok(self.char_of(settings, cf, style))
            }
        }
    }

    /// `getChar(wchar_t, const std::string& textStyle, TexStyle)`.
    pub fn char_of_text_style(
        &self,
        settings: &GeneralSettings,
        c: u16,
        text_style: &str,
        style: TexStyle,
    ) -> Result<Char, FontError> {
        match find_by_key(&tables::TEXT_STYLES, text_style, |e| e.name) {
            Some(mapping) => self.char_of_mappings(settings, c, mapping.fonts, style),
            None => Err(FontError::TextStyleMappingNotFound),
        }
    }

    /// `getDefaultChar()`.
    pub fn default_char(
        &self,
        settings: &GeneralSettings,
        c: u16,
        style: TexStyle,
    ) -> Result<Char, FontError> {
        let kind = match c {
            0x30..=0x39 => NUMBERS,
            0x61..=0x7A => SMALL,
            _ => CAPITAL,
        };
        self.char_of_text_style(settings, c, tables::TEXT_STYLE_DEFAULTS[kind], style)
    }

    /// `getExtension()`: parts are measured without the scale factor.
    pub fn extension(
        &self,
        settings: &GeneralSettings,
        c: &Char,
        style: TexStyle,
    ) -> Option<Extension> {
        let size = settings.size_factor(style);
        let ext = font_extension(info(c.font), c.c)?;
        let part = |code: i32| {
            (code != NONE).then(|| {
                let cf = CharFont {
                    chr: code as u16,
                    font: c.font,
                    bold_font: c.font,
                };
                Char {
                    c: code as u16,
                    font: c.font,
                    metrics: metrics(cf, size),
                }
            })
        };
        Some(Extension {
            top: part(ext[1]),
            middle: part(ext[2]),
            repeat: part(ext[3]),
            bottom: part(ext[4]),
        })
    }

    /// `getKern()`: only within one font.
    pub fn kern(
        &self,
        settings: &GeneralSettings,
        left: CharFont,
        right: CharFont,
        style: TexStyle,
    ) -> f32 {
        if left.font == right.font {
            font_kern(
                info(left.font),
                left.chr,
                right.chr,
                settings.size_factor(style) * PIXELS_PER_POINT,
            )
        } else {
            0.0
        }
    }

    /// `getLigature()`: only within one font.
    pub fn ligature(&self, left: CharFont, right: CharFont) -> Option<CharFont> {
        if left.font != right.font {
            return None;
        }
        indexed_find(info(left.font).ligatures, &[left.chr, right.chr]).map(|row| CharFont {
            chr: row[2],
            font: left.font,
            bold_font: left.font,
        })
    }

    /// `getMuFontId()`.
    pub fn mu_font_id(&self, settings: &GeneralSettings) -> i32 {
        settings.mu_font_id as i32
    }

    /// `getNextLarger()`: measured without the scale factor.
    pub fn next_larger(&self, settings: &GeneralSettings, c: &Char, style: TexStyle) -> Char {
        match font_next_larger(info(c.font), c.c) {
            None => *c,
            Some(cf) => Char {
                c: cf.chr,
                font: cf.font,
                metrics: metrics(cf, settings.size_factor(style)),
            },
        }
    }

    pub fn has_next_larger(&self, c: &Char) -> bool {
        font_next_larger(info(c.font), c.c).is_some()
    }

    pub fn is_extension_char(&self, c: &Char) -> bool {
        font_extension(info(c.font), c.c).is_some()
    }

    /// `getSpace()`.
    pub fn space(&self, settings: &GeneralSettings, style: TexStyle) -> f32 {
        let font = info(settings.space_font_id as i32);
        font.space * (settings.size_factor(style) * PIXELS_PER_POINT)
    }

    /// `styleParam()`.
    pub fn style_param(&self, settings: &GeneralSettings, name: &str, style: TexStyle) -> f32 {
        parameter(name) * settings.size_factor(style) * PIXELS_PER_POINT
    }

    pub fn quad(&self, settings: &GeneralSettings, style: TexStyle, font: i32) -> f32 {
        info(font).quad * (settings.size_factor(style) * PIXELS_PER_POINT)
    }

    pub fn x_height(&self, settings: &GeneralSettings, style: TexStyle, font: i32) -> f32 {
        info(font).x_height * (settings.size_factor(style) * PIXELS_PER_POINT)
    }

    pub fn em(&self, settings: &GeneralSettings, style: TexStyle) -> f32 {
        settings.size_factor(style) * PIXELS_PER_POINT
    }

    /// `getSkew()`. The C++ compares the `wchar_t` skew char with -1, which
    /// never holds for the unsigned 16-bit `wchar_t` of Windows, so it always
    /// looks the kern up; a font without a skew char stores 0xFFFF, which no
    /// kern row has, and both forms give 0.
    pub fn skew(&self, settings: &GeneralSettings, cf: CharFont, style: TexStyle) -> f32 {
        let skew = info(cf.font).skew_char;
        let right = CharFont {
            chr: skew,
            font: cf.font,
            bold_font: cf.font,
        };
        self.kern(settings, cf, right, style)
    }

    pub fn has_space(&self, font: i32) -> bool {
        info(font).space > PREC
    }
}
