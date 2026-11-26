//! `tex::TeXParser` and `tex::Formula`.
//!
//! The parser works on the formula as UTF-16 code units with `int` positions,
//! as the C++ does, because macro expansion splices text into the string and
//! rewinds onto it. Reads one past the end see the terminating NUL the C++
//! sees; every other out-of-range access the C++ makes goes through `wstr`
//! and fails as `std::out_of_range` would.

use crate::atoms::{AtomId, AtomKind, AtomNode, Ctx, SpaceData};
use crate::boxes::Alignment;
use crate::draw::TRANSPARENT;
use crate::enums::{AtomType, Unit};
use crate::env::{parse_length, wide_to_utf8};
use crate::error::{Error, ErrorKind, Result};
use crate::font::TexStyle;
use crate::layout::{new_big_operator, new_scripts};
use crate::macros::{self, MacroInfo};
use crate::tables;
use crate::wstr::{self, WStr, w};

pub const ESCAPE: u16 = b'\\' as u16;
pub const L_GROUP: u16 = b'{' as u16;
pub const R_GROUP: u16 = b'}' as u16;
pub const L_BRACK: u16 = b'[' as u16;
pub const R_BRACK: u16 = b']' as u16;
const DOLLAR: u16 = b'$' as u16;
const DQUOTE: u16 = b'"' as u16;
const PERCENT: u16 = b'%' as u16;
const SUB_SCRIPT: u16 = b'_' as u16;
const SUPER_SCRIPT: u16 = b'^' as u16;
const PRIME: u16 = b'\'' as u16;
const PRIME_UTF: u16 = 0x2019;
const BACKPRIME: u16 = 0x2035;
const DEGRE: u16 = 0x00B0;
const SPACE: u16 = b' ' as u16;

/// `kMaxParseDepth`: nesting the parser accepts.
pub const MAX_PARSE_DEPTH: i32 = 48;
/// `kMaxExpansions`.
const MAX_EXPANSIONS: i32 = 10_000;
/// `kMaxExpandedLength`.
const MAX_EXPANDED_LENGTH: i32 = 64 * 1024;
/// `kMaxExpansionWork`.
const MAX_EXPANSION_WORK: i64 = 32 * 1024 * 1024;

const UNPARSED_CONTENTS: [&str; 6] = [
    "dynamic",
    "Text",
    "Textit",
    "Textbf",
    "Textitbf",
    "externalFont",
];

pub type Args = Vec<WStr>;

/// `tex::Formula`: a root atom and the state the parser builds it with.
#[derive(Clone, Debug, Default)]
pub struct Formula {
    pub root: Option<AtomId>,
    pub middle: Vec<AtomId>,
    pub text_style: &'static str,
    /// Set for an `ArrayFormula`.
    pub array: Option<Box<crate::matrix::ArrayData>>,
}

impl Formula {
    /// `Formula::add()`.
    pub fn add(&mut self, ctx: &mut Ctx, atom: Option<AtomId>) {
        let Some(a) = atom else {
            return;
        };
        if matches!(ctx.get(a).kind, AtomKind::Middle { .. }) {
            self.middle.push(a);
        }
        let Some(root) = self.root else {
            self.root = Some(a);
            return;
        };
        let row = if ctx.get(root).is_row() {
            root
        } else {
            let row = ctx.new_row_of(Some(root));
            self.root = Some(row);
            row
        };
        ctx.row_add(row, Some(a));
        if matches!(ctx.get(a).kind, AtomKind::Typed { .. }) {
            let rt = ctx.right_type(a);
            if rt == AtomType::BinaryOperator || rt == AtomType::Relation {
                let mark = ctx.atom(AtomKind::BreakMark);
                ctx.row_add(row, Some(mark));
            }
        }
    }
}

/// Runs `f` one parse level deeper: `ParseDepthGuard`.
pub fn with_depth<T>(ctx: &mut Ctx, f: impl FnOnce(&mut Ctx) -> Result<T>) -> Result<T> {
    ctx.parse_depth += 1;
    if ctx.parse_depth > MAX_PARSE_DEPTH {
        ctx.parse_depth -= 1;
        return Err(Error::parse("Formula nesting is too deep!"));
    }
    let result = f(ctx);
    ctx.parse_depth -= 1;
    result
}

/// `countExpansion()`.
fn count_expansion(ctx: &mut Ctx, length: i32) -> Result<()> {
    ctx.expansion_work += i64::from(length);
    if ctx.expansion_work > MAX_EXPANSION_WORK {
        return Err(Error::parse("Formula expands too much!"));
    }
    Ok(())
}

fn is_ascii_alpha(c: u16) -> bool {
    (u16::from(b'a')..=u16::from(b'z')).contains(&c)
        || (u16::from(b'A')..=u16::from(b'Z')).contains(&c)
}

fn is_ascii_alnum(c: u16) -> bool {
    is_ascii_alpha(c) || (u16::from(b'0')..=u16::from(b'9')).contains(&c)
}

/// `tex::convertToRomanNumber()`: localized digits to ASCII.
pub fn convert_to_roman_number(c: u16) -> u16 {
    const RANGES: [(u16, u16); 21] = [
        (0x660, 0x630),
        (0x6f0, 0x6c0),
        (0x966, 0x936),
        (0x9e6, 0x9b6),
        (0xa66, 0xa36),
        (0xae6, 0xab6),
        (0xb66, 0xb36),
        (0xc66, 0xc36),
        (0xd66, 0xd36),
        (0xe50, 0xe20),
        (0xed0, 0xea0),
        (0xf20, 0xe90),
        (0x1040, 0x1010),
        (0x17e0, 0x17b0),
        (0x1810, 0x17e0),
        (0x1b50, 0x1b20),
        (0x1bb0, 0x1b80),
        (0x1c40, 0x1c10),
        (0x1c50, 0x1c20),
        (0xa8d0, 0xa8a0),
        (0, 0),
    ];
    if c == 0x66b {
        return u16::from(b'.');
    }
    for &(start, offset) in &RANGES[..20] {
        if (start..=start + 9).contains(&c) {
            return c - offset;
        }
    }
    c
}

/// `UnicodeBlock::of()`: the first defined block holding `c`, else the
/// unknown block.
fn block_of(c: u16) -> (u16, u16) {
    for &(start, end) in &tables::UNICODE_BLOCKS {
        if c >= start && c <= end {
            return (start, end);
        }
    }
    (0xFFFF, 0xFFFF)
}

/// `UnicodeBlock::contains()`.
fn block_contains(block: (u16, u16), c: u16) -> bool {
    if block == (0xFFFF, 0xFFFF) {
        return !tables::UNICODE_BLOCKS
            .iter()
            .any(|&(start, end)| c >= start && c <= end);
    }
    c >= block.0 && c <= block.1
}

fn find_mapping(table: &'static [(i32, &'static str)], c: u16) -> Option<&'static str> {
    let key = i32::from(c);
    table
        .binary_search_by(|e| e.0.cmp(&key))
        .ok()
        .map(|i| table[i].1)
}

pub struct Parser {
    pub latex: WStr,
    pub pos: i32,
    spos: i32,
    pub len: i32,
    line: i32,
    col: i32,
    group: i32,
    at_is_letter: i32,
    expansions: i32,
    insertion: bool,
    array_mode: bool,
    math_mode: bool,
    partial: bool,
    hide_unknown_char: bool,
    pub formula: Formula,
}

impl Parser {
    /// `TeXParser::init()`, with the math mode the five-argument constructor
    /// sets once preprocessing is done.
    pub fn new(
        ctx: &mut Ctx,
        partial: bool,
        latex: WStr,
        formula: Formula,
        first_pass: bool,
        math_mode: bool,
    ) -> Result<Self> {
        let mut parser = Self {
            latex: Vec::new(),
            pos: 0,
            spos: 0,
            len: 0,
            line: 0,
            col: 0,
            group: 0,
            at_is_letter: 0,
            expansions: 0,
            insertion: false,
            array_mode: false,
            math_mode: true,
            partial,
            hide_unknown_char: true,
            formula,
        };
        if !latex.is_empty() {
            parser.len = latex.len() as i32;
            parser.latex = latex;
            if first_pass {
                parser.preprocess(ctx)?;
            }
        }
        parser.array_mode = parser.formula.array.is_some();
        parser.math_mode = math_mode;
        Ok(parser)
    }

    pub fn is_partial(&self) -> bool {
        self.partial
    }

    pub fn is_math_mode(&self) -> bool {
        self.math_mode
    }

    pub fn is_array_mode(&self) -> bool {
        self.array_mode
    }

    pub fn line(&self) -> i32 {
        self.line
    }

    pub fn col(&self) -> i32 {
        self.pos - self.col - 1
    }

    pub fn is_at_letter(&self) -> bool {
        self.at_is_letter != 0
    }

    pub fn make_at_letter(&mut self) {
        self.at_is_letter += 1;
    }

    pub fn make_at_other(&mut self) {
        self.at_is_letter -= 1;
    }

    /// `_latex[i]`, with the NUL a `std::wstring` holds at its end.
    pub fn at(&self, i: i32) -> u16 {
        if i >= 0 && i < self.len {
            self.latex[i as usize]
        } else {
            0
        }
    }

    fn substr(&self, pos: i32, count: i32) -> Result<WStr> {
        wstr::substr(&self.latex, i64::from(pos), i64::from(count))
    }

    pub fn is_valid_char_in_cmd(&self, c: u16) -> bool {
        is_ascii_alpha(c) || (self.at_is_letter != 0 && c == u16::from(b'@'))
    }

    pub fn pop_last_atom(&mut self, ctx: &mut Ctx) -> Option<AtomId> {
        let a = self.formula.root;
        if let Some(root) = a
            && ctx.get(root).is_row()
        {
            return Some(ctx.row_pop_last(root));
        }
        self.formula.root = None;
        a
    }

    pub fn pop_formula_atom(&mut self) -> Option<AtomId> {
        self.formula.root.take()
    }

    pub fn add_atom(&mut self, ctx: &mut Ctx, atom: Option<AtomId>) {
        self.formula.add(ctx, atom);
    }

    /// `getDollarGroup()`.
    fn dollar_group(&mut self, open_close: u16) -> Result<WStr> {
        let spos = self.pos;
        let mut ch;
        loop {
            ch = self.at(self.pos);
            self.pos += 1;
            if ch == ESCAPE {
                self.pos += 1;
            }
            if !(self.pos < self.len && ch != open_close) {
                break;
            }
        }
        if ch == open_close {
            return self.substr(spos, self.pos - spos - 1);
        }
        self.substr(spos, self.pos - spos)
    }

    /// `getGroup(wchar_t, wchar_t)`.
    pub fn get_group(&mut self, open: u16, close: u16) -> Result<WStr> {
        if self.pos == self.len {
            return Ok(Vec::new());
        }
        let mut ch = self.at(self.pos);
        if self.pos < self.len && ch == open {
            let mut group = 1;
            let spos = self.pos;
            while self.pos < self.len - 1 && group != 0 {
                self.pos += 1;
                ch = self.at(self.pos);
                if ch == open {
                    group += 1;
                } else if ch == close {
                    group -= 1;
                } else if ch == ESCAPE && self.pos != self.len - 1 {
                    self.pos += 1;
                }
            }
            self.pos += 1;
            if group != 0 {
                return self.substr(spos + 1, self.pos - spos - 1);
            }
            return self.substr(spos + 1, self.pos - spos - 2);
        }
        Err(Error::parse(format!(
            "Missing '{}'!",
            wstr::char_message(open & 0xFF)
        )))
    }

    /// `getGroup(const wstring&, const wstring&)`.
    pub fn get_group_strings(&mut self, open: &[u16], close: &[u16]) -> Result<WStr> {
        let mut group = 1;
        let ol = open.len();
        let cl = close.len();
        let last_o = self.is_valid_char_in_cmd(open[ol - 1]);
        let last_c = self.is_valid_char_in_cmd(close[cl - 1]);
        let mut oc = 0usize;
        let mut cc = 0usize;
        let mut start_c = 0i32;
        let mut prev = 0u16;
        let mut buf: WStr = Vec::new();
        while self.pos < self.len && group != 0 {
            let mut c = self.at(self.pos);
            if prev != ESCAPE && c == SPACE {
                loop {
                    if self.pos >= self.len {
                        break;
                    }
                    let x = self.at(self.pos);
                    self.pos += 1;
                    if x != SPACE {
                        break;
                    }
                    buf.push(SPACE);
                }
                self.pos -= 1;
                c = self.at(self.pos);
                if self.is_valid_char_in_cmd(prev) && self.is_valid_char_in_cmd(c) {
                    oc = 0;
                    cc = 0;
                }
            }
            if c == open[oc] {
                oc += 1;
            } else {
                oc = 0;
            }
            if c == close[cc] {
                if cc == 0 {
                    start_c = self.pos;
                }
                cc += 1;
            } else {
                cc = 0;
            }
            if self.pos + 1 < self.len {
                let c1 = self.at(self.pos + 1);
                if oc == ol {
                    if !last_o || !self.is_valid_char_in_cmd(c1) {
                        group += 1;
                    }
                    oc = 0;
                }
                if cc == cl {
                    if !last_c || !self.is_valid_char_in_cmd(c1) {
                        group -= 1;
                    }
                    cc = 0;
                }
            } else {
                if oc == ol {
                    group += 1;
                    oc = 0;
                }
                if cc == cl {
                    group -= 1;
                    cc = 0;
                }
            }
            prev = c;
            buf.push(c);
            self.pos += 1;
        }
        if group != 0 {
            if self.partial {
                return Ok(buf);
            }
            return Err(Error::parse("Parse string not closed correctly!"));
        }
        let count = (buf.len() as u64)
            .wrapping_sub(self.pos as u64)
            .wrapping_add(start_c as u64);
        let count = count.min(buf.len() as u64) as usize;
        Ok(buf[..count].to_vec())
    }

    /// `getOverArgument()`.
    pub fn get_over_argument(&mut self) -> Result<WStr> {
        if self.pos == self.len {
            return Ok(Vec::new());
        }
        let mut ogroup = 1;
        let mut ch = 0u16;
        let spos = self.pos;
        while self.pos < self.len && ogroup != 0 {
            ch = self.at(self.pos);
            match ch {
                L_GROUP => ogroup += 1,
                0x26 => {
                    if ogroup == 1 {
                        ogroup -= 1;
                    }
                }
                R_GROUP => ogroup -= 1,
                ESCAPE => {
                    self.pos += 1;
                    // A \ or \cr at the level of \over ends the argument.
                    let row_end = self.pos < self.len && self.at(self.pos) == ESCAPE;
                    let cr = self.pos < self.len - 1
                        && self.at(self.pos) == u16::from(b'c')
                        && self.at(self.pos + 1) == u16::from(b'r');
                    if (row_end || cr) && ogroup == 1 {
                        ogroup -= 1;
                        self.pos -= 1;
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
        if ogroup >= 2 {
            return Err(Error::parse("Illegal end, missing '}'!"));
        }
        let text = if ogroup == 0 {
            self.substr(spos, self.pos - spos - 1)?
        } else {
            ch = 0;
            self.substr(spos, self.pos - spos)?
        };
        if ch == 0x26 || ch == ESCAPE || ch == R_GROUP {
            self.pos -= 1;
        }
        Ok(text)
    }

    /// `getCommand()`.
    pub fn get_command(&mut self) -> Result<WStr> {
        self.pos += 1;
        let spos = self.pos;
        let mut ch = 0u16;
        while self.pos < self.len {
            ch = self.at(self.pos);
            if !is_ascii_alpha(ch) && (self.at_is_letter == 0 || ch != u16::from(b'@')) {
                break;
            }
            self.pos += 1;
        }
        if ch == 0 {
            return Ok(Vec::new());
        }
        if self.pos == spos {
            self.pos += 1;
        }
        let com = self.substr(spos, self.pos - spos)?;
        if wstr::eq(&com, "cr") && self.pos < self.len && self.at(self.pos) == SPACE {
            self.pos += 1;
        }
        Ok(com)
    }

    /// `insert()`.
    fn insert(&mut self, beg: i32, end: i32, text: &[u16]) -> Result<()> {
        wstr::replace(&mut self.latex, i64::from(beg), i64::from(end - beg), text)?;
        self.len = self.latex.len() as i32;
        self.pos = beg;
        self.insertion = true;
        Ok(())
    }

    /// `getCommandWithArgs()`.
    fn command_with_args(&mut self, ctx: &mut Ctx, command: &[u16]) -> Result<WStr> {
        with_depth(ctx, |ctx| {
            if wstr::eq(command, "left") {
                return self.get_group_strings(&w("\\left"), &w("\\right"));
            }
            let Some(mac) = macros::get(ctx, command) else {
                let mut result = w("\\");
                result.extend_from_slice(command);
                return Ok(result);
            };
            let mut args = Vec::new();
            self.get_opts_args(ctx, mac.argc, mac.pos_opts, &mut args)?;
            let mut result = w("\\");
            result.extend_from_slice(command);
            for j in 0..mac.pos_opts {
                let arg = &args[(mac.argc + j + 1) as usize];
                if !arg.is_empty() {
                    result.push(L_BRACK);
                    result.extend_from_slice(arg);
                    result.push(R_BRACK);
                }
            }
            for j in 0..mac.argc {
                let arg = &args[(j + 1) as usize];
                if !arg.is_empty() {
                    result.push(L_GROUP);
                    result.extend_from_slice(arg);
                    result.push(R_GROUP);
                }
            }
            Ok(result)
        })
    }

    /// `skipWhiteSpace()`.
    pub fn skip_white_space(&mut self) {
        while self.pos < self.len {
            let c = self.at(self.pos);
            if c != SPACE && c != 0x09 && c != 0x0A && c != 0x0D {
                break;
            }
            if c == 0x0A {
                self.line += 1;
                self.col = self.pos;
            }
            self.pos += 1;
        }
    }

    pub fn group_level(&self) -> i32 {
        self.group
    }

    /// `finish()`.
    pub fn finish(&mut self) {
        self.pos = self.latex.len() as i32;
    }

    /// `addRow()`.
    pub fn add_row(&mut self) -> Result<()> {
        if !self.array_mode || self.formula.array.is_none() {
            return Err(Error::parse("Can not add row in none-array mode!"));
        }
        self.formula.add_row()
    }

    /// `forwardBalancedGroup()`: the rest of the current group, leaving the
    /// position on its closing brace.
    pub fn forward_balanced_group(&mut self) -> Result<WStr> {
        if self.group == 0 {
            let sub = wstr::substr_from(&self.latex, i64::from(self.pos))?;
            self.finish();
            return Ok(sub);
        }
        let mut closing = self.group;
        let mut i = self.latex.len() as i64 - 1;
        while i >= i64::from(self.pos) && i >= 0 {
            if self.latex[i as usize] == R_GROUP {
                closing -= 1;
                if closing == 0 {
                    break;
                }
            }
            i -= 1;
        }
        if closing != 0 {
            return Err(Error::parse("Found a closing '}' without an opening '{'!"));
        }
        let sub = self.substr(self.pos, (i - i64::from(self.pos)) as i32)?;
        self.pos = i as i32;
        Ok(sub)
    }

    /// `getOptsArgs()`.
    pub fn get_opts_args(
        &mut self,
        ctx: &mut Ctx,
        argc: i32,
        opts: i32,
        args: &mut Args,
    ) -> Result<()> {
        if !macros::is_valid_argc(argc) {
            return Err(Error::parse("Bad number of arguments for a command!"));
        }
        args.resize((argc + 12) as usize, Vec::new());
        if argc != 0 {
            if opts == 1 {
                self.get_opts(argc, args)?;
            }
            self.get_arg(ctx, 1, args)?;
            if opts == 2 {
                self.get_opts(argc, args)?;
            }
            for i in 2..=argc {
                self.get_arg(ctx, i, args)?;
            }
            if self.math_mode {
                self.skip_white_space();
            }
        }
        Ok(())
    }

    fn get_opts(&mut self, argc: i32, args: &mut Args) -> Result<()> {
        let mut j = argc + 1;
        while j < argc + 11 {
            self.skip_white_space();
            match self.get_group(L_BRACK, R_BRACK) {
                Ok(group) => args[j as usize] = group,
                Err(e) if e.is_parse() => {
                    args[j as usize] = Vec::new();
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
            j += 1;
        }
        Ok(())
    }

    fn get_arg(&mut self, ctx: &mut Ctx, i: i32, args: &mut Args) -> Result<()> {
        self.skip_white_space();
        match self.get_group(L_GROUP, R_GROUP) {
            Ok(group) => args[i as usize] = group,
            Err(e) if e.is_parse() => {
                if self.at(self.pos) != ESCAPE {
                    args[i as usize] = vec![self.at(self.pos)];
                    self.pos += 1;
                } else {
                    let command = self.get_command()?;
                    args[i as usize] = self.command_with_args(ctx, &command)?;
                }
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }

    /// `isValidName()`.
    pub fn is_valid_name(&self, com: &[u16]) -> bool {
        if com.is_empty() || com[0] != ESCAPE {
            return false;
        }
        let mut c = 0u16;
        for &x in &com[1..] {
            c = x;
            if !is_ascii_alpha(c) && (self.at_is_letter == 0 || c != u16::from(b'@')) {
                break;
            }
        }
        is_ascii_alpha(c)
    }

    /// `processEscape()`.
    fn process_escape(&mut self, ctx: &mut Ctx) -> Result<Option<AtomId>> {
        self.spos = self.pos;
        let command = self.get_command()?;
        if command.is_empty() {
            return Ok(Some(ctx.atom(AtomKind::Empty)));
        }
        if let Some(mac) = macros::get(ctx, &command) {
            return self.process_commands(ctx, &command, mac);
        }
        let cmd = String::from_utf8_lossy(&wide_to_utf8(&command)).into_owned();
        match predefined_formula(ctx, &command) {
            Ok(root) => return Ok(root),
            Err(e) if e.kind == ErrorKind::FormulaNotFound => match ctx.symbol(&cmd) {
                Ok(symbol) => return Ok(Some(symbol)),
                Err(e) if e.kind == ErrorKind::SymbolNotFound => {}
                Err(e) => return Err(e),
            },
            Err(e) => return Err(e),
        }
        if !self.partial {
            return Err(Error::parse(format!(
                "Unknown symbol or command or predefined Formula: '{cmd}'"
            )));
        }
        let mut text = w("\\backslash ");
        text.extend_from_slice(&command);
        let shown = parse_plain(ctx, text)?.root;
        let roman = ctx.atom(AtomKind::Roman { base: shown });
        Ok(Some(new_color(
            ctx,
            Some(roman),
            TRANSPARENT,
            UNKNOWN_COMMAND_COLOR,
        )))
    }

    /// `processCommands()`.
    fn process_commands(
        &mut self,
        ctx: &mut Ctx,
        cmd: &[u16],
        mac: MacroInfo,
    ) -> Result<Option<AtomId>> {
        let mut args = Vec::new();
        self.get_opts_args(ctx, mac.argc, mac.pos_opts, &mut args)?;
        args[0] = cmd.to_vec();
        if macros::is_new_command(ctx, cmd) {
            self.expansions += 1;
            if self.expansions > MAX_EXPANSIONS {
                return Err(Error::parse("Too many macro expansions!"));
            }
            count_expansion(ctx, self.len)?;
            let ret = macros::invoke(self, ctx, &mac, &mut args)?;
            let replacement = args.last().cloned().unwrap_or_default();
            self.insert(self.spos, self.pos, &replacement)?;
            if self.len > MAX_EXPANDED_LENGTH {
                return Err(Error::parse("Formula expands too much!"));
            }
            return Ok(ret);
        }
        macros::invoke(self, ctx, &mac, &mut args)
    }

    /// `getScripts()`.
    fn get_scripts(&mut self, ctx: &mut Ctx, first: u16) -> Result<Option<AtomId>> {
        self.pos += 1;
        let mut sub = self.get_argument(ctx)?;
        let mut sup = None;
        let mut second = 0u16;
        if self.pos < self.len {
            second = self.at(self.pos);
        }
        if first == SUPER_SCRIPT && second == SUPER_SCRIPT {
            sup = sub;
            sub = None;
        } else if first == SUB_SCRIPT && second == SUPER_SCRIPT {
            self.pos += 1;
            sup = self.get_argument(ctx)?;
        } else if first == SUPER_SCRIPT && second == SUB_SCRIPT {
            self.pos += 1;
            sup = sub;
            sub = self.get_argument(ctx)?;
        } else if first == SUPER_SCRIPT && second != SUB_SCRIPT {
            sup = sub;
            sub = None;
        }

        let atom = match self.formula.root {
            None => return Ok(Some(new_scripts(ctx, None, sub, sup, Alignment::Left)?)),
            Some(root) if ctx.get(root).is_row() => ctx.row_pop_last(root),
            Some(root) => {
                self.formula.root = None;
                root
            }
        };

        if let AtomKind::CumulativeScripts {
            sub: sub_row,
            sup: sup_row,
            ..
        } = ctx.get(atom).kind
        {
            ctx.row_add(sub_row, sub);
            ctx.row_add(sup_row, sup);
            return Ok(Some(atom));
        }
        if ctx.right_type(atom) == AtomType::BigOperator {
            return Ok(Some(new_big_operator(ctx, Some(atom), sub, sup)?));
        }
        Ok(Some(new_scripts(
            ctx,
            Some(atom),
            sub,
            sup,
            Alignment::Left,
        )?))
    }

    /// `getArgument()`.
    pub fn get_argument(&mut self, ctx: &mut Ctx) -> Result<Option<AtomId>> {
        loop {
            self.skip_white_space();
            if self.pos >= self.len {
                return Ok(Some(ctx.atom(AtomKind::Empty)));
            }
            let ch = self.at(self.pos);
            if ch == L_GROUP {
                let outer = std::mem::take(&mut self.formula);
                let array_mode = self.array_mode;
                self.array_mode = false;
                self.pos += 1;
                self.group += 1;
                let result = self.parse(ctx);
                let inner = std::mem::replace(&mut self.formula, outer);
                self.array_mode = array_mode;
                result?;
                if self.formula.root.is_none() {
                    let row = ctx.new_row();
                    ctx.row_add(row, inner.root);
                    return Ok(Some(row));
                }
                return Ok(inner.root);
            }
            if ch == ESCAPE {
                let atom = self.process_escape(ctx)?;
                if self.insertion {
                    self.insertion = false;
                    continue;
                }
                return Ok(atom);
            }
            let atom = self.convert_character(ctx, ch, true)?;
            self.pos += 1;
            return Ok(atom);
        }
    }

    /// `getLength()`.
    pub fn get_length(&mut self) -> Result<(Unit, f32)> {
        if self.pos == self.len {
            return Ok((Unit::None, -1.0));
        }
        let mut ch = 0u16;
        self.skip_white_space();
        let start = self.pos;
        while self.pos < self.len && ch != SPACE && ch != ESCAPE {
            ch = self.at(self.pos);
            self.pos += 1;
        }
        let end = self.pos;
        if ch == ESCAPE {
            self.pos -= 1;
        } else {
            self.skip_white_space();
        }
        let text = self.substr(start, end - start - 1)?;
        Ok(parse_length(&text))
    }

    /// `replaceScript()`.
    fn replace_script(&mut self) -> Result<bool> {
        let ch = self.at(self.pos);
        for (map, name) in [
            (&tables::SUP_SCRIPT_MAP[..], "\\mathcumsup{"),
            (&tables::SUB_SCRIPT_MAP[..], "\\mathcumsub{"),
        ] {
            if let Ok(i) = map.binary_search_by(|e| e.0.cmp(&ch)) {
                let mut text = w(name);
                text.push(u16::from(map[i].1));
                text.push(R_GROUP);
                wstr::replace(&mut self.latex, i64::from(self.pos), 1, &text)?;
                self.len = self.latex.len() as i32;
                self.pos += text.len() as i32;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// `preprocess(cmd, args, pos)`.
    fn preprocess_command(
        &mut self,
        ctx: &mut Ctx,
        cmd: &[u16],
        args: &mut Args,
        pos: &mut i32,
    ) -> Result<()> {
        if wstr::eq(cmd, "newcommand")
            || wstr::eq(cmd, "renewcommand")
            || wstr::eq(cmd, "newenvironment")
            || wstr::eq(cmd, "renewenvironment")
        {
            self.preprocess_new_cmd(ctx, cmd, args, pos)
        } else if macros::is_new_command(ctx, cmd) {
            self.inflate_new_cmd(ctx, cmd, args, pos)
        } else if wstr::eq(cmd, "begin") {
            self.inflate_env(ctx, args, pos)
        } else if wstr::eq(cmd, "makeatletter") {
            self.at_is_letter += 1;
            Ok(())
        } else if wstr::eq(cmd, "makeatother") {
            self.at_is_letter -= 1;
            Ok(())
        } else if UNPARSED_CONTENTS.iter().any(|name| wstr::eq(cmd, name)) {
            self.get_opts_args(ctx, 1, 0, args)
        } else {
            Ok(())
        }
    }

    /// `preprocessNewCmd()`.
    fn preprocess_new_cmd(
        &mut self,
        ctx: &mut Ctx,
        cmd: &[u16],
        args: &mut Args,
        pos: &mut i32,
    ) -> Result<()> {
        let mac = macros::get(ctx, cmd).ok_or_else(|| Error::invalid_state("Missing macro."))?;
        self.get_opts_args(ctx, mac.argc, mac.pos_opts, args)?;
        macros::invoke(self, ctx, &mac, args)?;
        wstr::erase(&mut self.latex, i64::from(*pos), i64::from(self.pos - *pos))?;
        self.len = self.latex.len() as i32;
        self.pos = *pos;
        Ok(())
    }

    /// `inflateNewCmd()`.
    fn inflate_new_cmd(
        &mut self,
        ctx: &mut Ctx,
        cmd: &[u16],
        args: &mut Args,
        pos: &mut i32,
    ) -> Result<()> {
        self.expansions += 1;
        if self.expansions > MAX_EXPANSIONS {
            return Err(Error::parse("Too many macro expansions!"));
        }
        count_expansion(ctx, self.len)?;
        let mac = macros::get(ctx, cmd).ok_or_else(|| Error::invalid_state("Missing macro."))?;
        self.get_opts_args(ctx, mac.argc, mac.pos_opts, args)?;
        args[0] = cmd.to_vec();
        let result = macros::invoke(self, ctx, &mac, args).and_then(|_| {
            let replacement = args.last().cloned().unwrap_or_default();
            wstr::replace(
                &mut self.latex,
                i64::from(*pos),
                i64::from(self.pos - *pos),
                &replacement,
            )
        });
        match result {
            Ok(()) => {}
            Err(e) if e.is_parse() => {
                if !self.partial {
                    return Err(e);
                }
                *pos += cmd.len() as i32 + 1;
            }
            Err(e) => return Err(e),
        }
        self.len = self.latex.len() as i32;
        self.pos = *pos;
        if self.len > MAX_EXPANDED_LENGTH {
            return Err(Error::parse("Formula expands too much!"));
        }
        Ok(())
    }

    /// `inflateEnv()`.
    fn inflate_env(&mut self, ctx: &mut Ctx, args: &mut Args, pos: &mut i32) -> Result<()> {
        self.expansions += 1;
        if self.expansions > MAX_EXPANSIONS {
            return Err(Error::parse("Too many macro expansions!"));
        }
        count_expansion(ctx, self.len)?;
        self.get_opts_args(ctx, 1, 0, args)?;
        let mut env = args[1].clone();
        env.extend_from_slice(&w("@env"));
        let Some(mac) = macros::get(ctx, &env) else {
            return Err(Error::parse(format!(
                "Unknown environment: {} at position {}:{}",
                wstr::narrow(&args[1]),
                self.line(),
                self.col()
            )));
        };
        let mut optargs = Vec::new();
        if mac.argc > 1 {
            self.get_opts_args(ctx, mac.argc - 1, 0, &mut optargs)?;
        }
        let mut open = w("\\begin{");
        open.extend_from_slice(&args[1]);
        open.push(R_GROUP);
        let mut close = w("\\end{");
        close.extend_from_slice(&args[1]);
        close.push(R_GROUP);
        let grp = self.get_group_strings(&open, &close)?;
        let mut expr = w("{\\makeatletter \\");
        expr.extend_from_slice(&args[1]);
        expr.extend_from_slice(&w("@env"));
        for i in 1..mac.argc {
            expr.push(L_GROUP);
            expr.extend_from_slice(&optargs[i as usize]);
            expr.push(R_GROUP);
        }
        expr.push(L_GROUP);
        expr.extend_from_slice(&grp);
        expr.extend_from_slice(&w("}\\makeatother}"));
        wstr::replace(
            &mut self.latex,
            i64::from(*pos),
            i64::from(self.pos - *pos),
            &expr,
        )?;
        self.len = self.latex.len() as i32;
        self.pos = *pos;
        if self.len > MAX_EXPANDED_LENGTH {
            return Err(Error::parse("Formula expands too much!"));
        }
        Ok(())
    }

    /// `preprocess()`.
    fn preprocess(&mut self, ctx: &mut Ctx) -> Result<()> {
        if self.len == 0 {
            return Ok(());
        }
        let mut args: Args = Vec::new();
        while self.pos < self.len {
            if self.replace_script()? {
                continue;
            }
            let ch = self.at(self.pos);
            match ch {
                ESCAPE => {
                    let mut spos = self.pos;
                    let cmd = self.get_command()?;
                    match self.preprocess_command(ctx, &cmd, &mut args, &mut spos) {
                        Ok(()) => {}
                        Err(e) if e.is_parse() => {
                            if !self.partial {
                                return Err(e);
                            }
                        }
                        Err(e) => return Err(e),
                    }
                    args.clear();
                }
                PERCENT => {
                    let spos = self.pos;
                    self.pos += 1;
                    while self.pos < self.len {
                        let chr = self.at(self.pos);
                        self.pos += 1;
                        if chr == 0x0D || chr == 0x0A {
                            break;
                        }
                    }
                    if self.pos < self.len {
                        self.pos -= 1;
                    }
                    wstr::replace(
                        &mut self.latex,
                        i64::from(spos),
                        i64::from(self.pos - spos),
                        &[],
                    )?;
                    self.len = self.latex.len() as i32;
                    self.pos = spos;
                }
                DEGRE => {
                    wstr::replace(&mut self.latex, i64::from(self.pos), 1, &w("^{\\circ}"))?;
                    self.len = self.latex.len() as i32;
                    self.pos += 1;
                }
                _ => self.pos += 1,
            }
        }
        self.pos = 0;
        self.len = self.latex.len() as i32;
        Ok(())
    }

    fn add_empty_root_if_needed(&mut self, ctx: &mut Ctx) {
        if self.formula.root.is_none() && !self.array_mode {
            let empty = ctx.atom(AtomKind::Empty);
            self.formula.add(ctx, Some(empty));
        }
    }

    /// `parse()`.
    pub fn parse(&mut self, ctx: &mut Ctx) -> Result<()> {
        with_depth(ctx, |ctx| self.parse_body(ctx))
    }

    fn parse_body(&mut self, ctx: &mut Ctx) -> Result<()> {
        if self.len == 0 {
            self.add_empty_root_if_needed(ctx);
            return Ok(());
        }
        while self.pos < self.len {
            let ch = self.at(self.pos);
            match ch {
                0x0A | 0x09 | 0x0D => {
                    if ch == 0x0A {
                        self.line += 1;
                        self.col = self.pos;
                    }
                    self.pos += 1;
                }
                SPACE => {
                    self.pos += 1;
                    if !self.math_mode {
                        let space = ctx.atom(AtomKind::Space(SpaceData::blank()));
                        self.formula.add(ctx, Some(space));
                        let mark = ctx.atom(AtomKind::BreakMark);
                        self.formula.add(ctx, Some(mark));
                    }
                }
                DOLLAR => {
                    self.pos += 1;
                    if !self.math_mode {
                        let mut style = TexStyle::Text;
                        let mut double = false;
                        if self.pos < self.len && self.at(self.pos) == DOLLAR {
                            style = TexStyle::Display;
                            double = true;
                            self.pos += 1;
                        }
                        let group = self.dollar_group(DOLLAR)?;
                        let base = parse_arg(ctx, self, group, false)?;
                        let atom = ctx.atom(AtomKind::Math { style, base });
                        self.formula.add(ctx, Some(atom));
                        if double && self.pos < self.len && self.at(self.pos) == DOLLAR {
                            self.pos += 1;
                        }
                    }
                }
                ESCAPE => {
                    let atom = self.process_escape(ctx)?;
                    self.formula.add(ctx, atom);
                    if self.array_mode
                        && atom.is_some_and(|a| matches!(ctx.get(a).kind, AtomKind::Hline { .. }))
                    {
                        self.formula.add_row()?;
                    }
                    if self.insertion {
                        self.insertion = false;
                    }
                }
                L_GROUP => {
                    let mut atom = self.get_argument(ctx)?;
                    if let Some(a) = atom
                        && ctx.get(a).atom_type != AtomType::Ordinary
                    {
                        let copy = ctx.private_copy(a);
                        ctx.get_mut(copy).atom_type = AtomType::Ordinary;
                        atom = Some(copy);
                    }
                    self.formula.add(ctx, atom);
                }
                R_GROUP => {
                    self.group -= 1;
                    self.pos += 1;
                    if self.group == -1 {
                        return Err(Error::parse("Found a closing '}' without an opening '{'!"));
                    }
                    return Ok(());
                }
                SUPER_SCRIPT => {
                    let atom = self.get_scripts(ctx, ch)?;
                    self.formula.add(ctx, atom);
                }
                SUB_SCRIPT => {
                    if self.math_mode {
                        let atom = self.get_scripts(ctx, ch)?;
                        self.formula.add(ctx, atom);
                    } else {
                        let atom = ctx.atom(AtomKind::UnderScore);
                        self.formula.add(ctx, Some(atom));
                        self.pos += 1;
                    }
                }
                0x26 => {
                    if !self.array_mode {
                        return Err(Error::parse(
                            "Character '&' is only available in array mode!",
                        ));
                    }
                    self.formula.add_col()?;
                    self.pos += 1;
                }
                0x7E => {
                    let atom = ctx.atom(AtomKind::Space(SpaceData::blank()));
                    self.formula.add(ctx, Some(atom));
                    self.pos += 1;
                }
                PRIME_UTF | PRIME | BACKPRIME => {
                    let name = if ch == BACKPRIME {
                        "backprime"
                    } else {
                        "prime"
                    };
                    if self.math_mode {
                        let base = self.pop_last_atom(ctx);
                        let symbol = ctx.symbol(name)?;
                        let atom = new_cumulative_scripts(ctx, base, None, Some(symbol));
                        self.formula.add(ctx, Some(atom));
                    } else {
                        let atom = self.convert_character(ctx, ch, true)?;
                        self.formula.add(ctx, atom);
                    }
                    self.pos += 1;
                }
                DQUOTE => {
                    if self.math_mode {
                        for _ in 0..2 {
                            let base = self.pop_last_atom(ctx);
                            let symbol = ctx.symbol("prime")?;
                            let atom = new_cumulative_scripts(ctx, base, None, Some(symbol));
                            self.formula.add(ctx, Some(atom));
                        }
                    } else {
                        for _ in 0..2 {
                            let atom = self.convert_character(ctx, PRIME, true)?;
                            self.formula.add(ctx, atom);
                        }
                    }
                    self.pos += 1;
                }
                _ => {
                    let atom = self.convert_character(ctx, ch, false)?;
                    self.formula.add(ctx, atom);
                    self.pos += 1;
                }
            }
        }
        self.add_empty_root_if_needed(ctx);
        Ok(())
    }

    /// `convertCharacter()`.
    pub fn convert_character(
        &mut self,
        ctx: &mut Ctx,
        c: u16,
        one_char: bool,
    ) -> Result<Option<AtomId>> {
        if self.math_mode {
            if (945..=969).contains(&c) {
                let name = find_mapping(&tables::FORMULA_SYMBOL_MAPPINGS, c).unwrap_or("");
                return Ok(Some(ctx.symbol(name)?));
            } else if (913..=937).contains(&c) {
                let latex = match find_mapping(&tables::FORMULA_FORMULA_MAPPINGS, c) {
                    Some(text) => w(text),
                    None => {
                        ctx.inserted_formula_mappings.insert(c);
                        Vec::new()
                    }
                };
                return Ok(parse_plain(ctx, latex)?.root);
            }
        }
        let c = convert_to_roman_number(c);
        if is_ascii_alnum(c) {
            return Ok(Some(ctx.atom(AtomKind::Char {
                c,
                text_style: self.formula.text_style,
                math_mode: self.math_mode,
                text_symbol: false,
            })));
        }
        let block = block_of(c);
        let symbol = find_mapping(&tables::FORMULA_SYMBOL_MAPPINGS, c);
        let formula = find_mapping(&tables::FORMULA_FORMULA_MAPPINGS, c)
            .or_else(|| ctx.inserted_formula_mappings.contains(&c).then_some(""));
        if symbol.is_none() && formula.is_none() {
            let is_latin = block == (0x0020, 0x007F);
            if !is_latin {
                let infos = Some(("SansSerif", "Serif"));
                if one_char {
                    return Ok(Some(ctx.atom(AtomKind::TextRendering {
                        text: vec![c],
                        style: 0,
                        infos,
                    })));
                }
                let start = self.pos;
                self.pos += 1;
                let mut en = self.len - 1;
                while self.pos < self.len {
                    let x = self.at(self.pos);
                    if !block_contains(block, x) {
                        self.pos -= 1;
                        en = self.pos;
                        break;
                    }
                    self.pos += 1;
                }
                let text = self.substr(start, en - start + 1)?;
                return Ok(Some(ctx.atom(AtomKind::TextRendering {
                    text,
                    style: 0,
                    infos,
                })));
            }
            if !self.partial {
                return Err(Error::parse(format!(
                    "Unknown character : '{}'",
                    wstr::char_message(c)
                )));
            }
            if self.hide_unknown_char {
                return Ok(None);
            }
            return Err(macros::unsupported(ctx, "shown unknown characters"));
        }
        if !self.math_mode
            && let Some(name) = find_mapping(&tables::FORMULA_TEXT_MAPPINGS, c)
        {
            match ctx.symbol(name) {
                Ok(symbol) => {
                    let copy = ctx.private_copy(symbol);
                    if let AtomKind::Symbol { unicode, .. } = &mut ctx.get_mut(copy).kind {
                        *unicode = c;
                    }
                    return Ok(Some(copy));
                }
                Err(e) if e.kind == ErrorKind::SymbolNotFound => {}
                Err(e) => return Err(e),
            }
        }
        if let Some(text) = formula {
            return Ok(parse_plain(ctx, w(text))?.root);
        }
        if let Some(name) = symbol {
            return match ctx.symbol(name) {
                Ok(symbol) => Ok(Some(symbol)),
                Err(e) if e.kind == ErrorKind::SymbolNotFound => Err(Error::parse(format!(
                    "The character '{}' was mapped to an unknown symbol with the name '{}'!\n caused by: {}",
                    wstr::char_message(c),
                    name,
                    e.message
                ))),
                Err(e) => Err(e),
            };
        }
        Ok(Some(ctx.atom(AtomKind::Char {
            c,
            text_style: self.formula.text_style,
            math_mode: self.math_mode,
            text_symbol: false,
        })))
    }
}

/// `ColorAtom(atom, bg, c)`.
pub fn new_color(ctx: &mut Ctx, atom: Option<AtomId>, background: u32, color: u32) -> AtomId {
    let elements = ctx.new_row_of(atom);
    ctx.atom(AtomKind::Color {
        background,
        color,
        elements,
    })
}

/// `CumulativeScriptsAtom(base, sub, sup)`.
pub fn new_cumulative_scripts(
    ctx: &mut Ctx,
    base: Option<AtomId>,
    sub: Option<AtomId>,
    sup: Option<AtomId>,
) -> AtomId {
    let (new_base, sub_row, sup_row) = match base.map(|b| ctx.get(b).kind.clone()) {
        Some(AtomKind::CumulativeScripts {
            base: inner,
            sub: sub_row,
            sup: sup_row,
        }) => {
            ctx.row_add(sup_row, sup);
            ctx.row_add(sub_row, sub);
            (inner, sub_row, sup_row)
        }
        Some(AtomKind::Scripts {
            base: inner,
            sub: old_sub,
            sup: old_sup,
            ..
        }) => {
            let sup_row = ctx.new_row_of(old_sup);
            let sub_row = ctx.new_row_of(old_sub);
            ctx.row_add(sup_row, sup);
            ctx.row_add(sub_row, sub);
            (inner, sub_row, sup_row)
        }
        _ => {
            let sup_row = ctx.new_row_of(sup);
            let sub_row = ctx.new_row_of(sub);
            (base, sub_row, sup_row)
        }
    };
    ctx.add(AtomNode::new(AtomKind::CumulativeScripts {
        base: new_base,
        sub: sub_row,
        sup: sup_row,
    }))
}

/// `Formula(latex)`: a partial parser whose errors are not caught.
pub fn parse_plain(ctx: &mut Ctx, latex: WStr) -> Result<Formula> {
    let mut parser = Parser::new(ctx, true, latex, Formula::default(), true, true)?;
    parser.parse(ctx)?;
    Ok(parser.formula)
}

/// `Formula(tp, latex, textStyle, preprocess, isMathMode)`: a sub-formula
/// that, in a partial parse, keeps what it parsed before an error and never
/// ends up without a root.
pub fn parse_sub(
    ctx: &mut Ctx,
    tp: &Parser,
    latex: WStr,
    text_style: &'static str,
    preprocess: bool,
    math_mode: bool,
) -> Result<Formula> {
    let formula = Formula {
        text_style,
        ..Formula::default()
    };
    let mut parser = Parser::new(ctx, tp.partial, latex, formula, preprocess, math_mode)?;
    if tp.partial {
        if parser.parse(ctx).is_err() && parser.formula.root.is_none() {
            parser.formula.root = Some(ctx.atom(AtomKind::Empty));
        }
    } else {
        parser.parse(ctx)?;
    }
    Ok(parser.formula)
}

/// `Formula(tp, latex, preprocess)`.
pub fn parse_arg(
    ctx: &mut Ctx,
    tp: &Parser,
    latex: WStr,
    preprocess: bool,
) -> Result<Option<AtomId>> {
    Ok(parse_sub(ctx, tp, latex, "", preprocess, true)?.root)
}

/// `Formula::get()`: a predefined formula, parsed from its definition.
pub fn predefined_formula(ctx: &mut Ctx, name: &[u16]) -> Result<Option<AtomId>> {
    let text = String::from_utf8_lossy(&wide_to_utf8(name)).into_owned();
    if let Some(&root) = ctx.predefined.get(name) {
        return Ok(root);
    }
    let Ok(index) = tables::PREDEFINED_FORMULAS
        .binary_search_by(|e| e.0.encode_utf16().cmp(name.iter().copied()))
    else {
        return Err(Error::formula_not_found(&text));
    };
    let formula = parse_plain(ctx, w(tables::PREDEFINED_FORMULAS[index].1))?;
    let root = formula.root;
    if !root.is_some_and(|r| ctx.get(r).is_row()) {
        ctx.predefined.insert(name.to_vec(), root);
    }
    Ok(root)
}

/// `ColorAtom` shown for an unknown command: red.
pub const UNKNOWN_COMMAND_COLOR: u32 = 0xFFFF_0000;
