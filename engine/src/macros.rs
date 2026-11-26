//! Commands: `MacroInfo`, `NewCommandMacro` and the predefined command
//! implementations of `core/macro_impl.*`.
//!
//! Built-in commands come from the generated table. Commands a formula
//! defines live in the formula's `Ctx`, as the C++ rolls its global registry
//! back to the built-ins before every formula.

use crate::atoms::UnderOverData;
use crate::atoms::{AtomId, AtomKind, Ctx, LimitsType, SpaceData};
use crate::boxes::Alignment;
use crate::boxes::cmax;
use crate::boxes::cmin;
use crate::decor::{Frame, rotation_option};
use crate::draw::{BLACK, TRANSPARENT};
use crate::enums::{AtomType, SpaceType, Unit};
use crate::env::parse_length;
use crate::error::{Error, Result};
use crate::font::TexStyle;
use crate::layout::new_fraction;
use crate::layout::new_scripts;
use crate::layout::{new_fraction_with, new_under_over_one};
use crate::matrix::{self, MulticolumnData};
use crate::numparse::{read_float, read_hex_u32};
use crate::parser::{
    Args, Parser, new_color, new_cumulative_scripts, parse_arg, parse_plain, parse_plain_with,
    parse_sub,
};
use crate::tables::{self, MacroKind};
use crate::wstr::{self, w};

/// `kMaxMacroArgs`.
const MAX_MACRO_ARGS: i32 = 9;

pub fn is_valid_argc(argc: i32) -> bool {
    (0..=MAX_MACRO_ARGS).contains(&argc)
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    /// A `PreDefMacro`: the built-in command of this name.
    Predefined(&'static str),
    /// An `InflationMacroInfo`: expands to its `NewCommandMacro` code.
    Inflation,
    /// A plain `MacroInfo`, which does nothing.
    Other,
}

#[derive(Clone, Copy, Debug)]
pub struct MacroInfo {
    pub argc: i32,
    pub pos_opts: i32,
    pub kind: Kind,
}

fn builtin(name: &[u16]) -> Option<&'static tables::MacroInfo> {
    tables::MACROS
        .binary_search_by(|m| m.name.encode_utf16().cmp(name.iter().copied()))
        .ok()
        .map(|i| &tables::MACROS[i])
}

/// `MacroInfo::get()`.
pub fn get(ctx: &Ctx, name: &[u16]) -> Option<MacroInfo> {
    if let Some(&(argc, pos_opts)) = ctx.user_macros.get(name) {
        return Some(MacroInfo {
            argc,
            pos_opts,
            kind: Kind::Inflation,
        });
    }
    builtin(name).map(|m| MacroInfo {
        argc: m.argc,
        pos_opts: m.pos_opts,
        kind: match m.kind {
            MacroKind::Predefined => Kind::Predefined(m.name),
            MacroKind::Inflation => Kind::Inflation,
            MacroKind::Other => Kind::Other,
        },
    })
}

fn builtin_code(name: &[u16]) -> Option<&'static str> {
    tables::NEW_COMMAND_CODES
        .binary_search_by(|e| e.0.encode_utf16().cmp(name.iter().copied()))
        .ok()
        .map(|i| tables::NEW_COMMAND_CODES[i].1)
}

fn builtin_replacement(name: &[u16]) -> Option<&'static str> {
    tables::NEW_COMMAND_REPLACEMENTS
        .binary_search_by(|e| e.0.encode_utf16().cmp(name.iter().copied()))
        .ok()
        .map(|i| tables::NEW_COMMAND_REPLACEMENTS[i].1)
}

/// `NewCommandMacro::isMacro()`.
pub fn is_new_command(ctx: &Ctx, name: &[u16]) -> bool {
    ctx.user_codes.contains_key(name) || builtin_code(name).is_some()
}

/// Notes that the formula needs a part of MicroTeX not ported yet. The note
/// survives the partial-mode catches that would swallow the error itself.
pub fn unsupported(ctx: &mut Ctx, what: &str) -> Error {
    if ctx.unsupported.is_none() {
        ctx.unsupported = Some(what.to_owned());
    }
    Error::unsupported(what)
}

/// `MacroInfo::invoke()`.
pub fn invoke(
    tp: &mut Parser,
    ctx: &mut Ctx,
    mac: &MacroInfo,
    args: &mut Args,
) -> Result<Option<AtomId>> {
    match mac.kind {
        Kind::Other => Ok(None),
        Kind::Inflation => {
            execute_new_command(ctx, args);
            Ok(None)
        }
        Kind::Predefined(name) => match call(name, tp, ctx, args) {
            Err(e) if e.is_parse() => Err(Error::parse(format!(
                "Problem with command {} at position {}:{}\n caused by: {}",
                wstr::narrow(&args[0]),
                tp.line(),
                tp.col(),
                e.message
            ))),
            other => other,
        },
    }
}

/// `NewCommandMacro::execute()`: appends the expanded code to `args`.
fn execute_new_command(ctx: &mut Ctx, args: &mut Args) {
    let name = args[0].clone();
    let mut code = match ctx.user_codes.get(&name) {
        Some(code) => code.clone(),
        None => builtin_code(&name).map(w).unwrap_or_default(),
    };
    let argc = args.len() - 12;
    let replacement = match ctx.user_replacements.get(&name) {
        Some(r) => Some(r.clone()),
        None => builtin_replacement(&name).map(w),
    };
    let mut dec = 0;
    if !args[argc + 1].is_empty() {
        dec = 1;
        wstr::replace_all(&mut code, &w("#1"), &args[argc + 1].clone());
    } else if let Some(r) = replacement {
        dec = 1;
        wstr::replace_all(&mut code, &w("#1"), &r);
    }
    for (i, value) in args.iter().enumerate().take(argc + 1).skip(1) {
        let mut key = w("#");
        key.extend(wstr::from_int((i + dec) as i64));
        wstr::replace_all(&mut code, &key, value);
    }
    args.push(code);
}

/// `valueof(str, int&)`: a wide stream read of an `int`. Anything without
/// digits reads as 0; values past `int` saturate, which every caller rejects
/// as an argument count anyway.
pub fn value_of_int(text: &[u16]) -> i32 {
    let mut i = 0;
    while i < text.len() && matches!(text[i], 0x20 | 0x09..=0x0D) {
        i += 1;
    }
    let mut negative = false;
    if i < text.len() && (text[i] == u16::from(b'+') || text[i] == u16::from(b'-')) {
        negative = text[i] == u16::from(b'-');
        i += 1;
    }
    let mut value: i64 = 0;
    let mut digits = 0;
    while i < text.len() && (0x30..=0x39).contains(&text[i]) {
        value = (value * 10 + i64::from(text[i] - 0x30)).min(i64::from(u32::MAX) + 1);
        digits += 1;
        i += 1;
    }
    if digits == 0 {
        return 0;
    }
    let value = if negative { -value } else { value };
    value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn check_new(ctx: &Ctx, name: &[u16]) -> Result<()> {
    if builtin(name).is_some() {
        return Err(Error::parse(format!(
            "Command {} is built-in and can not be redefined!",
            wstr::narrow(name)
        )));
    }
    if ctx.err_if_conflict && is_new_command(ctx, name) {
        return Err(Error::parse(format!(
            "Command {} already exists! Use renewcommand instead!",
            wstr::narrow(name)
        )));
    }
    Ok(())
}

fn check_renew(ctx: &Ctx, name: &[u16]) -> Result<()> {
    if builtin(name).is_some() {
        return Err(Error::parse(format!(
            "Command {} is built-in and can not be redefined!",
            wstr::narrow(name)
        )));
    }
    if ctx.err_if_conflict && !is_new_command(ctx, name) {
        return Err(Error::parse(format!(
            "Command {} is no defined! Use newcommand instead!",
            wstr::narrow(name)
        )));
    }
    Ok(())
}

/// `addNewCommand` and `addRenewCommand`, after their checks.
fn define(ctx: &mut Ctx, name: &[u16], code: &[u16], argc: i32, def: Option<&[u16]>) {
    ctx.user_codes.insert(name.to_vec(), code.to_vec());
    let pos_opts = if let Some(def) = def {
        ctx.user_replacements.insert(name.to_vec(), def.to_vec());
        1
    } else {
        0
    };
    ctx.user_macros.insert(name.to_vec(), (argc, pos_opts));
}

fn new_command(
    tp: &mut Parser,
    ctx: &mut Ctx,
    args: &mut Args,
    renew: bool,
) -> Result<Option<AtomId>> {
    let newcmd = args[1].clone();
    if !tp.is_valid_name(&newcmd) {
        return Err(Error::parse(if renew {
            format!("Invalid name for the command: {}", wstr::narrow(&newcmd))
        } else {
            format!("Invalid name for the command '{}", wstr::narrow(&newcmd))
        }));
    }
    let nb_args = if args[3].is_empty() {
        0
    } else {
        value_of_int(&args[3])
    };
    if !is_valid_argc(nb_args) {
        return Err(Error::parse(if renew {
            "Bad number of arguments in renewcommand!"
        } else {
            "Bad number of arguments in newcommand!"
        }));
    }
    let name = newcmd[1..].to_vec();
    if renew {
        check_renew(ctx, &name)?;
    } else {
        check_new(ctx, &name)?;
    }
    let def = (!args[4].is_empty()).then(|| args[4].clone());
    define(ctx, &name, &args[2].clone(), nb_args, def.as_deref());
    Ok(None)
}

fn new_environment(ctx: &mut Ctx, args: &mut Args, renew: bool) -> Result<Option<AtomId>> {
    let opt = if args[4].is_empty() {
        0
    } else {
        value_of_int(&args[4])
    };
    if !is_valid_argc(opt + 1) {
        return Err(Error::parse(if renew {
            "Bad number of arguments in renewenvironment!"
        } else {
            "Bad number of arguments in newenvironment!"
        }));
    }
    let mut name = args[1].clone();
    name.extend(w("@env"));
    let mut def = args[2].clone();
    def.extend(w(" #"));
    def.extend(wstr::from_int(i64::from(opt + 1)));
    def.extend(w(" "));
    def.extend_from_slice(&args[3]);
    if renew {
        if !is_new_command(ctx, &name) {
            return Err(Error::parse(format!(
                "Environment {}is not defined! Use newenvironment instead!",
                wstr::narrow(&args[1])
            )));
        }
        check_renew(ctx, &name)?;
    } else {
        check_new(ctx, &name)?;
    }
    define(ctx, &name, &def, opt + 1, None);
    Ok(None)
}

/// `StyleAtom(style, a)`: an empty atom when there is none.
fn new_style(ctx: &mut Ctx, style: TexStyle, a: Option<AtomId>) -> AtomId {
    let at = match a {
        Some(a) => a,
        None => ctx.atom(AtomKind::Empty),
    };
    let atom_type = ctx.get(at).atom_type;
    let id = ctx.atom(AtomKind::Style { style, at });
    ctx.get_mut(id).atom_type = atom_type;
    id
}

fn style_macro(tp: &mut Parser, ctx: &mut Ctx, style: TexStyle) -> Result<Option<AtomId>> {
    let text = tp.get_over_argument()?;
    let g = parse_arg(ctx, tp, text, false)?;
    Ok(Some(new_style(ctx, style, g)))
}

/// `TypedAtom(lt, rt, atom)`.
pub fn new_typed(
    ctx: &mut Ctx,
    left: AtomType,
    right: AtomType,
    atom: Option<AtomId>,
) -> Result<AtomId> {
    let atom = atom.ok_or_else(|| Error::invalid_state("A typed atom needs an atom."))?;
    let limits = ctx.get(atom).limits;
    let id = ctx.atom(AtomKind::Typed { left, right, atom });
    ctx.get_mut(id).limits = limits;
    Ok(id)
}

fn math_type(tp: &mut Parser, ctx: &mut Ctx, args: &Args, t: AtomType) -> Result<AtomId> {
    let root = parse_arg(ctx, tp, args[1].clone(), false)?;
    new_typed(ctx, t, t, root)
}

fn limits_type(tp: &mut Parser, ctx: &mut Ctx, t: LimitsType) -> Option<AtomId> {
    let atom = tp.pop_last_atom(ctx)?;
    let copy = ctx.private_copy(atom);
    ctx.get_mut(copy).limits = t;
    Some(copy)
}

fn fraction(tp: &mut Parser, ctx: &mut Ctx, args: &Args) -> Result<Option<AtomId>> {
    let num = parse_arg(ctx, tp, args[1].clone(), false)?;
    let den = parse_arg(ctx, tp, args[2].clone(), false)?;
    if num.is_none() || den.is_none() {
        return Err(Error::parse(
            "Both numerator and denominator of a fraction can't be empty!",
        ));
    }
    Ok(Some(new_fraction(ctx, num, den, true)))
}

fn over(tp: &mut Parser, ctx: &mut Ctx, rule: bool) -> Result<Option<AtomId>> {
    let num = tp.pop_formula_atom();
    let text = tp.get_over_argument()?;
    let den = parse_arg(ctx, tp, text, false)?;
    if num.is_none() || den.is_none() {
        return Err(Error::parse(
            "Both numerator and denominator of a fraction can't be empty!",
        ));
    }
    Ok(Some(new_fraction(ctx, num, den, rule)))
}

fn muskip(name: &[u16]) -> SpaceType {
    const TABLE: [(&str, SpaceType); 10] = [
        (",", SpaceType::ThinMuSkip),
        (":", SpaceType::MedMuSkip),
        (";", SpaceType::ThickMuSkip),
        ("thinspace", SpaceType::ThinMuSkip),
        ("medspace", SpaceType::MedMuSkip),
        ("thickspace", SpaceType::ThickMuSkip),
        ("!", SpaceType::NegThinMuSkip),
        ("negthinspace", SpaceType::NegThinMuSkip),
        ("negmedspace", SpaceType::NegMedMuSkip),
        ("negthickspace", SpaceType::NegThickMuSkip),
    ];
    TABLE
        .iter()
        .find(|(n, _)| wstr::eq(name, n))
        .map_or(SpaceType::None, |e| e.1)
}

fn cumulative(tp: &mut Parser, ctx: &mut Ctx, args: &Args, sup: bool) -> Result<Option<AtomId>> {
    let base = tp.pop_last_atom(ctx);
    let script = parse_arg(ctx, tp, args[1].clone(), true)?;
    Ok(Some(if sup {
        new_cumulative_scripts(ctx, base, None, script)
    } else {
        new_cumulative_scripts(ctx, base, script, None)
    }))
}

/// The built-in command `name`, as its `macro_*` function.
fn call(
    name: &'static str,
    tp: &mut Parser,
    ctx: &mut Ctx,
    args: &mut Args,
) -> Result<Option<AtomId>> {
    match name {
        "newcommand" => new_command(tp, ctx, args, false),
        "renewcommand" => new_command(tp, ctx, args, true),
        "newenvironment" => new_environment(ctx, args, false),
        "renewenvironment" => new_environment(ctx, args, true),
        "fatalIfCmdConflict" => {
            ctx.err_if_conflict = wstr::eq(&args[1], "true");
            Ok(None)
        }
        "frac" => fraction(tp, ctx, args),
        "over" => over(tp, ctx, true),
        "atop" => over(tp, ctx, false),
        "displaystyle" => style_macro(tp, ctx, TexStyle::Display),
        "textstyle" => style_macro(tp, ctx, TexStyle::Text),
        "scriptstyle" => style_macro(tp, ctx, TexStyle::Script),
        "scriptscriptstyle" => style_macro(tp, ctx, TexStyle::ScriptScript),
        "," | ":" | ";" | "!" | "thinspace" | "medspace" | "thickspace" | "negthinspace"
        | "negmedspace" | "negthickspace" => Ok(Some(
            ctx.atom(AtomKind::Space(SpaceData::blank_of(muskip(&args[0])))),
        )),
        "quad" => Ok(Some(ctx.atom(AtomKind::Space(SpaceData::sized(
            Unit::Em,
            1.0,
            0.0,
            0.0,
        ))))),
        "mathrm" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(ctx.atom(AtomKind::Roman { base })))
        }
        "mathbf" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            let roman = ctx.atom(AtomKind::Roman { base });
            Ok(Some(ctx.atom(AtomKind::Bold { base: Some(roman) })))
        }
        "mathop" => {
            let a = math_type(tp, ctx, args, AtomType::BigOperator)?;
            ctx.get_mut(a).limits = LimitsType::Normal;
            Ok(Some(a))
        }
        "mathpunct" => Ok(Some(math_type(tp, ctx, args, AtomType::Punctuation)?)),
        "mathord" => Ok(Some(math_type(tp, ctx, args, AtomType::Ordinary)?)),
        "mathrel" => Ok(Some(math_type(tp, ctx, args, AtomType::Relation)?)),
        "mathinner" => Ok(Some(math_type(tp, ctx, args, AtomType::Inner)?)),
        "mathbin" => Ok(Some(math_type(tp, ctx, args, AtomType::BinaryOperator)?)),
        "mathopen" => Ok(Some(math_type(tp, ctx, args, AtomType::Opening)?)),
        "mathclose" => Ok(Some(math_type(tp, ctx, args, AtomType::Closing)?)),
        "nolimits" => Ok(limits_type(tp, ctx, LimitsType::NoLimits)),
        "limits" => Ok(limits_type(tp, ctx, LimitsType::Limits)),
        "normal" => Ok(limits_type(tp, ctx, LimitsType::Normal)),
        "mathcumsup" => cumulative(tp, ctx, args, true),
        "mathcumsub" => cumulative(tp, ctx, args, false),
        "insertBreakMark" | "-" => Ok(Some(ctx.atom(AtomKind::BreakMark))),
        "makeatletter" => {
            tp.make_at_letter();
            Ok(None)
        }
        "makeatother" => {
            tp.make_at_other();
            Ok(None)
        }
        "text" => {
            let root = parse_sub(ctx, tp, args[1].clone(), "mathnormal", false, false)?.root;
            Ok(Some(ctx.atom(AtomKind::Roman { base: root })))
        }
        "mbox" => {
            let root = parse_sub(ctx, tp, args[1].clone(), "mathnormal", false, false)?.root;
            let group = ctx.atom(AtomKind::Roman { base: root });
            Ok(Some(new_style(ctx, TexStyle::Text, Some(group))))
        }
        "mathit" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(ctx.atom(AtomKind::It { base })))
        }
        "mathsf" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(ctx.atom(AtomKind::Ss { base })))
        }
        "mathtt" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(ctx.atom(AtomKind::Tt { base })))
        }
        "it" | "sf" | "tt" | "rm" | "bf" => {
            let text = tp.get_over_argument()?;
            let math_mode = tp.is_math_mode();
            let base = parse_sub(ctx, tp, text, "", false, math_mode)?.root;
            Ok(Some(match name {
                "it" => ctx.atom(AtomKind::It { base }),
                "sf" => ctx.atom(AtomKind::Ss { base }),
                "tt" => ctx.atom(AtomKind::Tt { base }),
                "rm" => ctx.atom(AtomKind::Roman { base }),
                _ => {
                    let roman = ctx.atom(AtomKind::Roman { base });
                    ctx.atom(AtomKind::Bold { base: Some(roman) })
                }
            }))
        }
        "mathscr" | "mathbb" | "mathcal" | "cal" | "mathfrak" | "mathds" | "frak" | "Bbb"
        | "oldstylenums" | "bold" => {
            let style: &'static str = match name {
                "frak" => "mathfrak",
                "Bbb" => "mathbb",
                "cal" => "mathcal",
                other => other,
            };
            let at = parse_arg(ctx, tp, args[1].clone(), false)?;
            if style == "bold" {
                return Ok(Some(ctx.atom(AtomKind::Bold { base: at })));
            }
            Ok(Some(ctx.atom(AtomKind::TextStyle { style, at })))
        }
        "phantom" | "hphantom" | "vphantom" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            let (w, h, d) = match name {
                "phantom" => (true, true, true),
                "hphantom" => (true, false, false),
                _ => (false, true, true),
            };
            let elements = ctx.new_row_of(base);
            Ok(Some(ctx.atom(AtomKind::Phantom { elements, w, h, d })))
        }
        "left" => left(tp, ctx, args),
        "middle" => {
            let base = parse_arg(ctx, tp, args[1].clone(), true)?;
            let b = ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?;
            Ok(Some(ctx.atom(AtomKind::Middle { base, r#box: b })))
        }
        "sqrt" | "sqrtsign" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            let root = if args[2].is_empty() {
                None
            } else {
                parse_arg(ctx, tp, args[2].clone(), false)?
            };
            let base = base.unwrap_or_else(|| ctx.atom(AtomKind::Empty));
            let root = root.unwrap_or_else(|| ctx.atom(AtomKind::Empty));
            Ok(Some(ctx.atom(AtomKind::NthRoot { base, root })))
        }
        "big" | "Big" | "bigg" | "Bigg" | "bigl" | "Bigl" | "biggl" | "Biggl" | "bigr" | "Bigr"
        | "biggr" | "Biggr" => {
            let size = match name {
                "big" | "bigl" | "bigr" => 1,
                "Big" | "Bigl" | "Bigr" => 2,
                "bigg" | "biggl" | "biggr" => 3,
                _ => 4,
            };
            let atom_type = match name.as_bytes().last() {
                Some(b'l') => AtomType::Opening,
                Some(b'r') => AtomType::Closing,
                _ => AtomType::None,
            };
            let a = parse_arg(ctx, tp, args[1].clone(), false)?;
            let Some(delim) = a.filter(|&a| matches!(ctx.get(a).kind, AtomKind::Symbol { .. }))
            else {
                return Ok(a);
            };
            let t = ctx.atom(AtomKind::BigDelimiter { delim, size });
            if atom_type != AtomType::None {
                ctx.get_mut(t).atom_type = atom_type;
            }
            Ok(Some(t))
        }
        "textcircled" => {
            let base = parse_arg(ctx, tp, args[1].clone(), true)?;
            let at = ctx.atom(AtomKind::Roman { base });
            Ok(Some(ctx.atom(AtomKind::TextCircled { at })))
        }
        "color" => {
            if tp.is_array_mode() {
                let color = color_of(ctx, &wstr::narrow_bytes(&args[1]));
                return Ok(Some(ctx.atom(AtomKind::CellSpecifier {
                    color,
                    foreground: true,
                })));
            }
            Ok(None)
        }
        "textcolor" | "bgcolor" => {
            let a = parse_arg(ctx, tp, args[2].clone(), true)?;
            let c = color_of(ctx, &wstr::narrow_bytes(&args[1]));
            let (bg, fg) = if name == "bgcolor" {
                (c, TRANSPARENT)
            } else {
                (TRANSPARENT, c)
            };
            Ok(Some(new_color(ctx, a, bg, fg)))
        }
        "colorbox" => {
            let c = color_of(ctx, &wstr::narrow_bytes(&args[1]));
            let a = parse_arg(ctx, tp, args[2].clone(), true)?;
            Ok(Some(new_fbox(ctx, a, c, c)))
        }
        "fcolorbox" => {
            let f = color_of(ctx, &wstr::narrow_bytes(&args[2]));
            let b = color_of(ctx, &wstr::narrow_bytes(&args[1]));
            let a = parse_arg(ctx, tp, args[3].clone(), true)?;
            Ok(Some(new_fbox(ctx, a, f, b)))
        }
        "fbox" => {
            let a = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(new_fbox(ctx, a, TRANSPARENT, TRANSPARENT)))
        }
        "definecolor" => define_color(ctx, args),
        "^" | "'" | "\"" | "`" | "=" | "." | "~" | "u" | "v" | "H" | "r" | "U" | "t" => {
            let accent = match name {
                "~" => "tilde",
                "'" => "acute",
                "^" => "hat",
                "\"" => "ddot",
                "`" => "grave",
                "=" => "bar",
                "." => "dot",
                "u" => "breve",
                "v" => "check",
                "H" => "doubleacute",
                "t" => "tie",
                "r" => "mathring",
                _ => "cyrbreve",
            };
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(new_accented_named(ctx, base, accent)?))
        }
        "hat" | "widehat" | "tilde" | "acute" | "grave" | "ddot" | "cyrddot" | "mathring"
        | "bar" | "breve" | "check" | "vec" | "dot" | "widetilde" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(new_accented_named(ctx, base, name)?))
        }
        "accent" | "grkaccent" => {
            let base = parse_arg(ctx, tp, args[2].clone(), false)?;
            let accent = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(new_accented(ctx, base, accent, name == "accent")?))
        }
        "kern" => {
            let (unit, value) = tp.get_length()?;
            if unit == Unit::None {
                return Err(Error::parse("Missing dimension for \\kern!"));
            }
            Ok(Some(ctx.atom(AtomKind::Space(SpaceData::sized(
                unit, value, 0.0, 0.0,
            )))))
        }
        "hspace" | "vspace" => {
            let (unit, value) = parse_length(&args[1]);
            Ok(Some(ctx.atom(AtomKind::Space(if name == "hspace" {
                SpaceData::sized(unit, value, 0.0, 0.0)
            } else {
                SpaceData::sized(unit, 0.0, value, 0.0)
            }))))
        }
        "rule" => {
            let (w_unit, w) = parse_length(&args[1]);
            let (h_unit, h) = parse_length(&args[2]);
            let (r_unit, r) = parse_length(&args[3]);
            Ok(Some(ctx.atom(AtomKind::Rule {
                w_unit,
                w,
                h_unit,
                h,
                r_unit,
                r: -r,
            })))
        }
        "raisebox" => {
            let (r_unit, r) = parse_length(&args[1]);
            let (h_unit, h) = parse_length(&args[3]);
            let (d_unit, d) = parse_length(&args[4]);
            let base = parse_arg(ctx, tp, args[2].clone(), true)?;
            Ok(Some(ctx.atom(AtomKind::Raise {
                base,
                r_unit,
                r,
                h_unit,
                h,
                d_unit,
                d,
            })))
        }
        "smash" => {
            let atom = parse_arg(ctx, tp, args[1].clone(), false)?
                .ok_or_else(|| Error::invalid_state("Nothing to smash."))?;
            let (h, d) = if wstr::eq(&args[2], "opt") {
                (true, false)
            } else if wstr::eq(&args[2], "b") {
                (false, true)
            } else {
                (true, true)
            };
            Ok(Some(ctx.atom(AtomKind::Smashed { atom, h, d })))
        }
        "stackrel" | "stackbin" => {
            let base = parse_arg(ctx, tp, args[2].clone(), false)?;
            let under = parse_arg(ctx, tp, args[3].clone(), false)?;
            let over = parse_arg(ctx, tp, args[1].clone(), false)?;
            let a = ctx.atom(AtomKind::UnderOver(Box::new(UnderOverData {
                base,
                under,
                under_unit: Unit::Mu,
                under_space: 0.5,
                under_small: true,
                over,
                over_unit: Unit::Mu,
                over_space: 2.5,
                over_small: true,
            })));
            let t = if name == "stackrel" {
                AtomType::Relation
            } else {
                AtomType::BinaryOperator
            };
            Ok(Some(new_typed(ctx, t, t, Some(a))?))
        }
        "underaccent" => {
            let base = parse_arg(ctx, tp, args[2].clone(), false)?;
            let script = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(new_under_over_one(
                ctx,
                base,
                script,
                Unit::Mu,
                0.3,
                true,
                false,
            )))
        }
        "lmoustache" | "rmoustache" => {
            let shared = ctx.symbol(name)?;
            let delim = ctx.private_copy(shared);
            let b = ctx.atom(AtomKind::BigDelimiter { delim, size: 1 });
            ctx.get_mut(b).atom_type = if name == "lmoustache" {
                AtomType::Opening
            } else {
                AtomType::Closing
            };
            Ok(Some(b))
        }
        "sfrac" => sfrac(tp, ctx, args),
        "genfrac" => genfrac(tp, ctx, args),
        "cr" | "\\" => matrix::cr(tp, ctx),
        "smallmatrix@@env" | "matrix@@env" | "array@@env" | "align@@env" | "flalign@@env"
        | "alignat@@env" | "aligned@@env" | "alignedat@@env" | "multline@@env" | "gather@@env"
        | "gathered@@env" => matrix::environment(name, tp, ctx, args),
        "multicolumn" | "hdotsfor" => {
            if !tp.is_array_mode() {
                return Err(Error::parse(format!(
                    "Command '{name}' only available in array mode!"
                )));
            }
            let n = value_of_int(&args[1]);
            if n <= 0 || n > MAX_ARRAY_COLUMN_SPAN {
                return Err(Error::parse(format!("Bad column span in {name}!")));
            }
            let data = if name == "multicolumn" {
                let align = wstr::narrow_bytes(&args[2]);
                let cols = parse_arg(ctx, tp, args[3].clone(), true)?;
                MulticolumnData::new(n, &align, cols)
            } else {
                let mut f = 1.0;
                if !args[2].is_empty() {
                    f = float_of_wide(ctx, &args[2]);
                }
                if !f.is_finite() {
                    f = 1.0;
                }
                let dots = ctx.symbol("ldotp")?;
                let mut data = MulticolumnData::new(n, b"c", Some(dots));
                data.hdots = Some(f);
                data
            };
            let atom = ctx.atom(AtomKind::Multicolumn(data));
            tp.add_atom(ctx, Some(atom));
            tp.formula.add_col_n(n)?;
            Ok(None)
        }
        "multirow" => {
            if !tp.is_array_mode() {
                return Err(Error::parse(
                    "Command \\multirow must used in array environment!",
                ));
            }
            let n = value_of_int(&args[1]);
            if !(-MAX_ARRAY_ROW_SPAN..=MAX_ARRAY_ROW_SPAN).contains(&n) {
                return Err(Error::parse("Bad row span in multirow!"));
            }
            let rows = parse_arg(ctx, tp, args[3].clone(), true)?;
            let atom = ctx.atom(AtomKind::MultiRow {
                rows,
                i: 0,
                j: 0,
                n: if n == 0 { 1 } else { n },
            });
            tp.add_atom(ctx, Some(atom));
            Ok(None)
        }
        "hline" => {
            if !tp.is_array_mode() {
                return Err(Error::parse(
                    "The macro \\hline only available in array mode!",
                ));
            }
            Ok(Some(ctx.atom(AtomKind::Hline {
                width: 0.0,
                shift: 0.0,
                color: TRANSPARENT,
            })))
        }
        "cellcolor" | "rowcolor" => {
            if !tp.is_array_mode() {
                return Err(Error::parse(format!(
                    "Command \\{name} must used in array environment!"
                )));
            }
            let color = color_of(ctx, &wstr::narrow_bytes(&args[1]));
            let atom = ctx.atom(AtomKind::CellSpecifier {
                color,
                foreground: false,
            });
            if name == "cellcolor" {
                tp.formula.add_cell_specifier(atom)?;
            } else {
                tp.formula.add_row_specifier(atom)?;
            }
            Ok(None)
        }
        "columncolor" => {
            let color = color_of(ctx, &wstr::narrow_bytes(&args[1]));
            Ok(Some(ctx.atom(AtomKind::CellSpecifier {
                color,
                foreground: false,
            })))
        }
        "arrayrulecolor" => {
            ctx.line_color = color_of(ctx, &wstr::narrow_bytes(&args[1]));
            Ok(None)
        }
        "newcolumntype" => {
            if args[1].len() > 16 {
                return Err(Error::parse("Column type name is too long!"));
            }
            ctx.column_types.insert(args[1].clone(), args[2].clone());
            Ok(None)
        }
        "intertext" => {
            if !tp.is_array_mode() {
                return Err(Error::parse(
                    "Command \\intertext must used in array environment!",
                ));
            }
            let mut text = args[1].clone();
            wstr::replace_all(&mut text, &w("^{\\prime}"), &w("'"));
            wstr::replace_all(&mut text, &w("^{\\prime\\prime}"), &w("''"));
            let root = parse_sub(ctx, tp, text, "mathnormal", false, false)?.root;
            let roman = ctx.atom(AtomKind::Roman { base: root });
            ctx.get_mut(roman).atom_type = AtomType::InterText;
            tp.add_atom(ctx, Some(roman));
            tp.add_row()?;
            Ok(None)
        }
        "shoveright" | "shoveleft" => {
            let Some(a) = parse_arg(ctx, tp, args[1].clone(), true)? else {
                return Ok(None);
            };
            let copy = ctx.private_copy(a);
            ctx.get_mut(copy).alignment = if name == "shoveright" {
                Alignment::Right
            } else {
                Alignment::Left
            };
            Ok(Some(copy))
        }
        "breakEverywhere" => {
            ctx.break_everywhere = wstr::eq(&args[1], "true");
            Ok(None)
        }
        "Braket" | "Set" => {
            let mut s = args[1].clone();
            let bar = w("\\|");
            let replacement = w("\\middle\\vert ");
            let (mut text, close) = if name == "Braket" {
                wstr::replace_all(&mut s, &bar, &replacement);
                (w("\\left\\langle "), w("\\right\\rangle"))
            } else {
                if let Some(at) = wstr::find(&s, &bar, 0) {
                    s.splice(at..at + bar.len(), replacement);
                }
                (w("\\left\\{"), w("\\right\\}"))
            };
            text.extend(s);
            text.extend(close);
            parse_arg(ctx, tp, text, true)
        }
        "choose" | "brack" | "bangle" | "brace" => {
            let (l, r) = match name {
                "choose" => ("lbrack", "rbrack"),
                "brack" => ("lsqbrack", "rsqbrack"),
                "bangle" => ("langle", "rangle"),
                _ => ("lbrace", "rbrace"),
            };
            let num = tp.pop_formula_atom();
            let text = tp.get_over_argument()?;
            let den = parse_arg(ctx, tp, text, false)?;
            if num.is_none() || den.is_none() {
                return Err(Error::parse(
                    "Both numerator and denominator of choose can't be empty!",
                ));
            }
            let f = new_fraction(ctx, num, den, false);
            let l = ctx.new_symbol(l, AtomType::Opening);
            let r = ctx.new_symbol(r, AtomType::Closing);
            Ok(Some(new_fenced(ctx, Some(f), Some(l), Some(r))))
        }
        "binom" => {
            let num = parse_arg(ctx, tp, args[1].clone(), false)?;
            let den = parse_arg(ctx, tp, args[2].clone(), false)?;
            if num.is_none() || den.is_none() {
                return Err(Error::parse(
                    "Both binomial coefficients must be not empty!",
                ));
            }
            let f = new_fraction(ctx, num, den, false);
            let l = ctx.new_symbol("lbrack", AtomType::Opening);
            let r = ctx.new_symbol("rbrack", AtomType::Closing);
            Ok(Some(new_fenced(ctx, Some(f), Some(l), Some(r))))
        }
        "above" => {
            let num = tp.pop_formula_atom();
            let (unit, value) = tp.get_length()?;
            if unit == Unit::None {
                return Err(Error::parse("Missing dimension for \\above!"));
            }
            let text = tp.get_over_argument()?;
            let den = parse_arg(ctx, tp, text, false)?;
            if num.is_none() || den.is_none() {
                return Err(Error::parse(
                    "Both numerator and denominator of a fraction can't be empty!",
                ));
            }
            Ok(Some(new_fraction_with(ctx, num, den, true, unit, value)))
        }
        "overwithdelims" => frac_with_delims(tp, ctx, args, true, false),
        "atopwithdelims" => frac_with_delims(tp, ctx, args, false, false),
        "abovewithdelims" => frac_with_delims(tp, ctx, args, true, true),
        "cfrac" => {
            let num_align = if wstr::eq(&args[3], "r") {
                Alignment::Right
            } else if wstr::eq(&args[3], "l") {
                Alignment::Left
            } else {
                Alignment::Center
            };
            let num = parse_arg(ctx, tp, args[1].clone(), false)?;
            let den = parse_arg(ctx, tp, args[2].clone(), false)?;
            if num.is_none() || den.is_none() {
                return Err(Error::parse(
                    "Both numerator and denominator of a fraction can't be empty!",
                ));
            }
            let f = new_fraction(ctx, num, den, true);
            if let AtomKind::Fraction(data) = &mut ctx.get_mut(f).kind {
                data.num_align = num_align;
                data.denom_align = Alignment::Center;
                data.use_kern = false;
            }
            ctx.get_mut(f).atom_type = AtomType::Inner;
            let styled = new_style(ctx, TexStyle::Display, Some(f));
            let row = ctx.new_row();
            ctx.row_add(row, Some(styled));
            Ok(Some(row))
        }
        "underscore" => Ok(Some(ctx.atom(AtomKind::UnderScore))),
        "nbsp" => Ok(Some(ctx.atom(AtomKind::Space(SpaceData::blank())))),
        "vdots" => Ok(Some(ctx.atom(AtomKind::Vdots))),
        "ddots" | "iddots" => {
            let dots = ctx.atom(AtomKind::DiagonalDots {
                iddots: name == "iddots",
            });
            Ok(Some(new_typed(
                ctx,
                AtomType::Inner,
                AtomType::Inner,
                Some(dots),
            )?))
        }
        "(" | "[" => {
            let (open, close, style) = if name == "(" {
                ("\\(", "\\)", TexStyle::Text)
            } else {
                ("\\[", "\\]", TexStyle::Display)
            };
            let group = tp.get_group_strings(&w(open), &w(close))?;
            let base = parse_arg(ctx, tp, group, false)?;
            Ok(Some(ctx.atom(AtomKind::Math { style, base })))
        }
        "questeq" => {
            let eq = ctx.symbol("equals")?;
            let quest = ctx.symbol("question")?;
            let sq = scale_atom(ctx, quest, 0.75, 0.75);
            let at = new_under_over_one(ctx, Some(eq), Some(sq), Unit::Mu, 2.5, true, true);
            typed_relation(ctx, at)
        }
        "cong" => {
            let eq = ctx.symbol("equals")?;
            let space = ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Mu, 0.0, 1.5, 0.0)));
            let sim = ctx.symbol("sim")?;
            let vra = ctx.atom(AtomKind::VRow {
                elements: vec![sim, space, eq],
                raise: SpaceData::sized(Unit::Mu, -1.0, 0.0, 0.0),
                add_interline: false,
                valign: Alignment::Center,
                halign: Alignment::None,
            });
            typed_relation(ctx, vra)
        }
        "doteq" => {
            let at = under_over_symbols(ctx, "equals", "ldotp", 3.7)?;
            typed_relation(ctx, at)
        }
        "dotminus" => {
            let at = under_over_symbols(ctx, "minus", "normaldot", -3.3)?;
            Ok(Some(new_typed(
                ctx,
                AtomType::BinaryOperator,
                AtomType::BinaryOperator,
                Some(at),
            )?))
        }
        "ratio" => {
            let at = colon(ctx)?;
            typed_relation(ctx, at)
        }
        "geoprop" => {
            let d1 = ctx.symbol("normaldot")?;
            let ddot = ctx.new_row_of(Some(d1));
            let space = ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Mu, 4.0, 0.0, 0.0)));
            ctx.row_add(ddot, Some(space));
            let d2 = ctx.symbol("normaldot")?;
            ctx.row_add(ddot, Some(d2));
            let minus = ctx.symbol("minus")?;
            let at = ctx.atom(AtomKind::UnderOver(Box::new(UnderOverData {
                base: Some(minus),
                under: Some(ddot),
                under_unit: Unit::Mu,
                under_space: -3.4,
                under_small: false,
                over: Some(ddot),
                over_unit: Unit::Mu,
                over_space: -3.4,
                over_small: false,
            })));
            typed_relation(ctx, at)
        }
        "smallfrowneq" => {
            let eq = ctx.symbol("equals")?;
            let frown = ctx.symbol("smallfrown")?;
            let at = new_under_over_one(ctx, Some(eq), Some(frown), Unit::Mu, -2.0, true, true);
            typed_relation(ctx, at)
        }
        "minuscolon" | "minuscoloncolon" | "simcolon" | "simcoloncolon" | "approxcolon"
        | "approxcoloncolon" | "coloncolon" | "equalscolon" | "equalscoloncolon" | "colonminus"
        | "coloncolonminus" | "colonequals" | "coloncolonequals" | "colonsim" | "coloncolonsim"
        | "colonapprox" | "coloncolonapprox" => colon_relation(ctx, name),
        "char" => {
            let x = wstr::narrow_bytes(&args[1]);
            let (digits, radix) = if x.starts_with(b"0x") || x.starts_with(b"0X") {
                (&x[2..], 16)
            } else if x.starts_with(b"x") || x.starts_with(b"X") {
                (&x[1..], 16)
            } else if x.starts_with(b"0") {
                (&x[1..], 8)
            } else {
                (&x[..], 10)
            };
            let n = str_to_int(digits, radix).unwrap_or(0);
            tp.convert_character(ctx, n as u16, true)
        }
        "surd" => {
            let at = ctx.symbol("surdsign")?;
            Ok(Some(ctx.atom(AtomKind::VCentered { at })))
        }
        "int" | "oint" => {
            let s = ctx.symbol(name)?;
            let copy = ctx.private_copy(s);
            ctx.get_mut(copy).limits = LimitsType::NoLimits;
            Ok(Some(copy))
        }
        "iint" | "iiint" | "iiiint" => {
            let s = ctx.symbol("int")?;
            let i = ctx.private_copy(s);
            ctx.get_mut(i).limits = LimitsType::NoLimits;
            let n = match name {
                "iint" => 2,
                "iiint" => 3,
                _ => 4,
            };
            let row = ctx.new_row_of(Some(i));
            for _ in 1..n {
                let space = ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Mu, -8.0, 0.0, 0.0)));
                ctx.row_add(row, Some(space));
                ctx.row_add(row, Some(i));
            }
            look_at_last_atom(ctx, row);
            Ok(Some(new_typed(
                ctx,
                AtomType::BigOperator,
                AtomType::BigOperator,
                Some(row),
            )?))
        }
        "idotsint" => {
            let s = ctx.symbol("int")?;
            let i = ctx.private_copy(s);
            ctx.get_mut(i).limits = LimitsType::NoLimits;
            let row = ctx.new_row_of(Some(i));
            let space = ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Mu, -1.0, 0.0, 0.0)));
            ctx.row_add(row, Some(space));
            let cdotp = ctx.symbol("cdotp")?;
            let cdots = ctx.new_row_of(Some(cdotp));
            ctx.row_add(cdots, Some(cdotp));
            ctx.row_add(cdots, Some(cdotp));
            let inner = new_typed(ctx, AtomType::Inner, AtomType::Inner, Some(cdots))?;
            ctx.row_add(row, Some(inner));
            let space = ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Mu, -1.0, 0.0, 0.0)));
            ctx.row_add(row, Some(space));
            ctx.row_add(row, Some(i));
            look_at_last_atom(ctx, row);
            Ok(Some(new_typed(
                ctx,
                AtomType::BigOperator,
                AtomType::BigOperator,
                Some(row),
            )?))
        }
        "tiny" | "scriptsize" | "footnotesize" | "small" | "normalsize" | "large" | "Large"
        | "LARGE" | "huge" | "Huge" => {
            let factor = match name {
                "tiny" => 0.5,
                "scriptsize" => 0.7,
                "footnotesize" => 0.8,
                "small" => 0.9,
                "large" => 1.2,
                "Large" => 1.4,
                "LARGE" => 1.8,
                "huge" => 2.0,
                "Huge" => 2.5,
                _ => 1.0,
            };
            let text = tp.get_over_argument()?;
            let math_mode = tp.is_math_mode();
            let base = parse_sub(ctx, tp, text, "", false, math_mode)?
                .root
                .ok_or_else(|| Error::invalid_state("Nothing to scale."))?;
            let atom_type = ctx.get(base).atom_type;
            let id = ctx.atom(AtomKind::MonoScale { base, factor });
            ctx.get_mut(id).atom_type = atom_type;
            Ok(Some(id))
        }
        "overset" | "underset" => {
            let base = parse_arg(ctx, tp, args[2].clone(), false)?;
            let script = parse_arg(ctx, tp, args[1].clone(), false)?;
            let (space, over) = if name == "overset" {
                (2.5, true)
            } else {
                (0.5, false)
            };
            let at = new_under_over_one(ctx, base, script, Unit::Mu, space, true, over);
            typed_relation(ctx, at)
        }
        "accentset" => {
            let base = parse_arg(ctx, tp, args[2].clone(), false)?;
            let accent = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(new_accented(ctx, base, accent, true)?))
        }
        "undertilde" => {
            let a = parse_arg(ctx, tp, args[1].clone(), false)?;
            let elements = ctx.new_row_of(a);
            let p = ctx.atom(AtomKind::Phantom {
                elements,
                w: true,
                h: false,
                d: false,
            });
            let acc = new_accented_named(ctx, Some(p), "widetilde")?;
            Ok(Some(new_under_over_one(
                ctx,
                a,
                Some(acc),
                Unit::Mu,
                0.3,
                true,
                false,
            )))
        }
        "boldsymbol" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(ctx.atom(AtomKind::Bold { base })))
        }
        "fgcolor" => {
            let a = parse_arg(ctx, tp, args[2].clone(), true)?;
            let c = color_of(ctx, &wstr::narrow_bytes(&args[1]));
            Ok(Some(new_color(ctx, a, TRANSPARENT, c)))
        }
        "boxed" => {
            let a = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(new_fbox(ctx, a, TRANSPARENT, TRANSPARENT)))
        }
        "Text" => {
            let base = parse_sub(ctx, tp, args[1].clone(), "mathnormal", false, false)?.root;
            Ok(Some(ctx.atom(AtomKind::Roman { base })))
        }
        "Textit" | "Textbf" | "Textitbf" => {
            let style = match name {
                "Textit" => 2,
                "Textbf" => 1,
                _ => 3,
            };
            Ok(Some(ctx.atom(AtomKind::TextRendering {
                text: args[1].clone(),
                style,
                infos: None,
            })))
        }
        "XML" => parse_arg(ctx, tp, args[1].clone(), true),
        "roman" | "Roman" => {
            let x = wstr::narrow_bytes(&args[1]);
            let start = x.iter().position(|&b| !is_space(b)).unwrap_or(x.len());
            let end = x
                .iter()
                .rposition(|&b| !is_space(b))
                .map_or(start, |e| e + 1);
            let trimmed: Vec<u16> = x[start..end.max(start)]
                .iter()
                .map(|&b| u16::from(b))
                .collect();
            let mut num = value_of_int(&trimmed);
            if num > MAX_ROMAN_NUMERAL {
                return Err(Error::parse("Value too large for a roman numeral!"));
            }
            const NUMBERS: [(i32, &str); 13] = [
                (1000, "M"),
                (900, "CM"),
                (500, "D"),
                (400, "CD"),
                (100, "C"),
                (90, "XC"),
                (50, "L"),
                (40, "XL"),
                (10, "X"),
                (9, "IX"),
                (5, "V"),
                (4, "IV"),
                (1, "I"),
            ];
            let mut roman = String::new();
            for (value, letters) in NUMBERS {
                while num >= value {
                    roman.push_str(letters);
                    num -= value;
                }
            }
            if name == "roman" {
                roman = roman.to_ascii_lowercase();
            }
            Ok(parse_plain_with(ctx, w(&roman), false)?.root)
        }
        "includegraphics" | "fcscore" | "GeoGebra" | "dynamic" => Ok(None),
        "sp@breve" | "sp@hat" => {
            let text = if name == "sp@breve" {
                "\\displaystyle\\!\\breve{}"
            } else {
                "\\displaystyle\\widehat{}"
            };
            let root = parse_plain(ctx, w(text))?.root;
            let atom = new_vrow_raised(ctx, root, 0.6);
            Ok(Some(ctx.atom(AtomKind::Smashed {
                atom,
                h: true,
                d: true,
            })))
        }
        "llap" | "rlap" | "clap" | "mathllap" | "mathrlap" | "mathclap" => {
            let at = parse_arg(ctx, tp, args[1].clone(), true)?;
            let kind = if name.len() > 4 {
                name.as_bytes()[4]
            } else {
                name.as_bytes()[0]
            };
            Ok(Some(ctx.atom(AtomKind::Laped {
                at,
                kind: u16::from(kind),
            })))
        }
        "cancel" | "bcancel" | "xcancel" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?
                .ok_or_else(|| Error::parse("Cancel content must not be empty!"))?;
            let kind = match name {
                "cancel" => 0,
                "bcancel" => 1,
                _ => 2,
            };
            Ok(Some(ctx.atom(AtomKind::Cancel { base, kind })))
        }
        "c" | "k" => {
            let base = parse_arg(ctx, tp, args[1].clone(), true)?;
            Ok(Some(ctx.atom(AtomKind::UnderMark {
                base,
                ogonek: name == "k",
            })))
        }
        "IJ" | "ij" => Ok(Some(ctx.atom(AtomKind::IJ {
            upper: name == "IJ",
        }))),
        "TStroke" | "tStroke" => Ok(Some(ctx.atom(AtomKind::TStroke {
            upper: name == "TStroke",
        }))),
        "Lcaron" | "lcaron" => Ok(Some(ctx.atom(AtomKind::LCaron {
            upper: name == "Lcaron",
        }))),
        "tcaron" => Ok(Some(ctx.atom(AtomKind::TCaron))),
        "hstrok" => strok(tp, ctx, b'h', -0.1, "bar", -0.1),
        "Hstrok" => strok(tp, ctx, b'H', -0.28, "textendash", 0.55),
        "dstrok" => strok(tp, ctx, b'd', 0.25, "bar", -0.1),
        "Dstrok" => strok(tp, ctx, b'D', -0.1, "bar", -0.55),
        "overline" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(ctx.atom(AtomKind::Overlined { base })))
        }
        "underline" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(ctx.atom(AtomKind::Underlined { base })))
        }
        "st" => {
            let at = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(ctx.atom(AtomKind::StrikeThrough { at })))
        }
        "textsc" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            Ok(Some(ctx.atom(AtomKind::SmallCap { base })))
        }
        "sc" => {
            let text = tp.get_over_argument()?;
            let math_mode = tp.is_math_mode();
            let base = parse_sub(ctx, tp, text, "", false, math_mode)?.root;
            Ok(Some(ctx.atom(AtomKind::SmallCap { base })))
        }
        "reflectbox" => {
            let base = required(parse_arg(ctx, tp, args[1].clone(), true)?)?;
            Ok(Some(with_base_type(ctx, base, AtomKind::Reflect { base })))
        }
        "rotatebox" => {
            let mut angle = 0.0;
            if !args[1].is_empty() {
                angle = float_of_wide(ctx, &args[1]);
            }
            let base = required(parse_arg(ctx, tp, args[2].clone(), true)?)?;
            let option = rotation_option(&wstr::narrow_bytes(&args[3]));
            Ok(Some(with_base_type(
                ctx,
                base,
                AtomKind::Rotate {
                    base,
                    angle,
                    option,
                },
            )))
        }
        "T" => {
            let base = required(parse_arg(ctx, tp, args[1].clone(), true)?)?;
            let option = rotation_option(b"origin=cc");
            Ok(Some(with_base_type(
                ctx,
                base,
                AtomKind::Rotate {
                    base,
                    angle: 180.0,
                    option,
                },
            )))
        }
        "scalebox" => {
            let mut sx = float_of_wide(ctx, &args[1]);
            let mut sy = if args[3].is_empty() {
                sx
            } else {
                float_of_wide(ctx, &args[3])
            };
            if sx == 0.0 {
                sx = 1.0;
            }
            if sy == 0.0 {
                sy = 1.0;
            }
            let base = required(parse_arg(ctx, tp, args[2].clone(), true)?)?;
            Ok(Some(scale_atom(ctx, base, sx, sy)))
        }
        "resizebox" => {
            let ws = wstr::narrow_bytes(&args[1]);
            let hs = wstr::narrow_bytes(&args[2]);
            let base = required(parse_arg(ctx, tp, args[3].clone(), true)?)?;
            let keep_aspect_ratio = ws == b"!" || hs == b"!";
            let (w_unit, w) = crate::env::parse_length_bytes(&ws);
            let (h_unit, h) = crate::env::parse_length_bytes(&hs);
            Ok(Some(with_base_type(
                ctx,
                base,
                AtomKind::Resize {
                    base,
                    w_unit,
                    w,
                    h_unit,
                    h,
                    keep_aspect_ratio,
                },
            )))
        }
        "shadowbox" | "ovalbox" | "doublebox" => {
            let base = parse_arg(ctx, tp, args[1].clone(), true)?;
            let kind = match name {
                "shadowbox" => Frame::Shadow,
                "ovalbox" => Frame::Oval,
                _ => Frame::Double,
            };
            let (base, atom_type) = match base {
                None => (ctx.new_row(), AtomType::Ordinary),
                Some(b) => (b, ctx.get(b).atom_type),
            };
            let id = ctx.atom(AtomKind::Frame { base, kind });
            ctx.get_mut(id).atom_type = atom_type;
            Ok(Some(id))
        }
        "cornersize" => {
            let mut size = float_of_wide(ctx, &args[1]);
            if !(size > 0.0 && size <= 0.5) {
                size = 0.5;
            }
            ctx.oval_multiplier = size;
            ctx.oval_diameter = 0.0;
            Ok(None)
        }
        "prescript" => {
            let base = parse_arg(ctx, tp, args[3].clone(), true)?;
            let elements = ctx.new_row_of(base);
            let p = ctx.atom(AtomKind::Phantom {
                elements,
                w: false,
                h: true,
                d: true,
            });
            let sup = parse_arg(ctx, tp, args[1].clone(), true)?;
            let sub = parse_arg(ctx, tp, args[2].clone(), true)?;
            let s = new_scripts(ctx, Some(p), sub, sup, Alignment::Right)?;
            tp.add_atom(ctx, Some(s));
            let space = ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Mu, -0.3, 0.0, 0.0)));
            tp.add_atom(ctx, Some(space));
            Ok(Some(new_typed(
                ctx,
                AtomType::Ordinary,
                AtomType::Ordinary,
                base,
            )?))
        }
        "magnification" => {
            let x = float_of_wide(ctx, &args[1]);
            if x.is_finite() && x > 0.0 {
                ctx.mag_factor = x / 1000.0;
            }
            Ok(None)
        }
        "DeclareMathSizes" => {
            let ds = float_of_wide(ctx, &args[1]);
            let ts = float_of_wide(ctx, &args[2]);
            let ss = float_of_wide(ctx, &args[3]);
            float_of_wide(ctx, &args[4]);
            let sss = ss;
            if !ds.is_finite() || !ts.is_finite() || !ss.is_finite() || ds == 0.0 {
                return Ok(None);
            }
            let sf = (ss / ds).abs();
            let ssf = (sss / ds).abs();
            let tf = (ts / ds).abs();
            let bad = |v: f32| !v.is_finite() || v <= 0.0;
            if bad(sf) || bad(ssf) || bad(tf) {
                return Ok(None);
            }
            ctx.settings.script_factor = sf;
            ctx.settings.script_script_factor = ssf;
            ctx.settings.text_factor = tf;
            Ok(None)
        }
        "overrightarrow"
        | "overleftarrow"
        | "overleftrightarrow"
        | "underrightarrow"
        | "underleftarrow"
        | "underleftrightarrow" => {
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            let over = name.starts_with("over");
            let double = name.ends_with("leftrightarrow");
            let left = !double && name.ends_with("leftarrow");
            Ok(Some(ctx.atom(AtomKind::UnderOverArrow {
                base,
                over,
                left,
                double,
            })))
        }
        "xleftarrow" | "xrightarrow" => {
            let over = parse_arg(ctx, tp, args[1].clone(), false)?;
            let under = parse_arg(ctx, tp, args[2].clone(), true)?;
            Ok(Some(ctx.atom(AtomKind::XArrow {
                over,
                under,
                left: name == "xleftarrow",
            })))
        }
        "underbrace" | "overbrace" | "underbrack" | "overbrack" | "underparen" | "overparen" => {
            let symbol = match name {
                "underbrace" => "rbrace",
                "overbrace" => "lbrace",
                "underbrack" => "rsqbrack",
                "overbrack" => "lsqbrack",
                "underparen" => "rbrack",
                _ => "lbrack",
            };
            let base = parse_arg(ctx, tp, args[1].clone(), false)?;
            ctx.symbol(symbol)?;
            let id = ctx.atom(AtomKind::OverUnderDelimiter {
                base,
                script: None,
                symbol,
                kern_unit: Unit::Ex,
                kern: 0.0,
                over: name.starts_with("over"),
            });
            ctx.get_mut(id).atom_type = AtomType::Inner;
            Ok(Some(id))
        }
        "sideset" => {
            let l = parse_arg(ctx, tp, args[1].clone(), true)?;
            let r = parse_arg(ctx, tp, args[2].clone(), true)?;
            let mut op = parse_arg(ctx, tp, args[3].clone(), true)?;
            if op.is_none() {
                let m = ctx.atom(AtomKind::Char {
                    c: u16::from(b'M'),
                    text_style: "mathnormal",
                    math_mode: false,
                    text_symbol: false,
                });
                let elements = ctx.new_row_of(Some(m));
                op = Some(ctx.atom(AtomKind::Phantom {
                    elements,
                    w: false,
                    h: true,
                    d: true,
                }));
            }
            let l = scripts_of_cumulative(ctx, l)?;
            let r = scripts_of_cumulative(ctx, r)?;
            let id = ctx.atom(AtomKind::SideSets {
                base: op,
                left: l,
                right: r,
            });
            let node = ctx.get_mut(id);
            node.atom_type = AtomType::BigOperator;
            node.limits = LimitsType::NoLimits;
            Ok(Some(id))
        }
        "longdiv" => {
            let dividend = value_of_int(&args[1]);
            let divisor = value_of_int(&args[2]);
            if divisor == 0 {
                return Err(Error::parse("Divisor must not be 0."));
            }
            const MAX_OPERAND: i32 = 1_000_000_000;
            if !(-MAX_OPERAND..=MAX_OPERAND).contains(&dividend)
                || !(-MAX_OPERAND..=MAX_OPERAND).contains(&divisor)
            {
                return Err(Error::parse("Operands are too large for longdiv!"));
            }
            long_division(ctx, divisor, dividend).map(Some)
        }
        "externalFont" => {
            let name = String::from_utf8_lossy(&wstr::narrow_bytes(&args[1])).into_owned();
            ctx.text_font = crate::draw::TextFont::new(&name, 0, 10.0);
            Ok(None)
        }
        "joinrel" => {
            let space = ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Mu, -2.6, 0.0, 0.0)));
            Ok(Some(new_typed(
                ctx,
                AtomType::Relation,
                AtomType::Relation,
                Some(space),
            )?))
        }
        _ => Err(unsupported(ctx, &format!("\\{name}"))),
    }
}

/// The delimiter inside a `BigDelimiterAtom`, or the atom itself.
fn unwrap_big(ctx: &Ctx, atom: Option<AtomId>) -> Option<AtomId> {
    match atom.map(|a| &ctx.get(a).kind) {
        Some(AtomKind::BigDelimiter { delim, .. }) => Some(*delim),
        _ => atom,
    }
}

fn is_symbol(ctx: &Ctx, atom: Option<AtomId>) -> bool {
    atom.is_some_and(|a| matches!(ctx.get(a).kind, AtomKind::Symbol { .. }))
}

/// `macro_left`: `\left` ... `\right` as a fenced atom when both delimiters
/// are symbols, else as a row of the three parts.
fn left(tp: &mut Parser, ctx: &mut Ctx, args: &mut Args) -> Result<Option<AtomId>> {
    let grep = tp.get_group_strings(&w(r"\left"), &w(r"\right"))?;
    let l = parse_arg(ctx, tp, args[1].clone(), false)?;
    let l = unwrap_big(ctx, l);
    let r = tp.get_argument(ctx)?;
    let r = unwrap_big(ctx, r);
    if is_symbol(ctx, l) && is_symbol(ctx, r) {
        let tf = parse_sub(ctx, tp, grep, "", false, true)?;
        let base = match tf.root {
            Some(b) => b,
            None => ctx.new_row(),
        };
        let keep = |ctx: &Ctx, s: Option<AtomId>| {
            s.filter(|&s| {
                !matches!(
                    ctx.get(s).kind,
                    AtomKind::Symbol {
                        name: "normaldot",
                        ..
                    }
                )
            })
        };
        let left = keep(ctx, l);
        let right = keep(ctx, r);
        return Ok(Some(ctx.atom(AtomKind::Fenced {
            base,
            left,
            right,
            middle: tf.middle,
        })));
    }
    let row = ctx.new_row();
    ctx.row_add(row, l);
    let inner = parse_arg(ctx, tp, grep, false)?;
    ctx.row_add(row, inner);
    ctx.row_add(row, r);
    Ok(Some(row))
}

/// `FBoxAtom(base, bg, line)`.
fn new_fbox(ctx: &mut Ctx, base: Option<AtomId>, bg: u32, line: u32) -> AtomId {
    let (base, atom_type) = match base {
        None => (ctx.new_row(), AtomType::Ordinary),
        Some(b) => (b, ctx.get(b).atom_type),
    };
    let id = ctx.atom(AtomKind::FBox { base, bg, line });
    ctx.get_mut(id).atom_type = atom_type;
    id
}

/// Classic-locale `isspace` for the `char`s of a narrow string.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t'..=b'\r')
}

/// `trim()`.
fn trim(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&b| !is_space(b)).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&b| !is_space(b))
        .map_or(start, |e| e + 1);
    &s[start..end.max(start)]
}

/// `StrTokenizer(str, delimiters)`: the tokens between delimiters.
fn tokens<'a>(s: &'a [u8], delimiters: &[u8]) -> Vec<&'a [u8]> {
    s.split(|b| delimiters.contains(b))
        .filter(|t| !t.is_empty())
        .collect()
}

/// `colorComponent()`.
fn color_component(value: f32) -> u32 {
    let scaled = value * 255.0;
    if !(scaled > 0.0) {
        return 0;
    }
    if !(scaled < 255.0) {
        return 255;
    }
    scaled as u32
}

/// `rgb(float, float, float)`.
fn rgb(r: f32, g: f32, b: f32) -> u32 {
    (255 << 24) | (color_component(r) << 16) | (color_component(g) << 8) | color_component(b)
}

/// `decode()`: `#RRGGBB` or `#AARRGGBB`.
fn decode(s: &[u8]) -> u32 {
    if s.first() != Some(&b'#') {
        return BLACK;
    }
    let mut c = read_hex_u32(&s[1..]);
    if s.len() == 7 {
        c |= 0xFF00_0000;
    } else if s.len() != 9 {
        return BLACK;
    }
    c
}

fn float_of(ctx: &mut Ctx, s: &[u8]) -> f32 {
    let f = read_float(s);
    if f.unsupported {
        unsupported(ctx, "hexadecimal floats");
    }
    f.value
}

/// `ColorAtom::getColor()`: a name, `#hex`, hex digits, a gray level, or
/// RGB or CMYK components.
pub fn color_of(ctx: &mut Ctx, name: &[u8]) -> u32 {
    if name.is_empty() {
        return BLACK;
    }
    let name = trim(name).to_ascii_lowercase();
    let original = trim(name.as_slice());
    if original.first() == Some(&b'#') {
        return decode(original);
    }
    if !name.contains(&b',') {
        let key = String::from_utf8_lossy(&name).into_owned();
        if let Some(&c) = ctx.user_colors.get(&key) {
            return c;
        }
        if let Ok(i) = tables::COLORS.binary_search_by(|e| e.0.as_bytes().cmp(name.as_slice())) {
            return tables::COLORS[i].1;
        }
        if !name.contains(&b'.') {
            let mut hex = vec![b'#'];
            hex.extend_from_slice(&name);
            return decode(&hex);
        }
        let x = float_of(ctx, &name);
        if x != 0.0 {
            let g = cmin(1.0, cmax(x, 0.0));
            return rgb(g, g, g);
        }
        return BLACK;
    }
    let parts = tokens(&name, b";,");
    match parts.len() {
        3 => {
            let r = float_of(ctx, trim(parts[0]));
            let g = float_of(ctx, trim(parts[1]));
            let b = float_of(ctx, trim(parts[2]));
            if r == 0.0 && g == 0.0 && b == 0.0 {
                return BLACK;
            }
            let whole = |v: f32| v.is_finite() && v == v.floor();
            if whole(r) && whole(g) && whole(b) && parts.iter().all(|p| !trim(p).contains(&b'.')) {
                let c = |v: f32| cmin(255.0, cmax(0.0, v)) as i32 as u32;
                return (255 << 24) | (c(r) << 16) | (c(g) << 8) | c(b);
            }
            rgb(
                cmin(1.0, cmax(0.0, r)),
                cmin(1.0, cmax(0.0, g)),
                cmin(1.0, cmax(0.0, b)),
            )
        }
        4 => {
            let v: Vec<f32> = parts.iter().map(|p| float_of(ctx, trim(p))).collect();
            if v.iter().all(|&x| x == 0.0) {
                return BLACK;
            }
            let c: Vec<f32> = v.iter().map(|&x| cmin(1.0, cmax(0.0, x))).collect();
            let kk = 1.0 - c[3];
            rgb(kk * (1.0 - c[0]), kk * (1.0 - c[1]), kk * (1.0 - c[2]))
        }
        _ => BLACK,
    }
}

/// `macro_definecolor`.
fn define_color(ctx: &mut Ctx, args: &mut Args) -> Result<Option<AtomId>> {
    let cs = wstr::narrow_bytes(&args[3]);
    let model = args[2].clone();
    let c = if wstr::eq(&model, "gray") {
        let f = float_of(ctx, &cs);
        rgb(f, f, f)
    } else if wstr::eq(&model, "rgb") {
        let parts = tokens(&cs, b":,");
        if parts.len() != 3 {
            return Err(Error::parse(
                "The color definition must have three components!",
            ));
        }
        let r = float_of(ctx, trim(parts[0]));
        let g = float_of(ctx, trim(parts[1]));
        let b = float_of(ctx, trim(parts[2]));
        rgb(r, g, b)
    } else if wstr::eq(&model, "cmyk") {
        let parts = tokens(&cs, b":,");
        if parts.len() != 4 {
            return Err(Error::parse(
                "The color definition must have four components!",
            ));
        }
        let v: Vec<f32> = parts.iter().map(|p| float_of(ctx, trim(p))).collect();
        let k = 1.0 - v[3];
        rgb(k * (1.0 - v[0]), k * (1.0 - v[1]), k * (1.0 - v[2]))
    } else {
        return Err(Error::parse("Color model is incorrect!"));
    };
    let name = String::from_utf8_lossy(&wstr::narrow_bytes(&args[1])).into_owned();
    ctx.user_colors.insert(name, c);
    Ok(None)
}

fn underbase_of(ctx: &Ctx, base: Option<AtomId>) -> Option<AtomId> {
    match base.map(|b| &ctx.get(b).kind) {
        Some(AtomKind::Accented { underbase, .. }) => *underbase,
        _ => base,
    }
}

/// `AccentedAtom(base, name)`: the accent named in the symbol table.
fn new_accented_named(ctx: &mut Ctx, base: Option<AtomId>, name: &str) -> Result<AtomId> {
    let accent = ctx.symbol(name)?;
    if ctx.get(accent).atom_type != AtomType::Accent {
        return Err(Error::new(
            crate::error::ErrorKind::Other,
            format!(
                "The symbol with the name '{name}' is not defined as an accent (type='acc') in 'TeXSymbols'!"
            ),
        ));
    }
    let underbase = underbase_of(ctx, base);
    Ok(ctx.atom(AtomKind::Accented {
        accent,
        acc: false,
        change_size: true,
        base,
        underbase,
    }))
}

/// `AccentedAtom(base, accent, changeSize)`: the accent given as an atom.
fn new_accented(
    ctx: &mut Ctx,
    base: Option<AtomId>,
    accent: Option<AtomId>,
    change_size: bool,
) -> Result<AtomId> {
    let underbase = underbase_of(ctx, base);
    let Some(accent) = accent.filter(|&a| matches!(ctx.get(a).kind, AtomKind::Symbol { .. }))
    else {
        return Err(Error::new(
            crate::error::ErrorKind::Other,
            "Invalid accent!",
        ));
    };
    Ok(ctx.atom(AtomKind::Accented {
        accent,
        acc: true,
        change_size,
        base,
        underbase,
    }))
}

/// `VRowAtom(el)` raised by `r` ex.
fn new_vrow_raised(ctx: &mut Ctx, el: Option<AtomId>, r: f32) -> AtomId {
    let elements = match el {
        None => Vec::new(),
        Some(e) => match &ctx.get(e).kind {
            AtomKind::VRow { elements, .. } => elements.clone(),
            _ => vec![e],
        },
    };
    ctx.atom(AtomKind::VRow {
        elements,
        raise: SpaceData::sized(Unit::Ex, r, 0.0, 0.0),
        add_interline: false,
        valign: Alignment::Center,
        halign: Alignment::None,
    })
}

fn scale_atom(ctx: &mut Ctx, base: AtomId, sx: f32, sy: f32) -> AtomId {
    let atom_type = ctx.get(base).atom_type;
    let id = ctx.atom(AtomKind::Scale { base, sx, sy });
    ctx.get_mut(id).atom_type = atom_type;
    id
}

/// `macro_sfrac`: a slanted fraction.
fn sfrac(tp: &mut Parser, ctx: &mut Ctx, args: &mut Args) -> Result<Option<AtomId>> {
    let num = parse_arg(ctx, tp, args[1].clone(), false)?;
    let den = parse_arg(ctx, tp, args[2].clone(), false)?;
    let (Some(num), Some(den)) = (num, den) else {
        return Err(Error::parse(
            "Both numerator and denominator of a fraction can't be empty!",
        ));
    };
    let (mut sx, mut sy, mut r, mut sl, mut sr) = (0.75, 0.75, 0.45, -0.13, -0.065);
    let mut slash = ctx.symbol("slash")?;
    if !tp.is_math_mode() {
        (sx, sy, r, sl, sr) = (0.6, 0.5, 0.75, -0.24, -0.24);
        let solidus = ctx.symbol("textfractionsolidus")?;
        let inner = scale_atom(ctx, solidus, 1.25, 0.65);
        slash = new_vrow_raised(ctx, Some(inner), 0.4);
    }
    let scaled = scale_atom(ctx, num, sx, sy);
    let snum = new_vrow_raised(ctx, Some(scaled), r);
    let row = ctx.new_row_of(Some(snum));
    let a = ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Em, sl, 0.0, 0.0)));
    ctx.row_add(row, Some(a));
    ctx.row_add(row, Some(slash));
    let b = ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Em, sr, 0.0, 0.0)));
    ctx.row_add(row, Some(b));
    let d = scale_atom(ctx, den, sx, sy);
    ctx.row_add(row, Some(d));
    Ok(Some(row))
}

/// `macro_genfrac`: `\genfrac{left}{right}{thickness}{style}{num}{den}`.
fn genfrac(tp: &mut Parser, ctx: &mut Ctx, args: &mut Args) -> Result<Option<AtomId>> {
    let left = parse_arg(ctx, tp, args[1].clone(), false)?;
    let left = left.filter(|&a| matches!(ctx.get(a).kind, AtomKind::Symbol { .. }));
    let right = parse_arg(ctx, tp, args[2].clone(), false)?;
    let right = right.filter(|&a| matches!(ctx.get(a).kind, AtomKind::Symbol { .. }));
    let mut rule = true;
    let (mut unit, mut value) = parse_length(&args[3]);
    if args[3].is_empty() {
        unit = Unit::Em;
        value = 0.0;
        rule = false;
    }
    let style = if args[4].is_empty() {
        0
    } else {
        value_of_int(&args[4])
    };
    if !(0..=3).contains(&style) {
        return Err(Error::parse("Bad style in genfrac!"));
    }
    let num = parse_arg(ctx, tp, args[5].clone(), false)?;
    let den = parse_arg(ctx, tp, args[6].clone(), false)?;
    if num.is_none() || den.is_none() {
        return Err(Error::parse(
            "Both numerator and denominator of a fraction can't be empty!",
        ));
    }
    let fa = new_fraction_with(ctx, num, den, rule, unit, value);
    let keep = |ctx: &Ctx, s: Option<AtomId>| {
        s.filter(|&s| {
            !matches!(
                ctx.get(s).kind,
                AtomKind::Symbol {
                    name: "normaldot",
                    ..
                }
            )
        })
    };
    let (left, right) = (keep(ctx, left), keep(ctx, right));
    let fenced = ctx.atom(AtomKind::Fenced {
        base: fa,
        left,
        right,
        middle: Vec::new(),
    });
    let tex_style = TexStyle::from_index(style * 2).unwrap_or(TexStyle::Display);
    let styled = new_style(ctx, tex_style, Some(fenced));
    let row = ctx.new_row();
    ctx.row_add(row, Some(styled));
    Ok(Some(row))
}

/// `kMaxArrayColumnSpan`.
const MAX_ARRAY_COLUMN_SPAN: i32 = 1000;
/// `kMaxArrayRowSpan`.
const MAX_ARRAY_ROW_SPAN: i32 = 1000;

/// `valueof(str, float&)` on a wide string: a character outside ASCII ends
/// the number as any other character that cannot continue it does.
fn float_of_wide(ctx: &mut Ctx, s: &[u16]) -> f32 {
    let bytes: Vec<u8> = s
        .iter()
        .map(|&c| if c < 0x80 { c as u8 } else { 0x80 })
        .collect();
    float_of(ctx, &bytes)
}

/// `kMaxRomanNumeral`.
const MAX_ROMAN_NUMERAL: i32 = 100_000;

/// An atom the C++ dereferences without a check.
fn required(atom: Option<AtomId>) -> Result<AtomId> {
    atom.ok_or_else(|| Error::invalid_state("A required atom is missing."))
}

/// An atom that takes the type of its base, as `ReflectAtom`, `RotateAtom`
/// and `ResizeAtom` do.
fn with_base_type(ctx: &mut Ctx, base: AtomId, kind: AtomKind) -> AtomId {
    let atom_type = ctx.get(base).atom_type;
    let id = ctx.atom(kind);
    ctx.get_mut(id).atom_type = atom_type;
    id
}

/// `FencedAtom(base, left, right)`: no base makes an empty row and a
/// `normaldot` delimiter is dropped.
fn new_fenced(
    ctx: &mut Ctx,
    base: Option<AtomId>,
    left: Option<AtomId>,
    right: Option<AtomId>,
) -> AtomId {
    let base = match base {
        Some(b) => b,
        None => ctx.new_row(),
    };
    let keep = |ctx: &Ctx, s: Option<AtomId>| {
        s.filter(|&s| {
            !matches!(
                ctx.get(s).kind,
                AtomKind::Symbol {
                    name: "normaldot",
                    ..
                }
            )
        })
    };
    let (left, right) = (keep(ctx, left), keep(ctx, right));
    ctx.atom(AtomKind::Fenced {
        base,
        left,
        right,
        middle: Vec::new(),
    })
}

fn typed_relation(ctx: &mut Ctx, atom: AtomId) -> Result<Option<AtomId>> {
    Ok(Some(new_typed(
        ctx,
        AtomType::Relation,
        AtomType::Relation,
        Some(atom),
    )?))
}

fn look_at_last_atom(ctx: &mut Ctx, row: AtomId) {
    if let AtomKind::Row {
        look_at_last_atom, ..
    } = &mut ctx.get_mut(row).kind
    {
        *look_at_last_atom = true;
    }
}

/// `_underover(base, script, space)`.
fn under_over_symbols(ctx: &mut Ctx, base: &str, script: &str, space: f32) -> Result<AtomId> {
    let b = ctx.symbol(base)?;
    let s = ctx.symbol(script)?;
    Ok(new_under_over_one(
        ctx,
        Some(b),
        Some(s),
        Unit::Mu,
        space,
        false,
        true,
    ))
}

/// `_colon()`.
fn colon(ctx: &mut Ctx) -> Result<AtomId> {
    under_over_symbols(ctx, "normaldot", "normaldot", 5.2)
}

fn em_space(ctx: &mut Ctx, width: f32) -> AtomId {
    ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Em, width, 0.0, 0.0)))
}

/// The `\colonequals` family: a symbol then one or two colons, or one or
/// two colons then a symbol, as a relation.
fn colon_relation(ctx: &mut Ctx, name: &str) -> Result<Option<AtomId>> {
    let row = if name == "coloncolon" {
        let u = colon(ctx)?;
        let row = ctx.new_row_of(Some(u));
        ctx.row_add(row, Some(u));
        row
    } else if let Some(rest) = name.strip_prefix("colon") {
        let (double, symbol) = match rest.strip_prefix("colon") {
            Some(s) => (true, s),
            None => (false, rest),
        };
        let u = colon(ctx)?;
        let row = ctx.new_row_of(Some(u));
        if double {
            ctx.row_add(row, Some(u));
        }
        let space = em_space(ctx, -0.32);
        ctx.row_add(row, Some(space));
        let s = ctx.symbol(symbol)?;
        ctx.row_add(row, Some(s));
        row
    } else {
        let (symbol, double) = match name.strip_suffix("coloncolon") {
            Some(s) => (s, true),
            None => (name.strip_suffix("colon").unwrap_or(name), false),
        };
        let s = ctx.symbol(symbol)?;
        let row = ctx.new_row_of(Some(s));
        let space = em_space(ctx, -0.095);
        ctx.row_add(row, Some(space));
        let u = colon(ctx)?;
        ctx.row_add(row, Some(u));
        if double {
            ctx.row_add(row, Some(u));
        }
        row
    };
    typed_relation(ctx, row)
}

/// `_frac_with_delims()`: `\overwithdelims` and its siblings.
fn frac_with_delims(
    tp: &mut Parser,
    ctx: &mut Ctx,
    args: &Args,
    rule: bool,
    has_length: bool,
) -> Result<Option<AtomId>> {
    let num = tp.pop_formula_atom();
    let (mut unit, mut value) = (Unit::Em, 0.0);
    if has_length {
        (unit, value) = tp.get_length()?;
        if unit == Unit::None {
            return Err(Error::parse("Missing dimension for \\abovewithdelims!"));
        }
    }
    let text = tp.get_over_argument()?;
    let den = parse_arg(ctx, tp, text, false)?;
    if num.is_none() || den.is_none() {
        return Err(Error::parse(
            "Both numerator and denominator of a fraction can't be empty!",
        ));
    }
    let left = parse_arg(ctx, tp, args[1].clone(), false)?;
    let left = unwrap_big(ctx, left);
    let right = parse_arg(ctx, tp, args[2].clone(), false)?;
    let right = unwrap_big(ctx, right);
    let make = |ctx: &mut Ctx| {
        if has_length {
            new_fraction_with(ctx, num, den, true, unit, value)
        } else {
            new_fraction(ctx, num, den, rule)
        }
    };
    if is_symbol(ctx, left) && is_symbol(ctx, right) {
        let f = make(ctx);
        return Ok(Some(new_fenced(ctx, Some(f), left, right)));
    }
    let row = ctx.new_row();
    ctx.row_add(row, left);
    let f = make(ctx);
    ctx.row_add(row, Some(f));
    ctx.row_add(row, right);
    Ok(Some(row))
}

/// `str2int(str, n, radix)`: `strtol` with Windows' 32-bit `long`. `None`
/// when the value is out of range, which leaves the target at 0.
fn str_to_int(s: &[u8], radix: u32) -> Option<i32> {
    let mut i = 0;
    while i < s.len() && is_space(s[i]) {
        i += 1;
    }
    let mut negative = false;
    if i < s.len() && (s[i] == b'+' || s[i] == b'-') {
        negative = s[i] == b'-';
        i += 1;
    }
    if radix == 16
        && i + 2 < s.len()
        && s[i] == b'0'
        && (s[i + 1] == b'x' || s[i + 1] == b'X')
        && s[i + 2].is_ascii_hexdigit()
    {
        i += 2;
    }
    let mut value: i64 = 0;
    while i < s.len() {
        let Some(d) = char::from(s[i]).to_digit(radix) else {
            break;
        };
        value = (value * i64::from(radix) + i64::from(d)).min(1 << 40);
        i += 1;
    }
    let value = if negative { -value } else { value };
    i32::try_from(value).ok()
}

/// `\hstrok` and its siblings: a lapped bar raised over a roman letter.
fn strok(
    tp: &mut Parser,
    ctx: &mut Ctx,
    letter: u8,
    kern: f32,
    symbol: &str,
    raise: f32,
) -> Result<Option<AtomId>> {
    let space = ctx.atom(AtomKind::Space(SpaceData::sized(Unit::Ex, kern, 0.0, 0.0)));
    let ra = ctx.new_row_of(Some(space));
    let s = ctx.symbol(symbol)?;
    ctx.row_add(ra, Some(s));
    let lap = ctx.atom(AtomKind::Laped {
        at: Some(ra),
        kind: u16::from(b'r'),
    });
    let vra = new_vrow_raised(ctx, Some(lap), raise);
    let a = ctx.new_row_of(Some(vra));
    let ch = ctx.atom(AtomKind::Char {
        c: u16::from(letter),
        text_style: tp.formula.text_style,
        math_mode: false,
        text_symbol: false,
    });
    let roman = ctx.atom(AtomKind::Roman { base: Some(ch) });
    ctx.row_add(a, Some(roman));
    Ok(Some(a))
}

/// `CumulativeScriptsAtom::getScriptsAtom()`, for `\sideset`.
fn scripts_of_cumulative(ctx: &mut Ctx, atom: Option<AtomId>) -> Result<Option<AtomId>> {
    let Some(a) = atom else {
        return Ok(None);
    };
    if let AtomKind::CumulativeScripts { base, sub, sup } = ctx.get(a).kind {
        return Ok(Some(new_scripts(
            ctx,
            base,
            Some(sub),
            Some(sup),
            Alignment::Left,
        )?));
    }
    Ok(atom)
}

/// A `double` converted to Windows' 32-bit `long` as x64 does it: out of
/// range gives the "integer indefinite" value.
fn double_to_long(value: f64) -> i32 {
    if (-2_147_483_648.0..2_147_483_648.0).contains(&value) {
        value as i32
    } else {
        i32::MIN
    }
}

/// `LongDivAtom::calculate()`: the quotient, the dividend, then each
/// product and remainder. A negative quotient's sign counts as a digit, as
/// it does in the C++.
fn long_division_steps(divisor: i32, dividend: i32) -> Vec<String> {
    let quotient = dividend.wrapping_div(divisor);
    let mut results = vec![quotient.to_string()];
    let x = quotient.to_string().into_bytes();
    let len = x.len();
    let mut remaining = dividend;
    results.push(remaining.to_string());
    for (i, &digit) in x.iter().enumerate() {
        let power = 10f64.powi((len - i - 1) as i32);
        let b = double_to_long(f64::from(i32::from(digit) - i32::from(b'0')) * power);
        let product = b.wrapping_mul(divisor);
        remaining = remaining.wrapping_sub(product);
        results.push(product.to_string());
        results.push(remaining.to_string());
    }
    results
}

/// `LongDivAtom(divisor, dividend)`: a vertical row of the steps.
fn long_division(ctx: &mut Ctx, divisor: i32, dividend: i32) -> Result<AtomId> {
    let rule = ctx.atom(AtomKind::Rule {
        w_unit: Unit::Ex,
        w: 0.0,
        h_unit: Unit::Ex,
        h: 2.6,
        r_unit: Unit::Ex,
        r: 0.5,
    });
    let mut elements = Vec::new();
    for (i, text) in long_division_steps(divisor, dividend).iter().enumerate() {
        let num = parse_plain(ctx, w(text))?.root;
        if i == 1 {
            let rparen = ctx.symbol("rbrack")?;
            let big = ctx.atom(AtomKind::BigDelimiter {
                delim: rparen,
                size: 1,
            });
            let ph_elements = ctx.new_row_of(Some(big));
            let ph = ctx.atom(AtomKind::Phantom {
                elements: ph_elements,
                w: false,
                h: true,
                d: true,
            });
            let ra = ctx.new_row_of(Some(ph));
            let raised = ctx.atom(AtomKind::Raise {
                base: Some(big),
                r_unit: Unit::X8,
                r: 3.5,
                h_unit: Unit::X8,
                h: 0.0,
                d_unit: Unit::X8,
                d: 0.0,
            });
            let smashed = ctx.atom(AtomKind::Smashed {
                atom: raised,
                h: true,
                d: true,
            });
            ctx.row_add(ra, Some(smashed));
            ctx.row_add(ra, num);
            let oa = ctx.atom(AtomKind::Overlined { base: Some(ra) });
            let divisor_root = parse_plain(ctx, w(&divisor.to_string()))?.root;
            let row = ctx.new_row_of(divisor_root);
            let space = ctx.atom(AtomKind::Space(SpaceData::blank_of(SpaceType::ThinMuSkip)));
            ctx.row_add(row, Some(space));
            ctx.row_add(row, Some(oa));
            elements.push(row);
            continue;
        }
        let row = ctx.new_row_of(num);
        ctx.row_add(row, Some(rule));
        if i % 2 == 0 && i != 0 {
            elements.push(ctx.atom(AtomKind::Underlined { base: Some(row) }));
        } else {
            elements.push(row);
        }
    }
    Ok(ctx.atom(AtomKind::VRow {
        elements,
        raise: SpaceData::sized(Unit::Ex, 0.0, 0.0, 0.0),
        add_interline: false,
        valign: Alignment::Top,
        halign: Alignment::Right,
    }))
}
