//! The box tree: `box/box*.h` and `box/box*.cpp`.
//!
//! Boxes live in one arena per formula and refer to each other by index. The
//! C++ shares boxes through `shared_ptr`, appends one box to a group more than
//! once and changes boxes after they were added; indices keep all of that
//! exactly, and dropping the arena never recurses.

use crate::draw::{CAP_BUTT, Graphics, JOIN_BEVEL, JOIN_MITER, Stroke, TextFont, is_transparent};
use crate::enums::AtomType;
use crate::env::NO_FONT;
use crate::error::{Error, Result};
use crate::font::Char;

/// Box budget per formula: constructions and group appends. Past it the
/// formula is rejected, as it would be by every caller's size cap anyway.
pub const MAX_BOX_BUDGET: i32 = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BoxId(u32);

/// `tex::Alignment`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    None = -1,
    Left = 0,
    Right = 1,
    Center = 2,
    Top = 3,
    Bottom = 4,
}

/// `std::max`: the first argument unless it is less than the second, which
/// keeps a NaN in the first argument where `f32::max` would drop it.
pub fn cmax(a: f32, b: f32) -> f32 {
    if a < b { b } else { a }
}

/// `std::min`: the first argument unless the second is less.
pub fn cmin(a: f32, b: f32) -> f32 {
    if b < a { b } else { a }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FrameShape {
    /// `FramedBox`.
    Square,
    /// `OvalBox`.
    Oval { multiplier: f32, diameter: f32 },
    /// `ShadowBox`.
    Shadow { rule: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoxKind {
    Strut,
    Glue {
        stretch: f32,
        shrink: f32,
    },
    Char {
        chr: u16,
        font: i32,
        size: f32,
        italic: f32,
    },
    TextRendering {
        font: TextFont,
        text: Vec<u16>,
        size: f32,
    },
    Line {
        lines: Vec<f32>,
        thickness: f32,
    },
    Rule {
        color: u32,
        spe_shift: f32,
    },
    HBox {
        children: Vec<BoxId>,
        breaks: Vec<i32>,
    },
    /// `VBox`, and `OverBar`, which only builds one.
    VBox {
        children: Vec<BoxId>,
        left_most: f32,
        right_most: f32,
    },
    Color {
        base: BoxId,
        fg: u32,
        bg: u32,
    },
    Scale {
        base: BoxId,
        sx: f32,
        sy: f32,
    },
    Reflect {
        base: BoxId,
    },
    Rotate {
        base: BoxId,
        angle: f32,
        xmin: f32,
        shift_x: f32,
        shift_y: f32,
    },
    Framed {
        base: BoxId,
        thickness: f32,
        space: f32,
        line: u32,
        bg: u32,
        shape: FrameShape,
    },
    Wrapper {
        base: BoxId,
        l: f32,
        fg: u32,
        bg: u32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoxNode {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
    pub shift: f32,
    pub atom_type: AtomType,
    pub kind: BoxKind,
}

impl BoxNode {
    fn new(kind: BoxKind) -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            depth: 0.0,
            shift: 0.0,
            atom_type: AtomType::None,
            kind,
        }
    }

    pub fn is_space(&self) -> bool {
        matches!(self.kind, BoxKind::Strut | BoxKind::Glue { .. })
    }

    pub fn is_hbox(&self) -> bool {
        matches!(self.kind, BoxKind::HBox { .. })
    }

    pub fn is_char(&self) -> bool {
        matches!(self.kind, BoxKind::Char { .. })
    }

    /// `descendants()`.
    pub fn children(&self) -> Vec<BoxId> {
        match &self.kind {
            BoxKind::HBox { children, .. } | BoxKind::VBox { children, .. } => children.clone(),
            BoxKind::Color { base, .. }
            | BoxKind::Scale { base, .. }
            | BoxKind::Reflect { base }
            | BoxKind::Rotate { base, .. }
            | BoxKind::Framed { base, .. }
            | BoxKind::Wrapper { base, .. } => vec![*base],
            _ => Vec::new(),
        }
    }

    /// The C++ class name, for box dumps.
    pub fn class_name(&self) -> &'static str {
        match &self.kind {
            BoxKind::Strut => "StrutBox",
            BoxKind::Glue { .. } => "GlueBox",
            BoxKind::Char { .. } => "CharBox",
            BoxKind::TextRendering { .. } => "TextRenderingBox",
            BoxKind::Line { .. } => "LineBox",
            BoxKind::Rule { .. } => "RuleBox",
            BoxKind::HBox { .. } => "HBox",
            BoxKind::VBox { .. } => "VBox",
            BoxKind::Color { .. } => "ColorBox",
            BoxKind::Scale { .. } => "ScaleBox",
            BoxKind::Reflect { .. } => "ReflectBox",
            BoxKind::Rotate { .. } => "RotateBox",
            BoxKind::Framed { shape, .. } => match shape {
                FrameShape::Square => "FramedBox",
                FrameShape::Oval { .. } => "OvalBox",
                FrameShape::Shadow { .. } => "ShadowBox",
            },
            BoxKind::Wrapper { .. } => "WrapperBox",
        }
    }
}

/// What a text box needs measured: `QFontMetricsF(font).boundingRect(text)`.
pub trait Measure {
    fn measure(&mut self, font: &TextFont, text: &[u16]) -> crate::draw::Rect;
}

#[derive(Debug, Default)]
pub struct Boxes {
    nodes: Vec<BoxNode>,
    used: i32,
}

impl Boxes {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn used_budget(&self) -> i32 {
        self.used
    }

    /// `countBoxAllocation()`.
    fn count(&mut self) -> Result<()> {
        self.used += 1;
        if self.used > MAX_BOX_BUDGET {
            return Err(Error::parse("Formula is too large to lay out!"));
        }
        Ok(())
    }

    fn push(&mut self, node: BoxNode) -> Result<BoxId> {
        self.count()?;
        let id = BoxId(self.nodes.len() as u32);
        self.nodes.push(node);
        Ok(id)
    }

    pub fn get(&self, id: BoxId) -> &BoxNode {
        &self.nodes[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: BoxId) -> &mut BoxNode {
        &mut self.nodes[id.0 as usize]
    }

    pub fn width(&self, id: BoxId) -> f32 {
        self.get(id).width
    }

    pub fn height(&self, id: BoxId) -> f32 {
        self.get(id).height
    }

    pub fn depth(&self, id: BoxId) -> f32 {
        self.get(id).depth
    }

    pub fn shift(&self, id: BoxId) -> f32 {
        self.get(id).shift
    }

    /// `Box::copyMetrics()`.
    pub fn copy_metrics(&mut self, to: BoxId, from: BoxId) {
        let (w, h, d, s) = {
            let b = self.get(from);
            (b.width, b.height, b.depth, b.shift)
        };
        let t = self.get_mut(to);
        t.width = w;
        t.height = h;
        t.depth = d;
        t.shift = s;
    }

    /// `Box::negWidth()`.
    pub fn neg_width(&mut self, id: BoxId) {
        let b = self.get_mut(id);
        b.width = -b.width;
    }

    /// `StrutBox(width, height, depth, shift)`.
    pub fn strut(&mut self, width: f32, height: f32, depth: f32, shift: f32) -> Result<BoxId> {
        self.push(BoxNode {
            width,
            height,
            depth,
            shift,
            ..BoxNode::new(BoxKind::Strut)
        })
    }

    /// `StrutBox(box)`: a space with the metrics of `of`.
    pub fn strut_of(&mut self, of: BoxId) -> Result<BoxId> {
        let id = self.push(BoxNode::new(BoxKind::Strut))?;
        self.copy_metrics(id, of);
        Ok(id)
    }

    /// `GlueBox(space, stretch, shrink)`.
    pub fn glue(&mut self, space: f32, stretch: f32, shrink: f32) -> Result<BoxId> {
        self.push(BoxNode {
            width: space,
            ..BoxNode::new(BoxKind::Glue { stretch, shrink })
        })
    }

    /// `CharBox(const Char&)`.
    pub fn char_box(&mut self, c: &Char) -> Result<BoxId> {
        self.push(BoxNode {
            width: c.metrics.width,
            height: c.metrics.height,
            depth: c.metrics.depth,
            ..BoxNode::new(BoxKind::Char {
                chr: c.c,
                font: c.font,
                size: c.metrics.size,
                italic: c.metrics.italic,
            })
        })
    }

    /// `CharBox::addItalicCorrectionToWidth()`.
    pub fn add_italic_correction_to_width(&mut self, id: BoxId) {
        let node = self.get_mut(id);
        if let BoxKind::Char { italic, .. } = &mut node.kind {
            node.width += *italic;
            *italic = 0.0;
        }
    }

    /// `TextRenderingBox(str, type, size, font, kerning)`. The layout keeps
    /// the text up to the first NUL, as the `QString` it is built from does.
    pub fn text_rendering(
        &mut self,
        text: &[u16],
        style: i32,
        size: f32,
        font: &TextFont,
        measure: &mut dyn Measure,
    ) -> Result<BoxId> {
        let id = self.push(BoxNode::new(BoxKind::Strut))?;
        let end = text.iter().position(|&u| u == 0).unwrap_or(text.len());
        let text = text[..end].to_vec();
        let font = font.derive(style);
        let rect = measure.measure(&font, &text);
        let height = -rect.y * size / 10.0;
        let node = self.get_mut(id);
        node.height = height;
        node.depth = rect.h * size / 10.0 - height;
        node.width = (rect.w + rect.x + 0.4) * size / 10.0;
        node.kind = BoxKind::TextRendering { font, text, size };
        Ok(id)
    }

    /// `LineBox(lines, thickness)`.
    pub fn line(&mut self, lines: Vec<f32>, thickness: f32) -> Result<BoxId> {
        self.count()?;
        if !lines.len().is_multiple_of(4) {
            return Err(Error::invalid_param("The vector not represent lines."));
        }
        let id = BoxId(self.nodes.len() as u32);
        self.nodes
            .push(BoxNode::new(BoxKind::Line { lines, thickness }));
        Ok(id)
    }

    /// `RuleBox(thickness, width, shift, color, trueshift)`.
    pub fn rule(
        &mut self,
        thickness: f32,
        width: f32,
        shift: f32,
        color: u32,
        true_shift: bool,
    ) -> Result<BoxId> {
        let (shift, spe_shift) = if true_shift {
            (shift, 0.0)
        } else {
            (0.0, shift)
        };
        self.push(BoxNode {
            height: thickness,
            width,
            shift,
            ..BoxNode::new(BoxKind::Rule { color, spe_shift })
        })
    }

    /// `HBox()`.
    pub fn hbox(&mut self) -> Result<BoxId> {
        self.push(BoxNode::new(BoxKind::HBox {
            children: Vec::new(),
            breaks: Vec::new(),
        }))
    }

    /// `HBox(box)`.
    pub fn hbox_of(&mut self, child: BoxId) -> Result<BoxId> {
        let id = self.hbox()?;
        self.hbox_add(id, child)?;
        Ok(id)
    }

    /// `HBox(box, width, alignment)`.
    pub fn hbox_aligned(
        &mut self,
        child: BoxId,
        width: f32,
        alignment: Alignment,
    ) -> Result<BoxId> {
        let id = self.hbox()?;
        if width == f32::INFINITY {
            self.hbox_add(id, child)?;
            return Ok(id);
        }
        let rest = width - self.width(child);
        if rest <= 0.0 {
            self.hbox_add(id, child)?;
            return Ok(id);
        }
        match alignment {
            Alignment::Center | Alignment::None => {
                let s = self.strut(rest / 2.0, 0.0, 0.0, 0.0)?;
                self.hbox_add(id, s)?;
                self.hbox_add(id, child)?;
                self.hbox_add(id, s)?;
            }
            Alignment::Left => {
                self.hbox_add(id, child)?;
                let s = self.strut(rest, 0.0, 0.0, 0.0)?;
                self.hbox_add(id, s)?;
            }
            Alignment::Right => {
                let s = self.strut(rest, 0.0, 0.0, 0.0)?;
                self.hbox_add(id, s)?;
                self.hbox_add(id, child)?;
            }
            _ => self.hbox_add(id, child)?,
        }
        Ok(id)
    }

    /// `HBox::recalculate()`.
    fn hbox_recalculate(&mut self, id: BoxId, child: BoxId) {
        let (cw, ch, cd, cs) = {
            let c = self.get(child);
            (c.width, c.height, c.depth, c.shift)
        };
        let node = self.get_mut(id);
        let empty = match &node.kind {
            BoxKind::HBox { children, .. } => children.is_empty(),
            _ => true,
        };
        node.width += cw;
        let h = if empty {
            f32::NEG_INFINITY
        } else {
            node.height
        };
        node.height = cmax(h, ch - cs);
        let d = if empty { f32::NEG_INFINITY } else { node.depth };
        node.depth = cmax(d, cd + cs);
    }

    fn group_children(&mut self, id: BoxId) -> &mut Vec<BoxId> {
        match &mut self.get_mut(id).kind {
            BoxKind::HBox { children, .. } | BoxKind::VBox { children, .. } => children,
            _ => unreachable!("not a group box"),
        }
    }

    /// `HBox::add(box)`.
    pub fn hbox_add(&mut self, id: BoxId, child: BoxId) -> Result<()> {
        self.hbox_recalculate(id, child);
        self.count()?;
        self.group_children(id).push(child);
        Ok(())
    }

    /// `HBox::add(pos, box)`.
    pub fn hbox_insert(&mut self, id: BoxId, pos: usize, child: BoxId) -> Result<()> {
        self.hbox_recalculate(id, child);
        self.count()?;
        self.group_children(id).insert(pos, child);
        Ok(())
    }

    /// `BoxGroup::addOnly()`: no metrics, no budget.
    pub fn add_only(&mut self, id: BoxId, child: BoxId) {
        self.group_children(id).push(child);
    }

    pub fn hbox_add_break_position(&mut self, id: BoxId, pos: i32) {
        if let BoxKind::HBox { breaks, .. } = &mut self.get_mut(id).kind {
            breaks.push(pos);
        }
    }

    pub fn group_len(&self, id: BoxId) -> usize {
        match &self.get(id).kind {
            BoxKind::HBox { children, .. } | BoxKind::VBox { children, .. } => children.len(),
            _ => 0,
        }
    }

    pub fn group_child(&self, id: BoxId, index: usize) -> BoxId {
        match &self.get(id).kind {
            BoxKind::HBox { children, .. } | BoxKind::VBox { children, .. } => children[index],
            _ => unreachable!("not a group box"),
        }
    }

    pub fn hbox_breaks(&self, id: BoxId) -> &[i32] {
        match &self.get(id).kind {
            BoxKind::HBox { breaks, .. } => breaks,
            _ => &[],
        }
    }

    /// `HBox::cloneBox()`: an empty box with the same shift.
    fn hbox_clone_empty(&mut self, id: BoxId) -> Result<BoxId> {
        let shift = self.shift(id);
        let b = self.hbox()?;
        self.get_mut(b).shift = shift;
        Ok(b)
    }

    /// `HBox::split(pos, shift)`.
    fn hbox_split_shifted(&mut self, id: BoxId, pos: i32, shift: i32) -> Result<(BoxId, BoxId)> {
        let hb1 = self.hbox_clone_empty(id)?;
        let hb2 = self.hbox_clone_empty(id)?;
        let children = self.get(id).children();
        let mut i = 0;
        while i <= pos {
            self.hbox_add(hb1, children[i as usize])?;
            i += 1;
        }
        let mut i = (pos + shift) as usize;
        while i < children.len() {
            self.hbox_add(hb2, children[i])?;
            i += 1;
        }
        let breaks = self.hbox_breaks(id).to_vec();
        for b in breaks {
            if b > pos + 1 {
                self.hbox_add_break_position(hb2, b - pos - 1);
            }
        }
        Ok((hb1, hb2))
    }

    pub fn hbox_split(&mut self, id: BoxId, pos: i32) -> Result<(BoxId, BoxId)> {
        self.hbox_split_shifted(id, pos, 1)
    }

    pub fn hbox_split_remove(&mut self, id: BoxId, pos: i32) -> Result<(BoxId, BoxId)> {
        self.hbox_split_shifted(id, pos, 2)
    }

    /// `VBox()`.
    pub fn vbox(&mut self) -> Result<BoxId> {
        self.push(BoxNode::new(BoxKind::VBox {
            children: Vec::new(),
            left_most: f32::MAX,
            right_most: f32::MIN,
        }))
    }

    fn vbox_recalculate_width(&mut self, id: BoxId, child: BoxId) {
        let (cw, cs) = {
            let c = self.get(child);
            (c.width, c.shift)
        };
        let node = self.get_mut(id);
        if let BoxKind::VBox {
            left_most,
            right_most,
            ..
        } = &mut node.kind
        {
            *left_most = cmin(*left_most, cs);
            *right_most = cmax(*right_most, cs + if cw > 0.0 { cw } else { 0.0 });
            node.width = *right_most - *left_most;
        }
    }

    /// `VBox::add(box)`.
    pub fn vbox_add(&mut self, id: BoxId, child: BoxId) -> Result<()> {
        self.count()?;
        self.group_children(id).push(child);
        let (ch, cd) = (self.height(child), self.depth(child));
        let first = self.group_len(id) == 1;
        let node = self.get_mut(id);
        if first {
            node.height = ch;
            node.depth = cd;
        } else {
            node.depth += ch + cd;
        }
        self.vbox_recalculate_width(id, child);
        Ok(())
    }

    /// `VBox::add(box, interline)`.
    pub fn vbox_add_interline(&mut self, id: BoxId, child: BoxId, interline: f32) -> Result<()> {
        if self.group_len(id) != 0 {
            let s = self.strut(0.0, interline, 0.0, 0.0)?;
            self.vbox_add(id, s)?;
        }
        self.vbox_add(id, child)
    }

    /// `VBox::add(pos, box)`.
    pub fn vbox_insert(&mut self, id: BoxId, pos: usize, child: BoxId) -> Result<()> {
        self.count()?;
        self.group_children(id).insert(pos, child);
        let (ch, cd) = (self.height(child), self.depth(child));
        let node = self.get_mut(id);
        if pos == 0 {
            node.depth += cd + node.height;
            node.height = ch;
        } else {
            node.depth += ch + cd;
        }
        self.vbox_recalculate_width(id, child);
        Ok(())
    }

    /// `BoxGroup::add` without the `VBox` metrics, as `VBox(box, rest,
    /// alignment)` calls it.
    fn group_insert_raw(&mut self, id: BoxId, pos: usize, child: BoxId) -> Result<()> {
        self.count()?;
        self.group_children(id).insert(pos, child);
        Ok(())
    }

    fn group_push_raw(&mut self, id: BoxId, child: BoxId) -> Result<()> {
        self.count()?;
        self.group_children(id).push(child);
        Ok(())
    }

    /// `VBox(box, rest, alignment)`.
    pub fn vbox_aligned(&mut self, child: BoxId, rest: f32, alignment: Alignment) -> Result<BoxId> {
        let id = self.vbox()?;
        self.vbox_add(id, child)?;
        match alignment {
            Alignment::Center => {
                let s = self.strut(0.0, rest / 2.0, 0.0, 0.0)?;
                self.group_insert_raw(id, 0, s)?;
                let node = self.get_mut(id);
                node.height += rest / 2.0;
                node.depth += rest / 2.0;
                self.group_push_raw(id, s)?;
            }
            Alignment::Top => {
                self.get_mut(id).depth += rest;
                let s = self.strut(0.0, rest, 0.0, 0.0)?;
                self.group_push_raw(id, s)?;
            }
            Alignment::Bottom => {
                self.get_mut(id).height += rest;
                let s = self.strut(0.0, rest, 0.0, 0.0)?;
                self.group_insert_raw(id, 0, s)?;
            }
            _ => {}
        }
        Ok(id)
    }

    /// `OverBar(b, kern, thickness)`.
    pub fn over_bar(&mut self, b: BoxId, kern: f32, thickness: f32) -> Result<BoxId> {
        let id = self.vbox()?;
        let s = self.strut(0.0, thickness, 0.0, 0.0)?;
        self.vbox_add(id, s)?;
        let width = self.width(b);
        let r = self.rule(thickness, width, 0.0, crate::draw::TRANSPARENT, true)?;
        self.vbox_add(id, r)?;
        let s = self.strut(0.0, kern, 0.0, 0.0)?;
        self.vbox_add(id, s)?;
        self.vbox_add(id, b)?;
        Ok(id)
    }

    /// `ColorBox(box, fg, bg)`.
    pub fn color_box(&mut self, base: BoxId, fg: u32, bg: u32) -> Result<BoxId> {
        let id = self.push(BoxNode::new(BoxKind::Color { base, fg, bg }))?;
        self.get_mut(id).atom_type = self.get(base).atom_type;
        self.copy_metrics(id, base);
        Ok(id)
    }

    /// `ScaleBox(b, sx, sy)`.
    pub fn scale_box(&mut self, base: BoxId, sx: f32, sy: f32) -> Result<BoxId> {
        let sx = if sx.is_nan() || sx.is_infinite() {
            1.0
        } else {
            sx
        };
        let sy = if sy.is_nan() || sy.is_infinite() {
            1.0
        } else {
            sy
        };
        let b = self.get(base).clone_metrics();
        self.push(BoxNode {
            width: b.0 * sx.abs(),
            height: if sy > 0.0 { b.1 * sy } else { -b.2 * sy },
            depth: if sy > 0.0 { b.2 * sy } else { -b.1 * sy },
            shift: b.3 * sy,
            ..BoxNode::new(BoxKind::Scale { base, sx, sy })
        })
    }

    /// `ReflectBox(b)`.
    pub fn reflect_box(&mut self, base: BoxId) -> Result<BoxId> {
        let id = self.push(BoxNode::new(BoxKind::Reflect { base }))?;
        self.copy_metrics(id, base);
        Ok(id)
    }

    /// `RotateBox(b, angle, x, y)`.
    pub fn rotate_box(&mut self, base: BoxId, angle: f32, x: f32, y: f32) -> Result<BoxId> {
        let id = self.push(BoxNode::new(BoxKind::Strut))?;
        let angle = if angle.is_nan() || angle.is_infinite() {
            0.0
        } else {
            angle
        };
        let angle = (f64::from(angle) * std::f64::consts::PI / 180.0) as f32;
        let (bw, bh, bd, _) = self.get(base).clone_metrics();
        let (height, depth, width) = (bh, bd, bw);
        let s = angle.sin();
        let c = angle.cos();
        let shift_x = x * (1.0 - c) + y * s;
        let shift_y = y * (1.0 - c) - x * s;
        let xmax = cmax(
            -height * s,
            cmax(
                depth * s,
                cmax(width * c + depth * s, width * c - height * s),
            ),
        ) + shift_x;
        let xmin = cmin(
            -height * s,
            cmin(
                depth * s,
                cmin(width * c + depth * s, width * c - height * s),
            ),
        ) + shift_x;
        let ymax = cmax(
            height * c,
            cmax(
                -depth * c,
                cmax(width * s - depth * c, width * s + height * c),
            ),
        );
        let ymin = cmin(
            height * c,
            cmin(
                -depth * c,
                cmin(width * s - depth * c, width * s + height * c),
            ),
        );
        let node = self.get_mut(id);
        node.width = xmax - xmin;
        node.height = ymax + shift_y;
        node.depth = -ymin - shift_y;
        node.kind = BoxKind::Rotate {
            base,
            angle,
            xmin,
            shift_x,
            shift_y,
        };
        Ok(id)
    }

    /// `FramedBox(box, thickness, space)` and the shapes built from one.
    pub fn framed_box(
        &mut self,
        base: BoxId,
        thickness: f32,
        space: f32,
        line: u32,
        bg: u32,
        shape: FrameShape,
    ) -> Result<BoxId> {
        let (bw, bh, bd, bs) = self.get(base).clone_metrics();
        let mut node = BoxNode {
            width: bw + 2.0 * thickness + 2.0 * space,
            height: bh + thickness + space,
            depth: bd + thickness + space,
            shift: bs,
            ..BoxNode::new(BoxKind::Framed {
                base,
                thickness,
                space,
                line,
                bg,
                shape,
            })
        };
        if let FrameShape::Shadow { rule } = shape {
            node.depth += rule;
            node.width += rule;
        }
        self.push(node)
    }

    /// `WrapperBox(base, width, rowheight, rowdepth, align)`.
    pub fn wrapper_box(
        &mut self,
        base: BoxId,
        width: f32,
        row_height: f32,
        row_depth: f32,
        align: Alignment,
    ) -> Result<BoxId> {
        let base_width = self.width(base);
        let l = match align {
            Alignment::Right => width - base_width,
            Alignment::Center => (width - base_width) / 2.0,
            _ => 0.0,
        };
        self.push(BoxNode {
            height: row_height,
            depth: row_depth,
            width: if base_width < 0.0 {
                width + base_width
            } else {
                width
            },
            ..BoxNode::new(BoxKind::Wrapper {
                base,
                l,
                fg: 0,
                bg: 0,
            })
        })
    }

    /// `WrapperBox::addInsets()`.
    pub fn wrapper_add_insets(&mut self, id: BoxId, l: f32, t: f32, r: f32, b: f32) {
        let node = self.get_mut(id);
        if let BoxKind::Wrapper { l: left, .. } = &mut node.kind {
            *left += l;
        }
        node.width += l + r;
        node.height += t;
        node.depth += b;
    }

    pub fn wrapper_set_colors(&mut self, id: BoxId, fg_color: Option<u32>, bg_color: Option<u32>) {
        if let BoxKind::Wrapper { fg, bg, .. } = &mut self.get_mut(id).kind {
            if let Some(c) = fg_color {
                *fg = c;
            }
            if let Some(c) = bg_color {
                *bg = c;
            }
        }
    }

    /// `Box::lastFontId()`.
    pub fn last_font_id(&self, id: BoxId) -> i32 {
        let node = self.get(id);
        match &node.kind {
            BoxKind::Char { font, .. } => *font,
            BoxKind::HBox { children, .. } | BoxKind::VBox { children, .. } => {
                let mut result = NO_FONT;
                for &child in children.iter().rev() {
                    result = self.last_font_id(child);
                    if result != NO_FONT {
                        break;
                    }
                }
                result
            }
            BoxKind::Color { base, .. }
            | BoxKind::Scale { base, .. }
            | BoxKind::Reflect { base }
            | BoxKind::Rotate { base, .. }
            | BoxKind::Framed { base, .. }
            | BoxKind::Wrapper { base, .. } => self.last_font_id(*base),
            _ => NO_FONT,
        }
    }

    /// `Box::draw()`.
    pub fn draw(&self, id: BoxId, g: &mut Graphics, x: f32, y: f32) {
        let node = self.get(id);
        match &node.kind {
            BoxKind::Strut | BoxKind::Glue { .. } => {}
            BoxKind::Char {
                chr, font, size, ..
            } => {
                let size = *size;
                if !(size > 0.0) {
                    return;
                }
                g.translate(x, y);
                if size != 1.0 {
                    g.scale(size, size);
                }
                if !g.is_tex_font(*font) {
                    g.set_tex_font(*font);
                }
                g.draw_char(*chr, 0.0, 0.0);
                if size != 1.0 {
                    g.scale(1.0 / size, 1.0 / size);
                }
                g.translate(-x, -y);
            }
            BoxKind::TextRendering { font, text, size } => {
                let size = *size;
                if !(size > 0.0) {
                    return;
                }
                g.translate(x, y);
                g.scale(0.1 * size, 0.1 * size);
                g.layout(font, text, 0.0, 0.0);
                g.scale(10.0 / size, 10.0 / size);
                g.translate(-x, -y);
            }
            BoxKind::Line { lines, thickness } => {
                let old = g.stroke().line_width;
                g.set_stroke_width(*thickness);
                g.translate(0.0, -node.height);
                for l in lines.chunks_exact(4) {
                    g.draw_line(l[0] + x, l[1] + y, l[2] + x, l[3] + y);
                }
                g.translate(0.0, node.height);
                g.set_stroke_width(old);
            }
            BoxKind::Rule { color, spe_shift } => {
                let old_color = g.color();
                if !is_transparent(*color) {
                    g.set_color(*color);
                }
                g.set_stroke(Stroke::new(node.height, CAP_BUTT, JOIN_BEVEL));
                let y = y - node.height / 2.0 - spe_shift;
                g.draw_line(x, y, x + node.width, y);
                g.restore_stroke_through_alias();
                g.set_color(old_color);
            }
            BoxKind::HBox { children, .. } => {
                let mut x_pos = x;
                for &child in children {
                    let c = self.get(child);
                    self.draw(child, g, x_pos, y + c.shift);
                    x_pos += self.get(child).width;
                }
            }
            BoxKind::VBox {
                children,
                left_most,
                ..
            } => {
                let mut y_pos = y - node.height;
                for &child in children {
                    let c = self.get(child);
                    y_pos += c.height;
                    self.draw(child, g, x + c.shift - left_most, y_pos);
                    y_pos += self.get(child).depth;
                }
            }
            BoxKind::Color { base, fg, bg } => {
                let prev = g.color();
                if !is_transparent(*bg) {
                    g.set_color(*bg);
                    g.fill_rect(x, y - node.height, node.width, node.height + node.depth);
                }
                g.set_color(if is_transparent(*fg) { prev } else { *fg });
                self.draw(*base, g, x, y);
                g.set_color(prev);
            }
            BoxKind::Scale { base, sx, sy } => {
                let (sx, sy) = (*sx, *sy);
                if sx == 0.0 || sy == 0.0 {
                    return;
                }
                let dec = if sx < 0.0 { node.width } else { 0.0 };
                g.translate(x + dec, y);
                g.scale(sx, sy);
                self.draw(*base, g, 0.0, 0.0);
                g.scale(1.0 / sx, 1.0 / sy);
                g.translate(-x - dec, -y);
            }
            BoxKind::Reflect { base } => {
                g.translate(x, y);
                g.scale(-1.0, 1.0);
                self.draw(*base, g, -node.width, 0.0);
                g.scale(-1.0, 1.0);
                g.translate(-x, -y);
            }
            BoxKind::Rotate {
                base,
                angle,
                xmin,
                shift_x,
                shift_y,
            } => {
                let y = y - shift_y;
                let x = x + (shift_x - xmin);
                g.rotate_at(-angle, x, y);
                self.draw(*base, g, x, y);
                g.rotate_at(*angle, x, y);
            }
            BoxKind::Framed {
                base,
                thickness,
                space,
                line,
                bg,
                shape,
            } => {
                let t = *thickness;
                let (w, h, d) = (node.width, node.height, node.depth);
                match shape {
                    FrameShape::Square => {
                        g.set_stroke(Stroke::new(t, CAP_BUTT, JOIN_MITER));
                        let th = t / 2.0;
                        if !is_transparent(*bg) {
                            let prev = g.color();
                            g.set_color(*bg);
                            g.fill_rect(x + th, y - h + th, w - t, h + d - t);
                            g.set_color(prev);
                        }
                        if !is_transparent(*line) {
                            let prev = g.color();
                            g.set_color(*line);
                            g.draw_rect(x + th, y - h + th, w - t, h + d - t);
                            g.set_color(prev);
                        } else {
                            g.draw_rect(x + th, y - h + th, w - t, h + d - t);
                        }
                        g.restore_stroke_through_alias();
                    }
                    FrameShape::Oval {
                        multiplier,
                        diameter,
                    } => {
                        g.set_stroke(Stroke::new(t, CAP_BUTT, JOIN_MITER));
                        let th = t / 2.0;
                        let r = if *diameter != 0.0 {
                            *diameter
                        } else {
                            multiplier * cmin(w - t, h + d - t)
                        };
                        g.draw_round_rect(x + th, y - h + th, w - t, h + d - t, r, r);
                        g.restore_stroke_through_alias();
                    }
                    FrameShape::Shadow { rule } => {
                        let sr = *rule;
                        let th = t / 2.0;
                        g.set_stroke(Stroke::new(t, CAP_BUTT, JOIN_MITER));
                        g.draw_rect(x + th, y - h + th, w - sr - t, h + d - sr - t);
                        let penth = (1.0 / g.sx()).abs();
                        g.set_stroke(Stroke::new(penth, CAP_BUTT, JOIN_MITER));
                        g.fill_rect(x + sr - penth, y + d - sr - penth, w - sr, sr);
                        g.fill_rect(
                            x + w - sr - penth,
                            y - h + th + sr,
                            sr,
                            d + h - 2.0 * sr - th,
                        );
                        g.restore_stroke_through_alias();
                    }
                }
                self.draw(*base, g, x + space + t, y);
            }
            BoxKind::Wrapper { base, l, fg, bg } => {
                let prev = g.color();
                if !is_transparent(*bg) {
                    g.set_color(*bg);
                    g.fill_rect(x, y - node.height, node.width, node.height + node.depth);
                }
                g.set_color(if is_transparent(*fg) { prev } else { *fg });
                let shift = self.get(*base).shift;
                self.draw(*base, g, x + l, y + shift);
                g.set_color(prev);
            }
        }
    }
}

impl BoxNode {
    fn clone_metrics(&self) -> (f32, f32, f32, f32) {
        (self.width, self.height, self.depth, self.shift)
    }
}
