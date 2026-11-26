#![forbid(unsafe_code)]
// Negated float comparisons are how the C++ treats NaN; they are kept as is.
#![allow(clippy::neg_cmp_op_on_partial_ord)]

pub mod atoms;
pub mod boxes;
pub mod decor;
pub mod draw;
pub mod enums;
pub mod env;
pub mod error;
pub mod font;
pub mod glue;
pub mod layout;
pub mod macros;
pub mod matrix;
pub mod numparse;
pub mod parser;
pub mod render;
pub mod tables;
pub mod text;
pub mod wstr;

#[cfg(test)]
mod env_tests;
#[cfg(test)]
mod font_tests;
#[cfg(test)]
mod predefined_tests;

use atoms::{AtomKind, Ctx};
use boxes::{Alignment, Boxes, Measure};
use draw::{Graphics, Op, is_transparent};
use enums::Unit;
use env::{Environment, unit_factor};
use error::{Error, Result};
use font::{TeXFont, TexStyle};
use parser::{Formula, Parser};
use render::TeXRender;

/// A laid-out formula: its boxes and the render that measures and draws them.
pub struct Laid {
    pub boxes: Boxes,
    pub render: TeXRender,
}

impl Laid {
    pub fn width(&self) -> i32 {
        self.render.width(&self.boxes)
    }

    pub fn height(&self) -> i32 {
        self.render.height(&self.boxes)
    }

    pub fn depth(&self) -> i32 {
        self.render.depth(&self.boxes)
    }

    pub fn baseline(&self) -> f32 {
        self.render.baseline(&self.boxes)
    }

    /// The `Graphics2D` calls `TeXRender::draw(g2, 0, 0)` makes.
    pub fn draw(&self) -> Vec<Op> {
        let mut g = Graphics::new();
        self.render.draw(&self.boxes, &mut g, 0, 0);
        g.ops
    }
}

/// `LaTeX::parse(latex, width, textSize, lineSpace, fg)` as tdesktop calls
/// it: `$$` and `\[` formulas are centered, the rest are laid out left
/// aligned at their natural width; rows wider than `width` pixels break.
pub fn layout(
    tex: &[u16],
    width: i32,
    text_size: f32,
    line_space: f32,
    fg: u32,
    measure: &mut dyn Measure,
) -> Result<Laid> {
    let mut ctx = Ctx::new(measure);
    let result = layout_in(&mut ctx, tex, width, text_size, line_space, fg);
    if let Some(what) = ctx.unsupported.take() {
        return Err(Error::unsupported(what));
    }
    let render = result?;
    Ok(Laid {
        boxes: std::mem::take(&mut ctx.boxes),
        render,
    })
}

fn layout_in(
    ctx: &mut Ctx,
    tex: &[u16],
    width: i32,
    text_size: f32,
    line_space: f32,
    fg: u32,
) -> Result<TeXRender> {
    let lined = !(wstr::starts_with(tex, "$$") || wstr::starts_with(tex, "\\["));
    let align = if lined {
        Alignment::Left
    } else {
        Alignment::Center
    };

    ctx.expanded_cap = parser::expanded_cap(tex.len());
    let mut parser = Parser::new(ctx, true, tex.to_vec(), Formula::default(), true, true)?;
    if !tex.is_empty() {
        parser.parse(ctx)?;
    }
    if ctx.limit_exceeded {
        return Err(Error::parse("Formula goes over a limit!"));
    }
    let root = match parser.formula.root {
        Some(root) => root,
        None => ctx.atom(AtomKind::Empty),
    };

    let font = TeXFont::new(text_size);
    let text_width = width as f32;
    let has_width = text_width != 0.0;
    let mut env = if has_width {
        Environment::with_width(
            TexStyle::Display,
            font,
            ctx.settings,
            Unit::Pixel,
            text_width,
        )
    } else {
        Environment::new(TexStyle::Display, font, ctx.settings)
    };
    env.set_interline(Unit::Pixel, line_space);
    let b = layout::create_box(ctx, root, &mut env)?;
    let root_box = if has_width {
        if line_space != 0.0 {
            let space = line_space * unit_factor(Unit::Pixel, &env);
            let split = render::split(&mut ctx.boxes, b, env.text_width(), space)?;
            let w = if lined {
                ctx.boxes.width(split)
            } else {
                env.text_width()
            };
            ctx.boxes.hbox_aligned(split, w, align)?
        } else {
            let w = if lined {
                ctx.boxes.width(b)
            } else {
                env.text_width()
            };
            ctx.boxes.hbox_aligned(b, w, align)?
        }
    } else {
        b
    };
    let mut render = TeXRender::new(root_box, text_size, true, ctx.mag_factor);
    if !is_transparent(fg) {
        render.fg = fg;
    }
    Ok(render)
}
