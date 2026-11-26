//! `TeXRender` (the laid-out formula: integer metrics and the top-level
//! draw) and `BoxSplitter` (line breaking of the top row).

use crate::boxes::{BoxId, Boxes};
use crate::draw::{BLACK, Graphics, is_transparent};
use crate::error::Result;

/// `rasterBound()`: the ceiling as an int, saturated, NaN as `INT_MIN`.
pub fn raster_bound(value: f32) -> i32 {
    let bound = value.ceil();
    if !(bound > i32::MIN as f32) {
        return i32::MIN;
    }
    if !(bound < i32::MAX as f32) {
        return i32::MAX;
    }
    bound as i32
}

/// `rasterSum()`: a saturating sum.
pub fn raster_sum(a: i32, b: i32) -> i32 {
    a.saturating_add(b)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// A laid-out formula.
#[derive(Clone, Debug)]
pub struct TeXRender {
    pub root: BoxId,
    pub text_size: f32,
    pub fg: u32,
    pub insets: Insets,
}

impl TeXRender {
    /// `TeXRender(box, textSize, trueValues)` with the magnification a
    /// formula set. The C++ also assigns `\DeclareMathSizes`' default size
    /// here, and then overwrites it on the next line, so it has no effect.
    pub fn new(root: BoxId, text_size: f32, true_values: bool, mag_factor: f32) -> Self {
        let size = if mag_factor != 0.0 {
            text_size * mag_factor.abs()
        } else {
            text_size
        };
        let mut insets = Insets::default();
        if !true_values {
            let offset = raster_bound(0.18 * text_size);
            insets.left += offset;
            insets.top += offset;
            insets.right += offset;
            insets.bottom += offset;
        }
        Self {
            root,
            text_size: size,
            fg: BLACK,
            insets,
        }
    }

    pub fn width(&self, boxes: &Boxes) -> i32 {
        let b = boxes.get(self.root);
        raster_sum(
            raster_bound(
                b.width * self.text_size + self.insets.left as f32 + self.insets.right as f32,
            ),
            1,
        )
    }

    pub fn depth(&self, boxes: &Boxes) -> i32 {
        let b = boxes.get(self.root);
        raster_sum(
            raster_bound(b.depth * self.text_size + self.insets.bottom as f32),
            1,
        )
    }

    pub fn height(&self, boxes: &Boxes) -> i32 {
        let b = boxes.get(self.root);
        raster_sum(
            raster_bound(b.height * self.text_size + self.insets.top as f32),
            self.depth(boxes),
        )
    }

    pub fn baseline(&self, boxes: &Boxes) -> f32 {
        let height = self.height(boxes);
        if height == 0 {
            return 0.0;
        }
        ((f64::from(height) - f64::from(self.depth(boxes))) / f64::from(height)) as f32
    }

    /// `TeXRender::draw(g2, x, y)`.
    pub fn draw(&self, boxes: &Boxes, g: &mut Graphics, x: i32, y: i32) {
        let old = g.color();
        g.scale(self.text_size, self.text_size);
        g.set_color(if is_transparent(self.fg) {
            BLACK
        } else {
            self.fg
        });
        let root = boxes.get(self.root);
        let left = x.wrapping_add(self.insets.left) as f32 / self.text_size;
        let top = y.wrapping_add(self.insets.top) as f32 / self.text_size + root.height;
        boxes.draw(self.root, g, left, top);
        g.reset();
        g.set_color(old);
    }
}

struct Position {
    index: i32,
    hbox: BoxId,
}

/// `BoxSplitter::split()`: breaks a row too wide for `width` into lines.
pub fn split(boxes: &mut Boxes, root: BoxId, width: f32, line_space: f32) -> Result<BoxId> {
    if !boxes.get(root).is_hbox() {
        return Ok(root);
    }
    if width == 0.0 || boxes.width(root) <= width {
        return Ok(root);
    }
    let vbox = boxes.vbox()?;
    let mut second: Option<BoxId> = None;
    let mut positions: Vec<Position> = Vec::new();
    let mut hbox = root;
    while boxes.width(hbox) > width
        && can_break(boxes, &mut positions, hbox, width) != boxes.width(hbox)
    {
        let pos = positions.pop().expect("a break position");
        let (mut first, mut rest) = boxes.hbox_split(pos.hbox, pos.index - 1)?;
        while let Some(pos) = positions.pop() {
            let (a, b) = boxes.hbox_split_remove(pos.hbox, pos.index)?;
            boxes.hbox_add(a, first)?;
            boxes.hbox_insert(b, 0, rest)?;
            first = a;
            rest = b;
        }
        boxes.vbox_add_interline(vbox, first, line_space)?;
        hbox = rest;
        second = Some(rest);
    }
    if let Some(second) = second {
        boxes.vbox_add_interline(vbox, second, line_space)?;
        return Ok(vbox);
    }
    Ok(hbox)
}

fn can_break(boxes: &Boxes, stack: &mut Vec<Position>, hbox: BoxId, width: f32) -> f32 {
    let children = boxes.get(hbox).children();
    let mut cum_width = vec![0.0f32; children.len() + 1];
    for (i, &child) in children.iter().enumerate() {
        let child_width = boxes.width(child);
        cum_width[i + 1] = cum_width[i] + child_width;
        if cum_width[i + 1] <= width {
            continue;
        }
        let pos = break_position(boxes, hbox, i as i32);
        if boxes.get(child).is_hbox() {
            let mut sub = Vec::new();
            let w = can_break(boxes, &mut sub, child, width - cum_width[i]);
            if w != child_width && (cum_width[i] + w <= width || pos == -1) {
                stack.push(Position {
                    index: i as i32 - 1,
                    hbox,
                });
                stack.extend(sub);
                return cum_width[i] + w;
            }
        }
        if pos != -1 {
            stack.push(Position { index: pos, hbox });
            return cum_width[pos as usize];
        }
    }
    boxes.width(hbox)
}

/// `BoxSplitter::getBreakPosition()`.
fn break_position(boxes: &Boxes, hbox: BoxId, i: i32) -> i32 {
    let breaks = boxes.hbox_breaks(hbox);
    if breaks.is_empty() {
        return -1;
    }
    if breaks.len() == 1 && breaks[0] <= i {
        return breaks[0];
    }
    for (pos, &b) in breaks.iter().enumerate() {
        if b > i {
            if pos == 0 {
                return -1;
            }
            return breaks[pos - 1];
        }
    }
    breaks[breaks.len() - 1]
}
