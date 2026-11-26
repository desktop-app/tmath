//! The display list: the `Graphics2D` calls a box tree makes when drawn.
//!
//! `Graphics` mirrors the state `Graphics2D_qt` keeps (color, stroke, font,
//! the scale product) because the draw code reads it back and branches on
//! it. Each call is recorded as an `Op`; the C++ side replays the list on a
//! real `Graphics2D_qt`, which then ends in the same state.

use crate::tables;
use crate::text::{format_float, format_units};
use std::fmt::Write;

pub const BLACK: u32 = 0xFF00_0000;
pub const TRANSPARENT: u32 = 0;

pub fn is_transparent(color: u32) -> bool {
    color >> 24 == 0
}

pub const CAP_BUTT: i32 = 0;
pub const CAP_ROUND: i32 = 1;
pub const JOIN_BEVEL: i32 = 0;
pub const JOIN_MITER: i32 = 1;
pub const JOIN_ROUND: i32 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub line_width: f32,
    pub miter_limit: f32,
    pub cap: i32,
    pub join: i32,
}

impl Stroke {
    pub fn new(line_width: f32, cap: i32, join: i32) -> Self {
        Self {
            line_width,
            miter_limit: 0.0,
            cap,
            join,
        }
    }
}

impl Default for Stroke {
    fn default() -> Self {
        Self::new(1.0, CAP_ROUND, JOIN_ROUND)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// A font text is laid out with: a family name, a `TypefaceStyle` and the
/// integer pixel size `Font_qt` rounds the requested size to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextFont {
    pub family: String,
    pub style: i32,
    pub pixel_size: i32,
}

impl TextFont {
    /// `Font::_create(name, style, size)`.
    pub fn new(family: &str, style: i32, size: f32) -> Self {
        Self {
            family: family.to_owned(),
            style,
            pixel_size: round_pixel_size(size),
        }
    }

    /// `Font::deriveFont(style)`.
    pub fn derive(&self, style: i32) -> Self {
        Self {
            family: self.family.clone(),
            style,
            pixel_size: self.pixel_size,
        }
    }

    pub fn size(&self) -> f32 {
        self.pixel_size as f32
    }
}

/// `qRound(size)`, at least 1, as `Font_qt` sets its pixel size.
pub fn round_pixel_size(size: f32) -> i32 {
    let rounded = if size >= 0.0 {
        (size + 0.5) as i32
    } else {
        (size - 0.5) as i32
    };
    rounded.max(1)
}

/// What a `Graphics2D` call was asked to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    SetColor(u32),
    SetStroke(Stroke),
    SetStrokeWidth(f32),
    /// A TeX font, by its index in `tables::FONTS`.
    SetFont(i32),
    Translate(f32, f32),
    Scale(f32, f32),
    Rotate(f32),
    RotateAt(f32, f32, f32),
    Reset,
    DrawChar(u16, f32, f32),
    DrawText(Vec<u16>, f32, f32),
    DrawLine(f32, f32, f32, f32),
    DrawRect(f32, f32, f32, f32),
    FillRect(f32, f32, f32, f32),
    DrawRoundRect(f32, f32, f32, f32, f32, f32),
    FillRoundRect(f32, f32, f32, f32, f32, f32),
    /// `TextLayout::draw`: text measured with `font` at layout time.
    Layout(TextFont, Vec<u16>, f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FontState {
    /// The font a fresh `Graphics2D_qt` starts with.
    Default,
    Tex(i32),
}

/// The state `Graphics2D_qt` tracks, and the calls made so far.
#[derive(Clone, Debug)]
pub struct Graphics {
    color: u32,
    stroke: Stroke,
    font: FontState,
    sx: f32,
    sy: f32,
    pub ops: Vec<Op>,
}

impl Default for Graphics {
    fn default() -> Self {
        Self::new()
    }
}

impl Graphics {
    pub fn new() -> Self {
        Self {
            color: BLACK,
            stroke: Stroke::default(),
            font: FontState::Default,
            sx: 1.0,
            sy: 1.0,
            ops: Vec::new(),
        }
    }

    pub fn color(&self) -> u32 {
        self.color
    }

    pub fn set_color(&mut self, color: u32) {
        self.color = color;
        self.ops.push(Op::SetColor(color));
    }

    pub fn stroke(&self) -> Stroke {
        self.stroke
    }

    pub fn set_stroke(&mut self, stroke: Stroke) {
        self.stroke = stroke;
        self.ops.push(Op::SetStroke(stroke));
    }

    /// The C++ "restores" a stroke through the reference `getStroke()`
    /// returned before changing it, which by then names the new stroke: the
    /// call sets the current stroke again.
    pub fn restore_stroke_through_alias(&mut self) {
        let current = self.stroke;
        self.set_stroke(current);
    }

    pub fn set_stroke_width(&mut self, width: f32) {
        self.stroke.line_width = width;
        self.ops.push(Op::SetStrokeWidth(width));
    }

    pub fn is_tex_font(&self, id: i32) -> bool {
        self.font == FontState::Tex(id)
    }

    pub fn set_tex_font(&mut self, id: i32) {
        self.font = FontState::Tex(id);
        self.ops.push(Op::SetFont(id));
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.ops.push(Op::Translate(dx, dy));
    }

    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.sx *= sx;
        self.sy *= sy;
        self.ops.push(Op::Scale(sx, sy));
    }

    pub fn rotate(&mut self, angle: f32) {
        self.ops.push(Op::Rotate(angle));
    }

    pub fn rotate_at(&mut self, angle: f32, px: f32, py: f32) {
        self.ops.push(Op::RotateAt(angle, px, py));
    }

    pub fn reset(&mut self) {
        self.sx = 1.0;
        self.sy = 1.0;
        self.ops.push(Op::Reset);
    }

    pub fn sx(&self) -> f32 {
        self.sx
    }

    pub fn sy(&self) -> f32 {
        self.sy
    }

    pub fn draw_char(&mut self, c: u16, x: f32, y: f32) {
        self.ops.push(Op::DrawChar(c, x, y));
    }

    pub fn draw_text(&mut self, text: &[u16], x: f32, y: f32) {
        self.ops.push(Op::DrawText(text.to_vec(), x, y));
    }

    pub fn draw_line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.ops.push(Op::DrawLine(x1, y1, x2, y2));
    }

    pub fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.ops.push(Op::DrawRect(x, y, w, h));
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.ops.push(Op::FillRect(x, y, w, h));
    }

    pub fn draw_round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) {
        self.ops.push(Op::DrawRoundRect(x, y, w, h, rx, ry));
    }

    pub fn fill_round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) {
        self.ops.push(Op::FillRoundRect(x, y, w, h, rx, ry));
    }

    pub fn layout(&mut self, font: &TextFont, text: &[u16], x: f32, y: f32) {
        self.ops.push(Op::Layout(font.clone(), text.to_vec(), x, y));
    }
}

/// How the oracle describes a text font: `name:<family>/<style>@<pixels>`.
pub fn describe_text_font(font: &TextFont) -> String {
    format!("name:{}/{}@{}", font.family, font.style, font.pixel_size)
}

/// How the oracle describes a TeX font: loaded from its file at one point,
/// which `Font_qt` rounds to one pixel.
pub fn describe_tex_font(id: i32) -> String {
    let path = usize::try_from(id)
        .ok()
        .and_then(|i| tables::FONTS.get(i))
        .map_or("", |info| info.path);
    format!("file:{path}/0@1")
}

/// The oracle's canonical line for one op.
pub fn format_op(op: &Op) -> String {
    let f = format_float;
    let mut out = String::new();
    let _ = match op {
        Op::SetColor(c) => write!(out, "setColor {c:08x}"),
        Op::SetStroke(s) => write!(
            out,
            "setStroke {} {} {} {}",
            f(s.line_width),
            f(s.miter_limit),
            s.cap,
            s.join
        ),
        Op::SetStrokeWidth(w) => write!(out, "setStrokeWidth {}", f(*w)),
        Op::SetFont(id) => write!(out, "setFont {}", describe_tex_font(*id)),
        Op::Translate(x, y) => write!(out, "translate {} {}", f(*x), f(*y)),
        Op::Scale(x, y) => write!(out, "scale {} {}", f(*x), f(*y)),
        Op::Rotate(a) => write!(out, "rotate {}", f(*a)),
        Op::RotateAt(a, x, y) => write!(out, "rotateAt {} {} {}", f(*a), f(*x), f(*y)),
        Op::Reset => write!(out, "reset"),
        Op::DrawChar(c, x, y) => write!(out, "drawChar {} {} {}", format_units([*c]), f(*x), f(*y)),
        Op::DrawText(t, x, y) => write!(
            out,
            "drawText {} {} {}",
            format_units(t.iter().copied()),
            f(*x),
            f(*y)
        ),
        Op::DrawLine(a, b, c, d) => {
            write!(out, "drawLine {} {} {} {}", f(*a), f(*b), f(*c), f(*d))
        }
        Op::DrawRect(a, b, c, d) => {
            write!(out, "drawRect {} {} {} {}", f(*a), f(*b), f(*c), f(*d))
        }
        Op::FillRect(a, b, c, d) => {
            write!(out, "fillRect {} {} {} {}", f(*a), f(*b), f(*c), f(*d))
        }
        Op::DrawRoundRect(a, b, c, d, e, g) => write!(
            out,
            "drawRoundRect {} {} {} {} {} {}",
            f(*a),
            f(*b),
            f(*c),
            f(*d),
            f(*e),
            f(*g)
        ),
        Op::FillRoundRect(a, b, c, d, e, g) => write!(
            out,
            "fillRoundRect {} {} {} {} {} {}",
            f(*a),
            f(*b),
            f(*c),
            f(*d),
            f(*e),
            f(*g)
        ),
        Op::Layout(font, t, x, y) => write!(
            out,
            "layout {} {} {} {}",
            describe_text_font(font),
            format_units(t.iter().copied()),
            f(*x),
            f(*y)
        ),
    };
    out
}
