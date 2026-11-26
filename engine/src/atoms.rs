//! The atom tree: `atom/atom*.h`.
//!
//! Atoms live in the per-formula arena of `Ctx` and refer to each other by
//! index, for the same reasons boxes do: the C++ shares atoms (the symbol
//! cache, macro arguments used twice), copies them shallowly with
//! `privateCopy` and changes some of them while laying them out.

use crate::boxes::{Alignment, BoxId, Boxes, Measure};
use crate::draw::TextFont;
use crate::enums::{AtomType, SpaceType, Unit};
use crate::error::{Error, Result};
use crate::font::{GeneralSettings, TexStyle};
use crate::tables::{self, CharFont};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AtomId(u32);

/// `tex::LimitsType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitsType {
    Normal,
    NoLimits,
    Limits,
}

/// The previous atom a row hands to the next, as `tex::Dummy`: an atom and
/// the type the row decided it has.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dummy {
    pub atom: AtomId,
    /// `AtomType::None` when the atom's own type applies.
    pub atom_type: AtomType,
    pub text_symbol: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpaceData {
    /// A hard space (`\ `, `~`, a mu skip) rather than explicit dimensions.
    pub blank_space: bool,
    pub blank_type: SpaceType,
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub w_unit: Unit,
    pub h_unit: Unit,
    pub d_unit: Unit,
}

impl SpaceData {
    /// `SpaceAtom()`.
    pub fn blank() -> Self {
        Self::blank_of(SpaceType::None)
    }

    /// `SpaceAtom(SpaceType)`.
    pub fn blank_of(kind: SpaceType) -> Self {
        Self {
            blank_space: true,
            blank_type: kind,
            width: 0.0,
            height: 0.0,
            depth: 0.0,
            w_unit: Unit::Em,
            h_unit: Unit::Em,
            d_unit: Unit::Em,
        }
    }

    /// `SpaceAtom(unit, width, height, depth)`.
    pub fn sized(unit: Unit, width: f32, height: f32, depth: f32) -> Self {
        Self::sized_units(unit, width, unit, height, unit, depth)
    }

    /// `SpaceAtom(wu, w, hu, h, du, d)`.
    pub fn sized_units(
        w_unit: Unit,
        width: f32,
        h_unit: Unit,
        height: f32,
        d_unit: Unit,
        depth: f32,
    ) -> Self {
        Self {
            blank_space: false,
            blank_type: SpaceType::None,
            width,
            height,
            depth,
            w_unit,
            h_unit,
            d_unit,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FractionData {
    pub nodefault: bool,
    pub unit: Unit,
    pub num_align: Alignment,
    pub denom_align: Alignment,
    pub numerator: Option<AtomId>,
    pub denominator: Option<AtomId>,
    pub thickness: f32,
    pub deffactor: f32,
    pub deffactor_set: bool,
    pub use_kern: bool,
}

/// The fields of an `UnderOverAtom`.
#[derive(Clone, Debug, PartialEq)]
pub struct UnderOverData {
    pub base: Option<AtomId>,
    pub under: Option<AtomId>,
    pub under_unit: Unit,
    pub under_space: f32,
    pub under_small: bool,
    pub over: Option<AtomId>,
    pub over_unit: Unit,
    pub over_space: f32,
    pub over_small: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AtomKind {
    Empty,
    Placeholder {
        width: f32,
        height: f32,
        depth: f32,
        shift: f32,
    },
    /// `CharAtom`.
    Char {
        c: u16,
        text_style: &'static str,
        math_mode: bool,
        text_symbol: bool,
    },
    /// `SymbolAtom`.
    Symbol {
        name: &'static str,
        unicode: u16,
        text_symbol: bool,
    },
    /// `FixedCharAtom`.
    FixedChar {
        cf: CharFont,
        text_symbol: bool,
    },
    BreakMark,
    /// `RowAtom`.
    Row {
        elements: Vec<AtomId>,
        breakable: bool,
        look_at_last_atom: bool,
        previous: Option<Dummy>,
    },
    Space(SpaceData),
    /// `ScriptsAtom`.
    Scripts {
        base: Option<AtomId>,
        sub: Option<AtomId>,
        sup: Option<AtomId>,
        align: Alignment,
    },
    /// `CumulativeScriptsAtom`: `sub` and `sup` are rows.
    CumulativeScripts {
        base: Option<AtomId>,
        sub: AtomId,
        sup: AtomId,
    },
    /// `BigOperatorAtom`.
    BigOperator {
        base: Option<AtomId>,
        under: Option<AtomId>,
        over: Option<AtomId>,
        limits_set: bool,
        limits: bool,
    },
    Fraction(Box<FractionData>),
    /// `StyleAtom`.
    Style {
        style: TexStyle,
        at: AtomId,
    },
    /// `TypedAtom`.
    Typed {
        left: AtomType,
        right: AtomType,
        atom: AtomId,
    },
    /// `MathAtom`.
    Math {
        style: TexStyle,
        base: Option<AtomId>,
    },
    /// `RomanAtom`.
    Roman {
        base: Option<AtomId>,
    },
    /// `BoldAtom`.
    Bold {
        base: Option<AtomId>,
    },
    /// `ItAtom`.
    It {
        base: Option<AtomId>,
    },
    /// `SsAtom`: sans serif while laying out `base`.
    Ss {
        base: Option<AtomId>,
    },
    /// `TtAtom`: typewriter while laying out `base`.
    Tt {
        base: Option<AtomId>,
    },
    /// `TextStyleAtom`: a math alphabet (`mathbb`, `mathcal`, ...).
    TextStyle {
        style: &'static str,
        at: Option<AtomId>,
    },
    /// `ColorAtom`: `elements` is a row.
    Color {
        background: u32,
        color: u32,
        elements: AtomId,
    },
    /// `PhantomAtom`: `elements` is a row.
    Phantom {
        elements: AtomId,
        w: bool,
        h: bool,
        d: bool,
    },
    /// `SmashedAtom`.
    Smashed {
        atom: AtomId,
        h: bool,
        d: bool,
    },
    /// `ScaleAtom`.
    Scale {
        base: AtomId,
        sx: f32,
        sy: f32,
    },
    /// `TextRenderingAtom`: `infos` is `(sansserif, serif)` when given.
    TextRendering {
        text: Vec<u16>,
        style: i32,
        infos: Option<(&'static str, &'static str)>,
    },
    UnderScore,
    UnderOver(Box<UnderOverData>),
    /// `BigDelimiterAtom`: `delim` is a symbol.
    BigDelimiter {
        delim: AtomId,
        size: i32,
    },
    /// `FencedAtom`: `left` and `right` are symbols, `middle` are `Middle`s.
    Fenced {
        base: AtomId,
        left: Option<AtomId>,
        right: Option<AtomId>,
        middle: Vec<AtomId>,
    },
    /// `NthRoot`.
    NthRoot {
        base: AtomId,
        root: AtomId,
    },
    /// `TextCircledAtom`.
    TextCircled {
        at: AtomId,
    },
    /// `AccentedAtom`: `accent` is a symbol.
    Accented {
        accent: AtomId,
        acc: bool,
        change_size: bool,
        base: Option<AtomId>,
        underbase: Option<AtomId>,
    },
    /// `RuleAtom`.
    Rule {
        w_unit: Unit,
        w: f32,
        h_unit: Unit,
        h: f32,
        r_unit: Unit,
        r: f32,
    },
    /// `RaiseAtom`.
    Raise {
        base: Option<AtomId>,
        r_unit: Unit,
        r: f32,
        h_unit: Unit,
        h: f32,
        d_unit: Unit,
        d: f32,
    },
    /// `VRowAtom`.
    VRow {
        elements: Vec<AtomId>,
        raise: SpaceData,
        add_interline: bool,
        valign: Alignment,
        halign: Alignment,
    },
    /// `FBoxAtom`: a frame, filled with `bg` unless it is transparent.
    FBox {
        base: AtomId,
        bg: u32,
        line: u32,
    },
    /// `MiddleAtom`.
    Middle {
        base: Option<AtomId>,
        r#box: BoxId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct AtomNode {
    pub atom_type: AtomType,
    pub limits: LimitsType,
    pub alignment: Alignment,
    pub wrap_depth: i32,
    pub kind: AtomKind,
}

impl AtomNode {
    pub fn new(kind: AtomKind) -> Self {
        Self {
            atom_type: AtomType::Ordinary,
            limits: LimitsType::NoLimits,
            alignment: Alignment::None,
            wrap_depth: 0,
            kind,
        }
    }

    pub fn is_char_symbol(&self) -> bool {
        matches!(
            self.kind,
            AtomKind::Char { .. } | AtomKind::Symbol { .. } | AtomKind::FixedChar { .. }
        )
    }

    pub fn is_row(&self) -> bool {
        matches!(self.kind, AtomKind::Row { .. })
    }
}

/// The deepest wrapper chain `inheritWrapDepth` allows.
pub const MAX_WRAP_DEPTH: i32 = 250;

/// Everything one formula's parse and layout share. The C++ keeps most of
/// this in globals and resets it before each formula.
pub struct Ctx<'a> {
    pub atoms: Vec<AtomNode>,
    pub boxes: Boxes,
    pub settings: GeneralSettings,
    /// `TeXRender::_magFactor`, set by `\magnification`.
    pub mag_factor: f32,
    pub measure: &'a mut dyn Measure,
    /// The depth `ParseDepthGuard` counts.
    pub parse_depth: i32,
    /// `gExpansionWork`.
    pub expansion_work: i64,
    symbols: HashMap<&'static str, AtomId>,
    /// User commands: name to (argc, posOpts).
    pub user_macros: BTreeMap<Vec<u16>, (i32, i32)>,
    pub user_codes: BTreeMap<Vec<u16>, Vec<u16>>,
    pub user_replacements: BTreeMap<Vec<u16>, Vec<u16>>,
    /// `NewCommandMacro::_errIfConflict`.
    pub err_if_conflict: bool,
    /// `TextRenderingBox::_font`.
    pub text_font: TextFont,
    /// `RowAtom::_breakEveywhere`.
    pub break_everywhere: bool,
    /// Colors `\definecolor` added or replaced.
    pub user_colors: BTreeMap<String, u32>,
    /// The first part of MicroTeX the formula needed that is not ported.
    pub unsupported: Option<String>,
    /// `Formula::_predefinedTeXFormulas`: the parsed predefined formulas
    /// whose root is not a row, shared by every use.
    pub predefined: HashMap<Vec<u16>, Option<AtomId>>,
    /// Characters `operator[]` added to `_symbolFormulaMappings` with an
    /// empty formula.
    pub inserted_formula_mappings: HashSet<u16>,
}

impl<'a> Ctx<'a> {
    pub fn new(measure: &'a mut dyn Measure) -> Self {
        Self {
            atoms: Vec::new(),
            boxes: Boxes::new(),
            settings: GeneralSettings::default(),
            mag_factor: 0.0,
            measure,
            parse_depth: 0,
            expansion_work: 0,
            symbols: HashMap::new(),
            user_macros: BTreeMap::new(),
            user_codes: BTreeMap::new(),
            user_replacements: BTreeMap::new(),
            err_if_conflict: true,
            text_font: TextFont::new("Serif", 0, 10.0),
            break_everywhere: false,
            user_colors: BTreeMap::new(),
            unsupported: None,
            predefined: HashMap::new(),
            inserted_formula_mappings: HashSet::new(),
        }
    }

    pub fn add(&mut self, node: AtomNode) -> AtomId {
        let id = AtomId(self.atoms.len() as u32);
        self.atoms.push(node);
        id
    }

    pub fn atom(&mut self, kind: AtomKind) -> AtomId {
        self.add(AtomNode::new(kind))
    }

    pub fn get(&self, id: AtomId) -> &AtomNode {
        &self.atoms[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: AtomId) -> &mut AtomNode {
        &mut self.atoms[id.0 as usize]
    }

    /// `privateCopy()`: a shallow copy, children shared.
    pub fn private_copy(&mut self, id: AtomId) -> AtomId {
        let node = self.get(id).clone();
        self.add(node)
    }

    /// `Atom::inheritWrapDepth()`.
    pub fn inherit_wrap_depth(&mut self, id: AtomId, base: Option<AtomId>) -> Result<()> {
        let Some(base) = base else {
            return Ok(());
        };
        let depth = self.get(base).wrap_depth + 1;
        self.get_mut(id).wrap_depth = depth;
        if depth > MAX_WRAP_DEPTH {
            return Err(Error::parse("Formula nesting is too deep!"));
        }
        Ok(())
    }

    /// `SymbolAtom::get()`: the formula's single copy of a predefined symbol.
    pub fn symbol(&mut self, name: &str) -> Result<AtomId> {
        if let Some(&id) = self.symbols.get(name) {
            return Ok(id);
        }
        let Ok(index) =
            tables::SYMBOLS.binary_search_by(|s| s.name.as_bytes().cmp(name.as_bytes()))
        else {
            return Err(Error::symbol_not_found(name));
        };
        let symbol = &tables::SYMBOLS[index];
        let id = self.new_symbol(symbol.name, atom_type_of(symbol.atom_type));
        self.symbols.insert(symbol.name, id);
        Ok(id)
    }

    /// `SymbolAtom(name, type, del)`.
    pub fn new_symbol(&mut self, name: &'static str, atom_type: AtomType) -> AtomId {
        let mut node = AtomNode::new(AtomKind::Symbol {
            name,
            unicode: 0,
            text_symbol: false,
        });
        node.atom_type = atom_type;
        if atom_type == AtomType::BigOperator {
            node.limits = LimitsType::Normal;
        }
        self.add(node)
    }

    /// `RowAtom()`.
    pub fn new_row(&mut self) -> AtomId {
        self.atom(AtomKind::Row {
            elements: Vec::new(),
            breakable: true,
            look_at_last_atom: false,
            previous: None,
        })
    }

    /// `RowAtom(atom)`: a row of one atom, or of the elements of a row.
    pub fn new_row_of(&mut self, atom: Option<AtomId>) -> AtomId {
        let elements = match atom {
            None => Vec::new(),
            Some(a) => match &self.get(a).kind {
                AtomKind::Row { elements, .. } => elements.clone(),
                _ => vec![a],
            },
        };
        self.atom(AtomKind::Row {
            elements,
            breakable: true,
            look_at_last_atom: false,
            previous: None,
        })
    }

    pub fn row_elements(&self, row: AtomId) -> &[AtomId] {
        match &self.get(row).kind {
            AtomKind::Row { elements, .. } => elements,
            _ => &[],
        }
    }

    /// `RowAtom::add()`.
    pub fn row_add(&mut self, row: AtomId, atom: Option<AtomId>) {
        if let (Some(a), AtomKind::Row { elements, .. }) = (atom, &mut self.get_mut(row).kind) {
            elements.push(a);
        }
    }

    /// `RowAtom::popLastAtom()`: an empty row pops a zero space.
    pub fn row_pop_last(&mut self, row: AtomId) -> AtomId {
        if let AtomKind::Row { elements, .. } = &mut self.get_mut(row).kind
            && let Some(last) = elements.pop()
        {
            return last;
        }
        self.atom(AtomKind::Space(SpaceData::sized(
            Unit::Point,
            0.0,
            0.0,
            0.0,
        )))
    }

    /// `Atom::leftType()`.
    pub fn left_type(&self, id: AtomId) -> AtomType {
        let node = self.get(id);
        match &node.kind {
            AtomKind::Scale { base, .. } => self.left_type(*base),
            AtomKind::Color { elements, .. } | AtomKind::Phantom { elements, .. } => {
                self.left_type(*elements)
            }
            AtomKind::Typed { left, .. } => *left,
            AtomKind::Fenced { .. } => AtomType::Inner,
            AtomKind::Raise { base: Some(b), .. } => self.left_type(*b),
            AtomKind::UnderOver(data) => {
                data.base.map_or(AtomType::Ordinary, |b| self.left_type(b))
            }
            AtomKind::Scripts { base, .. } => base.map_or(node.atom_type, |b| self.left_type(b)),
            AtomKind::Row { elements, .. } => elements
                .first()
                .map_or(AtomType::Ordinary, |&e| self.left_type(e)),
            _ => node.atom_type,
        }
    }

    /// `Atom::rightType()`.
    pub fn right_type(&self, id: AtomId) -> AtomType {
        let node = self.get(id);
        match &node.kind {
            AtomKind::Scale { base, .. } => self.right_type(*base),
            AtomKind::Color { elements, .. } | AtomKind::Phantom { elements, .. } => {
                self.right_type(*elements)
            }
            AtomKind::Typed { right, .. } => *right,
            AtomKind::Fenced { .. } => AtomType::Inner,
            AtomKind::Raise { base: Some(b), .. } => self.right_type(*b),
            AtomKind::UnderOver(data) => {
                data.base.map_or(AtomType::Ordinary, |b| self.right_type(b))
            }
            AtomKind::Scripts { base, .. } => base.map_or(node.atom_type, |b| self.right_type(b)),
            AtomKind::Row { elements, .. } => elements
                .last()
                .map_or(AtomType::Ordinary, |&e| self.right_type(e)),
            _ => node.atom_type,
        }
    }

    /// `Row::setPreviousAtom()` for the atoms that hold a row.
    pub fn set_previous_atom(&mut self, id: AtomId, prev: Option<Dummy>) {
        let row = match &self.get(id).kind {
            AtomKind::Row { .. } => id,
            AtomKind::Color { elements, .. } | AtomKind::Phantom { elements, .. } => *elements,
            _ => return,
        };
        if let AtomKind::Row { previous, .. } = &mut self.get_mut(row).kind {
            *previous = prev;
        }
    }

    /// `TypedAtom::getBase()`: the wrapped atom, copied when the limits the
    /// typed atom carries differ from its own.
    pub fn typed_base(&mut self, id: AtomId) -> Option<AtomId> {
        let (atom, limits) = match &self.get(id).kind {
            AtomKind::Typed { atom, .. } => (*atom, self.get(id).limits),
            _ => return None,
        };
        if self.get(atom).limits != limits {
            let copy = self.private_copy(atom);
            self.get_mut(copy).limits = limits;
            return Some(copy);
        }
        Some(atom)
    }
}

/// The symbol table stores types as their C++ values.
pub fn atom_type_of(value: i8) -> AtomType {
    AtomType::from_index(i32::from(value)).unwrap_or(AtomType::Ordinary)
}
