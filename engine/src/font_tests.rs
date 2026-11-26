//! Replays the oracle's `--font-queries` against the port.

use crate::font::{Char, GeneralSettings, TeXFont, TexStyle};
use crate::tables::{self, CharFont};
use crate::text::format_float;
use std::fmt::Write;

const FLAGS: [(&str, bool, bool, bool, bool, bool); 9] = [
    ("none", false, false, false, false, false),
    ("bold", true, false, false, false, false),
    ("roman", false, true, false, false, false),
    ("ss", false, false, true, false, false),
    ("tt", false, false, false, true, false),
    ("it", false, false, false, false, true),
    ("bold-it", true, false, false, false, true),
    ("bold-ss", true, false, true, false, false),
    ("roman-tt-it", false, true, false, true, true),
];

const STYLES: [TexStyle; 4] = [
    TexStyle::Display,
    TexStyle::Text,
    TexStyle::Script,
    TexStyle::ScriptScript,
];

const PARAMS: [&str; 20] = [
    "axisheight",
    "bigopspacing1",
    "bigopspacing2",
    "bigopspacing3",
    "bigopspacing4",
    "bigopspacing5",
    "num1",
    "num2",
    "num3",
    "sub1",
    "sub2",
    "subdrop",
    "sup1",
    "sup2",
    "sup3",
    "supdrop",
    "denom1",
    "denom2",
    "defaultrulethickness",
    "missing",
];

fn sample_chars() -> Vec<u16> {
    let mut result: Vec<u16> = (b'0'..=b'9').map(u16::from).collect();
    result.extend((b'a'..=b'z').map(u16::from));
    result.extend((b'A'..=b'Z').map(u16::from));
    result.extend([0x20, 0x2B, 0x28, 0xC0, 0x3B1, 0x416, 0xFFFF]);
    result
}

fn char_text(c: &Char) -> String {
    format!(
        "{} {} {} {} {} {} {}",
        c.c,
        c.font,
        format_float(c.metrics.width),
        format_float(c.metrics.height),
        format_float(c.metrics.depth),
        format_float(c.metrics.italic),
        format_float(c.metrics.size)
    )
}

fn guarded<E>(result: Result<Char, E>) -> String {
    match result {
        Ok(c) => char_text(&c),
        Err(_) => "throw".to_owned(),
    }
}

fn queries() -> String {
    let settings = GeneralSettings::default();
    let chars = sample_chars();
    let mut out = String::new();
    for (name, bold, roman, ss, tt, it) in FLAGS {
        for style in STYLES {
            let font = TeXFont {
                bold,
                roman,
                ss,
                tt,
                it,
                ..TeXFont::new(1024.0)
            };
            let tag = format!("{name} {}", style as i32);
            for (symbol, _) in &tables::SYMBOL_FONTS {
                let r = guarded(font.char_of_symbol(&settings, symbol, style));
                let _ = writeln!(out, "sym {tag} {symbol} -> {r}");
            }
            for mapping in &tables::TEXT_STYLES {
                for &c in &chars {
                    let r = guarded(font.char_of_text_style(&settings, c, mapping.name, style));
                    let _ = writeln!(out, "text {tag} {} {c} -> {r}", mapping.name);
                }
            }
            for &c in &chars {
                let r = guarded(font.default_char(&settings, c, style));
                let _ = writeln!(out, "default {tag} {c} -> {r}");
            }
        }
    }

    let font = TeXFont::new(1024.0);
    for style in STYLES {
        let s = style as i32;
        for (symbol, _) in &tables::SYMBOL_FONTS {
            let mut c = font
                .char_of_symbol(&settings, symbol, style)
                .expect("mapped symbol");
            let mut chain = String::new();
            let mut step = 0;
            while step != 12 && font.has_next_larger(&c) {
                c = font.next_larger(&settings, &c, style);
                let _ = write!(chain, " | {}", char_text(&c));
                step += 1;
            }
            let _ = writeln!(out, "larger {s} {symbol} ->{chain}");
            let ext = match font.extension(&settings, &c, style) {
                None => "none".to_owned(),
                Some(e) => {
                    let part = |p: Option<Char>| p.map_or("-".to_owned(), |c| char_text(&c));
                    format!(
                        "{} | {} | {} | {}",
                        part(e.top),
                        part(e.middle),
                        part(e.repeat),
                        part(e.bottom)
                    )
                }
            };
            let _ = writeln!(out, "ext {s} {symbol} -> {ext}");
            let _ = writeln!(
                out,
                "isext {s} {symbol} -> {}",
                i32::from(font.is_extension_char(&c))
            );
        }
        for (id, info) in tables::FONTS.iter().enumerate() {
            let id = id as i32;
            for row in info.kerns {
                let cf = |code: f32| CharFont {
                    chr: code as u16,
                    font: id,
                    bold_font: id,
                };
                let k = font.kern(&settings, cf(row[0]), cf(row[1]), style);
                let _ = writeln!(
                    out,
                    "kern {s} {id} {} {} -> {}",
                    row[0] as i64,
                    row[1] as i64,
                    format_float(k)
                );
            }
            for row in info.metrics {
                let code = row[0] as u16;
                let cf = CharFont {
                    chr: code,
                    font: id,
                    bold_font: id,
                };
                let _ = writeln!(
                    out,
                    "skew {s} {id} {code} -> {}",
                    format_float(font.skew(&settings, cf, style))
                );
            }
            let _ = writeln!(
                out,
                "fontparams {s} {id} -> {} {} {}",
                format_float(font.quad(&settings, style, id)),
                format_float(font.x_height(&settings, style, id)),
                i32::from(font.has_space(id))
            );
        }
        for name in PARAMS {
            let _ = writeln!(
                out,
                "param {s} {name} -> {}",
                format_float(font.style_param(&settings, name, style))
            );
        }
        let _ = writeln!(
            out,
            "space {s} -> {} {} {}",
            format_float(font.space(&settings, style)),
            format_float(font.em(&settings, style)),
            font.mu_font_id(&settings)
        );
    }
    for (id, info) in tables::FONTS.iter().enumerate() {
        let id = id as i32;
        for row in info.ligatures {
            let cf = |code: u16| CharFont {
                chr: code,
                font: id,
                bold_font: id,
            };
            let lig = match font.ligature(cf(row[0]), cf(row[1])) {
                Some(l) => format!("{} {} {}", l.chr, l.font, l.bold_font),
                None => "null".to_owned(),
            };
            let _ = writeln!(out, "lig {id} {} {} -> {lig}", row[0], row[1]);
        }
    }
    out
}

#[test]
fn font_queries_match_the_oracle() {
    let golden = include_str!("../../corpus/golden/font_queries.txt").replace("\r\n", "\n");
    let answered = queries();
    let mut mismatches = 0;
    let mut first = Vec::new();
    for (line, (expected, actual)) in golden.lines().zip(answered.lines()).enumerate() {
        if expected != actual {
            mismatches += 1;
            if first.len() < 10 {
                first.push(format!(
                    "line {}:\n  oracle: {expected}\n  engine: {actual}",
                    line + 1
                ));
            }
        }
    }
    assert_eq!(
        golden.lines().count(),
        answered.lines().count(),
        "line counts differ"
    );
    assert!(
        mismatches == 0,
        "{mismatches} mismatches\n{}",
        first.join("\n")
    );
}
