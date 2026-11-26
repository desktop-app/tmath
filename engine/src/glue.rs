//! `tex::Glue`: the space TeX puts between atoms of two types, and the
//! explicit math skips.

use crate::enums::{AtomType, SpaceType};
use crate::env::Environment;
use crate::tables;

const STYLE_COUNT: i32 = 5;

/// The width, stretch and shrink of a `GlueBox`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glue {
    pub width: f32,
    pub stretch: f32,
    pub shrink: f32,
}

impl Glue {
    pub const ZERO: Self = Self {
        width: 0.0,
        stretch: 0.0,
        shrink: 0.0,
    };
}

/// One mu: a quad of the mu font, over 18.
fn factor(env: &Environment) -> f32 {
    let font = &env.font;
    font.quad(&env.settings, env.style, font.mu_font_id(&env.settings)) / 18.0
}

fn create(kind: usize, env: &Environment) -> Glue {
    let [space, stretch, shrink] = tables::GLUE_TYPES[kind];
    let f = factor(env);
    Glue {
        width: f32::from(space) * f,
        stretch: f32::from(stretch) * f,
        shrink: f32::from(shrink) * f,
    }
}

/// `Glue::indexOf()`: types past `Inner`, and `None`, count as `Ordinary`.
fn index_of(left: AtomType, right: AtomType, env: &Environment) -> i32 {
    let clamp = |t: AtomType| {
        if t > AtomType::Inner || t < AtomType::Ordinary {
            AtomType::Ordinary
        } else {
            t
        }
    };
    let l = clamp(left) as usize;
    let r = clamp(right) as usize;
    let k = (env.style as i32 / 2).clamp(0, STYLE_COUNT - 1) as usize;
    i32::from(tables::GLUE_TABLE[l][r][k]) - i32::from(b'0')
}

fn kind_of(index: i32) -> Option<usize> {
    usize::try_from(index)
        .ok()
        .filter(|&i| i < tables::GLUE_TYPES.len())
}

/// `Glue::get(AtomType, AtomType, env)`.
pub fn between(left: AtomType, right: AtomType, env: &Environment) -> Glue {
    kind_of(index_of(left, right, env)).map_or(Glue::ZERO, |kind| create(kind, env))
}

/// `Glue::getSpace(AtomType, AtomType, env)`.
pub fn space_between(left: AtomType, right: AtomType, env: &Environment) -> f32 {
    kind_of(index_of(left, right, env)).map_or(0.0, |kind| {
        f32::from(tables::GLUE_TYPES[kind][0]) * factor(env)
    })
}

/// `Glue::get(SpaceType, env)`: a negative skip negates the width only.
pub fn skip(kind: SpaceType, env: &Environment) -> Glue {
    let index = kind as i32;
    let mut glue = create(index.unsigned_abs() as usize, env);
    if index < 0 {
        glue.width = -glue.width;
    }
    glue
}

/// `Glue::getSpace(SpaceType, env)`.
pub fn skip_space(kind: SpaceType, env: &Environment) -> f32 {
    let index = kind as i32;
    let value = f32::from(tables::GLUE_TYPES[index.unsigned_abs() as usize][0]) * factor(env);
    if index < 0 { -value } else { value }
}
