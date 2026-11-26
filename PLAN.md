# tmath: MicroTeX in safe Rust

## Goal

Replace everything MicroTeX does between a TeX string and a list of drawing
calls with a Rust engine that has no `unsafe`, and keep only Qt painting in
C++. For every input the engine must produce exactly what our C++ fork
(desktop-app/MicroTeX at `6d0b5c8`, the oracle) produces: the same parse
outcome, the same metrics and the same drawing calls, so formulas look
identical. The reason is the long trail of crash fixes in that fork
(use-after-free, out-of-bounds, stack overflows, runaway expansion, UB casts).
The goal is that none of those classes can happen again, rather than fixing
them one at a time.

## Boundary

| Stays in C++ (painting) | Moves to Rust (everything else) |
|---|---|
| Loading the bundled TTFs into Qt (`res/bundled.qrc`) | Tokenizer, parser, macro expansion, `\newcommand`, environments |
| Replaying the display list onto `QPainter` (today `Graphics2D_qt`: glyphs as `QPainterPath`, lines, rects, transforms) | Every built-in table: TeX font metrics, symbols, Unicode mappings, predefined formulas, params, units, colors |
| Answering "measure this text run" with `QFontMetricsF` (today `TextLayout_qt::getBounds`) | Atoms, `createBox` layout, glue, line breaking |
| Checking the display list it is handed (finite numbers, bounded counts) | Box metrics and `TeXRender` size, insets and baseline |
| | The `Box::draw` tree walk that today emits `Graphics2D` calls, now emitting a flat display list |
| | All limits and budgets (depth, expansion, box count, chain lengths) |

The C++ side is about 250 lines, and none of it reads formula structure. The
display list mirrors the `Graphics2D` interface call for call (`setColor`,
`setStroke`, `setStrokeWidth`, `setFont`, `translate`, `scale`, `rotate`,
`reset`, `drawChar`, `drawText`, `drawLine`, `drawRect`, `fillRect`,
`drawRoundRect`, `fillRoundRect`, plus `layout` for a measured text run), so
replaying it through the existing Qt code paints the same pixels by
construction.

## Where the past fixes land

106 of the 111 fixes in the fork land on the Rust side (`FIXES.md`). The 5
that stay in C++ are Qt integration fixes, not memory-safety ones: glyphs drawn
as paths to avoid FreeType raster overflow, font family shadowing, pixel size
instead of point size, and capping the font-family diagnostic set. Of the 106:

- 33 cannot happen by construction in safe Rust.
- 24 are memory-safe by construction, but need the fork's exact fallback value
  ported to keep the output identical.
- 49 need explicit design: the same depth limits, budgets, size caps and
  finite checks. These include all 9 STACK and all 16 CPU/MEM fixes, the class
  where Rust aborts instead of corrupting memory.

## What safe Rust does not give for free

Safe Rust removes use-after-free, out-of-bounds access, null dereferences,
uninitialized reads, data races, and UB from integer overflow or float-to-int
casts. Four classes from the fork's history remain and need explicit design:

- **Stack overflow.** Rust aborts the process on stack overflow, which is a
  crash. The fork's depth limits (parse depth, scripts-chain depth, `\over`
  chains, `\left` nesting) are ported as they are; they are part of the output
  anyway. A test runs the worst case at every limit on a thread with a
  deliberately small stack. Atoms and boxes live in arenas (`Vec` plus
  indices), so dropping a formula never recurses.
- **Runaway CPU or memory.** The per-formula budgets (expansion cap of 64K plus
  16 per source character, the box budget, chain limits) are ported at the same
  counting points, because where a formula is refused is part of the output.
  OOM aborts in Rust, so budgets are the only defense.
- **Panics.** Index and arithmetic panics are bugs. Every engine entry runs
  under `catch_unwind` (`tdesktop_rust` builds with `panic = 'unwind'`), so a
  panic becomes a failed formula, never a crash. Fuzzing must show zero panics.
- **Global state.** The C++ code resets process-global macros, colors, column
  types, fonts and math sizes before each parse. Rust keeps all of that in a
  per-formula context over immutable static tables, so nothing leaks between
  formulas and parsing is thread-safe.

## Identity

"Identical" means that for each corpus input:

1. the outcome matches: a render, or an error in the same place (exact message
   text is not required);
2. `getWidth`, `getHeight`, `getDepth`, `getBaseline` and the insets are equal;
3. the text runs measured during layout are equal (font, style, size, code
   units);
4. the display list equals the oracle's `Graphics2D` call stream: same
   operations, characters, fonts and colors, and floats bit-equal on x64;
5. pixels are byte-equal on a Qt-rendered sample (a consequence of 4, checked
   anyway).

Known hazards that the port has to mirror deliberately:

- `float` arithmetic in the same order with the same promotions (the
  `* 0.5` versus `* 0.5f` kind). MSVC and GCC on x64 do not contract
  `a * b + c`; clang on macOS arm64 does, so the current C++ there already
  differs from x64 by an ulp. Rust never contracts and will match x64 on every
  platform.
- Characters are UTF-16 code units, not code points: `ToWide()` in tdesktop
  passes one `wchar_t` per unit on every platform, and lone surrogates reach
  the library.
- Qt string construction stops at the first NUL (`wstring_to_QString`), and
  that affects measurement.
- The fork's saturation rules: `rasterBound` maps NaN to `INT_MIN`, while Rust
  `as` maps NaN to `0`. Every such cast is written out explicitly.
- About 15 exception catch sites, including `catch (...)`, decide where partial
  mode swallows an error. Rust `Result` handling must catch the same error
  kinds at the same points.
- `std::map` iteration order is sorted; use `BTreeMap` wherever iteration order
  can reach the output.
- Upstream quirks in the draw walk, such as `RuleBox::draw` keeping
  `oldStroke` as a reference so its "restore" re-sets the new stroke, and
  `Insets(t, l, b, r)` assigning `left = t`. See the classification below.
- Shared and mutated objects: atoms and boxes are `shared_ptr`s that get
  appended twice, copied shallowly by `privateCopy` and changed during layout
  (`ScriptsAtom` keeps the phantom base it makes, `BigOperatorAtom` keeps the
  base it unwraps on some paths). The engine keeps atoms and boxes in
  per-formula arenas addressed by index, which reproduces all of that and
  never recurses on drop.
- Process-wide state the C++ does not reset between formulas, and where
  results can depend on earlier formulas: the predefined-formula and symbol
  caches, and `operator[]` lookups that insert, such as U+03A2 in math mode
  adding an empty entry to `_symbolFormulaMappings`. The engine scopes all of
  it to one formula, which is what a fresh process does.

## Engine interface

```text
layout(tex: &[u16], width: i32, text_size: f32, line_space: f32, fg: u32,
       measure: &mut dyn Measure) -> Result<Formula, Failure>
Formula = { metrics, display_list }
trait Measure { fn bounds(font, style, pixel_size, text: &[u16]) -> Rect }
```

The engine crate knows nothing about C. The FFI crate implements `Measure` by
calling a C function pointer that answers with `QFontMetricsF`, as
`TextLayout_qt::getBounds` does today. That call is one of its few `unsafe`
sites, and the engine stays `#![forbid(unsafe_code)]`. Tests implement
`Measure` with the oracle's deterministic `MeasureText()`.

One call returns metrics and the display list together. Today tdesktop parses
each formula twice (`MeasureWithMicrotex`, then `RenderWithMicrotex`). Keeping
the display list from the measuring call removes the second parse.

## Hosting: in-process

The engine runs in-process. In Telegram Desktop it is one more crate in
`tdesktop_rust`, the single archive that also holds tlottie and wallet-engine,
so the process still has one Rust runtime.

Formulas are measured synchronously on the main thread while messages are laid
out (`MeasurePreparedFormulas` in `iv_markdown_prepare_formulas.cpp`), so that
nothing moves once layout is done. A separate process would not help: the main
thread would still wait for it, and it would add process start-up and an IPC
round trip. What keeps the main thread free is the worst-case time per formula,
which the budgets bound. See the worst-case gate below.

## Tests

### The oracle

`oracle/` builds the current C++ against a recording backend instead of Qt
(`oracle/recording.cpp`). Every `Graphics2D` call, every text measurement and
the box tree is printed as canonical text, with floats in `%.9g` (round-trips
an `f32`) and characters as UTF-16 hex units. `MeasureText()` is a
deterministic stand-in for `QFontMetricsF` that Rust implements bit for bit, so
logic is compared without Qt. Real Qt measurement is covered by the pixel
tests. It prints `@ <id>` and flushes before each formula, so a driver can
isolate inputs that crash or hang the oracle (the C++ still has such inputs;
for those, Rust only has to fail cleanly). `--digest` prints one hash per
formula for compact goldens, and `--bench N` times parse plus layout.

### Corpora (`corpus/*.jsonl`)

- **C1, generated feature matrix:** enumerated from the tables themselves.
  Covers every primitive command (about 296) with each argument shape (empty,
  simple, nested, optional present or absent); every symbol (about 650) in
  each style (D, T, S, SS), bare and with scripts; every Unicode mapping (826);
  every predefined formula (130); every environment by rows, columns, rules and
  column specs; every delimiter at every size and with `\left` / `\right`
  around tall content; every unit inside `\kern`, `\hspace`, `\rule` and
  `\raisebox`; every color model; every font command over Latin, Cyrillic,
  Greek, CJK, emoji and lone surrogates; and widths that force line breaking.
  Roughly 15-20k records.
- **C2, real world:** `res/SAMPLES.tex`, the formula editor samples, formulas
  from tdesktop tests, and a public formula corpus (im2latex-100k or the
  KaTeX/MathJax test suites, kept local if the license requires it).
- **C3, regressions** (`tools/corpus_regressions.py`): one record per fork fix, in the form that crashed, the
  form at each limit and the form one step past it. Inputs that crash a
  released build stay out of this repository, in the ignored
  `corpus/private/`, until a release with the fix ships. Only the limit forms
  are committed before then.
- **C4, fuzz:** a grammar-aware generator that nests commands from the tables,
  plus coverage-guided `cargo fuzz` on the engine (Linux or WSL).

### Checks

- **Tables:** an oracle `--dump-tables` mode against the Rust tables,
  exhaustively (font metrics for every character of every font, symbols,
  mappings, units, colors).
- **Font queries:** every character by style through `getChar`, next-larger
  chains, extensions, kerns and ligatures.
- **End to end:** `compare/` (`tmath_compare`) reads a corpus and prints what
  the oracle prints, so `tools/oracle_run.py --oracle tmath_compare` writes a
  table in the golden's format and `tools/compare_golden.py` diffs the two.
  Formulas that need a part not ported yet report `unsupported` and are
  counted by what they need. `cargo test` replays the features corpus against
  its golden and fails on any formula that renders differently.
- **Safety:** zero panics over all corpora and fuzzing; worst case at every
  limit on a small-stack thread; time and peak-memory bounds per formula, with
  a counting allocator in the test binary only.
- **Pixels:** a Qt test renders a sample through the old `Graphics2D_qt` path
  and through display-list replay and compares the images byte for byte.
- **Performance gate:** Rust at or below C++ at p50 and p90 per formula on C1
  and C2 in Release. Baseline (C++, x64 Release, best of 20 runs, 2026-11-26):
  editor samples take 29-48 µs; the larger `SAMPLES.tex` blocks have a median
  of 151 µs, a p90 of 559 µs and a maximum of 983 µs. The engine package needs
  `opt-level = 3`, because `tdesktop_rust` builds with `'z'`, as tlottie
  already does.
- **Worst-case gate:** this one decides whether chat open can freeze. Every
  adversarial formula in `tools/corpus_adversarial.py` must run no slower in
  Rust than in C++. These are 32K-character inputs (tdesktop's
  `kMaxFormulaTexLength`) at or just under each limit. C++ today (same setup,
  2026-11-26):
  - accepted: 4.7-17.0 ms (big operators 17.0, matrices 15.3, `\left`
    nesting 14.6, `\binom` 14.2);
  - refused: 0.5-12.5 ms.

  A budget that lets a formula run much longer than this is a freeze bug,
  even if the formula is otherwise handled correctly.

## Milestones

Each milestone lands on its own, with its exit check green.

| | Work | Exit check |
|---|---|---|
| M0 | Oracle CLI, recording backend, samples corpus, baseline timing | **done**: 32/32 samples recorded; baseline above |
| M1 | Runner with crash and hang isolation per record (`tools/oracle_run.py`), golden digests, C1 generator (`tools/corpus_features.py`). C3 records are added by the milestone that ports each fix's area. `compare/` comes with M2, when there is engine output to compare | **done** for C1: 9662 records, goldens in `corpus/golden/features.tsv` (9612 ok, 50 errors, no oracle crashes or hangs) |
| M2 | Cargo workspace, engine crate with `#![forbid(unsafe_code)]`. Tables are generated (`tools/gen_tables.py`) from the oracle's `--dump-tables`, which reads MicroTeX's tables after init through `friend struct OracleAccess` on the fork's `tmath-oracle` branch. Canonical text forms (`%.9g`, UTF-16 units) live in `engine/src/text.rs` | **done**: all 7432 table lines re-dump equal to `corpus/golden/tables.txt` |
| M3 | TeX font engine, environment (styles, sizes), glue, units, length parsing. The oracle answers `--font-queries` and `--env-queries`; the engine replays the same questions | **done**: all 66503 font queries and all 23437 environment, unit, glue and length answers equal, including MSVC's stream float parsing and the lone-surrogate handling of `wide2utf8` |
| M4 | Box types, draw walk to display list, `TeXRender` metrics; parser core with basic atoms (chars, symbols, groups, scripts, fractions, spaces, styles) | **done**: every box type, the draw walk, `TeXRender`, line breaking, the parser with preprocessing and user commands, and the basic atoms. With the parts of M5 and M6 below, 8116 of 9662 C1 records matched exactly at the end of M6; the rest report unsupported; none differ |
| M5 | Delimiters, radicals, accents, big operators and limits, over/under, `\stackrel`, `\xrightarrow` | **done**: the delimiter factory, `\left`/`\middle`/`\right`, the `\big` family, `\sqrt`, accents, big operators and limits, `\sideset`, `\stackrel`, `\underaccent`, `\genfrac`, `\sfrac`, `\cfrac`, the `withdelims` fractions, `\xleftarrow`/`\xrightarrow`, the over/under arrows and braces |
| M6 | Text and fonts: `\text`, `\mbox`, math alphabets, colors, frames, rotate/scale/raise, rules, kerns, units, `\externalfont`, text-run measurement | **done**: text mode, math alphabets and font switches, size commands, colors and `\definecolor`, every frame (`\fbox`, `\colorbox`, `\boxed`, double, shadow and oval boxes), `\rotatebox`, `\scalebox`, `\reflectbox`, `\resizebox`, `\raisebox`, rules, kerns, phantoms, `\smash`, laps, cancel lines, cedilla and ogonek, `\externalFont`, text-run measurement |
| M7 | Arrays and environments: array and the matrix family, cases, aligned, gathered, alignedat, multicolumn and multirow, rules, column specs, `@{}`, `\newcolumntype`, smallmatrix, `\hdotsfor` | **done**: `ArrayFormula`, `MatrixAtom`, `MultlineAtom`, `\\` and `\cr` inside and outside arrays, column specs with `@{}`, `*{}{}`, `>{}` and `\newcolumntype`, rules and `\arrayrulecolor`, cell, row and column colors, `\multicolumn`, `\multirow`, `\hdotsfor`, `\intertext`, `\shoveleft`/`\shoveright`. 150 array records added to C1 for the edge cases; with them 8716 of 9812 C1 records match, the rest report unsupported, none differ |
| M8 | Macro machinery and the rest: `\newcommand`, `\renewcommand`, `\newenvironment`, predefined formulas, partial-mode catch points, line breaking, `\DeclareMathSizes`, `\magnification`, remaining primitives | **done**: every built-in command. `\debug` and `\undebug` exist only in `GRAPHICS_DEBUG` builds, so like the oracle the engine treats them as unknown. The limits of the fork's last fixes are in: wrapper chains of 16, scripts layout depth 96, four `\over` per group, the shared expansion cap, `@{}` work, and refusing a formula that hit any limit. Predefined formulas parse as the C++ start-up parses them, checked against the oracle's `--predefined-cache`. C3 is `tools/corpus_regressions.py`: 428 records that sweep each limit across its threshold and reach each fallback of `FIXES.md`. All 9812 C1, 32 sample, 12 adversarial and 428 C3 records match exactly, and `cargo test` replays all four |
| M9 | Robustness campaign: fuzzing, differential fuzzing against the oracle in a subprocess, small-stack and budget tests | zero panics, bounded time and memory over the agreed CPU-hours |
| M10 | Performance pass | gate above |
| M11 | FFI crate (C ABI, `catch_unwind` at every entry, measure callback); C++ replay painter with list validation; display list kept from measuring to rendering; pixel identity test | images byte-equal; tdesktop tests pass |
| M12 | Switch tdesktop over; drop the C++ layout sources from its build (keep fonts); tmath in the `tdesktop_rust` build recipes | Debug and Release builds; a chat and an article with formulas |

M4-M8 are the bulk: about 12k lines of C++ logic (77 atom classes, 19 box
classes) becoming a similar amount of Rust.

## Repository layout

```text
tmath/
  PLAN.md, FIXES.md  this plan, the fork's fixes
  NOTICE             MicroTeX's MIT notice (the port carries its data)
  oracle/            the C++ fork with a recording backend (test-only)
  oracle/MicroTeX/   the fork's sources, a git submodule (test-only)
  tools/             corpus generators, table converter
  corpus/            *.jsonl inputs and golden digests
  out/               build output (ignored)
  Cargo.toml         workspace (M2)
  engine/            the engine crate, #![forbid(unsafe_code)] (M2)
  capi/              the C ABI crate, the only one with unsafe (M11)
  compare/           engine-against-oracle diff tool (M1)
  fuzz/              cargo-fuzz targets (M9)
```

The build recipes clone this repository without submodules, so no build ever
sees the C++. After the switch, tdesktop drops its MicroTeX submodule. The
TTFs move into tdesktop's resources together with their license files
(Knuth, OFL, dsrom, GPLv3 for the Cyrillic set), and the Qt replay moves into
`iv/markdown`.

Build the oracle (x64; the reason is in `oracle/CMakeLists.txt`).
`MICROTEX_ROOT` defaults to the submodule:

```text
cmake -S oracle -B out/oracle -A x64 [-DMICROTEX_ROOT=<MicroTeX checkout>]
cmake --build out/oracle --config Release
python -I tools/corpus_samples.py <MicroTeX checkout> corpus/samples.jsonl
out/oracle/Release/microtex_oracle < corpus/samples.jsonl
```

## Fix classification

`FIXES.md` has all 111 fixes from the fork's 34 commits, each with its bug
class, layer and Rust verdict. It also lists the bug-compatibility hazards and
upstream draw-walk quirks the port has to reproduce. Every C3 regression record
and every threshold test comes from it.
