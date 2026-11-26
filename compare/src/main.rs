//! Runs the engine over a JSONL corpus and prints, per formula, exactly what
//! the C++ oracle prints, so `tools/oracle_run.py --oracle <this>` produces a
//! table that can be compared with the oracle's goldens line by line.
//!
//! Usage: tmath_compare [--digest] [--no-boxes] < corpus.jsonl

use std::io::{BufRead, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use tmath::boxes::{BoxId, Boxes, Measure};
use tmath::draw::{Rect, TextFont, describe_text_font, format_op};
use tmath::text::{format_float, format_units};

struct Record {
    id: String,
    tex: Vec<u16>,
    size: f32,
    width: i32,
    line: f32,
    fg: u32,
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn skip_spaces(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\r') {
            self.i += 1;
        }
    }

    fn hex4(&mut self) -> Option<u16> {
        let digits = self.s.get(self.i..self.i + 4)?;
        let text = std::str::from_utf8(digits).ok()?;
        self.i += 4;
        u16::from_str_radix(text, 16).ok()
    }

    /// A JSON string as UTF-16 units; `\u` escapes give their unit as is,
    /// lone surrogates included.
    fn string(&mut self) -> Option<Vec<u16>> {
        if self.s.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        let mut out = Vec::new();
        let mut raw = Vec::new();
        let flush = |raw: &mut Vec<u8>, out: &mut Vec<u16>| {
            out.extend(String::from_utf8_lossy(raw).encode_utf16());
            raw.clear();
        };
        while self.i < self.s.len() {
            let c = self.s[self.i];
            self.i += 1;
            match c {
                b'"' => {
                    flush(&mut raw, &mut out);
                    return Some(out);
                }
                b'\\' => {
                    flush(&mut raw, &mut out);
                    let e = *self.s.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'"' => out.push(u16::from(b'"')),
                        b'\\' => out.push(u16::from(b'\\')),
                        b'/' => out.push(u16::from(b'/')),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(10),
                        b'r' => out.push(13),
                        b't' => out.push(9),
                        b'u' => out.push(self.hex4()?),
                        _ => return None,
                    }
                }
                _ => raw.push(c),
            }
        }
        None
    }

    fn number(&mut self) -> Option<f64> {
        let start = self.i;
        while self.i < self.s.len() && b"+-0123456789.eE".contains(&self.s[self.i]) {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i])
            .ok()?
            .parse()
            .ok()
    }
}

fn parse_record(line: &str) -> Option<Record> {
    let mut p = Parser {
        s: line.as_bytes(),
        i: 0,
    };
    let mut record = Record {
        id: String::new(),
        tex: Vec::new(),
        size: 1024.0,
        width: 600 * 64,
        line: 256.0,
        fg: 0xFFFF_FFFF,
    };
    let mut have_tex = false;
    p.skip_spaces();
    if p.s.get(p.i) != Some(&b'{') {
        return None;
    }
    p.i += 1;
    loop {
        p.skip_spaces();
        if p.s.get(p.i) == Some(&b'}') {
            break;
        }
        let key = String::from_utf16_lossy(&p.string()?);
        p.skip_spaces();
        if p.s.get(p.i) != Some(&b':') {
            return None;
        }
        p.i += 1;
        p.skip_spaces();
        match key.as_str() {
            "id" => record.id = String::from_utf16_lossy(&p.string()?),
            "tex" => {
                record.tex = p.string()?;
                have_tex = true;
            }
            _ => {
                let value = p.number()?;
                match key.as_str() {
                    "size" => record.size = value as f32,
                    "width" => record.width = value as i32,
                    "line" => record.line = value as f32,
                    "fg" => record.fg = value as u32,
                    _ => {}
                }
            }
        }
        p.skip_spaces();
        if p.s.get(p.i) == Some(&b',') {
            p.i += 1;
        }
    }
    have_tex.then_some(record)
}

/// The oracle's deterministic stand-in for `QFontMetricsF::boundingRect`.
struct Recorder {
    lines: Vec<String>,
}

/// The oracle's `MeasureText()`.
fn measure_text(font: &TextFont, text: &[u16]) -> Rect {
    let size = font.size();
    let mut width = 0.0f32;
    for &unit in text {
        width += (0.5 + f32::from(unit % 13) * 0.031_25) * size;
    }
    if font.style & 1 != 0 {
        width *= 1.0625;
    }
    if font.style & 2 != 0 {
        width += 0.125 * size;
    }
    let family_shift = (font.family.len() % 5) as f32 * 0.015_625 * size;
    Rect {
        x: 0.0625 * size + family_shift,
        y: -0.75 * size,
        w: width,
        h: size,
    }
}

/// Measures without recording, as the oracle does under `--bench`.
struct Quiet;

impl Measure for Quiet {
    fn measure(&mut self, font: &TextFont, text: &[u16]) -> Rect {
        measure_text(font, text)
    }
}

impl Measure for Recorder {
    fn measure(&mut self, font: &TextFont, text: &[u16]) -> Rect {
        let rect = measure_text(font, text);
        self.lines.push(format!(
            "measure {} {} -> {} {} {} {}",
            describe_text_font(font),
            format_units(text.iter().copied()),
            format_float(rect.x),
            format_float(rect.y),
            format_float(rect.w),
            format_float(rect.h)
        ));
        rect
    }
}

fn dump_box(boxes: &Boxes, id: BoxId, depth: usize, out: &mut Vec<String>) {
    let node = boxes.get(id);
    out.push(format!(
        "box {}{} {} {} {} {} {}",
        " ".repeat(depth * 2),
        node.class_name(),
        format_float(node.width),
        format_float(node.height),
        format_float(node.depth),
        format_float(node.shift),
        node.atom_type as i32
    ));
    for child in node.children() {
        dump_box(boxes, child, depth + 1, out);
    }
}

struct Output {
    status: String,
    metrics: String,
    boxes: Vec<String>,
    measures: Vec<String>,
    ops: Vec<String>,
}

fn run(record: &Record, with_boxes: bool) -> Output {
    let mut recorder = Recorder { lines: Vec::new() };
    let mut out = Output {
        status: String::new(),
        metrics: String::new(),
        boxes: Vec::new(),
        measures: Vec::new(),
        ops: Vec::new(),
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        tmath::layout(
            &record.tex,
            record.width,
            record.size,
            record.line,
            record.fg,
            &mut recorder,
        )
    }));
    match result {
        Err(_) => out.status = "panic".to_owned(),
        Ok(Err(e)) => {
            out.status = if e.kind == tmath::error::ErrorKind::Unsupported {
                format!("unsupported {}", e.message)
            } else {
                format!("error {}", e.message)
            };
        }
        Ok(Ok(laid)) => {
            let i = laid.render.insets;
            out.metrics = format!(
                "metrics {} {} {} {} {} {} {} {}",
                laid.width(),
                laid.height(),
                laid.depth(),
                format_float(laid.baseline()),
                i.left,
                i.top,
                i.right,
                i.bottom
            );
            if with_boxes {
                dump_box(&laid.boxes, laid.render.root, 0, &mut out.boxes);
            }
            out.ops = laid.draw().iter().map(format_op).collect();
            out.status = "ok".to_owned();
        }
    }
    out.measures = recorder.lines;
    out
}

fn fnv1a(lines: &[String], mut hash: u64) -> u64 {
    for line in lines {
        for &b in line.as_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= u64::from(b'\n');
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// The best of `repeat` runs of parse and layout, dropping the result
/// included, as the oracle's `--bench` times `LaTeX::parse` and the delete.
fn time(record: &Record, repeat: u32) -> (bool, std::time::Duration) {
    let mut best = std::time::Duration::MAX;
    let mut ok = true;
    for _ in 0..repeat {
        let mut quiet = Quiet;
        let start = std::time::Instant::now();
        let result = tmath::layout(
            &record.tex,
            record.width,
            record.size,
            record.line,
            record.fg,
            &mut quiet,
        );
        ok &= result.is_ok();
        drop(result);
        best = best.min(start.elapsed());
    }
    (ok, best)
}

fn main() {
    // `--stack <KiB>` runs everything on a thread with that much stack, to
    // check what the deepest formulas need.
    let mut args = std::env::args().skip(1).peekable();
    let mut stack = None;
    let mut rest = Vec::new();
    while let Some(arg) = args.next() {
        if arg == "--stack" {
            stack = args.next().and_then(|n| n.parse::<usize>().ok());
        } else {
            rest.push(arg);
        }
    }
    match stack {
        Some(kib) => {
            let worker = std::thread::Builder::new()
                .stack_size(kib * 1024)
                .spawn(move || run_all(rest))
                .expect("thread");
            let _ = worker.join();
        }
        None => run_all(rest),
    }
}

fn run_all(arguments: Vec<String>) {
    let mut digest = false;
    let mut with_boxes = true;
    let mut bench = 0;
    let mut args = arguments.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--digest" => digest = true,
            "--no-boxes" => with_boxes = false,
            "--bench" => {
                bench = args
                    .next()
                    .and_then(|n| n.parse::<u32>().ok())
                    .unwrap_or(1)
                    .max(1);
            }
            _ => {
                eprintln!("Unknown argument: {arg}");
                std::process::exit(2);
            }
        }
    }
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    for (number, line) in stdin.lock().lines().enumerate() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let Some(mut record) = parse_record(&line) else {
            eprintln!("Bad record at line {}", number + 1);
            std::process::exit(2);
        };
        if record.id.is_empty() {
            record.id = (number + 1).to_string();
        }
        if bench > 0 {
            let (ok, best) = time(&record, bench);
            let status = if ok { "ok" } else { "error" };
            let _ = writeln!(stdout, "{}	{status}	{}", record.id, best.as_nanos());
            continue;
        }
        let out = run(&record, with_boxes);
        let mut text = format!("@ {}\nstatus {}\n", record.id, out.status);
        if !out.metrics.is_empty() {
            text.push_str(&out.metrics);
            text.push('\n');
        }
        if digest {
            text.push_str(&format!("digest {}\n", digest_of(&out)));
        } else {
            for l in out.boxes.iter().chain(&out.measures).chain(&out.ops) {
                text.push_str(l);
                text.push('\n');
            }
        }
        text.push_str("end\n");
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }
}

/// The digest line `--digest --no-boxes` prints, without its prefix.
fn digest_of(out: &Output) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    hash = fnv1a(&out.boxes, hash);
    hash = fnv1a(&out.measures, hash);
    hash = fnv1a(&out.ops, hash);
    format!(
        "{hash:016x} ops={} boxes={} measures={}",
        out.ops.len(),
        out.boxes.len(),
        out.measures.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every formula must parse, lay out and draw exactly as the oracle did
    /// when the golden was recorded, or fail where the oracle failed.
    fn check(corpus: &str, golden: &str) {
        let expected: std::collections::HashMap<&str, Vec<&str>> = golden
            .lines()
            .map(|l| {
                let fields: Vec<&str> = l.split('\t').collect();
                (fields[0], fields)
            })
            .collect();
        let mut matched = 0;
        let mut differ = Vec::new();
        for line in corpus.lines().filter(|l| !l.trim().is_empty()) {
            let record = parse_record(line).expect("record");
            let out = run(&record, false);
            let g = &expected[record.id.as_str()];
            let kind = out.status.split(' ').next().unwrap_or("");
            let metrics = out.metrics.strip_prefix("metrics ").unwrap_or("");
            let same = if g[1] == "ok" {
                kind == "ok" && g[3] == metrics && g[4] == digest_of(&out)
            } else {
                kind == g[1] || g[1].starts_with("oracle-")
            };
            if same {
                matched += 1;
            } else {
                differ.push(record.id.clone());
            }
        }
        eprintln!("{matched} match, {} differ", differ.len());
        assert!(
            differ.is_empty(),
            "differ: {:?}",
            &differ[..differ.len().min(20)]
        );
    }

    #[test]
    fn features_match_the_golden() {
        check(
            include_str!("../../corpus/features.jsonl"),
            include_str!("../../corpus/golden/features.tsv"),
        );
    }

    #[test]
    fn samples_match_the_golden() {
        check(
            include_str!("../../corpus/samples.jsonl"),
            include_str!("../../corpus/golden/samples.tsv"),
        );
    }

    #[test]
    fn adversarial_match_the_golden() {
        check(
            include_str!("../../corpus/adversarial.jsonl"),
            include_str!("../../corpus/golden/adversarial.tsv"),
        );
    }

    /// tdesktop lays formulas out on its GUI thread, which on Windows has a
    /// 1 MiB stack. The deepest formulas the limits allow must fit in half.
    #[cfg(not(debug_assertions))]
    #[test]
    fn deepest_formulas_fit_half_the_stack() {
        std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(|| {
                check(
                    include_str!("../../corpus/regressions.jsonl"),
                    include_str!("../../corpus/golden/regressions.tsv"),
                );
                check(
                    include_str!("../../corpus/adversarial.jsonl"),
                    include_str!("../../corpus/golden/adversarial.tsv"),
                );
            })
            .expect("thread")
            .join()
            .expect("no panic");
    }

    #[test]
    fn regressions_match_the_golden() {
        check(
            include_str!("../../corpus/regressions.jsonl"),
            include_str!("../../corpus/golden/regressions.tsv"),
        );
    }
}
