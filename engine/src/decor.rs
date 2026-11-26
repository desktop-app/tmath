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

/// `RotateBox(box, angle, option)`.
pub fn rotate_box_origin(ctx: &mut Ctx, b: BoxId, angle: f32, option: Rotation) -> Result<BoxId> {
    let node = ctx.boxes.get(b);
    let (x, y) = rotation_shift(node.width, node.height, node.depth, option);
    ctx.boxes.rotate_box(b, angle, x, y)
}

/// `kMaxArrowPieces`.
const MAX_ARROW_PIECES: i32 = 4096;

/// `boundedArrowWidth()`: the shaft never takes more than a fixed number of
/// pieces.
fn bounded_arrow_width(width: f32, base: f32, piece: f32) -> f32 {
    if !base.is_finite() || !piece.is_finite() || piece <= 0.0 {
        return base;
    }
    let bounded = base + piece * MAX_ARROW_PIECES as f32;
    if !bounded.is_finite() {
        return base;
    }
    if width < bounded { width } else { bounded }
}

/// `SmashedAtom(MINUS, "")`: the minus with no height or depth.
fn smashed_minus(ctx: &mut Ctx, env: &mut Environment) -> Result<BoxId> {
    let minus = ctx.symbol("minus")?;
    let b = create_box(ctx, minus, env)?;
    let node = ctx.boxes.get_mut(b);
    node.height = 0.0;
    node.depth = 0.0;
    Ok(b)
}

/// `XLeftRightArrowFactory::create(env, width)`: a double arrow.
fn double_arrow(ctx: &mut Ctx, env: &mut Environment, width: f32) -> Result<BoxId> {
    let left_symbol = ctx.symbol("leftarrow")?;
    let right_symbol = ctx.symbol("rightarrow")?;
    let left = create_box(ctx, left_symbol, env)?;
    let right = create_box(ctx, right_symbol, env)?;
    let left_width = ctx.boxes.width(left);
    let mut swidth = left_width + ctx.boxes.width(right);
    if width < swidth {
        let hb = ctx.boxes.hbox_of(left)?;
        let s = ctx
            .boxes
            .strut(-cmin(swidth - width, left_width), 0.0, 0.0, 0.0)?;
        ctx.boxes.hbox_add(hb, s)?;
        ctx.boxes.hbox_add(hb, right)?;
        return Ok(hb);
    }
    let minu = smashed_minus(ctx, env)?;
    let kern = space_of(ctx, Unit::Mu, -3.4, 0.0, 0.0, env)?;
    let minu_width = ctx.boxes.width(minu);
    let kern_width = ctx.boxes.width(kern);
    let mwidth = minu_width + kern_width;
    swidth += 2.0 * kern_width;
    let width = bounded_arrow_width(width, swidth, mwidth);
    let hb = ctx.boxes.hbox()?;
    let mut w = 0.0f32;
    if mwidth > 0.0 {
        while w < width - swidth - mwidth {
            ctx.boxes.hbox_add(hb, minu)?;
            ctx.boxes.hbox_add(hb, kern)?;
            w += mwidth;
        }
    }
    let scaled = ctx
        .boxes
        .scale_box(minu, (width - swidth - w) / minu_width, 1.0)?;
    ctx.boxes.hbox_add(hb, scaled)?;
    ctx.boxes.hbox_insert(hb, 0, kern)?;
    ctx.boxes.hbox_insert(hb, 0, left)?;
    ctx.boxes.hbox_add(hb, kern)?;
    ctx.boxes.hbox_add(hb, right)?;
    Ok(hb)
}

/// `XLeftRightArrowFactory::create(left, env, width)`: one arrow.
fn single_arrow(ctx: &mut Ctx, left: bool, env: &mut Environment, width: f32) -> Result<BoxId> {
    let symbol = ctx.symbol(if left { "leftarrow" } else { "rightarrow" })?;
    let arr = create_box(ctx, symbol, env)?;
    let node = ctx.boxes.get(arr);
    let (h, d) = (node.height, node.depth);
    let mut swidth = node.width;
    if width <= swidth {
        ctx.boxes.get_mut(arr).depth = d / 2.0;
        return Ok(arr);
    }
    let minu = smashed_minus(ctx, env)?;
    let kern = space_of(ctx, Unit::Mu, -4.0, 0.0, 0.0, env)?;
    let minu_width = ctx.boxes.width(minu);
    let kern_width = ctx.boxes.width(kern);
    let mwidth = minu_width + kern_width;
    swidth += kern_width;
    let width = bounded_arrow_width(width, swidth, mwidth);
    let hb = ctx.boxes.hbox()?;
    let mut w = 0.0f32;
    if mwidth > 0.0 {
        while w < width - swidth - mwidth {
            ctx.boxes.hbox_add(hb, minu)?;
            ctx.boxes.hbox_add(hb, kern)?;
            w += mwidth;
        }
    }
    let sf = (width - swidth - w) / minu_width;
    let s = space_of(ctx, Unit::Mu, -2.0 * sf, 0.0, 0.0, env)?;
    ctx.boxes.hbox_add(hb, s)?;
    let minus = ctx.symbol("minus")?;
    let mb = create_box(ctx, minus, env)?;
    let scaled = ctx.boxes.scale_box(mb, sf, 1.0)?;
    ctx.boxes.hbox_add(hb, scaled)?;
    if left {
        let s = space_of(ctx, Unit::Mu, -3.5, 0.0, 0.0, env)?;
        ctx.boxes.hbox_insert(hb, 0, s)?;
        ctx.boxes.hbox_insert(hb, 0, arr)?;
    } else {
        let s = space_of(ctx, Unit::Mu, -2.0 * sf - 2.0, 0.0, 0.0, env)?;
        ctx.boxes.hbox_add(hb, s)?;
        ctx.boxes.hbox_add(hb, arr)?;
    }
    let node = ctx.boxes.get_mut(hb);
    node.depth = d / 2.0;
    node.height = h;
    Ok(hb)
}

/// `UnderOverArrowAtom::createBox()`.
pub fn under_over_arrow(
    ctx: &mut Ctx,
    base: Option<AtomId>,
    over: bool,
    left: bool,
    double: bool,
    env: &mut Environment,
) -> Result<BoxId> {
    let b = match base {
        Some(a) => create_box(ctx, a, env)?,
        None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
    };
    let sep = crate::env::unit_size(Unit::Mu, 1.0, env);
    let b_width = ctx.boxes.width(b);
    let arrow = if double {
        double_arrow(ctx, env, b_width)?
    } else {
        single_arrow(ctx, left, env, b_width)?
    };
    let arrow_width = ctx.boxes.width(arrow);
    let (bh, bd) = (ctx.boxes.height(b), ctx.boxes.depth(b));
    let vb = ctx.boxes.vbox()?;
    if over {
        ctx.boxes.vbox_add(vb, arrow)?;
        if double {
            let s = ctx.boxes.strut(0.0, -sep, 0.0, 0.0)?;
            ctx.boxes.vbox_add(vb, s)?;
        }
        let centered = ctx.boxes.hbox_aligned(b, arrow_width, Alignment::Center)?;
        ctx.boxes.vbox_add(vb, centered)?;
        let node = ctx.boxes.get_mut(vb);
        let h = node.depth + node.height;
        node.depth = bd;
        node.height = h - bd;
    } else {
        let centered = ctx.boxes.hbox_aligned(b, arrow_width, Alignment::Center)?;
        ctx.boxes.vbox_add(vb, centered)?;
        let s = ctx.boxes.strut(0.0, sep, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vb, s)?;
        ctx.boxes.vbox_add(vb, arrow)?;
        let node = ctx.boxes.get_mut(vb);
        let h = node.depth + node.height;
        node.depth = h - bh;
        node.height = bh;
    }
    Ok(vb)
}

/// `XArrowAtom::createBox()`.
pub fn x_arrow(
    ctx: &mut Ctx,
    over: Option<AtomId>,
    under: Option<AtomId>,
    left: bool,
    env: &mut Environment,
) -> Result<BoxId> {
    let o = match over {
        Some(a) => create_box(ctx, a, &mut env.sup_style())?,
        None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
    };
    let u = match under {
        Some(a) => create_box(ctx, a, &mut env.sub_style())?,
        None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
    };
    let oside = space_of(ctx, Unit::Em, 1.5, 0.0, 0.0, &mut env.sup_style())?;
    let uside = space_of(ctx, Unit::Em, 1.5, 0.0, 0.0, &mut env.sub_style())?;
    let sep = space_of(ctx, Unit::Mu, 0.0, 2.0, 0.0, env)?;
    let width = crate::boxes::cmax(
        ctx.boxes.width(o) + 2.0 * ctx.boxes.width(oside),
        ctx.boxes.width(u) + 2.0 * ctx.boxes.width(uside),
    );
    let arrow = single_arrow(ctx, left, env, width)?;
    let ohb = ctx.boxes.hbox_aligned(o, width, Alignment::Center)?;
    let uhb = ctx.boxes.hbox_aligned(u, width, Alignment::Center)?;
    let vb = ctx.boxes.vbox()?;
    for b in [ohb, sep, arrow, sep, uhb] {
        ctx.boxes.vbox_add(vb, b)?;
    }
    let (sh, sd) = (ctx.boxes.height(sep), ctx.boxes.depth(sep));
    let (uh, ud) = (ctx.boxes.height(uhb), ctx.boxes.depth(uhb));
    let node = ctx.boxes.get_mut(vb);
    let h = node.height + node.depth;
    let d = sh + sd + uh + ud;
    node.depth = d;
    node.height = h - d;
    let vb_width = node.width;
    ctx.boxes
        .hbox_aligned(vb, vb_width + 2.0 * sh, Alignment::Center)
}

/// `OverUnderDelimiter::createBox()`.
#[allow(clippy::too_many_arguments)]
pub fn over_under_delimiter(
    ctx: &mut Ctx,
    base: Option<AtomId>,
    script: Option<AtomId>,
    symbol: &str,
    kern_unit: Unit,
    kern: f32,
    over: bool,
    env: &mut Environment,
) -> Result<BoxId> {
    let mut base = match base {
        None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
        Some(a) => create_box(ctx, a, env)?,
    };
    let base_width = ctx.boxes.width(base);
    let del = crate::layout::delimiter(ctx, symbol, env, base_width)?;
    let del = rotate_box_origin(ctx, del, -90.0, Rotation::CenterCenter)?;
    let mut sb = match script {
        None => None,
        Some(s) => Some(if over {
            create_box(ctx, s, &mut env.sup_style())?
        } else {
            create_box(ctx, s, &mut env.sub_style())?
        }),
    };
    let mut mx = crate::boxes::cmax(base_width, ctx.boxes.width(del));
    if let Some(s) = sb {
        mx = crate::boxes::cmax(mx, ctx.boxes.width(s));
    }
    if mx - base_width > PREC {
        base = ctx.boxes.hbox_aligned(base, mx, Alignment::Center)?;
    }
    let del = ctx.boxes.hbox_aligned(del, mx, Alignment::Center)?;
    if let Some(s) = sb
        && mx - ctx.boxes.width(s) > PREC
    {
        sb = Some(ctx.boxes.hbox_aligned(s, mx, Alignment::Center)?);
    }
    let kb = space_of(ctx, kern_unit, 0.0, kern, 0.0, env)?;
    let kb_height = ctx.boxes.height(kb);
    let (bh, bd) = (ctx.boxes.height(base), ctx.boxes.depth(base));
    let vbox = ctx.boxes.vbox()?;
    if over {
        if let Some(s) = sb {
            ctx.boxes.vbox_add(vbox, s)?;
            if kb_height > PREC {
                ctx.boxes.vbox_add(vbox, kb)?;
            }
        }
        ctx.boxes.vbox_add(vbox, del)?;
        ctx.boxes.vbox_add(vbox, base)?;
        let node = ctx.boxes.get_mut(vbox);
        let total = node.height + node.depth;
        node.height = total - bd;
        node.depth = bd;
    } else {
        ctx.boxes.vbox_add(vbox, base)?;
        ctx.boxes.vbox_add(vbox, del)?;
        if let Some(s) = sb {
            if kb_height > PREC {
                ctx.boxes.vbox_add(vbox, kb)?;
            }
            ctx.boxes.vbox_add(vbox, s)?;
        }
        let node = ctx.boxes.get_mut(vbox);
        let total = node.height + node.depth;
        node.height = bh;
        node.depth = total - bh;
    }
    Ok(vbox)
}

/// `SideSetsAtom::createBox()`: the side scripts take a placeholder with
/// the operator's height and depth as their base, kept on the atoms.
pub fn side_sets(ctx: &mut Ctx, id: AtomId, env: &mut Environment) -> Result<BoxId> {
    let crate::atoms::AtomKind::SideSets { base, left, right } = ctx.get(id).kind else {
        return Err(null_atom());
    };
    let base = match base {
        Some(b) => b,
        None => {
            let m = ctx.atom(crate::atoms::AtomKind::Char {
                c: u16::from(b'M'),
                text_style: "mathnormal",
                math_mode: false,
                text_symbol: false,
            });
            let elements = ctx.new_row_of(Some(m));
            let phantom = ctx.atom(crate::atoms::AtomKind::Phantom {
                elements,
                w: false,
                h: true,
                d: true,
            });
            if let crate::atoms::AtomKind::SideSets { base, .. } = &mut ctx.get_mut(id).kind {
                *base = Some(phantom);
            }
            phantom
        }
    };
    let bb = create_box(ctx, base, env)?;
    let node = ctx.boxes.get(bb);
    let pa = ctx.atom(crate::atoms::AtomKind::Placeholder {
        width: 0.0,
        height: node.height,
        depth: node.depth,
        shift: node.shift,
    });
    for (side, is_left) in [(left, true), (right, false)] {
        if let Some(s) = side
            && let crate::atoms::AtomKind::Scripts { base, align, .. } = &mut ctx.get_mut(s).kind
            && base.is_none()
        {
            *base = Some(pa);
            if is_left {
                *align = Alignment::Right;
            }
        }
    }
    let hb = ctx.boxes.hbox()?;
    if let Some(l) = left {
        let b = create_box(ctx, l, env)?;
        ctx.boxes.hbox_add(hb, b)?;
    }
    ctx.boxes.hbox_add(hb, bb)?;
    if let Some(r) = right {
        let b = create_box(ctx, r, env)?;
        ctx.boxes.hbox_add(hb, b)?;
    }
    Ok(hb)
}

/// A phantom `M`, the base `\sideset` uses when it has none.
fn phantom_m(ctx: &mut Ctx) -> AtomId {
    let m = ctx.atom(crate::atoms::AtomKind::Char {
        c: u16::from(b'M'),
        text_style: "mathnormal",
        math_mode: false,
        text_symbol: false,
    });
    let elements = ctx.new_row_of(Some(m));
    ctx.atom(crate::atoms::AtomKind::Phantom {
        elements,
        w: false,
        h: true,
        d: true,
    })
}

/// `BigOperatorAtom::createSideSets()`: limits over and under a `\sideset`,
/// centered on its operator.
pub fn side_sets_operator(
    ctx: &mut Ctx,
    sideset: AtomId,
    under: Option<AtomId>,
    over: Option<AtomId>,
    env: &mut Environment,
) -> Result<BoxId> {
    use crate::atoms::{AtomKind, AtomNode, LimitsType};
    use crate::enums::AtomType;
    let AtomKind::SideSets { base, left, right } = ctx.get(sideset).kind else {
        return Err(null_atom());
    };
    let sb = match base {
        Some(b) => b,
        None => phantom_m(ctx),
    };
    let opbox = create_box(ctx, sb, env)?;
    let node = ctx.boxes.get(opbox);
    let (op_width, op_height, op_depth, op_shift) =
        (node.width, node.height, node.depth, node.shift);
    let mut placeholder = AtomNode::new(AtomKind::Placeholder {
        width: 0.0,
        height: op_height,
        depth: op_depth,
        shift: op_shift,
    });
    placeholder.limits = LimitsType::NoLimits;
    placeholder.atom_type = AtomType::BigOperator;
    let pa = ctx.add(placeholder);
    for (side, is_left) in [(left, true), (right, false)] {
        if let Some(s) = side
            && let AtomKind::Scripts { base, align, .. } = &mut ctx.get_mut(s).kind
            && base.is_none()
        {
            *base = Some(pa);
            if is_left {
                *align = Alignment::Right;
            }
        }
    }
    let y = ctx.boxes.hbox()?;
    let mut limits_shift = 0.0;
    if let Some(l) = left {
        let lb = create_box(ctx, l, env)?;
        ctx.boxes.hbox_add(y, lb)?;
        limits_shift = ctx.boxes.width(lb) + op_width / 2.0;
    }
    ctx.boxes.hbox_add(y, opbox)?;
    if let Some(r) = right {
        let rb = create_box(ctx, r, env)?;
        ctx.boxes.hbox_add(y, rb)?;
    }
    let style = env.style;
    let mut delta = 0.0;
    if ctx.get(sb).atom_type == AtomType::BigOperator
        && let AtomKind::Symbol { name, .. } = ctx.get(sb).kind
    {
        delta = symbol_char(env, name, style)?.metrics.italic;
    }
    let x = match over {
        Some(o) => Some(create_box(ctx, o, &mut env.sup_style())?),
        None => None,
    };
    let z = match under {
        Some(u) => Some(create_box(ctx, u, &mut env.sub_style())?),
        None => None,
    };
    let vbox = ctx.boxes.vbox()?;
    let settings = env.settings;
    let font = env.font;
    let param = |name: &str| font.style_param(&settings, name, style);
    let bigop5 = param("bigopspacing5");
    let mut kern = 0.0;
    if let Some(x) = x {
        let s = ctx.boxes.strut(0.0, bigop5, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vbox, s)?;
        let x_width = ctx.boxes.width(x);
        ctx.boxes.get_mut(x).shift = limits_shift - x_width / 2.0 + delta / 2.0;
        ctx.boxes.vbox_add(vbox, x)?;
        kern = crate::boxes::cmax(
            param("bigopspacing1"),
            param("bigopspacing3") - ctx.boxes.depth(x),
        );
        let s = ctx.boxes.strut(0.0, kern, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vbox, s)?;
    }
    ctx.boxes.vbox_add(vbox, y)?;
    if let Some(z) = z {
        let k = crate::boxes::cmax(
            param("bigopspacing2"),
            param("bigopspacing4") - ctx.boxes.height(z),
        );
        let s = ctx.boxes.strut(0.0, k, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vbox, s)?;
        let z_width = ctx.boxes.width(z);
        ctx.boxes.get_mut(z).shift = limits_shift - z_width / 2.0 - delta / 2.0;
        ctx.boxes.vbox_add(vbox, z)?;
        let s = ctx.boxes.strut(0.0, bigop5, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vbox, s)?;
    }
    let mut h = ctx.boxes.height(y);
    if let Some(x) = x {
        h += bigop5 + kern + ctx.boxes.height(x) + ctx.boxes.depth(x);
    }
    let node = ctx.boxes.get_mut(vbox);
    let total = node.height + node.depth;
    node.height = h;
    node.depth = total - h;
    Ok(vbox)
}
