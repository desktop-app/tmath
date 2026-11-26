//! The small atoms of `atom/atom_impl.h`: dots, laps, strokes, carons,
//! cedilla and ogonek, rules over and under, strike-through, cancel lines,
//! reflection, rotation, resizing, small capitals and the framed variants.

use crate::atoms::{AtomId, Ctx};
use crate::boxes::{Alignment, BoxId, FrameShape, cmin};
use crate::draw::TRANSPARENT;
use crate::enums::Unit;
use crate::env::{Environment, unit_factor};
use crate::error::Result;
use crate::font::{Char, PREC};
use crate::layout::{create_box, null_atom, space_of, symbol_char, text_style_char};
use crate::wstr::w;

/// `FBoxAtom::INTERSPACE`.
const INTERSPACE: f32 = 0.65;

fn drt_of(env: &Environment) -> f32 {
    env.font
        .style_param(&env.settings, "defaultrulethickness", env.style)
}

fn axis_of(env: &Environment) -> f32 {
    env.font.style_param(&env.settings, "axisheight", env.style)
}

/// The three dots of `\ddots` or `\iddots`, aligned as given.
pub fn diagonal_dots(
    ctx: &mut Ctx,
    aligns: [Alignment; 3],
    env: &mut Environment,
) -> Result<BoxId> {
    let ldots = crate::parser::predefined_formula(ctx, &w("ldots"))?.ok_or_else(null_atom)?;
    let ldots = create_box(ctx, ldots, env)?;
    let width = ctx.boxes.width(ldots);
    let ldotp = ctx.symbol("ldotp")?;
    let dot = create_box(ctx, ldotp, env)?;
    let hb1 = ctx.boxes.hbox_aligned(dot, width, aligns[0])?;
    let hb2 = ctx.boxes.hbox_aligned(dot, width, aligns[1])?;
    let hb3 = ctx.boxes.hbox_aligned(dot, width, aligns[2])?;
    let pt4 = space_of(ctx, Unit::Mu, 0.0, 4.0, 0.0, env)?;
    let vb = ctx.boxes.vbox()?;
    for b in [hb1, pt4, hb2, pt4, hb3] {
        ctx.boxes.vbox_add(vb, b)?;
    }
    let node = ctx.boxes.get_mut(vb);
    let h = node.height + node.depth;
    node.height = h;
    node.depth = 0.0;
    Ok(vb)
}

/// `VdotsAtom::createBox()`.
pub fn vdots(ctx: &mut Ctx, env: &mut Environment) -> Result<BoxId> {
    let ldotp = ctx.symbol("ldotp")?;
    let dot = create_box(ctx, ldotp, env)?;
    let vb = ctx.boxes.vbox_aligned(dot, 0.0, Alignment::Bottom)?;
    let b = space_of(ctx, Unit::Mu, 0.0, 4.0, 0.0, env)?;
    for x in [b, dot, b, dot] {
        ctx.boxes.vbox_add(vb, x)?;
    }
    let node = ctx.boxes.get_mut(vb);
    let (d, h) = (node.depth, node.height);
    node.depth = 0.0;
    node.height = d + h;
    Ok(vb)
}

/// `VCenteredAtom::createBox()`.
pub fn vcentered(ctx: &mut Ctx, at: AtomId, env: &mut Environment) -> Result<BoxId> {
    let b = create_box(ctx, at, env)?;
    let node = ctx.boxes.get(b);
    let total = node.height + node.depth;
    let axis = axis_of(env);
    ctx.boxes.get_mut(b).shift = -(total / 2.0) - axis;
    ctx.boxes.hbox_of(b)
}

/// `MonoScaleAtom::createBox()`.
pub fn mono_scale(
    ctx: &mut Ctx,
    base: AtomId,
    factor: f32,
    env: &mut Environment,
) -> Result<BoxId> {
    let mut e = env.copy();
    let f = e.scale_factor;
    e.scale_factor = factor;
    let b = create_box(ctx, base, &mut e)?;
    ctx.boxes.scale_box(b, factor / f, factor / f)
}

fn char_of(ctx: &mut Ctx, env: &Environment, c: u8) -> Result<BoxId> {
    let ch = text_style_char(env, u16::from(c), "mathnormal", env.style)?;
    ctx.boxes.char_box(&ch)
}

fn symbol_box(ctx: &mut Ctx, env: &Environment, name: &str) -> Result<BoxId> {
    let ch = symbol_char(env, name, env.style)?;
    ctx.boxes.char_box(&ch)
}

/// `IJAtom::createBox()`.
pub fn ij(ctx: &mut Ctx, upper: bool, env: &mut Environment) -> Result<BoxId> {
    let i = char_of(ctx, env, if upper { b'I' } else { b'i' })?;
    let j = char_of(ctx, env, if upper { b'J' } else { b'j' })?;
    let hb = ctx.boxes.hbox_of(i)?;
    let s = space_of(ctx, Unit::Em, -0.065, 0.0, 0.0, env)?;
    ctx.boxes.hbox_add(hb, s)?;
    ctx.boxes.hbox_add(hb, j)?;
    Ok(hb)
}

/// `LCaronAtom::createBox()`.
pub fn lcaron(ctx: &mut Ctx, upper: bool, env: &mut Environment) -> Result<BoxId> {
    let a = symbol_box(ctx, env, "textapos")?;
    let l = char_of(ctx, env, if upper { b'L' } else { b'l' })?;
    let hb = ctx.boxes.hbox_of(l)?;
    let kern = if upper { -0.3 } else { -0.13 };
    let s = space_of(ctx, Unit::Em, kern, 0.0, 0.0, env)?;
    ctx.boxes.hbox_add(hb, s)?;
    ctx.boxes.hbox_add(hb, a)?;
    Ok(hb)
}

/// `TCaronAtom::createBox()`.
pub fn tcaron(ctx: &mut Ctx, env: &mut Environment) -> Result<BoxId> {
    let a = symbol_box(ctx, env, "textapos")?;
    let t = char_of(ctx, env, b't')?;
    let hb = ctx.boxes.hbox_of(t)?;
    let s = space_of(ctx, Unit::Em, -0.3, 0.0, 0.0, env)?;
    ctx.boxes.hbox_add(hb, s)?;
    ctx.boxes.hbox_add(hb, a)?;
    Ok(hb)
}

/// The accent char, moved left by its italic correction when it has one.
fn italic_corrected(ctx: &mut Ctx, b: BoxId, ch: &Char) -> Result<BoxId> {
    let italic = ch.metrics.italic;
    if italic.abs() > PREC {
        let s = ctx.boxes.strut(-italic, 0.0, 0.0, 0.0)?;
        let hbox = ctx.boxes.hbox_of(s)?;
        ctx.boxes.hbox_add(hbox, b)?;
        return Ok(hbox);
    }
    Ok(b)
}

/// `TStrokeAtom::createBox()`.
pub fn tstroke(ctx: &mut Ctx, upper: bool, env: &mut Environment) -> Result<BoxId> {
    let ch = symbol_char(env, "bar", env.style)?;
    let t = char_of(ctx, env, if upper { b'T' } else { b't' })?;
    let bar = ctx.boxes.char_box(&ch)?;
    let y = italic_corrected(ctx, bar, &ch)?;
    let t_width = ctx.boxes.width(t);
    let b = ctx.boxes.hbox_aligned(y, t_width, Alignment::Center)?;
    let vb = ctx.boxes.vbox()?;
    ctx.boxes.vbox_add(vb, t)?;
    let s = ctx.boxes.strut(0.0, -0.5 * t_width, 0.0, 0.0)?;
    ctx.boxes.vbox_add(vb, s)?;
    ctx.boxes.vbox_add(vb, b)?;
    Ok(vb)
}

/// `CedillaAtom::createBox()` and `OgonekAtom::createBox()`.
pub fn under_mark(
    ctx: &mut Ctx,
    base: Option<AtomId>,
    ogonek: bool,
    env: &mut Environment,
) -> Result<BoxId> {
    let b = create_box(ctx, base.ok_or_else(null_atom)?, env)?;
    let vb = ctx.boxes.vbox()?;
    ctx.boxes.vbox_add(vb, b)?;
    let name = if ogonek { "ogonek" } else { "mathcedilla" };
    let ch = symbol_char(env, name, env.style)?;
    let mark = ctx.boxes.char_box(&ch)?;
    let y = italic_corrected(ctx, mark, &ch)?;
    let b_width = ctx.boxes.width(b);
    let align = if ogonek {
        Alignment::Right
    } else {
        Alignment::Center
    };
    let placed = ctx.boxes.hbox_aligned(y, b_width, align)?;
    let raise = if ogonek {
        -ctx.boxes.height(mark)
    } else {
        -(0.4 * unit_factor(Unit::Mu, env))
    };
    let s = ctx.boxes.strut(0.0, raise, 0.0, 0.0)?;
    ctx.boxes.vbox_add(vb, s)?;
    ctx.boxes.vbox_add(vb, placed)?;
    let b_height = ctx.boxes.height(b);
    let node = ctx.boxes.get_mut(vb);
    let f = node.height + node.depth;
    node.height = b_height;
    node.depth = f - b_height;
    Ok(vb)
}

/// `LapedAtom::createBox()`: no width, the content to the left, right or
/// both sides of where it stands.
pub fn laped(ctx: &mut Ctx, at: Option<AtomId>, kind: u16, env: &mut Environment) -> Result<BoxId> {
    let b = create_box(ctx, at.ok_or_else(null_atom)?, env)?;
    let vb = ctx.boxes.vbox()?;
    ctx.boxes.vbox_add(vb, b)?;
    ctx.boxes.get_mut(vb).width = 0.0;
    let width = ctx.boxes.width(b);
    ctx.boxes.get_mut(b).shift = match kind {
        0x6C => -width,
        0x72 => 0.0,
        _ => -width / 2.0,
    };
    Ok(vb)
}

/// `OverlinedAtom::createBox()`.
pub fn overlined(ctx: &mut Ctx, base: Option<AtomId>, env: &mut Environment) -> Result<BoxId> {
    let drt = drt_of(env);
    let b = match base {
        None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
        Some(a) => create_box(ctx, a, &mut env.cramp_style())?,
    };
    let ob = ctx.boxes.over_bar(b, 3.0 * drt, drt)?;
    let (h, d) = (ctx.boxes.height(b), ctx.boxes.depth(b));
    let node = ctx.boxes.get_mut(ob);
    node.depth = d;
    node.height = h + 5.0 * drt;
    Ok(ob)
}

/// `UnderlinedAtom::createBox()`.
pub fn underlined(ctx: &mut Ctx, base: Option<AtomId>, env: &mut Environment) -> Result<BoxId> {
    let drt = drt_of(env);
    let b = match base {
        None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
        Some(a) => create_box(ctx, a, env)?,
    };
    let vb = ctx.boxes.vbox()?;
    ctx.boxes.vbox_add(vb, b)?;
    let s = ctx.boxes.strut(0.0, 3.0 * drt, 0.0, 0.0)?;
    ctx.boxes.vbox_add(vb, s)?;
    let width = ctx.boxes.width(b);
    let rule = ctx.boxes.rule(drt, width, 0.0, TRANSPARENT, true)?;
    ctx.boxes.vbox_add(vb, rule)?;
    let (h, d) = (ctx.boxes.height(b), ctx.boxes.depth(b));
    let node = ctx.boxes.get_mut(vb);
    node.depth = d + 5.0 * drt;
    node.height = h;
    Ok(vb)
}

/// `StrikeThroughAtom::createBox()`. The C++ passes `false` where the rule
/// color goes, so the rule takes the current color.
pub fn strike_through(ctx: &mut Ctx, at: Option<AtomId>, env: &mut Environment) -> Result<BoxId> {
    let axis = axis_of(env);
    let drt = drt_of(env);
    let b = create_box(ctx, at.ok_or_else(null_atom)?, env)?;
    let width = ctx.boxes.width(b);
    let rule = ctx.boxes.rule(drt, width, -axis + drt, TRANSPARENT, true)?;
    let hb = ctx.boxes.hbox()?;
    ctx.boxes.hbox_add(hb, b)?;
    let s = ctx.boxes.strut(-width, 0.0, 0.0, 0.0)?;
    ctx.boxes.hbox_add(hb, s)?;
    ctx.boxes.hbox_add(hb, rule)?;
    Ok(hb)
}

/// `CancelAtom::createBox()`: 0 slash, 1 backslash, 2 cross.
pub fn cancel(ctx: &mut Ctx, base: AtomId, kind: i32, env: &mut Environment) -> Result<BoxId> {
    let b = create_box(ctx, base, env)?;
    let (width, height, depth) = (ctx.boxes.width(b), ctx.boxes.height(b), ctx.boxes.depth(b));
    let total = height + depth;
    let lines = match kind {
        0 => vec![0.0, 0.0, width, total],
        1 => vec![width, 0.0, 0.0, total],
        2 => vec![0.0, 0.0, width, total, width, 0.0, 0.0, total],
        _ => return Ok(b),
    };
    let rt = drt_of(env);
    let overlap = ctx.boxes.line(lines, rt)?;
    let node = ctx.boxes.get_mut(overlap);
    node.width = width;
    node.height = height;
    node.depth = depth;
    let hbox = ctx.boxes.hbox_of(b)?;
    let s = ctx.boxes.strut(-width, 0.0, 0.0, 0.0)?;
    ctx.boxes.hbox_add(hbox, s)?;
    ctx.boxes.hbox_add(hbox, overlap)?;
    Ok(hbox)
}

/// `Rotation`: the rotation origins `\rotatebox` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    BottomLeft,
    BottomCenter,
    BottomRight,
    TopLeft,
    TopCenter,
    TopRight,
    BaselineLeft,
    BaselineRight,
    BaselineCenter,
    CenterLeft,
    CenterCenter,
    CenterRight,
}

/// `RotateBox::getOrigin()`.
pub fn origin_of(option: &[u8]) -> Rotation {
    if option.is_empty() {
        return Rotation::BaselineLeft;
    }
    let mut option = option.to_vec();
    if option.len() == 1 {
        option.push(b'c');
    }
    match option.as_slice() {
        b"bl" | b"lb" => Rotation::BottomLeft,
        b"bc" | b"cb" => Rotation::BottomCenter,
        b"br" | b"rb" => Rotation::BottomRight,
        b"cl" | b"lc" => Rotation::CenterLeft,
        b"cc" => Rotation::CenterCenter,
        b"cr" | b"rc" => Rotation::CenterRight,
        b"tl" | b"lt" => Rotation::TopLeft,
        b"tc" | b"ct" => Rotation::TopCenter,
        b"tr" | b"rt" => Rotation::TopRight,
        b"Bl" | b"lB" => Rotation::BaselineLeft,
        b"Bc" | b"cB" => Rotation::BaselineCenter,
        b"Br" | b"rB" => Rotation::BaselineRight,
        _ => Rotation::BaselineLeft,
    }
}

fn is_ascii_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

fn trim(s: &[u8]) -> &[u8] {
    let start = s
        .iter()
        .position(|&b| !is_ascii_space(b))
        .unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&b| !is_ascii_space(b))
        .map_or(start, |e| e + 1);
    &s[start..end.max(start)]
}

/// `RotateAtom(base, angle, option)`: the origin `parseOption()` finds
/// under `origin`. Without one the C++ reads `x` and `y` but keeps the
/// bottom-left origin it started with, which is all that is used.
pub fn rotation_option(option: &[u8]) -> Rotation {
    let mut origin = None;
    for token in option.split(|&b| b == b',').filter(|t| !t.is_empty()) {
        let token = trim(token);
        let mut parts: Vec<&[u8]> = token.split(|&b| b == b'=').collect();
        if parts.last().is_some_and(|p| p.is_empty()) {
            parts.pop();
        }
        if parts.len() == 1 && parts[0].is_empty() {
            continue;
        }
        let (key, value) = match parts.len() {
            2 => (trim(parts[0]), trim(parts[1])),
            1 => (trim(parts[0]), &b""[..]),
            _ => continue,
        };
        if key == b"origin" {
            origin = Some(value.to_vec());
        }
    }
    match origin {
        Some(value) => origin_of(&value),
        None => Rotation::BottomLeft,
    }
}

/// `RotateBox::calculateShift()`.
fn rotation_shift(width: f32, height: f32, depth: f32, option: Rotation) -> (f32, f32) {
    match option {
        Rotation::BottomLeft => (0.0, -depth),
        Rotation::BottomRight => (width, -depth),
        Rotation::BottomCenter => (width / 2.0, -depth),
        Rotation::TopLeft => (0.0, height),
        Rotation::TopRight => (width, height),
        Rotation::TopCenter => (width / 2.0, height),
        Rotation::BaselineLeft => (0.0, 0.0),
        Rotation::BaselineRight => (width, 0.0),
        Rotation::BaselineCenter => (width / 2.0, 0.0),
        Rotation::CenterLeft => (0.0, (height - depth) / 2.0),
        Rotation::CenterRight => (width, (height - depth) / 2.0),
        Rotation::CenterCenter => (width / 2.0, (height - depth) / 2.0),
    }
}

/// `RotateAtom::createBox()`.
pub fn rotate(
    ctx: &mut Ctx,
    base: AtomId,
    angle: f32,
    option: Rotation,
    env: &mut Environment,
) -> Result<BoxId> {
    let b = create_box(ctx, base, env)?;
    let node = ctx.boxes.get(b);
    let (x, y) = rotation_shift(node.width, node.height, node.depth, option);
    ctx.boxes.rotate_box(b, angle, x, y)
}

/// `ResizeAtom::createBox()`.
#[allow(clippy::too_many_arguments)]
pub fn resize(
    ctx: &mut Ctx,
    base: AtomId,
    w_unit: Unit,
    w: f32,
    h_unit: Unit,
    h: f32,
    keep_aspect_ratio: bool,
    env: &mut Environment,
) -> Result<BoxId> {
    let b = create_box(ctx, base, env)?;
    if w_unit == Unit::None && h_unit == Unit::None {
        return Ok(b);
    }
    let (bw, bh) = (ctx.boxes.width(b), ctx.boxes.height(b));
    let (sx, sy);
    if w_unit != Unit::None && h_unit != Unit::None {
        let x = w * unit_factor(w_unit, env) / bw;
        let y = h * unit_factor(h_unit, env) / bh;
        if keep_aspect_ratio {
            sx = cmin(x, y);
            sy = sx;
        } else {
            sx = x;
            sy = y;
        }
    } else if w_unit != Unit::None {
        sx = w * unit_factor(w_unit, env) / bw;
        sy = sx;
    } else {
        sx = h * unit_factor(h_unit, env) / bh;
        sy = sx;
    }
    ctx.boxes.scale_box(b, sx, sy)
}

/// `SmallCapAtom::createBox()`.
pub fn small_cap(ctx: &mut Ctx, base: Option<AtomId>, env: &mut Environment) -> Result<BoxId> {
    let prev = env.small_cap;
    env.small_cap = true;
    let b = create_box(ctx, base.ok_or_else(null_atom)?, env);
    env.small_cap = prev;
    b
}

/// The frames `\doublebox`, `\shadowbox` and `\ovalbox` draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    Double,
    Shadow,
    Oval,
}

/// `DoubleFramedAtom`, `ShadowAtom` and `OvalAtom::createBox()`. Shadow and
/// oval frames are made from the plain frame `FBoxAtom` makes, as the C++
/// does, so that frame counts against the budget too.
pub fn frame(ctx: &mut Ctx, base: AtomId, kind: Frame, env: &mut Environment) -> Result<BoxId> {
    let bbase = create_box(ctx, base, env)?;
    let drt = drt_of(env);
    let space = INTERSPACE * unit_factor(Unit::Em, env);
    if kind == Frame::Double {
        let sspace = 1.5 * drt + 0.5 * unit_factor(Unit::Point, env);
        let inner = ctx.boxes.framed_box(
            bbase,
            0.75 * drt,
            space,
            TRANSPARENT,
            TRANSPARENT,
            FrameShape::Square,
        )?;
        return ctx.boxes.framed_box(
            inner,
            1.5 * drt,
            sspace,
            TRANSPARENT,
            TRANSPARENT,
            FrameShape::Square,
        );
    }
    ctx.boxes.framed_box(
        bbase,
        drt,
        space,
        TRANSPARENT,
        TRANSPARENT,
        FrameShape::Square,
    )?;
    let shape = if kind == Frame::Shadow {
        FrameShape::Shadow { rule: drt * 4.0 }
    } else {
        FrameShape::Oval {
            multiplier: ctx.oval_multiplier,
            diameter: ctx.oval_diameter,
        }
    };
    ctx.boxes
        .framed_box(bbase, drt, space, TRANSPARENT, TRANSPARENT, shape)
}
