# Fork fixes and where they land

Every distinct fix in the fork's own commits (`0e3707f..6d0b5c8`, 34 commits,
111 fixes), classified by bug class, by the layer it lives in, and by what a
safe Rust port does about it. Each commit was read with `git show`. Pure
build and packaging commits are not rows: fe12988 (tinyxml2 and the XML
loaders removed), e956481 (bundled qrc and `initBundled`), 5de3952 (Xcode
build), and 1815d58 with e02ffd7 (hinting disabled and reverted, net zero).

Classes: OOB, NULL, UNINIT, UAF, STACK (unbounded recursion), CPU/MEM
(runaway work or memory), INTOVF, FCAST (float-to-int UB or NaN/inf
propagation), DIV0, EXC (uncaught exception, terminate), GLOBAL (state leaking
between formulas or into caches), OUTPUT (wrong output), OTHER.

Layers: PARSE, ATOM (atom layout, box construction), BOXM (box metrics,
`TeXRender` sizes), DRAW (the `Box::draw` walk), FONT (font tables and
lookups), INIT (init, reset, caches), PLAT (`platform/qt/graphic_qt.*`).

Rust column:

- **Y**: impossible by construction. Anything left, such as an index panic,
  becomes a failed formula through `catch_unwind` at the entry.
- **Y\***: memory-safe by construction, but the fork renders a specific
  fallback (a value, a clamp or a skip). A naive port would refuse the formula
  instead, so the fallback must be ported to keep the output identical.
- **D x**: needs explicit design x: depth limit, work budget, size cap, finite
  check, checked arithmetic, or per-formula context.
- **C++**: stays on the painting side.
- **H**: bug-compatibility hazard (see below).

## Summary

| Layer | Fixes | Y | Y\* | D | C++ |
|-|-|-|-|-|-|
| PARSE | 56 | 18 | 11 | 27 | - |
| ATOM | 27 | 10 | 9 | 8 | - |
| INIT | 10 | 1 | - | 9 | - |
| FONT | 7 | 2 | 3 | 2 | - |
| BOXM | 4 | 1 | 1 | 2 | - |
| DRAW | 2 | 1 | - | 1 | - |
| PLAT | 5 | - | - | - | 5 |
| **Total** | **111** | **33** | **24** | **49** | **5** |

By primary class: OOB 20, GLOBAL 17, CPU/MEM 16, OUTPUT 13, STACK 9, NULL 9,
FCAST 8, EXC 4, INTOVF 4, OTHER 3, UNINIT 3, DIV0 3, UAF 2.

106 of the 111 fixes land on the Rust side. The 5 that stay in C++ are Qt
integration fixes, not memory-safety ones:

- #2 and #110 set up and load Qt fonts.
- #111 draws glyphs as paths.
- #98 caps a diagnostics set.
- #4 fixes `Font_qt::getSize`. It decides the pixel size a measured text run
  uses, so the measurement request must carry family, style and the integer
  pixel size (`qMax(1, qRound(size))`), and Rust must pass the same size.

Fixes that span the boundary:

- #84 `\externalfont`: the family name becomes per-formula state in Rust,
  while creating the `QFont` and measuring stay in C++.
- #94 and the text-run op: `TextRenderingBox::draw` calls
  `TextLayout_qt::draw`, so the display list carries a text-run operation.
- #90 `reset()`: the replayer applies it as `setTransform(QTransform())`.

What safe Rust does not remove: every STACK and CPU/MEM row still needs its
limit, because stack overflow and OOM abort a Rust process and cannot be
caught. Dropping a deep `Box` tree recurses in Rust too, which is the
destructor overflow bf053f2 fixed, so atoms and boxes live in arenas.
`i64::MIN / -1` panics even in release builds, so #45 needs checked
arithmetic.

## Bug-compatibility hazards

### Output semantics the fork changed on purpose

- #1: `\mathop` limits.
- #3: sizes are rounded up (ceil) plus 1 px, and the baseline is taken from
  the integer sizes.
- #99: insets went from truncation to ceil, so `0.18 * textSize` of 3.6 now
  gives 4 instead of 3.
- #6: the `\matrix`, `\pmatrix` and `\cases` aliases.
- #9: alignat accepts fewer columns.
- #11: `\\` inside `{}` within an array stacks a VRow line instead of
  starting an array row.
- #26: null padding puts the `@{}` separator in its intended column.
- #54: a negative multirow anchors on the topmost spanned row that is not a
  rule.
- #83: the delimiter ladder uses a shared counter plus the `walkedSmaller`
  route.
- #51: smallmatrix no longer switches the outer environment to script style.
- #87: a symbol miss inside `\text` falls through to the formula mapping and
  character path.
- #12: a repeat count above 1000 throws.
- #63: character classification is ASCII-only, while Rust `is_alphabetic` is
  Unicode. Trimming uses C `isspace`, which includes `\v`, while
  `is_ascii_whitespace` does not.

### Fallback values a naive port gets wrong

- #49: `rasterBound` maps NaN to `INT_MIN` (Rust `as` gives 0); ±inf and
  overflow saturate, and sums saturate.
- #89: the baseline is computed in double.
- #50: an unknown unit gives factor 0.
- #33: missing metrics are zeros.
- #72: a next-larger miss keeps the same character.
- #28: when there is no extension recipe, the tallest character is used.
- #40 and #64 (glue): style clamps to the last entry, `none` maps to
  ordinary, and a bad index gives zero glue.
- #37: an infinite length becomes 0.
- #66: an infinite angle becomes 0.
- #67: the hdots coefficient becomes 1.
- #80: the hdots count is 0 for NaN or values ≤ 0, and `INT_MAX` for values
  ≥ `INT_MAX` (Rust `floor() as i32` would go negative).
- #48: color components clamp to 0..255.
- #82: non-finite rgb values take the float path.
- #59: popping an empty VRow returns a zero `SpaceAtom`.
- #31, #7 and #8: `EmptyAtom` roots.
- #94: drawing is skipped when the size is ≤ 0 or NaN.
- #34 and #97: invalid `\DeclareMathSizes` and `\magnification` values are
  ignored.
- #96: a failed alphabet block falls back to the default font.
- #100: a value outside `int` range is unparseable.

### Accept and refuse thresholds (must match exactly)

- Parse depth is 48. It is counted only in `parse()`, `getCommandWithArgs`
  and the escape branch of `getArgument`.
- A chain has at most 16 links, and scripts layout depth is at most 96.
- At most 4 commands of the `\over` family per group, reset per cell.
- The box budget is 100000. It counts every Box construction (including stack
  temporaries such as `CharBox A` in `DelimiterFactory`) and every
  `Box::add`.
- Expansion work is capped at 32M:
  - each expansion charges the current `_len`;
  - each `@{}` charges rows × (cols + 1).
- Expanded text:
  - 64K per parser;
  - a shared total of 64K + 16 × source length, which counts macro
    replacement text but not environment bodies;
  - 10000 expansions per parser.
- Column specs: 100000 steps (probes included), length 10000, type names of
  at most 16.
- Other caps:
  - matrix cells: 1e5, at parse and at layout;
  - spans: 1000;
  - argc: 0..9;
  - genfrac style: 0..3;
  - roman numerals: at most 1e5;
  - longdiv: at most 1e9.

### Refuse or truncate (#104)

- These `failLimit` paths refuse the whole formula: depth, expansion work,
  expansion length, expansion total, `@{}` work, the `\over` count and wrap
  depth.
- Errors thrown during layout (the box budget, `ScriptsLayoutGuard`) also
  refuse the formula.
- Every other `ex_parse` is swallowed in partial mode and truncates the
  output there. This includes the expansion count, column-spec and matrix
  caps, spans and roman numerals.

### Other parser-level hazards

- Lengths are counted in `wchar_t` units. tdesktop's `ToWide()` passes UTF-16
  code units on every platform, so the port counts UTF-16 units.
- `valueof` (`stringstream >>`) is not Rust `str::parse`:
  - it parses a prefix and skips whitespace;
  - empty input leaves the target untouched, and garbage stores 0;
  - integer overflow saturates;
  - float overflow gives inf on MSVC;
  - it never accepts `nan` or `inf`.
- Formula-wide state (`\cornersize`, `LINE_COLOR`, `\breakEverywhere`, math
  sizes, `\externalfont`) is read at layout time. The last value in a formula
  therefore affects earlier constructs. It resets only between formulas.
- #103: predefined formulas are built at init in a clean context. Building
  them lazily inside a per-formula macro context brings the cache poisoning
  back.

## Upstream draw-walk quirks to reproduce

1. **Stroke alias.** RuleBox, FramedBox, OvalBox, ShadowBox and DebugBox hold
   `const Stroke &` from `getStroke()`, so the "restore" re-applies the new
   stroke and the last stroke carries forward. LineBox (the `\cancel` family)
   only calls `setStrokeWidth`, so its cap and join are whatever carried
   forward:
   - at first `Stroke()`: 1, ROUND, ROUND, miter 0;
   - after a RuleBox: BUTT, BEVEL;
   - after a frame: BUTT, MITER.
2. **Undo by inverse operations, not save and restore.** Float drift
   accumulates, so these relative operations are replayed in the same order:
   - CharBox: `scale(s)` then `scale(1/s)`, both skipped when s == 1.
   - TextRenderingBox: `scale(0.1s)` then `scale(10/s)`.
   - ScaleBox: `scale(1/sx, 1/sy)`, with `dec = _width` when sx < 0.
   - ReflectBox: `scale(-1, 1)` twice, with the base at (-w, 0).
   - RotateBox: `rotate(-a, x, y)` then `rotate(a, x, y)`.
   - LineBox: `translate(0, -h)` then `translate(0, h)`.
3. **Running scale product.** `sx()` and `sy()` multiply every `scale()`
   argument and reset only on `reset()`; `rotate` does not touch them.
   ShadowBox and DebugBox set the pen width to `|1/sx()|`, so the f32 product
   must be identical.
4. **The `TeXRender::draw` sequence:**
   1. read `getColor()`;
   2. `scale(ts)`;
   3. `setColor(fg or black)`;
   4. draw the box at `(x + insets.left) / ts`, `(y + insets.top) / ts + h`,
      with the ints added before the f32 divide;
   5. `reset()`, then `setColor(old)`.

   Stroke and font are not restored.
5. **Colors.** ColorBox, WrapperBox, RuleBox and FramedBox read `getColor()`
   and always emit `setColor(prev)`. RuleBox does so even when it never
   changed the color.
6. **Fonts.** CharBox calls `setFont` only when the font pointer differs,
   draws at (0, 0) after a translate, and never draws the italic correction.
   The initial state is SansSerif 20, BLACK, `Stroke()`.
7. **Shifts.**
   - HBox draws each child at `y + child._shift`.
   - VBox draws at `x + shift - _leftMostPos`.
   - WrapperBox draws its base at `(x + _l, y + base._shift)`.
   - ScaleBox draws its base at (0, 0), with the shift folded in at init.
   - Framed, Oval and Shadow boxes draw their base at
     `x + space + thickness`.
8. **Rules and frames.**
   - RuleBox is a stroked `drawLine` (width = height, BUTT and BEVEL) at
     `y - h/2 - speShift`, not a fill.
   - ShadowBox sets a stroke before `fillRect`, which has no effect.
   - A FramedBox with a transparent line strokes in the current color.
   - ScaleBox returns early only for an exact 0, because NaN and inf were
     mapped to 1 at init.
9. **Platform side.** `drawRect` and `drawRoundRect` fill with whatever brush
   the caller's QPainter has, because `Graphics2D_qt` never sets one, except
   that `fillRoundRect` leaves NoBrush behind. Text is filled as a path with
   antialiasing and no pen.

## All fixes

|#|commit|file :: function|class|layer|Rust|
|-|-|-|-|-|-|
|1|9895a82|macro_impl.h :: macro(mathop) -> LimitsType::normal|OUTPUT|PARSE|Y, H|
|2|9895a82|graphic_qt.cpp :: Font_qt ctors setPixelSize|OUTPUT|PLAT|C++|
|3|0258b24|render.cpp :: TeXRender::getHeight/getDepth/getWidth/getBaseline (ceil, +1 px)|OUTPUT|BOXM|Y, H|
|4|f62f9dc|graphic_qt.cpp :: Font_qt::getSize returns pixelSize|OUTPUT|PLAT|C++|
|5|89228ac|parser.cpp :: TeXParser::getArgument restore _formula on throw|UAF|PARSE|Y|
|6|f622841|macro_def.cpp :: NewCommandMacro::_init_ (\matrix,\pmatrix,\cases cmd forms)|OUTPUT|PARSE|Y, H|
|7|43904b5|atom_impl.h :: StyleAtom ctor null -> EmptyAtom|NULL|PARSE|Y*|
|8|43904b5|formula.cpp :: Formula(tp,latex,pre) partial catch -> EmptyAtom root|NULL|PARSE|Y*|
|9|43904b5|macro_impl.h :: alignat/alignedat: cols > 2n (was !=)|OUTPUT|PARSE|Y, H|
|10|cb5e9e1|macro_impl.h :: multicolumn array-mode check; parser.cpp :: addRow dynamic_cast|OOB|PARSE|Y|
|11|cb5e9e1|parser.cpp :: getArgument clears _arrayMode inside group|OOB|PARSE|Y, H|
|12|9631fe5|atom_matrix.cpp :: parsePositions *{n} repeat <=1000 (clamp; throw since 55b801c)|CPU/MEM|PARSE|D size cap, H|
|13|9631fe5|macro_impl.h :: multicolumn/hdotsfor span 1..1000|CPU/MEM|PARSE|D size cap|
|14|9631fe5|macro_impl.h :: _limits_type null popLastAtom|NULL|PARSE|Y|
|15|9631fe5|parser.cpp :: ParseDepthGuard in parse() (250)|STACK|PARSE|D depth|
|16|9631fe5|parser.cpp :: processCommands/inflateNewCmd _expansions<=10000|CPU/MEM|PARSE|D work|
|17|55b801c|atom_matrix.cpp :: parsePositions step budget 100000|CPU/MEM|PARSE|D work|
|18|55b801c|atom_matrix.cpp :: MatrixAtom::createBox rows*cols<=100000|CPU/MEM|ATOM|D size cap|
|19|55b801c|macro.cpp :: checkNew/checkRenew built-in guard, _captureBuiltins/_reset|GLOBAL|INIT|D ctx, H|
|20|55b801c|macro_impl.h :: multirow span +-1000|INTOVF|PARSE|D range|
|21|55b801c|parser.cpp :: parse() '$' bounds|OOB|PARSE|Y|
|22|55b801c|latex.cpp :: LaTeX::parse resets colspecs, LINE_COLOR, _breakEveywhere|GLOBAL|INIT|D ctx|
|23|4473bb1|atom_matrix.cpp :: recalculateLine mr=min(i+n,rows)|OOB|ATOM|Y*|
|24|4473bb1|macro_impl.cpp :: macro(kern) UnitType::none -> throw|OOB|PARSE|Y*, H|
|25|51ba631|macro_impl :: above/_frac_with_delims none -> throw|OOB|PARSE|Y*, H|
|26|3a6ce6d|formula.cpp :: ArrayFormula::insertAtomIntoCol pads short rows|OOB|PARSE|Y*, H|
|27|3a6ce6d|indexed_arr.h :: IndexedArray::operator() h=_rows-1|OOB|FONT|Y|
|28|674ef54|box_factory.cpp :: DelimiterFactory::create(name,minH) ext==nullptr -> tallest char|NULL|ATOM|Y*|
|29|674ef54|box_factory.cpp :: same, repeat>0 guard + 4096 pieces|CPU/MEM|ATOM|D work, H|
|30|674ef54|macro_impl.h :: declaremathsizes/magnification zero-init|UNINIT|PARSE|Y|
|31|674ef54|parser.cpp :: addEmptyRootIfNeeded after loop|NULL|PARSE|Y*|
|32|674ef54|font_info.cpp :: getMetrics/getExtension nullptr+1|NULL|FONT|Y|
|33|674ef54|fonts.cpp :: DefaultTeXFont::getMetrics zeros, getExtension nullptr|NULL|FONT|Y*, H|
|34|674ef54|fonts.cpp :: setMathSizes/setMagnification reject non-finite/0|DIV0|FONT|D finite|
|35|674ef54|latex.cpp/fonts.cpp :: resetMathSizes per parse|GLOBAL|INIT|D ctx|
|36|59533ed|atom_basic :: Atom::inheritWrapDepth (Scripts/BigOperator, 250)|STACK|PARSE|D depth|
|37|59533ed|atom_space.cpp :: SpaceAtom::getLength non-finite -> 0|NaN|PARSE|D finite, H|
|38|59533ed|box.h/box.cpp :: countBoxAllocation 100000 (Box ctor + Box::add)|CPU/MEM|ATOM|D work, H|
|39|59533ed|box_factory.cpp :: XLeftRightArrowFactory boundedArrowWidth 4096, mwidth>0|CPU/MEM|ATOM|D size cap, H|
|40|59533ed|glue.cpp :: Glue::indexOf style clamp, Glue::get index check|OOB|ATOM|Y*|
|41|59533ed|macro.h :: isValidMacroArgc (newcommand/renewcommand/newenvironment/renewenvironment, getOptsArgs)|OOB|PARSE|D range|
|42|59533ed|macro_impl.cpp :: genfrac style 0..3|OOB|PARSE|D range|
|43|59533ed|macro_impl.cpp :: definecolor rgb/cmyk zero-init|UNINIT|PARSE|Y|
|44|59533ed|macro_impl.cpp :: romannumeral num=0, <=100000|CPU/MEM|PARSE|D size cap|
|45|59533ed|macro_impl.h :: longdiv operands <=1e9|INTOVF|PARSE|D checked|
|46|59533ed|parser.cpp :: getCommandWithArgs ParseDepthGuard|STACK|PARSE|D depth|
|47|59533ed|parser.cpp :: inflateEnv argc>1 guard|OOB|PARSE|Y|
|48|59533ed|graphic_basic.h :: argb(float) colorComponent clamp|FCAST|PARSE|Y*|
|49|59533ed|render.cpp :: rasterBound saturate, rasterSum|FCAST|BOXM|D, H|
|50|0f6daa1|atom_space.h :: SpaceAtom::getFactor bounds -> 0|OOB|ATOM|Y*, H|
|51|d4131b0|atom_matrix.cpp :: MatrixAtom::createBox smallmatrix env|UAF|ATOM|Y|
|52|5952f66|atom.h :: privateCopy; TypedAtom::getBase, parse() L_GROUP, shoveleft/right|GLOBAL|PARSE|Y|
|53|5952f66|atom_matrix.cpp :: parsePositions length<=10000, probe <= longest name|CPU/MEM|PARSE|D size cap|
|54|5952f66|atom_matrix.cpp :: recalculateLine dynamic_cast, `top`, skipped>0|OOB|ATOM|Y*, H|
|55|5952f66|atom_matrix.cpp :: multicolumnSpan clamp|OOB|ATOM|Y*|
|56|5952f66|atom_matrix.cpp :: createBox cell-type resolution, short rows|OOB|ATOM|Y*|
|57|5952f66|atom_matrix.cpp :: MultlineAtom::createBox null/short rows|NULL|ATOM|Y*|
|58|5952f66|box_single.h :: StrutBox not noexcept|EXC|ATOM|Y|
|59|5952f66|parser.cpp :: 64K expanded length, 32M work, inflateEnv counted|CPU/MEM|PARSE|D work, H|
|60|7059649|atom_basic.cpp :: VRowAtom::popLastAtom empty -> zero SpaceAtom|OOB|PARSE|Y*|
|61|7059649|atom_basic :: ColorAtom::resetColors|GLOBAL|INIT|D ctx|
|62|7059649|atom_char.cpp :: SymbolAtom::createBox small-caps end() of wrong map|OOB|ATOM|Y|
|63|7059649|string_utils.h + parser/atoms :: isAscii*/trim replace <cctype>|OOB|PARSE|Y, H|
|64|7059649|atom_row.cpp inTypeSet; glue.cpp indexOf low clamp, getSpace check|OOB|ATOM|Y*|
|65|7059649|atom_row.cpp :: Dummy::createBox Unmark RAII|GLOBAL|ATOM|Y|
|66|7059649|box_group.cpp :: RotateBox::init non-finite angle -> 0|NaN|ATOM|D finite, H|
|67|7059649|macro_impl.h :: hdotsfor coefficient non-finite -> 1|FCAST|PARSE|D finite, H|
|68|7059649|parser.cpp :: kMaxParseDepth 250 -> 48|STACK|PARSE|D depth, H|
|69|7059649|parser.cpp :: TeXParser::reset clears _middle|GLOBAL|PARSE|Y|
|70|7059649|parser.cpp :: getArgument loop not recursion|STACK|PARSE|D depth|
|71|7059649|parser.cpp :: convertCharacter copies SymbolAtom before setUnicode|GLOBAL|PARSE|Y|
|72|7059649|fonts.cpp :: getNextLarger null -> same char|NULL|FONT|Y*|
|73|7059649|atom_char :: SymbolAtom ctor not noexcept|EXC|PARSE|Y|
|74|3b3141c|~40 createBox sites, matrix arrays, Extension, Environment (TeXRenderBuilder::build), env macros: raw new -> owning|OTHER|ATOM|Y|
|75|3b3141c|atom_basic.h :: TypedAtom::getBase copy per call|GLOBAL|ATOM|Y|
|76|3b3141c|atom_char.cpp :: CharAtom::createBox local textStyle|GLOBAL|ATOM|Y|
|77|3b3141c|atom_impl.cpp :: FractionAtom::createBox local thickness|OUTPUT|ATOM|Y|
|78|3b3141c|atom_basic.cpp :: BigOperatorAtom::createBox row copy|GLOBAL|ATOM|Y|
|79|3b3141c|atom_impl.h OvalAtom::resetState; macro(cornersize) NaN-safe|GLOBAL|INIT|D ctx|
|80|3b3141c|atom_matrix.cpp :: HdotsforAtom::createBox count saturate|FCAST|ATOM|D, H|
|81|3b3141c|atom_row.cpp :: RowAtom::createBox _previousAtom reset on all exits|GLOBAL|ATOM|Y|
|82|3b3141c|colors_def.cpp :: ColorAtom::getColor isfinite/floor|FCAST|PARSE|Y*|
|83|3b3141c|box_factory.cpp :: DelimiterFactory::create(SymbolAtom&) shadowed i, walkedSmaller|OUTPUT|ATOM|D, H|
|84|3b3141c|box_single :: TextRenderingBox::resetFont|GLOBAL|INIT|D ctx (font obj C++)|
|85|3b3141c|formula.cpp :: ArrayFormula::checkDimensions cells<=100000|CPU/MEM|PARSE|D size cap|
|86|3b3141c|macro.cpp :: checkNew built-in guard regardless of _errIfConflict; reset flag|GLOBAL|INIT|D ctx, H|
|87|3b3141c|parser.cpp :: convertCharacter catch ex_symbol_not_found|EXC|PARSE|Y*, H|
|88|3b3141c|latex.cpp :: queryResourceLocation asprintf/free|OTHER|INIT|Y|
|89|3b3141c|render.cpp :: getBaseline in double|INTOVF|BOXM|Y*|
|90|3b3141c|render.cpp :: TeXRender::draw G2Restore|OTHER|DRAW|Y|
|91|3b3141c|string_utils.h :: tostring(wchar_t) wctomb -1|EXC|PARSE|Y|
|92|3b3141c|utf.cpp :: utf82wide codepoint=0|UNINIT|PARSE|Y|
|93|61aaa7c|atom_matrix.cpp :: parsePositions repeat product precheck, probes charged|CPU/MEM|PARSE|D size cap|
|94|61aaa7c|box_single.cpp :: CharBox::draw, TextRenderingBox::draw skip !(size>0)|DIV0|DRAW|D, H|
|95|61aaa7c|macro_impl.h :: newcolumntype name <=16|CPU/MEM|PARSE|D size cap|
|96|61aaa7c|fonts.cpp :: addAlphabet restore _isLoading, record blocks on throw|GLOBAL|FONT|Y*, H|
|97|61aaa7c|fonts.cpp :: setMathSizes quotients finite >0|DIV0|FONT|D finite|
|98|61aaa7c|graphic_qt.cpp :: warnIfResolvedDifferently cap 256|CPU/MEM|PLAT|C++|
|99|61aaa7c|render.cpp :: TeXRender ctor/setInsets rasterBound|FCAST|BOXM|D, H|
|100|61aaa7c|string_utils.h :: str2int INT range|INTOVF|PARSE|Y*|
|101|bf053f2|atom_basic.cpp :: inheritWrapDepth 16 per chain, refuse formula|STACK|PARSE|D depth, H|
|102|bf053f2|atom_basic.cpp :: ScriptsLayoutGuard 96 (Scripts/BigOperator createBox)|STACK|ATOM|D depth, H|
|103|683820a|formula.cpp :: Formula::_cachePredefined at init|GLOBAL|INIT|D|
|104|939670e|parser.cpp failLimit/limitExceeded; latex.cpp refuse|OUTPUT|INIT|D, H|
|105|cdd552a|parser.cpp :: getOverArgument "\cr" not followed by letter|OUTPUT|PARSE|D, H|
|106|cdd552a|parser.cpp :: popFormulaAtom <=4 per group (_overCount)|STACK|PARSE|D depth, H|
|107|2ab84a9|parser.cpp :: getArgument ESCAPE ParseDepthGuard|STACK|PARSE|D depth|
|108|34d315f|parser.cpp countExpanded shared cap; atom_matrix countParseWork for @{}|CPU/MEM|PARSE|D work|
|109|6d0b5c8|parser.cpp :: cap 64K+16*len, env bodies uncounted|OUTPUT|PARSE|D, H|
|110|c154621|graphic_qt.cpp + renamed TTF families, diagnostics|OUTPUT|PLAT|C++|
|111|b14bac2|graphic_qt.cpp :: Graphics2D_qt::drawTextAsPath|OUTPUT|PLAT|C++|
