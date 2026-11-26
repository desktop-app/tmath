//! Checks the start-up parse of predefined formulas against the oracle's
//! `--predefined-cache`: the C++ keeps exactly these after init.

use crate::atoms::Ctx;
use crate::boxes::Measure;
use crate::draw::{Rect, TextFont};
use crate::parser::predefined_formula;
use crate::tables;
use crate::text::format_units;

struct NoMeasure;

impl Measure for NoMeasure {
    fn measure(&mut self, _font: &TextFont, _text: &[u16]) -> Rect {
        Rect::default()
    }
}

#[test]
fn startup_keeps_what_the_oracle_keeps() {
    let golden = include_str!("../../corpus/golden/predefined_cache.txt").replace("\r\n", "\n");
    let mut kept = Vec::new();
    for (name, _) in tables::PREDEFINED_FORMULAS.iter() {
        let units: Vec<u16> = name.encode_utf16().collect();
        let mut measure = NoMeasure;
        let mut ctx = Ctx::new(&mut measure);
        let _ = predefined_formula(&mut ctx, &units);
        if !ctx.predefined_uncached.contains(&units) {
            kept.push(units);
        }
    }
    kept.sort();
    let answered: String = kept
        .iter()
        .map(|units| format!("cached {}\n", format_units(units.iter().copied())))
        .collect();
    assert_eq!(answered, golden);
}
