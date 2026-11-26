//! `Atom::createBox()` for every atom kind: the layout pass.
//!
//! Each function follows its C++ counterpart statement by statement. The
//! order boxes are made in, the order environments are derived and the order
//! float operations are done in are all kept, because each can show in the
//! output.

use crate::atoms::{
    AtomId, AtomKind, Ctx, Dummy, FractionData, LimitsType, SpaceData, UnderOverData,
};
use crate::boxes::{Alignment, BoxId, cmax};
use crate::draw::TextFont;
use crate::enums::{AtomType, SpaceType, Unit};
use crate::env::{Environment, NO_FONT, unit_factor, unit_size};
use crate::error::{Error, Result};
use crate::font::{Char, PREC, TexStyle};
use crate::glue;
use crate::tables::{self, CharFont};

const PLAIN: i32 = 0;
const BOLD: i32 = 1;
const ITALIC: i32 = 2;

/// `kMaxScriptsLayoutDepth`.
const MAX_SCRIPTS_LAYOUT_DEPTH: i32 = 96;

/// `ScriptsLayoutGuard`: chains nested in wrappers add up, so scripts and
/// big operator layout counts its own depth.
fn scripts_layout_guard<T>(ctx: &mut Ctx, f: impl FnOnce(&mut Ctx) -> Result<T>) -> Result<T> {
    ctx.scripts_layout_depth += 1;
    if ctx.scripts_layout_depth > MAX_SCRIPTS_LAYOUT_DEPTH {
        ctx.scripts_layout_depth -= 1;
        return Err(Error::parse("Formula nesting is too deep!"));
    }
    let result = f(ctx);
    ctx.scripts_layout_depth -= 1;
    result
}

pub(crate) fn null_atom() -> Error {
    Error::invalid_state("A required atom is missing.")
}

pub(crate) fn symbol_char(env: &Environment, name: &str, style: TexStyle) -> Result<Char> {
    env.font
        .char_of_symbol(&env.settings, name, style)
        .map_err(|_| Error::symbol_mapping_not_found(name))
}

pub(crate) fn text_style_char(
    env: &Environment,
    c: u16,
    text_style: &str,
    style: TexStyle,
) -> Result<Char> {
    env.font
        .char_of_text_style(&env.settings, c, text_style, style)
        .map_err(|_| Error::text_style_mapping_not_found(text_style))
}

fn default_char(env: &Environment, c: u16, style: TexStyle) -> Result<Char> {
    let kind = match c {
        0x30..=0x39 => 0,
        0x61..=0x7A => 2,
        _ => 1,
    };
    text_style_char(env, c, tables::TEXT_STYLE_DEFAULTS[kind], style)
}

/// `CharAtom::getChar()`.
fn char_atom_char(
    env: &Environment,
    c: u16,
    style: TexStyle,
    small_cap: bool,
    text_style: &str,
) -> Result<Char> {
    let mut chr = c;
    if small_cap && c.is_ascii_lowercase_unit() {
        chr = c - 0x20;
    }
    if text_style.is_empty() {
        default_char(env, chr, style)
    } else {
        text_style_char(env, chr, text_style, style)
    }
}

trait AsciiUnit {
    fn is_ascii_lowercase_unit(&self) -> bool;
    fn is_ascii_digit_unit(&self) -> bool;
}

impl AsciiUnit for u16 {
    fn is_ascii_lowercase_unit(&self) -> bool {
        (0x61..=0x7A).contains(self)
    }

    fn is_ascii_digit_unit(&self) -> bool {
        (0x30..=0x39).contains(self)
    }
}

/// `CharSymbol::getCharFont()`.
pub fn char_font_of(ctx: &Ctx, id: AtomId, env: &Environment) -> Result<CharFont> {
    match &ctx.get(id).kind {
        AtomKind::FixedChar { cf, .. } => Ok(*cf),
        AtomKind::Symbol { name, .. } => Ok(symbol_char(env, name, TexStyle::Display)?.char_font()),
        AtomKind::Char { c, text_style, .. } => {
            Ok(char_atom_char(env, *c, TexStyle::Display, false, text_style)?.char_font())
        }
        _ => Err(null_atom()),
    }
}

fn is_text_symbol(ctx: &Ctx, id: AtomId) -> bool {
    match &ctx.get(id).kind {
        AtomKind::Char { text_symbol, .. }
        | AtomKind::Symbol { text_symbol, .. }
        | AtomKind::FixedChar { text_symbol, .. } => *text_symbol,
        _ => false,
    }
}

fn set_text_symbol(ctx: &mut Ctx, id: AtomId, value: bool) {
    match &mut ctx.get_mut(id).kind {
        AtomKind::Char { text_symbol, .. }
        | AtomKind::Symbol { text_symbol, .. }
        | AtomKind::FixedChar { text_symbol, .. } => *text_symbol = value,
        _ => {}
    }
}

/// `Atom::createBox()`.
pub fn create_box(ctx: &mut Ctx, id: AtomId, env: &mut Environment) -> Result<BoxId> {
    let kind = ctx.get(id).kind.clone();
    match kind {
        AtomKind::Empty | AtomKind::BreakMark => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0),
        AtomKind::Placeholder {
            width,
            height,
            depth,
            shift,
        } => ctx.boxes.strut(width, height, depth, shift),
        AtomKind::Char {
            c,
            text_style,
            math_mode: _,
            text_symbol: _,
        } => char_box(ctx, c, text_style, env),
        AtomKind::Symbol { name, unicode, .. } => symbol_box(ctx, id, name, unicode, env),
        AtomKind::FixedChar { cf, .. } => {
            let c = env.font.char_of(&env.settings, cf, env.style);
            ctx.boxes.char_box(&c)
        }
        AtomKind::Row { .. } => row_box(ctx, id, env),
        AtomKind::Space(data) => space_box(ctx, &data, env),
        AtomKind::Scripts { .. } => scripts_layout_guard(ctx, |ctx| scripts_box(ctx, id, env)),
        AtomKind::CumulativeScripts { base, sub, sup } => {
            let temp = new_scripts(ctx, base, Some(sub), Some(sup), Alignment::Left)?;
            scripts_layout_guard(ctx, |ctx| scripts_box(ctx, temp, env))
        }
        AtomKind::BigOperator { .. } => {
            scripts_layout_guard(ctx, |ctx| big_operator_box(ctx, id, env))
        }
        AtomKind::Fraction(data) => fraction_box(ctx, &data, env),
        AtomKind::Style { style, at } => {
            let previous = env.style;
            env.style = style;
            let result = create_box(ctx, at, env);
            env.style = previous;
            result
        }
        AtomKind::Typed { atom, .. } => create_box(ctx, atom, env),
        AtomKind::Math { style, base } => {
            let mut font = env.font;
            font.roman = false;
            let mut e = env.copy_with_font(font);
            if style > e.style {
                e.style = style;
            }
            create_box(ctx, base.ok_or_else(null_atom)?, &mut e)
        }
        AtomKind::Roman { base } => match base {
            None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0),
            Some(base) => {
                let mut font = env.font;
                font.roman = true;
                let mut e = env.copy_with_font(font);
                create_box(ctx, base, &mut e)
            }
        },
        AtomKind::Bold { base } => match base {
            None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0),
            Some(base) => {
                let mut font = env.font;
                font.bold = true;
                let mut e = env.copy_with_font(font);
                create_box(ctx, base, &mut e)
            }
        },
        AtomKind::It { base } => match base {
            None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0),
            Some(base) => {
                let mut font = env.font;
                font.it = true;
                let mut e = env.copy_with_font(font);
                create_box(ctx, base, &mut e)
            }
        },
        AtomKind::Ss { base } => {
            let previous = env.font.ss;
            env.font.ss = true;
            let result = create_box(ctx, base.ok_or_else(null_atom)?, env);
            env.font.ss = previous;
            result
        }
        AtomKind::Tt { base } => {
            let previous = env.font.tt;
            env.font.tt = true;
            let result = create_box(ctx, base.ok_or_else(null_atom)?, env);
            env.font.tt = previous;
            result
        }
        AtomKind::TextStyle { style, at } => {
            let previous = env.text_style;
            env.text_style = style;
            let result = create_box(ctx, at.ok_or_else(null_atom)?, env);
            env.text_style = previous;
            result
        }
        AtomKind::Color {
            background,
            color,
            elements,
        } => {
            let b = create_box(ctx, elements, env)?;
            ctx.boxes.color_box(b, color, background)
        }
        AtomKind::Phantom { elements, w, h, d } => {
            let res = create_box(ctx, elements, env)?;
            let r = ctx.boxes.get(res);
            let (rw, rh, rd, rs) = (r.width, r.height, r.depth, r.shift);
            ctx.boxes.strut(
                if w { rw } else { 0.0 },
                if h { rh } else { 0.0 },
                if d { rd } else { 0.0 },
                rs,
            )
        }
        AtomKind::Smashed { atom, h, d } => {
            let b = create_box(ctx, atom, env)?;
            let node = ctx.boxes.get_mut(b);
            if h {
                node.height = 0.0;
            }
            if d {
                node.depth = 0.0;
            }
            Ok(b)
        }
        AtomKind::Scale { base, sx, sy } => {
            let b = create_box(ctx, base, env)?;
            ctx.boxes.scale_box(b, sx, sy)
        }
        AtomKind::TextRendering { text, style, infos } => {
            text_rendering_box(ctx, &text, style, infos, env)
        }
        AtomKind::UnderScore => {
            let drt = env
                .font
                .style_param(&env.settings, "defaultrulethickness", env.style);
            let s = space_of(ctx, Unit::Em, 0.06, 0.0, 0.0, env)?;
            let hb = ctx.boxes.hbox_of(s)?;
            let w = space_of(ctx, Unit::Em, 0.7, 0.0, 0.0, env)?;
            let width = ctx.boxes.width(w);
            let rule = ctx
                .boxes
                .rule(drt, width, 0.0, crate::draw::TRANSPARENT, true)?;
            ctx.boxes.hbox_add(hb, rule)?;
            Ok(hb)
        }
        AtomKind::Middle { r#box, .. } => Ok(r#box),
        AtomKind::UnderOver(data) => under_over_box(ctx, &data, env),
        AtomKind::BigDelimiter { delim, size } => {
            let b = delimiter_sized(ctx, delim, env, size)?;
            let hb = ctx.boxes.hbox()?;
            let h = ctx.boxes.height(b);
            let total = h + ctx.boxes.depth(b);
            let axis = env.font.style_param(&env.settings, "axisheight", env.style);
            ctx.boxes.get_mut(b).shift = -total / 2.0 + h - axis;
            ctx.boxes.hbox_add(hb, b)?;
            Ok(hb)
        }
        AtomKind::Fenced {
            base,
            left,
            right,
            middle,
        } => fenced_box(ctx, base, left, right, &middle, env),
        AtomKind::NthRoot { base, root } => nth_root_box(ctx, base, root, env),
        AtomKind::Accented {
            accent,
            acc,
            change_size,
            base,
            underbase,
        } => accented_box(ctx, accent, acc, change_size, base, underbase, env),
        AtomKind::Rule {
            w_unit,
            w,
            h_unit,
            h,
            r_unit,
            r,
        } => {
            let w = unit_factor(w_unit, env) * w;
            let h = unit_factor(h_unit, env) * h;
            let r = unit_factor(r_unit, env) * r;
            ctx.boxes.rule(h, w, r, crate::draw::TRANSPARENT, true)
        }
        AtomKind::Raise {
            base,
            r_unit,
            r,
            h_unit,
            h,
            d_unit,
            d,
        } => {
            let b = create_box(ctx, base.ok_or_else(null_atom)?, env)?;
            ctx.boxes.get_mut(b).shift = if r_unit == Unit::None {
                0.0
            } else {
                unit_size(r_unit, -r, env)
            };
            if h_unit == Unit::None {
                return Ok(b);
            }
            let hbox = ctx.boxes.hbox_of(b)?;
            let height = unit_size(h_unit, h, env);
            let depth = if d_unit == Unit::None {
                0.0
            } else {
                unit_size(d_unit, d, env)
            };
            let node = ctx.boxes.get_mut(hbox);
            node.height = height;
            node.depth = depth;
            Ok(hbox)
        }
        AtomKind::VRow {
            elements,
            raise,
            add_interline,
            valign,
            halign,
        } => vrow_box(ctx, &elements, &raise, add_interline, valign, halign, env),
        AtomKind::UnderOverArrow {
            base,
            over,
            left,
            double,
        } => crate::decor::under_over_arrow(ctx, base, over, left, double, env),
        AtomKind::XArrow { over, under, left } => {
            crate::decor::x_arrow(ctx, over, under, left, env)
        }
        AtomKind::OverUnderDelimiter {
            base,
            script,
            symbol,
            kern_unit,
            kern,
            over,
        } => crate::decor::over_under_delimiter(
            ctx, base, script, symbol, kern_unit, kern, over, env,
        ),
        AtomKind::SideSets { .. } => crate::decor::side_sets(ctx, id, env),
        AtomKind::Vdots => crate::decor::vdots(ctx, env),
        AtomKind::DiagonalDots { iddots } => {
            let aligns = if iddots {
                [Alignment::Right, Alignment::Center, Alignment::Left]
            } else {
                [Alignment::Left, Alignment::Center, Alignment::Right]
            };
            crate::decor::diagonal_dots(ctx, aligns, env)
        }
        AtomKind::VCentered { at } => crate::decor::vcentered(ctx, at, env),
        AtomKind::MonoScale { base, factor } => crate::decor::mono_scale(ctx, base, factor, env),
        AtomKind::IJ { upper } => crate::decor::ij(ctx, upper, env),
        AtomKind::LCaron { upper } => crate::decor::lcaron(ctx, upper, env),
        AtomKind::TCaron => crate::decor::tcaron(ctx, env),
        AtomKind::TStroke { upper } => crate::decor::tstroke(ctx, upper, env),
        AtomKind::UnderMark { base, ogonek } => crate::decor::under_mark(ctx, base, ogonek, env),
        AtomKind::Laped { at, kind } => crate::decor::laped(ctx, at, kind, env),
        AtomKind::Overlined { base } => crate::decor::overlined(ctx, base, env),
        AtomKind::Underlined { base } => crate::decor::underlined(ctx, base, env),
        AtomKind::StrikeThrough { at } => crate::decor::strike_through(ctx, at, env),
        AtomKind::Cancel { base, kind } => crate::decor::cancel(ctx, base, kind, env),
        AtomKind::Reflect { base } => {
            let b = create_box(ctx, base, env)?;
            ctx.boxes.reflect_box(b)
        }
        AtomKind::Rotate {
            base,
            angle,
            option,
        } => crate::decor::rotate(ctx, base, angle, option, env),
        AtomKind::Resize {
            base,
            w_unit,
            w,
            h_unit,
            h,
            keep_aspect_ratio,
        } => crate::decor::resize(ctx, base, w_unit, w, h_unit, h, keep_aspect_ratio, env),
        AtomKind::SmallCap { base } => crate::decor::small_cap(ctx, base, env),
        AtomKind::Frame { base, kind } => crate::decor::frame(ctx, base, kind, env),
        AtomKind::Matrix(m) => crate::matrix::matrix_box(ctx, &m, env),
        AtomKind::Multline {
            array,
            line_type,
            partial,
        } => crate::matrix::multline_box(ctx, &array, line_type, partial, env),
        AtomKind::Hline {
            width,
            shift,
            color,
        } => crate::matrix::hline_box(ctx, width, shift, color, env),
        AtomKind::CellSpecifier { .. } => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0),
        AtomKind::Multicolumn(data) => crate::matrix::multicolumn_box(ctx, data, env),
        AtomKind::MultiRow { rows, .. } => crate::matrix::multirow_box(ctx, rows, env),
        AtomKind::FBox { base, bg, line } => {
            let b = create_box(ctx, base, env)?;
            let drt = env
                .font
                .style_param(&env.settings, "defaultrulethickness", env.style);
            let space = 0.65 * unit_factor(Unit::Em, env);
            if crate::draw::is_transparent(bg) {
                let t = crate::draw::TRANSPARENT;
                return ctx
                    .boxes
                    .framed_box(b, drt, space, t, t, crate::boxes::FrameShape::Square);
            }
            ctx.boxes
                .framed_box(b, drt, space, line, bg, crate::boxes::FrameShape::Square)
        }
        AtomKind::TextCircled { at } => {
            let bigcirc = ctx.symbol("bigcirc")?;
            let circle = create_box(ctx, bigcirc, env)?;
            ctx.boxes.get_mut(circle).shift = -0.07 * unit_factor(Unit::Ex, env);
            let b = create_box(ctx, at, env)?;
            let w = ctx.boxes.width(circle);
            let hb = ctx.boxes.hbox_aligned(b, w, Alignment::Center)?;
            let back = ctx.boxes.strut(-ctx.boxes.width(hb), 0.0, 0.0, 0.0)?;
            ctx.boxes.hbox_add(hb, back)?;
            ctx.boxes.hbox_add(hb, circle)?;
            Ok(hb)
        }
    }
}

fn char_box(
    ctx: &mut Ctx,
    c: u16,
    own_style: &'static str,
    env: &mut Environment,
) -> Result<BoxId> {
    let text_style = if own_style.is_empty() {
        env.text_style
    } else {
        own_style
    };
    let small_cap = env.small_cap;
    let ch = char_atom_char(env, c, env.style, small_cap, text_style)?;
    let b = ctx.boxes.char_box(&ch)?;
    if small_cap && c.is_ascii_lowercase_unit() {
        return ctx.boxes.scale_box(b, 0.8, 0.8);
    }
    Ok(b)
}

fn symbol_box(
    ctx: &mut Ctx,
    id: AtomId,
    name: &'static str,
    unicode: u16,
    env: &mut Environment,
) -> Result<BoxId> {
    let style = env.style;
    let mut c = symbol_char(env, name, style)?;
    let mut cb = ctx.boxes.char_box(&c)?;
    if env.small_cap && unicode != 0 && unicode.is_ascii_lowercase_unit() {
        let upper = i32::from(unicode - 0x20);
        if let Ok(index) = tables::FORMULA_TEXT_MAPPINGS.binary_search_by(|e| e.0.cmp(&upper)) {
            let mapped = tables::FORMULA_TEXT_MAPPINGS[index].1;
            if let Ok(ch) = env.font.char_of_symbol(&env.settings, mapped, style) {
                let cx = ctx.boxes.char_box(&ch)?;
                cb = ctx.boxes.scale_box(cx, 0.8, 0.8)?;
            }
        }
    }
    if ctx.get(id).atom_type == AtomType::BigOperator {
        if style < TexStyle::Text && env.font.has_next_larger(&c) {
            c = env.font.next_larger(&env.settings, &c, style);
        }
        let cb = ctx.boxes.char_box(&c)?;
        let axis = env.font.style_param(&env.settings, "axisheight", style);
        let node = ctx.boxes.get_mut(cb);
        node.shift = -(node.height + node.depth) / 2.0 - axis;
        let delta = c.metrics.italic;
        let hb = ctx.boxes.hbox_of(cb)?;
        if delta > PREC {
            let s = ctx.boxes.strut(delta, 0.0, 0.0, 0.0)?;
            ctx.boxes.hbox_add(hb, s)?;
        }
        return Ok(hb);
    }
    Ok(cb)
}

pub(crate) fn space_box(ctx: &mut Ctx, data: &SpaceData, env: &mut Environment) -> Result<BoxId> {
    if !data.blank_space {
        let w = data.width * unit_factor(data.w_unit, env);
        let h = data.height * unit_factor(data.h_unit, env);
        let d = data.depth * unit_factor(data.d_unit, env);
        return ctx.boxes.strut(w, h, d, 0.0);
    }
    if data.blank_type == SpaceType::None {
        return ctx.boxes.strut(env.space(), 0.0, 0.0, 0.0);
    }
    let g = glue::skip(data.blank_type, env);
    ctx.boxes.glue(g.width, g.stretch, g.shrink)
}

/// `SpaceAtom(unit, w, h, d).createBox(env)` for the temporaries the C++
/// builds on the stack.
pub(crate) fn space_of(
    ctx: &mut Ctx,
    unit: Unit,
    w: f32,
    h: f32,
    d: f32,
    env: &mut Environment,
) -> Result<BoxId> {
    space_box(ctx, &SpaceData::sized(unit, w, h, d), env)
}

fn text_rendering_box(
    ctx: &mut Ctx,
    text: &[u16],
    style: i32,
    infos: Option<(&'static str, &'static str)>,
    env: &mut Environment,
) -> Result<BoxId> {
    let size = env.settings.size_factor(env.style);
    let Some((sansserif, serif)) = infos else {
        let font = ctx.text_font.clone();
        return ctx
            .boxes
            .text_rendering(text, style, size, &font, &mut *ctx.measure);
    };
    let tf = env.font;
    let mut kind = if tf.it { ITALIC } else { PLAIN };
    kind |= if tf.bold { BOLD } else { 0 };
    let family = if tf.ss {
        if sansserif.is_empty() {
            serif
        } else {
            sansserif
        }
    } else if serif.is_empty() {
        sansserif
    } else {
        serif
    };
    let font = TextFont::new(family, PLAIN, 10.0);
    ctx.boxes
        .text_rendering(text, kind, size, &font, &mut *ctx.measure)
}

impl Dummy {
    pub fn new(atom: AtomId) -> Self {
        Self {
            atom,
            atom_type: AtomType::None,
            text_symbol: false,
        }
    }

    fn left_type(&self, ctx: &Ctx) -> AtomType {
        if self.atom_type != AtomType::None {
            self.atom_type
        } else {
            ctx.left_type(self.atom)
        }
    }

    fn right_type(&self, ctx: &Ctx) -> AtomType {
        if self.atom_type != AtomType::None {
            self.atom_type
        } else {
            ctx.right_type(self.atom)
        }
    }

    fn is_kern(&self, ctx: &Ctx) -> bool {
        matches!(ctx.get(self.atom).kind, AtomKind::Space(_))
    }

    fn create_box(&self, ctx: &mut Ctx, env: &mut Environment) -> Result<BoxId> {
        if !self.text_symbol {
            return create_box(ctx, self.atom, env);
        }
        set_text_symbol(ctx, self.atom, true);
        let result = create_box(ctx, self.atom, env);
        set_text_symbol(ctx, self.atom, false);
        result
    }
}

fn in_bin_set(t: AtomType) -> bool {
    matches!(
        t,
        AtomType::BinaryOperator
            | AtomType::BigOperator
            | AtomType::Relation
            | AtomType::Opening
            | AtomType::Punctuation
    )
}

fn in_lig_kern_set(t: AtomType) -> bool {
    matches!(
        t,
        AtomType::Ordinary
            | AtomType::BigOperator
            | AtomType::BinaryOperator
            | AtomType::Relation
            | AtomType::Opening
            | AtomType::Closing
            | AtomType::Punctuation
    )
}

/// `RowAtom::changeToOrd()`.
fn change_to_ord(ctx: &Ctx, cur: &mut Dummy, prev: Option<&Dummy>, next: Option<AtomId>) {
    let t = cur.left_type(ctx);
    if t == AtomType::BinaryOperator
        && (prev.is_none_or(|p| in_bin_set(p.right_type(ctx))) || next.is_none())
    {
        cur.atom_type = AtomType::Ordinary;
    } else if let Some(next) = next
        && cur.right_type(ctx) == AtomType::BinaryOperator
    {
        let next_type = ctx.left_type(next);
        if next_type == AtomType::Relation
            || next_type == AtomType::Closing
            || next_type == AtomType::Punctuation
        {
            cur.atom_type = AtomType::Ordinary;
        }
    }
}

fn row_element(ctx: &Ctx, row: AtomId, index: i32) -> AtomId {
    ctx.row_elements(row)[index as usize]
}

fn row_box(ctx: &mut Ctx, id: AtomId, env: &mut Environment) -> Result<BoxId> {
    let mut previous = match &mut ctx.get_mut(id).kind {
        AtomKind::Row { previous, .. } => previous.take(),
        _ => None,
    };
    let result = row_box_inner(ctx, id, env, &mut previous);
    if let AtomKind::Row { previous, .. } = &mut ctx.get_mut(id).kind {
        *previous = None;
    }
    result
}

fn row_box_inner(
    ctx: &mut Ctx,
    id: AtomId,
    env: &mut Environment,
    previous: &mut Option<Dummy>,
) -> Result<BoxId> {
    let breakable = match &ctx.get(id).kind {
        AtomKind::Row { breakable, .. } => *breakable,
        _ => true,
    };
    let hbox = ctx.boxes.hbox()?;
    let end = ctx.row_elements(id).len() as i32 - 1;
    let mut i = -1;
    while i < end {
        i += 1;
        let mut at = row_element(ctx, id, i);
        let mut mark_added = false;
        let mut is_mark = matches!(ctx.get(at).kind, AtomKind::BreakMark);
        while is_mark {
            mark_added = true;
            if i < end {
                i += 1;
                at = row_element(ctx, id, i);
                is_mark = matches!(ctx.get(at).kind, AtomKind::BreakMark);
            } else {
                break;
            }
        }
        let mut atom = Dummy::new(at);
        let next_atom = if i < end {
            Some(row_element(ctx, id, i + 1))
        } else {
            None
        };
        change_to_ord(ctx, &mut atom, previous.as_ref(), next_atom);

        let mut kern = 0.0f32;
        while i < end
            && atom.right_type(ctx) == AtomType::Ordinary
            && ctx.get(atom.atom).is_char_symbol()
        {
            i += 1;
            let next = row_element(ctx, id, i);
            if ctx.get(next).is_char_symbol() && in_lig_kern_set(ctx.left_type(next)) {
                atom.text_symbol = true;
                let l = char_font_of(ctx, atom.atom, env)?;
                let r = char_font_of(ctx, next, env)?;
                match env.font.ligature(l, r) {
                    None => {
                        kern = env.font.kern(&env.settings, l, r, env.style);
                        i -= 1;
                        break;
                    }
                    Some(lig) => {
                        let fixed = ctx.atom(AtomKind::FixedChar {
                            cf: lig,
                            text_symbol: false,
                        });
                        atom = Dummy::new(fixed);
                    }
                }
            } else {
                i -= 1;
                break;
            }
        }

        if i != 0
            && let Some(prev) = previous.as_ref()
            && !prev.is_kern(ctx)
            && !atom.is_kern(ctx)
        {
            let g = glue::between(prev.right_type(ctx), atom.left_type(ctx), env);
            let b = ctx.boxes.glue(g.width, g.stretch, g.shrink)?;
            ctx.boxes.hbox_add(hbox, b)?;
        }

        ctx.set_previous_atom(atom.atom, *previous);
        let b = atom.create_box(ctx, env)?;
        if ctx.boxes.get(b).is_char()
            && !matches!(
                ctx.get(atom.atom).kind,
                AtomKind::Char {
                    math_mode: true,
                    ..
                }
            )
            && next_atom.is_some_and(|n| ctx.get(n).is_char_symbol())
        {
            ctx.boxes.add_italic_correction_to_width(b);
        }
        if breakable {
            let position = ctx.boxes.group_len(hbox) as i32;
            if ctx.break_everywhere {
                ctx.boxes.hbox_add_break_position(hbox, position);
            } else {
                let digit = matches!(
                    ctx.get(at).kind,
                    AtomKind::Char { c, .. } if c.is_ascii_digit_unit()
                );
                if mark_added || digit {
                    ctx.boxes.hbox_add_break_position(hbox, position);
                }
            }
        }
        ctx.boxes.hbox_add(hbox, b)?;
        env.set_last_font_id(ctx.boxes.last_font_id(b));
        if kern.abs() > PREC {
            let s = ctx.boxes.strut(kern, 0.0, 0.0, 0.0)?;
            ctx.boxes.hbox_add(hbox, s)?;
        }
        if !atom.is_kern(ctx) {
            *previous = Some(atom);
        }
    }
    Ok(hbox)
}

/// `ScriptsAtom(base, sub, sup)` / `(…, left)`.
pub fn new_scripts(
    ctx: &mut Ctx,
    base: Option<AtomId>,
    sub: Option<AtomId>,
    sup: Option<AtomId>,
    align: Alignment,
) -> Result<AtomId> {
    let id = ctx.atom(AtomKind::Scripts {
        base,
        sub,
        sup,
        align,
    });
    ctx.inherit_wrap_depth(id, base)?;
    Ok(id)
}

/// `UnderOverAtom(base, script, unit, space, small, over)`.
pub fn new_under_over_one(
    ctx: &mut Ctx,
    base: Option<AtomId>,
    script: Option<AtomId>,
    unit: Unit,
    space: f32,
    small: bool,
    over: bool,
) -> AtomId {
    let data = if over {
        UnderOverData {
            base,
            under: None,
            under_unit: Unit::Em,
            under_space: 0.0,
            under_small: false,
            over: script,
            over_unit: unit,
            over_space: space,
            over_small: small,
        }
    } else {
        UnderOverData {
            base,
            under: script,
            under_unit: unit,
            under_space: space,
            under_small: small,
            over: None,
            over_unit: Unit::Em,
            over_space: 0.0,
            over_small: false,
        }
    };
    ctx.atom(AtomKind::UnderOver(Box::new(data)))
}

/// `UnderOverAtom::changeWidth()`.
fn change_width(ctx: &mut Ctx, b: Option<BoxId>, max_width: f32) -> Result<Option<BoxId>> {
    match b {
        Some(b) if (max_width - ctx.boxes.width(b)).abs() > PREC => Ok(Some(
            ctx.boxes.hbox_aligned(b, max_width, Alignment::Center)?,
        )),
        _ => Ok(b),
    }
}

fn under_over_box(ctx: &mut Ctx, data: &UnderOverData, env: &mut Environment) -> Result<BoxId> {
    let b = match data.base {
        None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
        Some(base) => create_box(ctx, base, env)?,
    };
    let mut o = None;
    let mut u = None;
    let mut mx = ctx.boxes.width(b);
    if let Some(over) = data.over {
        let ob = if data.over_small {
            create_box(ctx, over, &mut env.sub_style())?
        } else {
            create_box(ctx, over, env)?
        };
        mx = cmax(mx, ctx.boxes.width(ob));
        o = Some(ob);
    }
    if let Some(under) = data.under {
        let ub = if data.under_small {
            create_box(ctx, under, &mut env.sub_style())?
        } else {
            create_box(ctx, under, env)?
        };
        mx = cmax(mx, ctx.boxes.width(ub));
        u = Some(ub);
    }
    let vbox = ctx.boxes.vbox()?;
    env.set_last_font_id(ctx.boxes.last_font_id(b));
    if data.over.is_some() {
        let o = change_width(ctx, o, mx)?.ok_or_else(null_atom)?;
        ctx.boxes.vbox_add(vbox, o)?;
        let s = space_of(ctx, data.over_unit, 0.0, data.over_space, 0.0, env)?;
        ctx.boxes.vbox_add(vbox, s)?;
    }
    let c = change_width(ctx, Some(b), mx)?.ok_or_else(null_atom)?;
    ctx.boxes.vbox_add(vbox, c)?;
    let v = ctx.boxes.get(vbox);
    let h = v.height + v.depth - ctx.boxes.depth(c);
    if data.under.is_some() {
        let s = space_of(ctx, data.under_unit, 0.0, data.under_space, 0.0, env)?;
        ctx.boxes.vbox_add(vbox, s)?;
        let u = change_width(ctx, u, mx)?.ok_or_else(null_atom)?;
        ctx.boxes.vbox_add(vbox, u)?;
    }
    let v = ctx.boxes.get_mut(vbox);
    v.depth = v.height + v.depth - h;
    v.height = h;
    Ok(vbox)
}

fn scripts_fields(
    ctx: &Ctx,
    id: AtomId,
) -> (Option<AtomId>, Option<AtomId>, Option<AtomId>, Alignment) {
    match &ctx.get(id).kind {
        AtomKind::Scripts {
            base,
            sub,
            sup,
            align,
        } => (*base, *sub, *sup, *align),
        _ => (None, None, None, Alignment::Left),
    }
}

/// `CharAtom('M', "mathnormal")` in a phantom with no width: the base the
/// C++ puts under scripts that have none.
fn phantom_m(ctx: &mut Ctx) -> AtomId {
    let m = ctx.atom(AtomKind::Char {
        c: u16::from(b'M'),
        text_style: "mathnormal",
        math_mode: false,
        text_symbol: false,
    });
    let row = ctx.new_row_of(Some(m));
    ctx.atom(AtomKind::Phantom {
        elements: row,
        w: false,
        h: true,
        d: true,
    })
}

fn scripts_box(ctx: &mut Ctx, id: AtomId, env: &mut Environment) -> Result<BoxId> {
    let (base, sub, sup, align) = scripts_fields(ctx, id);
    let base = match base {
        Some(b) => b,
        None => {
            let p = phantom_m(ctx);
            if let AtomKind::Scripts { base, .. } = &mut ctx.get_mut(id).kind {
                *base = Some(p);
            }
            p
        }
    };

    let b = create_box(ctx, base, env)?;
    let delta_symbol = ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?;
    let mut delta_symbol = Some(delta_symbol);
    if sub.is_none() && sup.is_none() {
        return Ok(b);
    }
    let style = env.style;
    let base_limits = ctx.get(base).limits;
    if base_limits == LimitsType::Limits
        || (base_limits == LimitsType::Normal && style == TexStyle::Display)
    {
        let inner = new_under_over_one(ctx, Some(base), sub, Unit::Point, 0.3, true, false);
        let data = UnderOverData {
            base: Some(inner),
            ..scripts_over(sup)
        };
        return under_over_box(ctx, &data, env);
    }

    let mut hor = ctx.boxes.hbox_of(b)?;
    let mut last_font_id = ctx.boxes.last_font_id(b);
    if last_font_id == NO_FONT {
        last_font_id = env.font.mu_font_id(&env.settings);
    }
    let mut sub_style = env.sub_style();
    let mut sup_style = env.sup_style();

    let mut delta = 0.0f32;
    let mut shift_up;
    let mut shift_down;
    let settings = env.settings;
    let font = env.font;
    let param = |name: &str, s: TexStyle| font.style_param(&settings, name, s);

    let base_node = ctx.get(base).clone();
    if let AtomKind::Accented { base: inner, .. } = base_node.kind {
        let inner = inner.ok_or_else(null_atom)?;
        let b = create_box(ctx, inner, &mut env.cramp_style())?;
        shift_up = ctx.boxes.height(b) - param("supdrop", sup_style.style);
        shift_down = ctx.boxes.depth(b) + param("subdrop", sub_style.style);
    } else if let AtomKind::Symbol { name, .. } = base_node.kind
        && base_node.atom_type == AtomType::BigOperator
    {
        let mut c = symbol_char(env, name, style)?;
        if style < TexStyle::Text && font.has_next_larger(&c) {
            c = font.next_larger(&settings, &c, style);
        }
        let x = ctx.boxes.char_box(&c)?;
        let axis = param("axisheight", env.style);
        let node = ctx.boxes.get_mut(x);
        node.shift = -(node.height + node.depth) / 2.0 - axis;
        hor = ctx.boxes.hbox_of(x)?;
        delta = c.metrics.italic;
        let ds = space_box(ctx, &SpaceData::blank_of(SpaceType::MedMuSkip), env)?;
        delta_symbol = Some(ds);
        if delta > PREC && sub.is_none() {
            let s = ctx.boxes.strut(delta, 0.0, 0.0, 0.0)?;
            ctx.boxes.hbox_add(hor, s)?;
        }
        shift_up = ctx.boxes.height(hor) - param("supdrop", sup_style.style);
        shift_down = ctx.boxes.depth(hor) + param("subdrop", sub_style.style);
    } else if base_node.is_char_symbol() {
        shift_up = 0.0;
        shift_down = 0.0;
        let cf = char_font_of(ctx, base, env)?;
        if !is_text_symbol(ctx, base) || !font.has_space(cf.font) {
            delta = font.char_of(&settings, cf, style).metrics.italic;
        }
        if delta > PREC && sub.is_none() {
            let s = ctx.boxes.strut(delta, 0.0, 0.0, 0.0)?;
            ctx.boxes.hbox_add(hor, s)?;
            delta = 0.0;
        }
    } else {
        shift_up = ctx.boxes.height(b) - param("supdrop", sup_style.style);
        shift_down = ctx.boxes.depth(b) + param("subdrop", sub_style.style);
    }

    let x_height = |env: &Environment| env.font.x_height(&env.settings, style, last_font_id);

    let Some(sup) = sup else {
        let sub = sub.ok_or_else(null_atom)?;
        let x = create_box(ctx, sub, &mut sub_style)?;
        let xh = ctx.boxes.height(x);
        let shift = cmax(
            cmax(shift_down, param("sub1", style)),
            xh - 0.8 * x_height(env).abs(),
        );
        ctx.boxes.get_mut(x).shift = shift;
        ctx.boxes.hbox_add(hor, x)?;
        ctx.boxes
            .hbox_add(hor, delta_symbol.ok_or_else(null_atom)?)?;
        return Ok(hor);
    };

    let x = create_box(ctx, sup, &mut sup_style)?;
    let mut msiz = ctx.boxes.width(x);
    if let Some(sub) = sub
        && align == Alignment::Right
    {
        let w = create_box(ctx, sub, &mut sub_style)?;
        msiz = cmax(msiz, ctx.boxes.width(w));
    }
    let sup_box = ctx.boxes.hbox_aligned(x, msiz, align)?;
    let space = script_space(ctx, env)?;
    ctx.boxes.hbox_add(sup_box, space)?;
    let p = if style == TexStyle::Display {
        param("sup1", style)
    } else if env.cramp_style().style == style {
        param("sup3", style)
    } else {
        param("sup2", style)
    };
    let x_depth = ctx.boxes.depth(x);
    shift_up = cmax(cmax(shift_up, p), x_depth + x_height(env).abs() / 4.0);

    match sub {
        None => {
            ctx.boxes.get_mut(sup_box).shift = -shift_up;
            ctx.boxes.hbox_add(hor, sup_box)?;
        }
        Some(sub) => {
            let y = create_box(ctx, sub, &mut sub_style)?;
            let sub_box = ctx.boxes.hbox_aligned(y, msiz, align)?;
            let space = script_space(ctx, env)?;
            ctx.boxes.hbox_add(sub_box, space)?;
            shift_down = cmax(shift_down, param("sub2", style));
            let drt = param("defaultrulethickness", style);
            let x_height_value = ctx.boxes.height(x);
            let y_height = ctx.boxes.height(y);
            let mut interspace = shift_up - x_depth + shift_down - y_height;
            if interspace < 4.0 * drt {
                shift_up += 4.0 * drt - interspace;
                let psi = 0.8 * x_height(env).abs() - (shift_up - x_depth);
                if psi > 0.0 {
                    shift_up += psi;
                    shift_down -= psi;
                }
            }
            let vbox = ctx.boxes.vbox()?;
            ctx.boxes.get_mut(sup_box).shift = delta;
            ctx.boxes.vbox_add(vbox, sup_box)?;
            interspace = shift_up - x_depth + shift_down - y_height;
            let s = ctx.boxes.strut(0.0, interspace, 0.0, 0.0)?;
            ctx.boxes.vbox_add(vbox, s)?;
            ctx.boxes.vbox_add(vbox, sub_box)?;
            let y_depth = ctx.boxes.depth(y);
            let v = ctx.boxes.get_mut(vbox);
            v.height = shift_up + x_height_value;
            v.depth = shift_down + y_depth;
            ctx.boxes.hbox_add(hor, vbox)?;
        }
    }
    ctx.boxes
        .hbox_add(hor, delta_symbol.ok_or_else(null_atom)?)?;
    Ok(hor)
}

/// The over half of `UnderOverAtom(in, sup, point, 3, true, true)`.
fn scripts_over(sup: Option<AtomId>) -> UnderOverData {
    UnderOverData {
        base: None,
        under: None,
        under_unit: Unit::Em,
        under_space: 0.0,
        under_small: false,
        over: sup,
        over_unit: Unit::Point,
        over_space: 3.0,
        over_small: true,
    }
}

/// `ScriptsAtom::SCRIPT_SPACE`: half a point.
fn script_space(ctx: &mut Ctx, env: &mut Environment) -> Result<BoxId> {
    space_of(ctx, Unit::Point, 0.5, 0.0, 0.0, env)
}

fn big_operator_fields(
    ctx: &Ctx,
    id: AtomId,
) -> (Option<AtomId>, Option<AtomId>, Option<AtomId>, bool, bool) {
    match &ctx.get(id).kind {
        AtomKind::BigOperator {
            base,
            under,
            over,
            limits_set,
            limits,
        } => (*base, *under, *over, *limits_set, *limits),
        _ => (None, None, None, false, false),
    }
}

fn set_big_operator_base(ctx: &mut Ctx, id: AtomId, value: Option<AtomId>) {
    if let AtomKind::BigOperator { base, .. } = &mut ctx.get_mut(id).kind {
        *base = value;
    }
}

/// `BigOperatorAtom(base, under, over)`.
pub fn new_big_operator(
    ctx: &mut Ctx,
    base: Option<AtomId>,
    under: Option<AtomId>,
    over: Option<AtomId>,
) -> Result<AtomId> {
    let id = ctx.atom(AtomKind::BigOperator {
        base,
        under,
        over,
        limits_set: false,
        limits: false,
    });
    ctx.inherit_wrap_depth(id, base)?;
    ctx.get_mut(id).atom_type = AtomType::BigOperator;
    Ok(id)
}

fn big_operator_box(ctx: &mut Ctx, id: AtomId, env: &mut Environment) -> Result<BoxId> {
    let (original, under, over, limits_set, limits) = big_operator_fields(ctx, id);
    if let Some(b) = original
        && matches!(ctx.get(b).kind, AtomKind::SideSets { .. })
    {
        return crate::decor::side_sets_operator(ctx, b, under, over, env);
    }
    let style = env.style;
    let mut row: Option<AtomId> = None;
    let mut base = original;

    if let Some(b) = base
        && matches!(ctx.get(b).kind, AtomKind::Typed { .. })
    {
        let atom = ctx.typed_base(b).ok_or_else(null_atom)?;
        let look = matches!(
            ctx.get(atom).kind,
            AtomKind::Row {
                look_at_last_atom: true,
                ..
            }
        );
        if look && ctx.get(b).limits != LimitsType::Limits {
            let copy = ctx.private_copy(atom);
            row = Some(copy);
            base = Some(ctx.row_pop_last(copy));
        } else {
            base = Some(atom);
        }
        set_big_operator_base(ctx, id, base);
    }
    let base_id = base.ok_or_else(null_atom)?;
    let base_limits = ctx.get(base_id).limits;

    if (limits_set && !limits)
        || (!limits_set && style >= TexStyle::Text)
        || base_limits == LimitsType::NoLimits
        || (base_limits == LimitsType::Normal && style >= TexStyle::Text)
    {
        if let Some(row) = row {
            let scripts = new_scripts(ctx, base, under, over, Alignment::Left)?;
            ctx.row_add(row, Some(scripts));
            let b = create_box(ctx, row, env)?;
            ctx.row_pop_last(row);
            ctx.row_add(row, base);
            set_big_operator_base(ctx, id, original);
            return Ok(b);
        }
        let temp = new_scripts(ctx, base, under, over, Alignment::Left)?;
        return scripts_layout_guard(ctx, |ctx| scripts_box(ctx, temp, env));
    }

    let y;
    let delta;
    let base_node = ctx.get(base_id).clone();
    if let AtomKind::Symbol { name, .. } = base_node.kind
        && base_node.atom_type == AtomType::BigOperator
    {
        let c = symbol_char(env, name, style)?;
        y = create_box(ctx, base_id, env)?;
        delta = c.metrics.italic;
    } else {
        delta = 0.0;
        let inner = create_box(ctx, base_id, env)?;
        y = ctx.boxes.hbox_of(inner)?;
    }

    let x = match over {
        Some(over) => Some(create_box(ctx, over, &mut env.sup_style())?),
        None => None,
    };
    let z = match under {
        Some(under) => Some(create_box(ctx, under, &mut env.sub_style())?),
        None => None,
    };
    let max_w = cmax(
        cmax(x.map_or(0.0, |x| ctx.boxes.width(x)), ctx.boxes.width(y)),
        z.map_or(0.0, |z| ctx.boxes.width(z)),
    );
    let x = change_width(ctx, x, max_w)?;
    let y = change_width(ctx, Some(y), max_w)?.ok_or_else(null_atom)?;
    let z = change_width(ctx, z, max_w)?;

    let vbox = ctx.boxes.vbox()?;
    let settings = env.settings;
    let font = env.font;
    let param = |name: &str| font.style_param(&settings, name, style);
    let bigop5 = param("bigopspacing5");
    let mut kern = 0.0;
    if over.is_some() {
        let x = x.ok_or_else(null_atom)?;
        let s = ctx.boxes.strut(0.0, bigop5, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vbox, s)?;
        ctx.boxes.get_mut(x).shift = delta / 2.0;
        ctx.boxes.vbox_add(vbox, x)?;
        kern = cmax(
            param("bigopspacing1"),
            param("bigopspacing3") - ctx.boxes.depth(x),
        );
        let s = ctx.boxes.strut(0.0, kern, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vbox, s)?;
    }
    ctx.boxes.vbox_add(vbox, y)?;
    if under.is_some() {
        let z = z.ok_or_else(null_atom)?;
        let k = cmax(
            param("bigopspacing2"),
            param("bigopspacing4") - ctx.boxes.height(z),
        );
        let s = ctx.boxes.strut(0.0, k, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vbox, s)?;
        ctx.boxes.get_mut(z).shift = -delta / 2.0;
        ctx.boxes.vbox_add(vbox, z)?;
        let s = ctx.boxes.strut(0.0, bigop5, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vbox, s)?;
    }
    let mut h = ctx.boxes.height(y);
    let v = ctx.boxes.get(vbox);
    let total = v.height + v.depth;
    if let Some(x) = x {
        h += bigop5 + kern + ctx.boxes.height(x) + ctx.boxes.depth(x);
    }
    let v = ctx.boxes.get_mut(vbox);
    v.height = h;
    v.depth = total - h;

    if let Some(row) = row {
        let inner = create_box(ctx, row, env)?;
        let hb = ctx.boxes.hbox_of(inner)?;
        ctx.row_add(row, base);
        ctx.boxes.hbox_add(hb, vbox)?;
        set_big_operator_base(ctx, id, original);
        return Ok(hb);
    }
    Ok(vbox)
}

/// `FractionAtom(num, den, rule)`.
pub fn new_fraction(ctx: &mut Ctx, num: Option<AtomId>, den: Option<AtomId>, rule: bool) -> AtomId {
    new_fraction_with(ctx, num, den, !rule, Unit::Pixel, 0.0)
}

/// `FractionAtom::init()`.
pub fn new_fraction_with(
    ctx: &mut Ctx,
    num: Option<AtomId>,
    den: Option<AtomId>,
    nodefault: bool,
    unit: Unit,
    thickness: f32,
) -> AtomId {
    ctx.atom(AtomKind::Fraction(Box::new(FractionData {
        nodefault,
        unit,
        num_align: Alignment::Center,
        denom_align: Alignment::Center,
        numerator: num,
        denominator: den,
        thickness,
        deffactor: 1.0,
        deffactor_set: false,
        use_kern: true,
    })))
}

fn fraction_box(ctx: &mut Ctx, data: &FractionData, env: &mut Environment) -> Result<BoxId> {
    let style = env.style;
    let settings = env.settings;
    let font = env.font;
    let param = |name: &str| font.style_param(&settings, name, style);
    let drt = param("defaultrulethickness");
    let thickness = if data.nodefault {
        data.thickness * unit_factor(data.unit, env)
    } else if data.deffactor_set {
        data.deffactor * drt
    } else {
        drt
    };

    let mut num = match data.numerator {
        None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
        Some(n) => create_box(ctx, n, &mut env.num_style())?,
    };
    let mut denom = match data.denominator {
        None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
        Some(d) => create_box(ctx, d, &mut env.dnom_style())?,
    };
    if ctx.boxes.width(num) < ctx.boxes.width(denom) {
        let w = ctx.boxes.width(denom);
        num = ctx.boxes.hbox_aligned(num, w, data.num_align)?;
    } else {
        let w = ctx.boxes.width(num);
        denom = ctx.boxes.hbox_aligned(denom, w, data.denom_align)?;
    }

    let mut shiftup;
    let mut shiftdown;
    if style < TexStyle::Text {
        shiftup = param("num1");
        shiftdown = param("denom1");
    } else {
        shiftdown = param("denom2");
        shiftup = if thickness > 0.0 {
            param("num2")
        } else {
            param("num3")
        };
    }

    let vb = ctx.boxes.vbox()?;
    ctx.boxes.vbox_add(vb, num)?;
    let axis = param("axisheight");
    let num_depth = ctx.boxes.depth(num);
    let denom_height = ctx.boxes.height(denom);

    if thickness > 0.0 {
        let clr = if style < TexStyle::Text {
            3.0 * thickness
        } else {
            thickness
        };
        let delta = thickness / 2.0;
        let mut kern1 = shiftup - num_depth - (axis + delta);
        let mut kern2 = axis - delta - (denom_height - shiftdown);
        let delta1 = clr - kern1;
        let delta2 = clr - kern2;
        if delta1 > 0.0 {
            shiftup += delta1;
            kern1 += delta1;
        }
        if delta2 > 0.0 {
            shiftdown += delta2;
            kern2 += delta2;
        }
        let s = ctx.boxes.strut(0.0, kern1, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vb, s)?;
        let w = ctx.boxes.width(num);
        let r = ctx
            .boxes
            .rule(thickness, w, 0.0, crate::draw::TRANSPARENT, true)?;
        ctx.boxes.vbox_add(vb, r)?;
        let s = ctx.boxes.strut(0.0, kern2, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vb, s)?;
    } else {
        let clr = if style < TexStyle::Text {
            7.0 * drt
        } else {
            3.0 * drt
        };
        let mut kern = shiftup - num_depth - (denom_height - shiftdown);
        let delta = (clr - kern) / 2.0;
        if delta > 0.0 {
            shiftup += delta;
            shiftdown += delta;
            kern += 2.0 * delta;
        }
        let s = ctx.boxes.strut(0.0, kern, 0.0, 0.0)?;
        ctx.boxes.vbox_add(vb, s)?;
    }

    ctx.boxes.vbox_add(vb, denom)?;
    let num_height = ctx.boxes.height(num);
    let denom_depth = ctx.boxes.depth(denom);
    let v = ctx.boxes.get_mut(vb);
    v.height = shiftup + num_height;
    v.depth = shiftdown + denom_depth;

    if !data.use_kern {
        return Ok(vb);
    }
    let f = unit_size(Unit::Em, 0.12, env);
    let w = ctx.boxes.width(vb);
    ctx.boxes.hbox_aligned(vb, w + 2.0 * f, Alignment::Center)
}

fn symbol_name(ctx: &Ctx, id: AtomId) -> Result<&'static str> {
    match ctx.get(id).kind {
        AtomKind::Symbol { name, .. } => Ok(name),
        _ => Err(null_atom()),
    }
}

/// `DelimiterFactory::create(SymbolAtom&, env, size)`: the delimiter `size`
/// steps up its next-larger ladder, or one built to `size` times the height
/// of an 'A' when the ladder runs out or shrinks.
fn delimiter_sized(
    ctx: &mut Ctx,
    symbol: AtomId,
    env: &mut Environment,
    size: i32,
) -> Result<BoxId> {
    if size > 4 {
        return create_box(ctx, symbol, env);
    }
    let name = symbol_name(ctx, symbol)?;
    let style = env.style;
    let mut c = symbol_char(env, name, style)?;
    let base_total = c.metrics.height + c.metrics.depth;
    let mut i = 1;
    while i <= size && env.font.has_next_larger(&c) {
        c = env.font.next_larger(&env.settings, &c, style);
        i += 1;
    }
    let walked_smaller = (c.metrics.height + c.metrics.depth) < base_total;
    if (i <= size && !env.font.has_next_larger(&c)) || walked_smaller {
        let a = text_style_char(env, u16::from(b'A'), "mathnormal", style)?;
        let a_box = ctx.boxes.char_box(&a)?;
        let a_total = ctx.boxes.height(a_box) + ctx.boxes.depth(a_box);
        return delimiter(ctx, name, env, size as f32 * a_total);
    }
    ctx.boxes.char_box(&c)
}

/// `DelimiterFactory::create(name, env, minHeight)`: the smallest version
/// at least `min_height` tall, assembled from its extension parts when no
/// single glyph is.
pub fn delimiter(
    ctx: &mut Ctx,
    name: &str,
    env: &mut Environment,
    min_height: f32,
) -> Result<BoxId> {
    let style = env.style;
    let mut c = symbol_char(env, name, style)?;
    let mut total = c.metrics.height + c.metrics.depth;
    while total < min_height && env.font.has_next_larger(&c) {
        c = env.font.next_larger(&env.settings, &c, style);
        total = c.metrics.height + c.metrics.depth;
    }
    if total >= min_height {
        return ctx.boxes.char_box(&c);
    }
    if !env.font.is_extension_char(&c) {
        return ctx.boxes.char_box(&c);
    }
    let Some(ext) = env.font.extension(&env.settings, &c, style) else {
        return ctx.boxes.char_box(&c);
    };
    let vbox = ctx.boxes.vbox()?;
    for part in [ext.top, ext.middle, ext.bottom].into_iter().flatten() {
        let b = ctx.boxes.char_box(&part)?;
        ctx.boxes.vbox_add(vbox, b)?;
    }
    if let Some(repeat) = ext.repeat {
        let rep = ctx.boxes.char_box(&repeat)?;
        const MAX_PIECES: i32 = 4096;
        let mut pieces = 0;
        if ctx.boxes.height(rep) + ctx.boxes.depth(rep) > 0.0 {
            loop {
                let v = ctx.boxes.get(vbox);
                if !(v.height + v.depth <= min_height) {
                    break;
                }
                let more = pieces < MAX_PIECES;
                pieces += 1;
                if !more {
                    break;
                }
                if ext.top.is_some() && ext.bottom.is_some() {
                    ctx.boxes.vbox_insert(vbox, 1, rep)?;
                    if ext.middle.is_some() {
                        let pos = ctx.boxes.group_len(vbox) - 1;
                        ctx.boxes.vbox_insert(vbox, pos, rep)?;
                    }
                } else if ext.bottom.is_some() {
                    ctx.boxes.vbox_insert(vbox, 0, rep)?;
                } else {
                    ctx.boxes.vbox_add(vbox, rep)?;
                }
            }
        }
    }
    Ok(vbox)
}

/// `FencedAtom::center()`.
fn center(ctx: &mut Ctx, b: BoxId, axis: f32) {
    let node = ctx.boxes.get_mut(b);
    let h = node.height;
    let total = h + node.depth;
    node.shift = -(total / 2.0 - h) - axis;
}

fn fenced_box(
    ctx: &mut Ctx,
    base: AtomId,
    left: Option<AtomId>,
    right: Option<AtomId>,
    middle: &[AtomId],
    env: &mut Environment,
) -> Result<BoxId> {
    if let AtomKind::Row { breakable, .. } = &mut ctx.get_mut(base).kind {
        *breakable = false;
    }
    let mut content = create_box(ctx, base, env)?;
    let shortfall = 5.0 * unit_factor(Unit::Point, env);
    let axis = env.font.style_param(&env.settings, "axisheight", env.style);
    let delta = cmax(
        ctx.boxes.height(content) - axis,
        ctx.boxes.depth(content) + axis,
    );
    let minh = cmax(delta / 500.0 * 901.0, 2.0 * delta - shortfall);
    let hb = ctx.boxes.hbox()?;
    if !middle.is_empty() {
        for &atom in middle {
            let sym = match ctx.get(atom).kind {
                AtomKind::Middle { base: Some(b), .. } => match ctx.get(b).kind {
                    AtomKind::Symbol { name, .. } => Some(name),
                    _ => None,
                },
                _ => None,
            };
            if let Some(name) = sym {
                let b = delimiter(ctx, name, env, minh)?;
                center(ctx, b, axis);
                if let AtomKind::Middle { r#box, .. } = &mut ctx.get_mut(atom).kind {
                    *r#box = b;
                }
            }
        }
        content = create_box(ctx, base, env)?;
    }
    if let Some(left) = left {
        let name = symbol_name(ctx, left)?;
        let b = delimiter(ctx, name, env, minh)?;
        center(ctx, b, axis);
        ctx.boxes.hbox_add(hb, b)?;
    }
    let is_space = matches!(ctx.get(base).kind, AtomKind::Space(_));
    if !is_space {
        let g = glue::between(AtomType::Opening, ctx.left_type(base), env);
        let b = ctx.boxes.glue(g.width, g.stretch, g.shrink)?;
        ctx.boxes.hbox_add(hb, b)?;
    }
    ctx.boxes.hbox_add(hb, content)?;
    if !is_space {
        let g = glue::between(ctx.right_type(base), AtomType::Closing, env);
        let b = ctx.boxes.glue(g.width, g.stretch, g.shrink)?;
        ctx.boxes.hbox_add(hb, b)?;
    }
    if let Some(right) = right {
        let name = symbol_name(ctx, right)?;
        let b = delimiter(ctx, name, env, minh)?;
        center(ctx, b, axis);
        ctx.boxes.hbox_add(hb, b)?;
    }
    Ok(hb)
}

fn nth_root_box(ctx: &mut Ctx, base: AtomId, root: AtomId, env: &mut Environment) -> Result<BoxId> {
    let style = env.style;
    let settings = env.settings;
    let font = env.font;
    let drt = font.style_param(&settings, "defaultrulethickness", style);
    let mut clr = if style < TexStyle::Text {
        let sqrt = symbol_char(env, "sqrt", style)?;
        font.x_height(&settings, style, sqrt.font)
    } else {
        drt
    };
    clr = drt + clr.abs() / 4.0;

    let mut cramped = env.cramp_style();
    let bs = create_box(ctx, base, &mut cramped)?;
    let b = ctx.boxes.hbox_of(bs)?;
    let kern = space_of(ctx, Unit::Mu, 1.0, 0.0, 0.0, &mut cramped)?;
    ctx.boxes.hbox_add(b, kern)?;

    let total_h = ctx.boxes.height(b) + ctx.boxes.depth(b);
    let root_sign = delimiter(ctx, "sqrt", env, total_h + clr + drt)?;
    let delta = ctx.boxes.depth(root_sign) - (total_h + clr);
    clr += delta / 2.0;

    let b_height = ctx.boxes.height(b);
    ctx.boxes.get_mut(root_sign).shift = -(b_height + clr);
    let sign_height = ctx.boxes.height(root_sign);
    let ob = ctx.boxes.over_bar(b, clr, sign_height)?;
    ctx.boxes.get_mut(ob).shift = -(b_height + clr + drt);
    let square_root = ctx.boxes.hbox_of(root_sign)?;
    ctx.boxes.hbox_add(square_root, ob)?;

    let r = create_box(ctx, root, &mut env.root_style())?;
    let sq = ctx.boxes.get(square_root);
    let bottom_shift = 0.55 * (sq.height + sq.depth);
    let sq_depth = sq.depth;
    let r_depth = ctx.boxes.depth(r);
    ctx.boxes.get_mut(r).shift = sq_depth - r_depth - bottom_shift;
    let negkern = space_of(ctx, Unit::Mu, -10.0, 0.0, 0.0, env)?;
    let res = ctx.boxes.hbox()?;
    let pos = ctx.boxes.width(r) + ctx.boxes.width(negkern);
    if pos < 0.0 {
        let s = ctx.boxes.strut(-pos, 0.0, 0.0, 0.0)?;
        ctx.boxes.hbox_add(res, s)?;
    }
    ctx.boxes.hbox_add(res, r)?;
    ctx.boxes.hbox_add(res, negkern)?;
    ctx.boxes.hbox_add(res, square_root)?;
    Ok(res)
}

#[allow(clippy::too_many_arguments)]
fn accented_box(
    ctx: &mut Ctx,
    accent: AtomId,
    acc: bool,
    change_size: bool,
    base: Option<AtomId>,
    underbase: Option<AtomId>,
    env: &mut Environment,
) -> Result<BoxId> {
    let style = env.style;
    let settings = env.settings;
    let font = env.font;
    let mut b = match base {
        None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
        Some(base) => create_box(ctx, base, &mut env.cramp_style())?,
    };
    let u = ctx.boxes.width(b);
    let mut s = 0.0;
    if let Some(under) = underbase
        && ctx.get(under).is_char_symbol()
    {
        let cf = char_font_of(ctx, under, env)?;
        s = font.skew(&settings, cf, style);
    }
    let name = symbol_name(ctx, accent)?;
    let mut ch = symbol_char(env, name, style)?;
    while font.has_next_larger(&ch) {
        let larger = font.next_larger(&settings, &ch, style);
        if larger.metrics.width <= u {
            ch = larger;
        } else {
            break;
        }
    }
    let ec = -unit_factor(Unit::Mu, env);
    let delta = if acc {
        ec
    } else {
        crate::boxes::cmin(
            ctx.boxes.height(b),
            font.x_height(&settings, style, ch.font),
        )
    };
    let vbox = ctx.boxes.vbox()?;
    let italic = ch.metrics.italic;
    let mut cb = ctx.boxes.char_box(&ch)?;
    if acc {
        cb = if change_size {
            create_box(ctx, accent, &mut env.sub_style())?
        } else {
            create_box(ctx, accent, env)?
        };
    }
    let y = if italic.abs() > PREC {
        let s = ctx.boxes.strut(-italic, 0.0, 0.0, 0.0)?;
        let hbox = ctx.boxes.hbox_of(s)?;
        ctx.boxes.hbox_add(hbox, cb)?;
        hbox
    } else {
        cb
    };
    let y_width = ctx.boxes.width(y);
    let diff = (u - y_width) / 2.0;
    ctx.boxes.get_mut(y).shift = s + if diff > 0.0 { diff } else { 0.0 };
    if diff < 0.0 {
        b = ctx.boxes.hbox_aligned(b, y_width, Alignment::Center)?;
    }
    ctx.boxes.vbox_add(vbox, y)?;
    let kern = if change_size {
        -delta
    } else {
        -ctx.boxes.width(b)
    };
    let k = ctx.boxes.strut(0.0, kern, 0.0, 0.0)?;
    ctx.boxes.vbox_add(vbox, k)?;
    ctx.boxes.vbox_add(vbox, b)?;
    let v = ctx.boxes.get(vbox);
    let total = v.height + v.depth;
    let d = ctx.boxes.depth(b);
    let v = ctx.boxes.get_mut(vbox);
    v.depth = d;
    v.height = total - d;
    if diff < 0.0 {
        let s = ctx.boxes.strut(diff, 0.0, 0.0, 0.0)?;
        let hb = ctx.boxes.hbox_of(s)?;
        ctx.boxes.hbox_add(hb, vbox)?;
        ctx.boxes.get_mut(hb).width = u;
        return Ok(hb);
    }
    Ok(vbox)
}

fn vrow_box(
    ctx: &mut Ctx,
    elements: &[AtomId],
    raise: &SpaceData,
    add_interline: bool,
    valign: Alignment,
    halign: Alignment,
    env: &mut Environment,
) -> Result<BoxId> {
    let vb = ctx.boxes.vbox()?;
    let interline = ctx.boxes.strut(0.0, env.interline(), 0.0, 0.0)?;
    let s = elements.len();
    if halign != Alignment::None {
        let mut max_width = f32::MIN;
        let mut boxes = Vec::with_capacity(s);
        for &e in elements {
            let b = create_box(ctx, e, env)?;
            if max_width < ctx.boxes.width(b) {
                max_width = ctx.boxes.width(b);
            }
            boxes.push(b);
        }
        for (i, &b) in boxes.iter().enumerate() {
            let hb = ctx.boxes.hbox_aligned(b, max_width, halign)?;
            ctx.boxes.vbox_add(vb, hb)?;
            if add_interline && i + 1 < s {
                ctx.boxes.vbox_add(vb, interline)?;
            }
        }
    } else {
        for (i, &e) in elements.iter().enumerate() {
            let b = create_box(ctx, e, env)?;
            ctx.boxes.vbox_add(vb, b)?;
            if add_interline && i + 1 < s {
                ctx.boxes.vbox_add(vb, interline)?;
            }
        }
    }
    let r = space_box(ctx, raise, env)?;
    ctx.boxes.get_mut(vb).shift = -ctx.boxes.width(r);
    let count = ctx.boxes.group_len(vb);
    match valign {
        Alignment::Top => {
            let t = if count == 0 {
                0.0
            } else {
                let first = ctx.boxes.group_child(vb, 0);
                ctx.boxes.height(first)
            };
            let node = ctx.boxes.get_mut(vb);
            node.height = t;
            node.depth = node.depth + node.height - t;
        }
        Alignment::Center => {
            let axis = env.font.style_param(&env.settings, "axisheight", env.style);
            let node = ctx.boxes.get_mut(vb);
            let h = node.height + node.depth;
            node.height = h / 2.0 + axis;
            node.depth = h / 2.0 - axis;
        }
        _ => {
            let t = if count == 0 {
                0.0
            } else {
                let last = ctx.boxes.group_child(vb, count - 1);
                ctx.boxes.depth(last)
            };
            let node = ctx.boxes.get_mut(vb);
            node.height = node.depth + node.height - t;
            node.depth = t;
        }
    }
    Ok(vb)
}
