//! Commands: `MacroInfo`, `NewCommandMacro` and the predefined command
//! implementations of `core/macro_impl.*`.
//!
//! Built-in commands come from the generated table. Commands a formula
//! defines live in the formula's `Ctx`, as the C++ rolls its global registry
//! back to the built-ins before every formula.

use crate::atoms::{AtomId, AtomKind, Ctx, LimitsType, SpaceData};
use crate::enums::{AtomType, SpaceType, Unit};
use crate::error::{Error, Result};
use crate::font::TexStyle;
use crate::layout::new_fraction;
use crate::parser::{Args, Parser, new_cumulative_scripts, parse_arg, parse_sub};
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
        "insertBreakMark" => Ok(Some(ctx.atom(AtomKind::BreakMark))),
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
        "sqrt" => {
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
