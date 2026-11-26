//! MicroTeX's static tables, generated from the oracle's `--dump-tables`.
//!
//! Rows keep the order the C++ holds them in: the C++ finds rows by binary
//! search over that order, and so does the port.

// The literals are the oracle's nine-digit forms, exact for an f32.
#[rustfmt::skip]
#[allow(clippy::excessive_precision)]
mod generated;

pub use generated::*;

/// A character in a TeX font, as `tex::CharFont`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharFont {
    pub chr: u16,
    pub font: i32,
    pub bold_font: i32,
}

/// One TeX font, as `tex::FontInfo`. Metric and kern rows store their keys as
/// `f32`, the way the C++ arrays do.
#[derive(Debug)]
pub struct FontInfo {
    pub path: &'static str,
    pub x_height: f32,
    pub space: f32,
    pub quad: f32,
    /// `wchar_t(-1)`, read as 0xFFFF, when the font has no skew character.
    pub skew_char: u16,
    pub bold: i32,
    pub roman: i32,
    pub ss: i32,
    pub tt: i32,
    pub it: i32,
    /// `[code, width, height, depth, italic]`.
    pub metrics: &'static [[f32; 5]],
    /// `[code, top, middle, repeat, bottom]`.
    pub extensions: &'static [[i32; 5]],
    /// `[code, larger code, larger font]`.
    pub largers: &'static [[i32; 3]],
    /// `[left, right, ligature]`.
    pub ligatures: &'static [[u16; 3]],
    /// `[left, right, kern]`.
    pub kerns: &'static [[f32; 3]],
}

#[derive(Debug)]
pub struct TextStyle {
    pub name: &'static str,
    pub fonts: &'static [Option<CharFont>],
}

#[derive(Debug)]
pub struct Symbol {
    pub name: &'static str,
    pub atom_type: i8,
    pub limits: i8,
    pub unicode: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroKind {
    Predefined,
    Inflation,
    Other,
}

#[derive(Debug)]
pub struct MacroInfo {
    pub name: &'static str,
    pub argc: i32,
    pub pos_opts: i32,
    pub kind: MacroKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{format_float, format_units};
    use std::fmt::Write;

    fn units(value: &str) -> String {
        format_units(value.encode_utf16())
    }

    fn char_font(font: &Option<CharFont>) -> String {
        match font {
            Some(f) => format!("{} {} {}", f.chr, f.font, f.bold_font),
            None => "null".to_owned(),
        }
    }

    /// The tables in the oracle's `--dump-tables` form.
    fn dump() -> String {
        let mut out = String::new();
        for (id, name) in FONT_NAMES.iter().enumerate() {
            let _ = writeln!(out, "fontname {id} {name}");
        }
        for (id, f) in FONTS.iter().enumerate() {
            let _ = writeln!(
                out,
                "font {id} {} {} {} {} {} {} {} {} {} {}",
                f.path,
                format_float(f.x_height),
                format_float(f.space),
                format_float(f.quad),
                f.skew_char,
                f.bold,
                f.roman,
                f.ss,
                f.tt,
                f.it
            );
            for row in f.metrics {
                let _ = writeln!(
                    out,
                    "metric {id} {} {} {} {} {}",
                    row[0] as i64,
                    format_float(row[1]),
                    format_float(row[2]),
                    format_float(row[3]),
                    format_float(row[4])
                );
            }
            for row in f.extensions {
                let _ = writeln!(
                    out,
                    "ext {id} {} {} {} {} {}",
                    row[0], row[1], row[2], row[3], row[4]
                );
            }
            for row in f.largers {
                let _ = writeln!(out, "larger {id} {} {} {}", row[0], row[1], row[2]);
            }
            for row in f.ligatures {
                let _ = writeln!(out, "lig {id} {} {} {}", row[0], row[1], row[2]);
            }
            for row in f.kerns {
                let _ = writeln!(
                    out,
                    "kern {id} {} {} {}",
                    row[0] as i64,
                    row[1] as i64,
                    format_float(row[2])
                );
            }
        }
        for (i, name) in TEXT_STYLE_DEFAULTS.iter().enumerate() {
            let _ = writeln!(out, "textstyle-default {i} {name}");
        }
        for style in &TEXT_STYLES {
            for (i, font) in style.fonts.iter().enumerate() {
                let _ = writeln!(out, "textstyle {} {i} {}", style.name, char_font(font));
            }
        }
        for (name, font) in &SYMBOL_FONTS {
            let _ = writeln!(out, "symfont {name} {}", char_font(font));
        }
        for (name, value) in &PARAMETERS {
            let _ = writeln!(out, "param {name} {}", format_float(*value));
        }
        for (name, value) in &GENERAL_SETTINGS {
            let _ = writeln!(out, "general {name} {}", format_float(*value));
        }
        for s in &SYMBOLS {
            let _ = writeln!(
                out,
                "symbol {} {} {} {}",
                s.name, s.atom_type, s.limits, s.unicode
            );
        }
        for (name, code) in &PREDEFINED_FORMULAS {
            let _ = writeln!(out, "formula {} {}", units(name), units(code));
        }
        for (tag, table) in [
            ("fsym", &FORMULA_SYMBOL_MAPPINGS[..]),
            ("fsymtext", &FORMULA_TEXT_MAPPINGS[..]),
            ("fsymformula", &FORMULA_FORMULA_MAPPINGS[..]),
        ] {
            for (code, value) in table {
                let _ = writeln!(out, "{tag} {code} {}", units(value));
            }
        }
        for m in &MACROS {
            let kind = match m.kind {
                MacroKind::Predefined => "predef",
                MacroKind::Inflation => "inflation",
                MacroKind::Other => "other",
            };
            let _ = writeln!(
                out,
                "macro {} {} {} {kind}",
                units(m.name),
                m.argc,
                m.pos_opts
            );
        }
        for (name, code) in &NEW_COMMAND_CODES {
            let _ = writeln!(out, "newcmd-code {} {}", units(name), units(code));
        }
        for (name, code) in &NEW_COMMAND_REPLACEMENTS {
            let _ = writeln!(out, "newcmd-repl {} {}", units(name), units(code));
        }
        for (name, value) in &COLORS {
            let _ = writeln!(out, "color {name} {value:08x}");
        }
        for (name, value) in &UNITS {
            let _ = writeln!(out, "unit {name} {value}");
        }
        for (i, glue) in GLUE_TYPES.iter().enumerate() {
            let _ = writeln!(out, "glue {i} {} {} {}", glue[0], glue[1], glue[2]);
        }
        for (l, row) in GLUE_TABLE.iter().enumerate() {
            for (r, styles) in row.iter().enumerate() {
                let _ = write!(out, "gluetable {l} {r}");
                for style in styles {
                    let _ = write!(out, " {style}");
                }
                out.push('\n');
            }
        }
        for (from, to) in &SUP_SCRIPT_MAP {
            let _ = writeln!(out, "supmap {from} {to}");
        }
        for (from, to) in &SUB_SCRIPT_MAP {
            let _ = writeln!(out, "submap {from} {to}");
        }
        for content in &UNPARSED_CONTENTS {
            let _ = writeln!(out, "unparsed {}", units(content));
        }
        for (start, end) in &UNICODE_BLOCKS {
            let _ = writeln!(out, "block {start} {end}");
        }
        out
    }

    #[test]
    fn tables_match_the_oracle() {
        let golden = include_str!("../../../corpus/golden/tables.txt").replace("\r\n", "\n");
        let dumped = dump();
        if dumped != golden {
            let mismatch = golden
                .lines()
                .zip(dumped.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b);
            match mismatch {
                Some((line, (expected, actual))) => panic!(
                    "line {}:\n  oracle: {expected}\n  engine: {actual}",
                    line + 1
                ),
                None => panic!(
                    "line counts differ: oracle {}, engine {}",
                    golden.lines().count(),
                    dumped.lines().count()
                ),
            }
        }
    }
}
