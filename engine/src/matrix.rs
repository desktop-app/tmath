//! Arrays: `ArrayFormula`, `MatrixAtom`, `MultlineAtom` and the atoms that
//! only live inside arrays (`core/formula.*`, `atom/atom_matrix.*`).
//!
//! The C++ keeps the column types `\newcolumntype` defines and the rule color
//! `\arrayrulecolor` sets in statics it resets before each formula; here they
//! live in the formula's `Ctx`. The rule color is read when the array is laid
//! out, after the whole formula is parsed, as in the C++.

use std::collections::BTreeMap;
use std::rc::Rc;

use crate::atoms::{AtomId, AtomKind, Ctx, SpaceData};
use crate::boxes::{Alignment, BoxId, cmax};
use crate::enums::{AtomType, SpaceType, Unit};
use crate::env::Environment;
use crate::error::{Error, Result};
use crate::font::{PREC, TexStyle};
use crate::glue;
use crate::layout::{create_box, null_atom, space_box, space_of};
use crate::macros::value_of_int;
use crate::parser::{Formula, Parser};
use crate::wstr::{self, WStr};

/// `kMaxArrayCells`.
const MAX_ARRAY_CELLS: i64 = 100_000;
/// `kMaxColumnSpecSteps`.
const MAX_COLUMN_SPEC_STEPS: i32 = 100_000;
/// `kMaxColumnSpecLength`.
const MAX_COLUMN_SPEC_LENGTH: i32 = 10_000;
/// `kMaxColumnSpecRepeat`.
const MAX_COLUMN_SPEC_REPEAT: i32 = 1000;

/// `ArrayFormula`'s cells and specifiers. `row` and `col` count what the
/// parser has added so far; after `check_dimensions` they are the size.
#[derive(Clone, Debug, PartialEq)]
pub struct ArrayData {
    row: usize,
    col: usize,
    pub cells: Vec<Vec<Option<AtomId>>>,
    pub row_specifiers: BTreeMap<usize, Vec<AtomId>>,
    /// Keyed by the row and column numbers written one after the other, so
    /// row 1 column 12 and row 11 column 2 share a key, as in the C++.
    pub cell_specifiers: BTreeMap<String, Vec<AtomId>>,
}

impl Default for ArrayData {
    fn default() -> Self {
        Self {
            row: 0,
            col: 0,
            cells: vec![Vec::new()],
            row_specifiers: BTreeMap::new(),
            cell_specifiers: BTreeMap::new(),
        }
    }
}

impl ArrayData {
    pub fn rows(&self) -> i32 {
        self.row as i32
    }

    pub fn cols(&self) -> i32 {
        self.col as i32
    }

    fn add_col(&mut self, root: Option<AtomId>) {
        self.cells[self.row].push(root);
        self.col += 1;
    }

    fn add_col_n(&mut self, root: Option<AtomId>, n: i32) {
        self.cells[self.row].push(root);
        for _ in 1..n - 1 {
            self.cells[self.row].push(None);
        }
        self.col = (self.col as i64 + i64::from(n)).max(0) as usize;
    }

    /// `insertAtomIntoCol()`.
    fn insert_atom_into_col(&mut self, col: usize, atom: Option<AtomId>) {
        self.col += 1;
        for row in self.cells.iter_mut().take(self.row) {
            if row.len() < col {
                row.resize(col, None);
            }
            row.insert(col, atom);
        }
    }

    fn add_row(&mut self, root: Option<AtomId>) {
        self.add_col(root);
        self.cells.push(Vec::new());
        self.row += 1;
        self.col = 0;
    }
}

impl Formula {
    /// An `ArrayFormula`.
    pub fn array() -> Self {
        Self {
            array: Some(Box::default()),
            ..Self::default()
        }
    }

    fn array_mut(&mut self) -> Result<&mut ArrayData> {
        self.array
            .as_deref_mut()
            .ok_or_else(|| Error::invalid_state("The formula is not an array."))
    }

    /// `ArrayFormula::addCol()`.
    pub fn add_col(&mut self) -> Result<()> {
        let root = self.root;
        self.array_mut()?.add_col(root);
        self.root = None;
        self.over_count = 0;
        Ok(())
    }

    /// `ArrayFormula::addCol(n)`.
    pub fn add_col_n(&mut self, n: i32) -> Result<()> {
        let root = self.root;
        self.array_mut()?.add_col_n(root, n);
        self.root = None;
        self.over_count = 0;
        Ok(())
    }

    /// `ArrayFormula::addRow()`.
    pub fn add_row(&mut self) -> Result<()> {
        let root = self.root;
        self.array_mut()?.add_row(root);
        self.root = None;
        self.over_count = 0;
        Ok(())
    }

    /// `ArrayFormula::addRowSpecifier()`.
    pub fn add_row_specifier(&mut self, spe: AtomId) -> Result<()> {
        let array = self.array_mut()?;
        let row = array.row;
        array.row_specifiers.entry(row).or_default().push(spe);
        Ok(())
    }

    /// `ArrayFormula::addCellSpecifier()`.
    pub fn add_cell_specifier(&mut self, spe: AtomId) -> Result<()> {
        let array = self.array_mut()?;
        let key = format!("{}{}", array.row, array.col);
        array.cell_specifiers.entry(key).or_default().push(spe);
        Ok(())
    }

    /// `ArrayFormula::checkDimensions()`: closes the last row and pads the
    /// rows that start with a cell to the widest row.
    pub fn check_dimensions(&mut self, ctx: &Ctx) -> Result<()> {
        let root = self.root;
        let array = self.array_mut()?;
        if array.cells.last().is_some_and(|r| !r.is_empty()) || root.is_some() {
            array.add_row(root);
            self.root = None;
        }
        let array = self.array_mut()?;
        array.row = array.cells.len() - 1;
        array.col = array.cells[0].len();
        for i in 1..array.row {
            array.col = array.col.max(array.cells[i].len());
        }
        if array.row as i64 * array.col as i64 > MAX_ARRAY_CELLS {
            return Err(Error::parse("Matrix is too large!"));
        }
        let col = array.col;
        for row in array.cells.iter_mut().take(array.row) {
            if row.len() != col
                && let Some(Some(first)) = row.first()
                && ctx.get(*first).atom_type != AtomType::InterText
            {
                row.resize(row.len().max(col), None);
            }
        }
        Ok(())
    }

    /// The array, once parsing is done.
    pub fn into_array(self) -> Result<ArrayData> {
        self.array
            .map(|a| *a)
            .ok_or_else(|| Error::invalid_state("The formula is not an array."))
    }
}

/// `ArrayFormula::getAsVRow()`.
fn as_vrow(ctx: &mut Ctx, array: &ArrayData) -> AtomId {
    let elements = array.cells.iter().flatten().flatten().copied().collect();
    ctx.atom(AtomKind::VRow {
        elements,
        raise: SpaceData::sized(Unit::Ex, 0.0, 0.0, 0.0),
        add_interline: true,
        valign: Alignment::Center,
        halign: Alignment::None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixType {
    Array,
    Matrix,
    Align,
    AlignAt,
    FlAlign,
    SmallMatrix,
    Aligned,
    AlignedAt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MultiLineType {
    Multline,
    Gather,
    Gathered,
}

/// `MatrixAtom`.
#[derive(Clone, Debug, PartialEq)]
pub struct MatrixData {
    pub array: Rc<ArrayData>,
    pub position: Vec<Alignment>,
    /// Column to the number of rules drawn before it (`VlineAtom`).
    pub vlines: BTreeMap<i32, i32>,
    pub column_specifiers: BTreeMap<i32, Option<AtomId>>,
    pub mat_type: MatrixType,
    pub partial: bool,
    pub space_around: bool,
}

impl MatrixData {
    /// `MatrixAtom(isPartial, arr, options[, spaceAround])`.
    pub fn with_options(
        ctx: &mut Ctx,
        partial: bool,
        array: Rc<ArrayData>,
        options: &[u16],
        space_around: bool,
    ) -> Result<Self> {
        let mut m = Self {
            array,
            position: Vec::new(),
            vlines: BTreeMap::new(),
            column_specifiers: BTreeMap::new(),
            mat_type: MatrixType::Array,
            partial,
            space_around,
        };
        m.position = m.parse_positions(ctx, options.to_vec())?;
        Ok(m)
    }

    /// `MatrixAtom(isPartial, arr, type)`.
    pub fn with_type(partial: bool, array: Rc<ArrayData>, mat_type: MatrixType) -> Self {
        let cols = array.cols().max(0) as usize;
        let position = if mat_type != MatrixType::Matrix && mat_type != MatrixType::SmallMatrix {
            (0..cols)
                .map(|i| {
                    if i % 2 == 0 {
                        Alignment::Right
                    } else {
                        Alignment::Left
                    }
                })
                .collect()
        } else {
            vec![Alignment::Center; cols]
        };
        Self {
            array,
            position,
            vlines: BTreeMap::new(),
            column_specifiers: BTreeMap::new(),
            mat_type,
            partial,
            space_around: false,
        }
    }

    /// `parsePositions()`: the column specification.
    fn parse_positions(&mut self, ctx: &mut Ctx, mut opt: WStr) -> Result<Vec<Alignment>> {
        let too_complex = || Error::parse("Column specification is too complex!");
        let mut len = opt.len() as i32;
        let mut pos: i32 = 0;
        let mut lpos = Vec::new();
        let longest = ctx.column_types.keys().map(Vec::len).max().unwrap_or(0);
        let mut steps = 0;
        while pos < len {
            steps += 1;
            if steps > MAX_COLUMN_SPEC_STEPS || len > MAX_COLUMN_SPEC_LENGTH {
                return Err(too_complex());
            }
            let ch = opt[pos as usize];
            match ch {
                0x6C => lpos.push(Alignment::Left),
                0x72 => lpos.push(Alignment::Right),
                0x63 => lpos.push(Alignment::Center),
                0x7C => {
                    let mut nb = 1;
                    loop {
                        pos += 1;
                        if pos >= len {
                            break;
                        }
                        if opt[pos as usize] != 0x7C {
                            pos -= 1;
                            break;
                        }
                        nb += 1;
                    }
                    self.vlines.insert(lpos.len() as i32, nb);
                }
                0x40 => {
                    pos += 1;
                    let sub = wstr::substr_from(&opt, i64::from(pos))?;
                    let mut tp =
                        Parser::new(ctx, self.partial, sub, Formula::default(), false, true)?;
                    let atom = tp.get_argument(ctx)?;
                    let cols = self.array.cols().max(0) as usize;
                    if lpos.len() > cols {
                        lpos.truncate(cols);
                    }
                    let work = i64::from(self.array.rows()) * i64::from(self.array.cols() + 1);
                    crate::parser::count_parse_work(ctx, work)?;
                    Rc::make_mut(&mut self.array).insert_atom_into_col(lpos.len(), atom);
                    lpos.push(Alignment::None);
                    pos += tp.pos;
                    pos -= 1;
                }
                0x2A => {
                    pos += 1;
                    let sub = wstr::substr_from(&opt, i64::from(pos))?;
                    let mut tp =
                        Parser::new(ctx, self.partial, sub, Formula::default(), false, true)?;
                    let mut args = Vec::new();
                    tp.get_opts_args(ctx, 2, 0, &mut args)?;
                    pos += tp.pos;
                    let nrep = value_of_int(&args[1]);
                    if !(0..=MAX_COLUMN_SPEC_REPEAT).contains(&nrep) {
                        return Err(Error::parse("Bad column-spec repeat count!"));
                    }
                    if i64::from(len) + i64::from(nrep) * args[2].len() as i64
                        > i64::from(MAX_COLUMN_SPEC_LENGTH)
                    {
                        return Err(too_complex());
                    }
                    let repeated = args[2].repeat(nrep as usize);
                    insert_at(&mut opt, pos, &repeated)?;
                    len = opt.len() as i32;
                    pos -= 1;
                }
                0x3E => {
                    pos += 1;
                    let sub = wstr::substr_from(&opt, i64::from(pos))?;
                    let mut tp =
                        Parser::new(ctx, self.partial, sub, Formula::array(), false, true)?;
                    let cs = tp.get_argument(ctx)?;
                    self.column_specifiers.insert(lpos.len() as i32, cs);
                    pos += tp.pos;
                    pos -= 1;
                }
                0x20 | 0x09 => {}
                _ => {
                    let mut spos = pos + ((len - pos) as usize).min(longest) as i32 + 1;
                    let mut replaced = false;
                    loop {
                        spos -= 1;
                        if spos <= pos {
                            break;
                        }
                        steps += 1;
                        if steps > MAX_COLUMN_SPEC_STEPS {
                            return Err(too_complex());
                        }
                        let key = wstr::substr(&opt, i64::from(pos), i64::from(spos - pos))?;
                        if let Some(spec) = ctx.column_types.get(&key).cloned() {
                            replaced = true;
                            insert_at(&mut opt, spos, &spec)?;
                            len = opt.len() as i32;
                            pos = spos - 1;
                            break;
                        }
                    }
                    if !replaced {
                        lpos.push(Alignment::Center);
                    }
                }
            }
            pos += 1;
        }
        let cols = self.array.cols().max(0) as usize;
        while lpos.len() < cols {
            lpos.push(Alignment::Center);
        }
        if lpos.is_empty() {
            lpos.push(Alignment::Center);
        }
        Ok(lpos)
    }
}

/// `std::wstring::insert(pos, str)`.
fn insert_at(s: &mut WStr, pos: i32, what: &[u16]) -> Result<()> {
    if pos < 0 || pos as usize > s.len() {
        return Err(Error::out_of_range());
    }
    let at = pos as usize;
    s.splice(at..at, what.iter().copied());
    Ok(())
}

/// `MulticolumnAtom`, or `HdotsforAtom` when `hdots` holds the coefficient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MulticolumnData {
    pub n: i32,
    pub align: Alignment,
    pub width: f32,
    pub before_vlines: i32,
    pub after_vlines: i32,
    pub row: i32,
    pub col: i32,
    pub cols: Option<AtomId>,
    pub hdots: Option<f32>,
}

impl MulticolumnData {
    /// `MulticolumnAtom(n, align, cols)`.
    pub fn new(n: i32, align: &[u8], cols: Option<AtomId>) -> Self {
        let mut data = Self {
            n: if n >= 1 { n } else { 1 },
            align: Alignment::Center,
            width: 0.0,
            before_vlines: 0,
            after_vlines: 0,
            row: 0,
            col: 0,
            cols,
            hdots: None,
        };
        data.align = data.parse_align(align);
        data
    }

    /// `MulticolumnAtom::parseAlign()`.
    fn parse_align(&mut self, s: &[u8]) -> Alignment {
        let len = s.len() as i32;
        let mut pos = 0;
        let mut align = Alignment::Center;
        let mut first = true;
        while pos < len {
            match s[pos as usize] {
                b'l' => {
                    align = Alignment::Left;
                    first = false;
                }
                b'r' => {
                    align = Alignment::Right;
                    first = false;
                }
                b'c' => {
                    align = Alignment::Center;
                    first = false;
                }
                b'|' => {
                    if first {
                        self.before_vlines = 1;
                    } else {
                        self.after_vlines = 1;
                    }
                    loop {
                        pos += 1;
                        if pos >= len {
                            break;
                        }
                        if s[pos as usize] != b'|' {
                            pos -= 1;
                            break;
                        }
                        if first {
                            self.before_vlines += 1;
                        } else {
                            self.after_vlines += 1;
                        }
                    }
                }
                _ => {}
            }
            pos += 1;
        }
        align
    }
}

fn multicolumn_of(ctx: &Ctx, atom: Option<AtomId>) -> Option<MulticolumnData> {
    match atom.map(|a| &ctx.get(a).kind) {
        Some(AtomKind::Multicolumn(data)) => Some(*data),
        _ => None,
    }
}

fn set_multicolumn(ctx: &mut Ctx, atom: AtomId, f: impl FnOnce(&mut MulticolumnData)) {
    if let AtomKind::Multicolumn(data) = &mut ctx.get_mut(atom).kind {
        f(data);
    }
}

fn is_hline(ctx: &Ctx, atom: Option<AtomId>) -> bool {
    atom.is_some_and(|a| matches!(ctx.get(a).kind, AtomKind::Hline { .. }))
}

fn drt_of(env: &Environment) -> f32 {
    env.font
        .style_param(&env.settings, "defaultrulethickness", env.style)
}

/// `VlineAtom::getWidth()`.
fn vline_width(n: i32, env: &Environment) -> f32 {
    if n != 0 {
        return drt_of(env) * (3 * n - 2) as f32;
    }
    0.0
}

/// `VlineAtom::createBox()` with the height and shift the matrix sets.
fn vline_box(ctx: &mut Ctx, n: i32, height: f32, shift: f32, env: &Environment) -> Result<BoxId> {
    if n == 0 {
        return ctx.boxes.strut(0.0, 0.0, 0.0, 0.0);
    }
    let drt = drt_of(env);
    let b = ctx.boxes.rule(height, drt, shift, ctx.line_color, true)?;
    let sep = ctx.boxes.strut(2.0 * drt, 0.0, 0.0, 0.0)?;
    let hb = ctx.boxes.hbox()?;
    for _ in 0..n - 1 {
        ctx.boxes.hbox_add(hb, b)?;
        ctx.boxes.hbox_add(hb, sep)?;
    }
    if n > 0 {
        ctx.boxes.hbox_add(hb, b)?;
    }
    Ok(hb)
}

/// `HlineAtom::createBox()`.
pub fn hline_box(
    ctx: &mut Ctx,
    width: f32,
    shift: f32,
    color: u32,
    env: &Environment,
) -> Result<BoxId> {
    let drt = drt_of(env);
    let vb = ctx.boxes.vbox()?;
    let rule = ctx.boxes.rule(drt, width, shift, color, false)?;
    ctx.boxes.vbox_add(vb, rule)?;
    ctx.boxes.get_mut(vb).atom_type = AtomType::HLine;
    Ok(vb)
}

/// `MulticolumnAtom::createBox()` and `HdotsforAtom::createBox()`.
pub fn multicolumn_box(
    ctx: &mut Ctx,
    data: MulticolumnData,
    env: &mut Environment,
) -> Result<BoxId> {
    let cols = data.cols.ok_or_else(null_atom)?;
    if let Some(coeff) = data.hdots {
        return hdots_box(ctx, cols, data.width, coeff, env);
    }
    let b = if data.width == 0.0 {
        create_box(ctx, cols, env)?
    } else {
        let inner = create_box(ctx, cols, env)?;
        ctx.boxes.hbox_aligned(inner, data.width, data.align)?
    };
    ctx.boxes.get_mut(b).atom_type = AtomType::MultiColumn;
    Ok(b)
}

/// `HdotsforAtom::createBox(space, b, env)`.
fn hdots_frame(ctx: &mut Ctx, space: f32, b: BoxId) -> Result<BoxId> {
    let sb = ctx.boxes.strut(0.0, space, 0.0, 0.0)?;
    let vb = ctx.boxes.vbox()?;
    ctx.boxes.vbox_add(vb, sb)?;
    ctx.boxes.vbox_add(vb, b)?;
    ctx.boxes.vbox_add(vb, sb)?;
    ctx.boxes.get_mut(vb).atom_type = AtomType::MultiColumn;
    Ok(vb)
}

fn hdots_box(
    ctx: &mut Ctx,
    dot_atom: AtomId,
    width: f32,
    coeff: f32,
    env: &mut Environment,
) -> Result<BoxId> {
    let dot = create_box(ctx, dot_atom, env)?;
    let mut space = glue::skip_space(SpaceType::ThinMuSkip, env) * coeff * 2.0;
    if width == 0.0 {
        return hdots_frame(ctx, space, dot);
    }
    let dot_width = ctx.boxes.width(dot);
    let x = (width - dot_width) / (space + dot_width);
    let mut count: i32 = 0;
    if x > 0.0 {
        count = if x >= i32::MAX as f32 {
            i32::MAX
        } else {
            x.floor() as i32
        };
    }
    if count == 0 {
        let b = ctx.boxes.hbox_aligned(dot, width, Alignment::Center)?;
        return hdots_frame(ctx, space, b);
    }
    space += (x - count as f32) * space / count as f32;
    let sb = ctx.boxes.strut(space, 0.0, 0.0, 0.0)?;
    let b = ctx.boxes.hbox()?;
    for _ in 0..count {
        ctx.boxes.hbox_add(b, dot)?;
        ctx.boxes.hbox_add(b, sb)?;
    }
    ctx.boxes.hbox_add(b, dot)?;
    let hb = ctx.boxes.hbox_aligned(b, width, Alignment::Center)?;
    hdots_frame(ctx, space, hb)
}

/// `MultiRowAtom::createBox()`.
pub fn multirow_box(ctx: &mut Ctx, rows: Option<AtomId>, env: &mut Environment) -> Result<BoxId> {
    let b = create_box(ctx, rows.ok_or_else(null_atom)?, env)?;
    ctx.boxes.get_mut(b).atom_type = AtomType::MultiRow;
    Ok(b)
}

/// `MatrixAtom::multicolumnSpan()`.
fn multicolumn_span(m: &MatrixData, data: &MulticolumnData, j: usize) -> usize {
    let left = m.array.cols() - j as i32;
    data.n.min(left).max(1) as usize
}

/// `MatrixAtom::getColumnSep()`.
fn column_sep(
    ctx: &mut Ctx,
    m: &MatrixData,
    env: &mut Environment,
    width: f32,
) -> Result<Vec<f32>> {
    let cols = m.array.cols().max(0) as usize;
    let mut arr = vec![0.0f32; cols + 1];
    let mut w = env.text_width();
    if m.mat_type == MatrixType::Aligned || m.mat_type == MatrixType::AlignedAt {
        w = f32::INFINITY;
    }
    let half_cols = (cols / 2) as f32;
    match m.mat_type {
        MatrixType::Array => {
            let hsep = space_of(ctx, Unit::Em, 1.0, 0.0, 0.0, env)?;
            let hsep_width = ctx.boxes.width(hsep);
            let mut i = 0;
            while i < cols {
                if m.position[i] == Alignment::None {
                    arr[i] = 0.0;
                    arr[i + 1] = 0.0;
                    i += 1;
                } else {
                    arr[i] = hsep_width;
                }
                i += 1;
            }
            if m.space_around {
                let half = hsep_width / 2.0;
                if m.position.first() != Some(&Alignment::None) {
                    arr[0] = half;
                }
                if m.position.last() != Some(&Alignment::None) {
                    arr[cols] = half;
                }
            }
            return Ok(arr);
        }
        MatrixType::Matrix | MatrixType::SmallMatrix => {
            arr[0] = 0.0;
            arr[cols] = arr[0];
            let hsep = space_of(ctx, Unit::Em, 1.0, 0.0, 0.0, env)?;
            let hsep_width = ctx.boxes.width(hsep);
            for a in arr.iter_mut().take(cols).skip(1) {
                *a = hsep_width;
            }
            return Ok(arr);
        }
        MatrixType::Aligned | MatrixType::Align => {
            let align = space_box(ctx, &SpaceData::blank_of(SpaceType::MedMuSkip), env)?;
            let align_width = ctx.boxes.width(align);
            let sep = if w != f32::INFINITY {
                let h = cmax(
                    (w - width - half_cols * align_width) / ((cols + 3) as f32 / 2.0).floor(),
                    0.0,
                );
                ctx.boxes.strut(h, 0.0, 0.0, 0.0)?
            } else {
                space_of(ctx, Unit::Em, 1.0, 0.0, 0.0, env)?
            };
            let sep_width = ctx.boxes.width(sep);
            arr[cols] = sep_width;
            for (i, a) in arr.iter_mut().take(cols).enumerate() {
                *a = if i % 2 == 0 { sep_width } else { align_width };
            }
        }
        MatrixType::AlignedAt | MatrixType::AlignAt => {
            let h = if w != f32::INFINITY {
                cmax((w - width) / 2.0, 0.0)
            } else {
                0.0
            };
            let align = space_box(ctx, &SpaceData::blank_of(SpaceType::MedMuSkip), env)?;
            let align_width = ctx.boxes.width(align);
            arr[0] = h;
            arr[cols] = arr[0];
            for (i, a) in arr.iter_mut().enumerate().take(cols).skip(1) {
                *a = if i % 2 == 0 { 0.0 } else { align_width };
            }
        }
        MatrixType::FlAlign => {
            let align = space_box(ctx, &SpaceData::blank_of(SpaceType::MedMuSkip), env)?;
            let align_width = ctx.boxes.width(align);
            let sep = if w != f32::INFINITY {
                let h = cmax(
                    (w - width - half_cols * align_width)
                        / ((cols as i32 - 1) as f32 / 2.0).floor(),
                    0.0,
                );
                ctx.boxes.strut(h, 0.0, 0.0, 0.0)?
            } else {
                space_of(ctx, Unit::Em, 1.0, 0.0, 0.0, env)?
            };
            let sep_width = ctx.boxes.width(sep);
            arr[0] = 0.0;
            arr[cols] = arr[0];
            for (i, a) in arr.iter_mut().enumerate().take(cols).skip(1) {
                *a = if i % 2 == 0 { sep_width } else { align_width };
            }
        }
    }
    if w == f32::INFINITY {
        arr[0] = 0.0;
        arr[cols] = arr[0];
    }
    Ok(arr)
}

/// `MatrixAtom::recalculateLine()`: spreads each `\multirow` over the rows
/// it spans, moving the box of an upward span to its top row.
#[allow(clippy::too_many_arguments)]
fn recalculate_line(
    ctx: &mut Ctx,
    rows: usize,
    boxarr: &mut [Vec<BoxId>],
    multi_rows: &[AtomId],
    height: &mut [f32],
    depth: &mut [f32],
    drt: f32,
    vspace: f32,
) {
    let is_rule_row = |ctx: &Ctx, boxarr: &[Vec<BoxId>], j: usize| {
        ctx.boxes.get(boxarr[j][0]).atom_type == AtomType::HLine
    };
    for &m in multi_rows {
        let AtomKind::MultiRow { i: r, j: c, n, .. } = ctx.get(m).kind else {
            continue;
        };
        let mut n = n;
        let mut skipped = 0;
        let mut h = 0.0f32;
        let mut top_row = r;
        if n < 0 {
            let mut j = r;
            let mut top = r;
            while j >= 0 && j > r + n {
                if is_rule_row(ctx, boxarr, j as usize) {
                    if j == 0 {
                        break;
                    }
                    h += drt;
                    n -= 1;
                } else {
                    skipped += 1;
                    top = j;
                    h += height[j as usize] + depth[j as usize] + vspace;
                }
                j -= 1;
            }
            top_row = top;
            let (r, c, t) = (r as usize, c as usize, top as usize);
            let tmp = boxarr[r][c];
            boxarr[r][c] = boxarr[t][c];
            boxarr[t][c] = tmp;
        } else {
            let mut j = r;
            while j < r + n && (j as usize) < rows {
                if is_rule_row(ctx, boxarr, j as usize) {
                    if j as usize == rows - 1 {
                        break;
                    }
                    h += drt;
                    n += 1;
                } else {
                    skipped += 1;
                    h += height[j as usize] + depth[j as usize] + vspace;
                }
                j += 1;
            }
        }
        let n = n.abs();
        if let AtomKind::MultiRow { i, n: stored, .. } = &mut ctx.get_mut(m).kind {
            *i = top_row;
            *stored = n;
        }
        let (mi, mj) = (top_row as usize, c as usize);
        let b = boxarr[mi][mj];
        let bh = ctx.boxes.height(b) + ctx.boxes.depth(b) + vspace;
        if h > bh {
            ctx.boxes.get_mut(b).height = (h - bh + vspace) / 2.0;
        } else if h < bh && skipped > 0 {
            let ex = (bh - h) / skipped as f32 / 2.0;
            let mr = ((top_row + n) as usize).min(rows);
            for j in mi..mr {
                if !is_rule_row(ctx, boxarr, j) {
                    height[j] += ex;
                    depth[j] += ex;
                }
            }
            let node = ctx.boxes.get_mut(b);
            node.height = height[mi];
            node.depth = bh - node.height - vspace;
        }
        ctx.boxes.get_mut(boxarr[mi][mj]).atom_type = AtomType::None;
    }
}

/// `MatrixAtom::generateMulticolumn()`.
#[allow(clippy::too_many_arguments)]
fn generate_multicolumn(
    ctx: &mut Ctx,
    m: &MatrixData,
    env: &mut Environment,
    b: BoxId,
    hsep: &[f32],
    col_width: &[f32],
    mca: AtomId,
    data: &MulticolumnData,
    j: usize,
) -> Result<BoxId> {
    let mut w = 0.0f32;
    let n = multicolumn_span(m, data, j);
    let mut k = j;
    while k < j + n - 1 {
        w += col_width[k] + hsep[k + 1];
        if let Some(&nb) = m.vlines.get(&(k as i32 + 1)) {
            w += vline_width(nb, env);
        }
        k += 1;
    }
    w += col_width[k];
    if data.hdots.is_some() && data.width <= PREC {
        set_multicolumn(ctx, mca, |d| d.width = w);
        let data = multicolumn_of(ctx, Some(mca)).ok_or_else(null_atom)?;
        return multicolumn_box(ctx, data, env);
    }
    if ctx.boxes.width(b) >= w {
        return Ok(b);
    }
    ctx.boxes.hbox_aligned(b, w, data.align)
}

/// `MatrixAtom::applyCell()`: column, row and cell colors.
fn apply_cell(ctx: &mut Ctx, m: &MatrixData, wb: BoxId, i: usize, j: usize) {
    let apply = |ctx: &mut Ctx, spe: AtomId| {
        if let AtomKind::CellSpecifier { color, foreground } = ctx.get(spe).kind {
            if foreground {
                ctx.boxes.wrapper_set_colors(wb, Some(color), None);
            } else {
                ctx.boxes.wrapper_set_colors(wb, None, Some(color));
            }
        }
    };
    if let Some(&spe) = m.column_specifiers.get(&(j as i32)) {
        let mut spe = spe;
        let mut p = None;
        while let Some(s) = spe
            && ctx.get(s).is_row()
        {
            spe = ctx.row_elements(s).first().copied();
            p = Some(s);
        }
        if let Some(p) = p {
            let elements = ctx.row_elements(p).to_vec();
            for k in elements {
                apply(ctx, k);
            }
        }
    }
    if let Some(list) = m.array.row_specifiers.get(&i) {
        for &s in list {
            apply(ctx, s);
        }
    }
    let key = format!("{i}{j}");
    if let Some(list) = m.array.cell_specifiers.get(&key) {
        for &s in list {
            apply(ctx, s);
        }
    }
}

/// `MatrixAtom::createBox()`.
pub fn matrix_box(ctx: &mut Ctx, m: &MatrixData, e: &mut Environment) -> Result<BoxId> {
    let rows = m.array.rows().max(0) as usize;
    let cols = m.array.cols().max(0) as usize;
    if rows as i64 * cols as i64 > MAX_ARRAY_CELLS {
        return Err(Error::parse("Matrix is too large!"));
    }
    let null = ctx.boxes.null_box();
    let mut line_depth = vec![0.0f32; rows];
    let mut line_height = vec![0.0f32; rows];
    let mut col_width = vec![0.0f32; cols];
    let mut boxarr = vec![vec![null; cols]; rows];
    let mut mat_w = 0.0f32;
    let drt = drt_of(e);

    let mut small;
    let env: &mut Environment = if m.mat_type == MatrixType::SmallMatrix {
        small = e.copy();
        small.style = TexStyle::Script;
        &mut small
    } else {
        e
    };

    let mut multi_cols = Vec::new();
    let mut multi_rows = Vec::new();
    for i in 0..rows {
        line_depth[i] = 0.0;
        line_height[i] = 0.0;
        let size = m.array.cells[i].len();
        for j in 0..cols {
            if j >= size {
                for b in boxarr[i].iter_mut().skip(j) {
                    *b = null;
                }
                break;
            }
            let atom = m.array.cells[i][j];
            boxarr[i][j] = match atom {
                None => null,
                Some(a) => create_box(ctx, a, env)?,
            };
            if let Some(a) = atom
                && ctx.get(a).atom_type == AtomType::InterText
            {
                ctx.boxes.get_mut(boxarr[i][j]).atom_type = AtomType::InterText;
            }
            let box_type = ctx.boxes.get(boxarr[i][j]).atom_type;
            let mra = box_type == AtomType::MultiRow
                && atom.is_some_and(|a| matches!(ctx.get(a).kind, AtomKind::MultiRow { .. }));
            let mca = box_type == AtomType::MultiColumn && multicolumn_of(ctx, atom).is_some();
            let b = ctx.boxes.get(boxarr[i][j]);
            let (bw, bh, bd) = (b.width, b.height, b.depth);
            if !mra {
                line_depth[i] = cmax(bd, line_depth[i]);
                line_height[i] = cmax(bh, line_height[i]);
            } else if let Some(a) = atom {
                if let AtomKind::MultiRow { i: r, j: c, .. } = &mut ctx.get_mut(a).kind {
                    *r = i as i32;
                    *c = j as i32;
                }
                multi_rows.push(a);
            }
            if !mca {
                col_width[j] = cmax(bw, col_width[j]);
            } else if let Some(a) = atom {
                set_multicolumn(ctx, a, |d| {
                    d.row = i as i32;
                    d.col = j as i32;
                });
                multi_cols.push(a);
            }
        }
    }

    for w in &col_width {
        mat_w += w;
    }

    let hsep = column_sep(ctx, m, env, mat_w)?;

    for &a in &multi_cols {
        let Some(data) = multicolumn_of(ctx, Some(a)) else {
            continue;
        };
        let (c, r) = (data.col as usize, data.row as usize);
        let n = multicolumn_span(m, &data, c);
        let mut w = 0.0f32;
        let mut j = c;
        while j < c + n - 1 {
            w += col_width[j] + hsep[j + 1];
            j += 1;
        }
        w += col_width[j];
        let bw = ctx.boxes.width(boxarr[r][c]);
        if bw > w {
            mat_w += bw - w;
            let extra = (bw - w) / n as f32;
            for cw in col_width.iter_mut().skip(c).take(n) {
                *cw += extra;
            }
        }
    }

    for (j, sep) in hsep.iter().enumerate() {
        mat_w += sep;
        if let Some(&nb) = m.vlines.get(&(j as i32)) {
            mat_w += vline_width(nb, env);
        }
    }

    let vsep = space_of(ctx, Unit::Ex, 0.0, 1.0, 0.0, env)?;
    let vsep_height = ctx.boxes.height(vsep);
    recalculate_line(
        ctx,
        rows,
        &mut boxarr,
        &multi_rows,
        &mut line_height,
        &mut line_depth,
        drt,
        vsep_height,
    );

    let vb = ctx.boxes.vbox()?;
    let vspace = vsep_height / 2.0;

    for i in 0..rows {
        let mut hb = ctx.boxes.hbox()?;
        let cells = &m.array.cells[i];
        let mut j = 0;
        while j < cols {
            let cell = cells.get(j).copied().flatten();
            let mut box_type = ctx.boxes.get(boxarr[i][j]).atom_type;
            let mca = if box_type == AtomType::MultiColumn {
                multicolumn_of(ctx, cell)
            } else {
                None
            };
            let hla = box_type == AtomType::HLine && is_hline(ctx, cell);
            if (box_type == AtomType::MultiColumn && mca.is_none())
                || (box_type == AtomType::HLine && !hla)
            {
                box_type = AtomType::None;
            }
            match box_type {
                AtomType::None | AtomType::MultiColumn => {
                    if j == 0
                        && let Some(&nb) = m.vlines.get(&0)
                    {
                        let height = line_height[i] + line_depth[i] + vsep_height;
                        let shift = line_depth[i] + vspace;
                        let vat = vline_box(ctx, nb, height, shift, env)?;
                        ctx.boxes.hbox_add(hb, vat)?;
                    }
                    let mut last_vline = true;
                    let tj = j;
                    let l = if j == 0 { hsep[j] } else { hsep[j] / 2.0 };
                    let wb = match (mca, cell) {
                        (Some(data), Some(atom)) => {
                            let b = generate_multicolumn(
                                ctx,
                                m,
                                env,
                                boxarr[i][j],
                                &hsep,
                                &col_width,
                                atom,
                                &data,
                                j,
                            )?;
                            j += multicolumn_span(m, &data, j) - 1;
                            let bw = ctx.boxes.width(b);
                            last_vline = data.after_vlines != 0;
                            ctx.boxes.wrapper_box(
                                b,
                                bw,
                                line_height[i],
                                line_depth[i],
                                Alignment::Left,
                            )?
                        }
                        _ => ctx.boxes.wrapper_box(
                            boxarr[i][j],
                            col_width[j],
                            line_height[i],
                            line_depth[i],
                            m.position[j],
                        )?,
                    };
                    let r = if j == cols - 1 {
                        hsep[j + 1]
                    } else {
                        hsep[j + 1] / 2.0
                    };
                    ctx.boxes.wrapper_add_insets(wb, l, vspace, r, vspace);
                    apply_cell(ctx, m, wb, i, j);
                    boxarr[i][tj] = wb;
                    ctx.boxes.hbox_add(hb, wb)?;
                    if last_vline && let Some(&nb) = m.vlines.get(&(j as i32 + 1)) {
                        let height = line_height[i] + line_depth[i] + vsep_height;
                        let shift = line_depth[i] + vspace;
                        let vat = vline_box(ctx, nb, height, shift, env)?;
                        ctx.boxes.hbox_add(hb, vat)?;
                    }
                }
                AtomType::InterText => {
                    let mut f = env.text_width();
                    if f == f32::INFINITY {
                        f = col_width[j];
                    }
                    hb = ctx.boxes.hbox_aligned(boxarr[i][j], f, Alignment::Left)?;
                    j = cols;
                }
                AtomType::HLine => {
                    if let Some(atom) = cell {
                        let line_color = ctx.line_color;
                        if let AtomKind::Hline { width, color, .. } = &mut ctx.get_mut(atom).kind {
                            *color = line_color;
                            *width = mat_w;
                        }
                        if i >= 1 {
                            let above = &m.array.cells[i - 1];
                            if j < above.len() && is_hline(ctx, above[j]) {
                                let s = ctx.boxes.strut(0.0, 2.0 * drt, 0.0, 0.0)?;
                                ctx.boxes.hbox_add(hb, s)?;
                            }
                        }
                        let b = create_box(ctx, atom, env)?;
                        ctx.boxes.hbox_add(hb, b)?;
                    }
                    j = cols;
                }
                _ => {}
            }
            j += 1;
        }
        if ctx.boxes.get(boxarr[i][0]).atom_type != AtomType::HLine {
            let node = ctx.boxes.get_mut(hb);
            node.height = line_height[i] + vspace;
            node.depth = line_depth[i] + vspace;
        }
        ctx.boxes.vbox_add(vb, hb)?;
    }

    let node = ctx.boxes.get(vb);
    let total = node.height + node.depth;
    let axis = env.font.style_param(&env.settings, "axisheight", env.style);
    let node = ctx.boxes.get_mut(vb);
    node.height = total / 2.0 + axis;
    node.depth = total / 2.0 - axis;
    Ok(vb)
}

/// `MultlineAtom::createBox()`.
pub fn multline_box(
    ctx: &mut Ctx,
    array: &Rc<ArrayData>,
    line_type: MultiLineType,
    partial: bool,
    env: &mut Environment,
) -> Result<BoxId> {
    let tw = env.text_width();
    if tw == f32::INFINITY || line_type == MultiLineType::Gathered {
        let m = MatrixData::with_options(ctx, partial, array.clone(), &[], false)?;
        return matrix_box(ctx, &m, env);
    }
    let rows = array.rows().max(0) as usize;
    let vb = ctx.boxes.vbox()?;
    let vsep = space_of(ctx, Unit::Ex, 0.0, 1.0, 0.0, env)?;
    let gather = line_type == MultiLineType::Gather;
    for i in 0..rows {
        let atom = array.cells[i].first().copied().flatten();
        let mut alignment = Alignment::Center;
        if i == 0 {
            alignment = if gather {
                Alignment::Center
            } else {
                Alignment::Left
            };
        } else if i == rows - 1 {
            alignment = if gather {
                Alignment::Center
            } else {
                Alignment::Right
            };
        }
        if let Some(a) = atom
            && ctx.get(a).alignment != Alignment::None
        {
            alignment = ctx.get(a).alignment;
        }
        if i > 0 {
            ctx.boxes.vbox_add(vb, vsep)?;
        }
        let b = match atom {
            None => ctx.boxes.strut(0.0, 0.0, 0.0, 0.0)?,
            Some(a) => create_box(ctx, a, env)?,
        };
        let hb = ctx.boxes.hbox_aligned(b, tw, alignment)?;
        ctx.boxes.vbox_add(vb, hb)?;
    }
    let node = ctx.boxes.get_mut(vb);
    let h = node.height + node.depth;
    node.height = h / 2.0;
    node.depth = h / 2.0;
    Ok(vb)
}

/// `macro_cr`: `\\` and `\cr`. Outside an array the rest of the group
/// becomes the following rows of a vertical row.
pub fn cr(tp: &mut Parser, ctx: &mut Ctx) -> Result<Option<AtomId>> {
    if tp.is_array_mode() {
        tp.add_row()?;
        return Ok(None);
    }
    let mut arr = Formula::array();
    arr.add(ctx, tp.formula.root);
    arr.add_row()?;
    let rest = tp.forward_balanced_group()?;
    let math_mode = tp.is_math_mode();
    let mut parser = Parser::new(ctx, tp.is_partial(), rest, arr, false, math_mode)?;
    parser.parse(ctx)?;
    let mut arr = parser.formula;
    arr.check_dimensions(ctx)?;
    let array = arr.into_array()?;
    tp.formula.root = Some(as_vrow(ctx, &array));
    Ok(None)
}

/// Parses an environment body into an array (the `TeXParser` every
/// `@@env` macro runs on an `ArrayFormula`).
fn parse_body(ctx: &mut Ctx, tp: &Parser, body: WStr) -> Result<ArrayData> {
    let mut parser = Parser::new(ctx, tp.is_partial(), body, Formula::array(), false, true)?;
    parser.parse(ctx)?;
    let mut formula = parser.formula;
    formula.check_dimensions(ctx)?;
    formula.into_array()
}

/// `valueof(str, size_t&)`: a wide stream read into a 64-bit `size_t`. A
/// minus sign wraps the value as `strtoull` does, and anything past 64 bits
/// reads as the largest value.
fn value_of_size(text: &[u16]) -> u64 {
    let mut i = 0;
    while i < text.len() && matches!(text[i], 0x20 | 0x09..=0x0D) {
        i += 1;
    }
    let mut negative = false;
    if i < text.len() && (text[i] == u16::from(b'+') || text[i] == u16::from(b'-')) {
        negative = text[i] == u16::from(b'-');
        i += 1;
    }
    let mut value: u64 = 0;
    let mut digits = 0;
    let mut overflow = false;
    while i < text.len() && (0x30..=0x39).contains(&text[i]) {
        let digit = u64::from(text[i] - 0x30);
        match value.checked_mul(10).and_then(|v| v.checked_add(digit)) {
            Some(v) => value = v,
            None => overflow = true,
        }
        digits += 1;
        i += 1;
    }
    if digits == 0 {
        return 0;
    }
    if overflow {
        return u64::MAX;
    }
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

/// The `@@env` macros of the array environments.
pub fn environment(
    name: &str,
    tp: &mut Parser,
    ctx: &mut Ctx,
    args: &[WStr],
) -> Result<Option<AtomId>> {
    let partial = tp.is_partial();
    let typed = |mat_type| -> Option<MatrixType> { Some(mat_type) };
    let mat_type = match name {
        "smallmatrix@@env" => typed(MatrixType::SmallMatrix),
        "matrix@@env" => typed(MatrixType::Matrix),
        "align@@env" => typed(MatrixType::Align),
        "flalign@@env" => typed(MatrixType::FlAlign),
        "aligned@@env" => typed(MatrixType::Aligned),
        _ => None,
    };
    if let Some(mat_type) = mat_type {
        let array = parse_body(ctx, tp, args[1].clone())?;
        let m = MatrixData::with_type(partial, Rc::new(array), mat_type);
        return Ok(Some(ctx.atom(AtomKind::Matrix(Rc::new(m)))));
    }
    match name {
        "array@@env" => {
            let array = parse_body(ctx, tp, args[2].clone())?;
            let m = MatrixData::with_options(ctx, partial, Rc::new(array), &args[1], true)?;
            Ok(Some(ctx.atom(AtomKind::Matrix(Rc::new(m)))))
        }
        "alignat@@env" | "alignedat@@env" => {
            let array = parse_body(ctx, tp, args[2].clone())?;
            let n = value_of_size(&args[1]);
            if array.cols().max(0) as u64 > n.wrapping_mul(2) {
                let env = if name == "alignat@@env" {
                    "alignat"
                } else {
                    "alignedat"
                };
                return Err(Error::parse(format!(
                    "Bad number of equations in {env} environment!"
                )));
            }
            let mat_type = if name == "alignat@@env" {
                MatrixType::AlignAt
            } else {
                MatrixType::AlignedAt
            };
            let m = MatrixData::with_type(partial, Rc::new(array), mat_type);
            Ok(Some(ctx.atom(AtomKind::Matrix(Rc::new(m)))))
        }
        _ => {
            let (env, line_type) = match name {
                "multline@@env" => ("multiline", MultiLineType::Multline),
                "gather@@env" => ("gather", MultiLineType::Gather),
                _ => ("gathered", MultiLineType::Gathered),
            };
            let array = parse_body(ctx, tp, args[1].clone())?;
            if array.cols() > 1 {
                return Err(Error::parse(format!(
                    "Requires exact one column in {env} envrionment!"
                )));
            }
            if array.cols() == 0 {
                return Ok(None);
            }
            Ok(Some(ctx.atom(AtomKind::Multline {
                array: Rc::new(array),
                line_type,
                partial,
            })))
        }
    }
}
