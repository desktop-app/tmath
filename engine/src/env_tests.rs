//! Replays the oracle's `--env-queries` against the port.

use crate::enums::{AtomType, SpaceType, Unit};
use crate::env::{Environment, parse_length, unit_factor};
use crate::font::{GeneralSettings, TeXFont, TexStyle};
use crate::glue::{self, Glue};
use crate::tables;
use crate::text::{format_float, format_units};
use std::fmt::Write;

const SIZES: [f32; 4] = [1024.0, 20.0, 13.5, 0.75];

const DERIVATIONS: [&str; 8] = [
    "copy", "copytf", "cramp", "dnom", "num", "root", "sub", "sup",
];

fn derive(env: &Environment, which: usize) -> Environment {
    match which {
        0 => env.copy(),
        1 => env.copy_with_font(env.font),
        2 => env.cramp_style(),
        3 => env.dnom_style(),
        4 => env.num_style(),
        5 => env.root_style(),
        6 => env.sub_style(),
        _ => env.sup_style(),
    }
}

fn describe(env: &Environment) -> String {
    format!(
        "{} {} {} {} {} {} {} {} {}",
        env.style as i32,
        env.last_font_id(),
        format_float(env.text_width()),
        format_float(env.interline()),
        format_float(env.space()),
        format_float(env.size()),
        i32::from(env.small_cap),
        if env.text_style.is_empty() {
            "-"
        } else {
            env.text_style
        },
        format_float(env.scale_factor)
    )
}

fn glue_text(glue: Glue) -> String {
    format!(
        "{} {} {}",
        format_float(glue.width),
        format_float(glue.stretch),
        format_float(glue.shrink)
    )
}

/// Every type outside `Ordinary..=Inner` is looked up as `Ordinary`, so the
/// integers the oracle casts that name no type stand for `Ordinary` here.
fn atom_type(index: i32) -> AtomType {
    AtomType::from_index(index).unwrap_or(AtomType::Ordinary)
}

fn environment_queries(out: &mut String, size: f32, style: i32) {
    let settings = GeneralSettings::default();
    let tag = format!("{} {style}", format_float(size));
    let font = TeXFont::new(size);
    let style = TexStyle::from_index(style).expect("style");
    let env = Environment::new(style, font, settings);
    let _ = writeln!(out, "env {tag} -> {}", describe(&env));
    for unit in -2..16 {
        let f = unit_factor(Unit::from_index(unit), &env);
        let _ = writeln!(out, "factor {tag} {unit} -> {}", format_float(f));
    }
    for (which, name) in DERIVATIONS.iter().enumerate() {
        let derived = derive(&env, which);
        let _ = writeln!(out, "derive {tag} {name} -> {}", describe(&derived));
        let mut line = String::new();
        for next in 0..8 {
            let _ = write!(line, " {}", derive(&derived, next).style as i32);
        }
        let _ = writeln!(out, "chain {tag} {name} ->{line}");
    }

    let mut modified = Environment::new(style, font, settings);
    modified.text_style = "mathbb";
    modified.small_cap = true;
    modified.scale_factor = 2.5;
    modified.set_last_font_id(3);
    modified.set_text_width(Unit::Em, 7.0);
    modified.set_interline(Unit::Mu, 3.0);
    let _ = writeln!(out, "modified {tag} -> {}", describe(&modified));
    for (which, name) in DERIVATIONS.iter().enumerate() {
        let derived = derive(&modified, which);
        let _ = writeln!(out, "mderive {tag} {name} -> {}", describe(&derived));
    }

    for id in -1..tables::FONTS.len() as i32 {
        let mut with_font = Environment::new(style, font, settings);
        with_font.set_last_font_id(id);
        let _ = writeln!(
            out,
            "lastfont {tag} {id} -> {} {} {}",
            with_font.last_font_id(),
            format_float(unit_factor(Unit::Ex, &with_font)),
            format_float(with_font.interline())
        );
    }

    for unit in -1..15 {
        let mut sized =
            Environment::with_width(style, font, settings, Unit::from_index(unit), 37.5);
        let _ = writeln!(out, "width {tag} {unit} -> {}", describe(&sized));
        sized.set_interline(Unit::from_index(unit), 2.25);
        let _ = writeln!(
            out,
            "interline {tag} {unit} -> {}",
            format_float(sized.interline())
        );
    }

    for l in -1..15 {
        for r in -1..15 {
            let (left, right) = (atom_type(l), atom_type(r));
            let _ = writeln!(
                out,
                "glue {tag} {l} {r} -> {} {}",
                format_float(glue::space_between(left, right, &env)),
                glue_text(glue::between(left, right, &env))
            );
        }
    }
    for kind in -3..4 {
        let kind_type = SpaceType::from_index(kind).expect("space type");
        let _ = writeln!(
            out,
            "skip {tag} {kind} -> {} {}",
            format_float(glue::skip_space(kind_type, &env)),
            glue_text(glue::skip(kind_type, &env))
        );
    }
}

fn parse_units(field: &str) -> Vec<u16> {
    let inner = field
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .expect("units");
    if inner.is_empty() {
        return Vec::new();
    }
    inner
        .split(' ')
        .map(|x| u16::from_str_radix(x, 16).expect("unit"))
        .collect()
}

#[test]
fn env_queries_match_the_oracle() {
    let golden = include_str!("../../corpus/golden/env_queries.txt").replace("\r\n", "\n");
    let mut answered = String::new();
    for size in SIZES {
        for style in 0..8 {
            environment_queries(&mut answered, size, style);
        }
    }
    for line in golden.lines().filter(|l| l.starts_with("length ")) {
        let field = &line["length ".len()..line.find(" -> ").expect("arrow")];
        let units = parse_units(field);
        assert_eq!(format_units(units.iter().copied()), field);
        let (unit, value) = parse_length(&units);
        let _ = writeln!(
            answered,
            "length {field} -> {} {}",
            unit as i32,
            format_float(value)
        );
    }

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
