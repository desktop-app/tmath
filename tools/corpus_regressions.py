"""Builds corpus/regressions.jsonl, the C3 records of PLAN.md.

One family of records per fork fix that has a limit or a fallback (FIXES.md,
by fix number): each limit is swept across its threshold, so the oracle
decides where accepting stops and the port has to stop at the same place, and
each fallback gets the inputs that reach it. Every record here is harmless to
a build without the fix: the sweeps stay next to the limits, far from the
sizes that crashed or hung. Such inputs belong in the ignored corpus/private/.

Usage: python -I corpus_regressions.py <output.jsonl>
"""

import json
import sys
from pathlib import Path

records = []
seen = set()


def add(id_, tex, **params):
    if id_ in seen:
        raise SystemExit(f"duplicate id {id_}")
    seen.add(id_)
    record = {"id": "c3/" + id_, "tex": tex}
    record.update(params)
    records.append(record)


def sweep(name, values, make, key=str, **params):
    for value in values:
        add(f"{name}/{key(value)}", make(value), **params)


def nested(open_, close, n, inner="x"):
    return open_ * n + inner + close * n


def gen_depth():
    # #15, #68: parse depth 48, counted in parse(), getCommandWithArgs and,
    # since #107, a command read as an argument.
    sweep("15/braces", range(44, 52), lambda n: nested("{", "}", n))
    sweep("15/sqrt", range(42, 52), lambda n: nested("\\sqrt{", "}", n))
    sweep("15/frac", range(42, 52), lambda n: nested("\\frac{", "}{y}", n))
    sweep("15/mathrm", range(42, 52), lambda n: nested("\\mathrm{", "}", n))
    sweep("107/bare-sqrt", range(40, 52), lambda n: "\\sqrt" * n + "x")
    sweep("107/bare-hat", range(40, 52), lambda n: "\\hat" * n + "x")
    sweep("46/command-args", range(42, 52),
          lambda n: "\\newcommand{\\a}[1]{(#1)}" + nested("\\a{", "}", n))
    # #70: a self-referential command read as an argument.
    add("70/self-argument", "\\newcommand{\\a}{\\a}\\frac{x^\\a}{y}")
    add("70/self-sqrt", "\\newcommand{\\a}{\\a}\\sqrt\\a")
    # Every way of nesting, at the depth limit: each takes its own stack per
    # level, in parsing and in layout, and all of them have to fit.
    wrappers = (
        ("left", "\\left(", "\\right)"), ("text", "\\text{", "}"), ("mbox", "\\mbox{", "}"),
        ("color", "\\textcolor{red}{", "}"), ("colorbox", "\\colorbox{red}{", "}"),
        ("fbox", "\\fbox{", "}"), ("boxed", "\\boxed{", "}"), ("shadowbox", "\\shadowbox{", "}"),
        ("ovalbox", "\\ovalbox{", "}"), ("doublebox", "\\doublebox{", "}"),
        ("overbrace", "\\overbrace{", "}"), ("underbrace", "\\underbrace{", "}^{a}"),
        ("xrightarrow", "\\xrightarrow{", "}"), ("overrightarrow", "\\overrightarrow{", "}"),
        ("overline", "\\overline{", "}"), ("underline", "\\underline{", "}"), ("hat", "\\hat{", "}"),
        ("widehat", "\\widehat{", "}"), ("cancel", "\\cancel{", "}"), ("phantom", "\\phantom{", "}x"),
        ("smash", "\\smash{", "}"), ("raisebox", "\\raisebox{1pt}{", "}"),
        ("rotatebox", "\\rotatebox{10}{", "}"), ("scalebox", "\\scalebox{1.01}{", "}"),
        ("reflectbox", "\\reflectbox{", "}"), ("stackrel", "\\stackrel{a}{", "}"),
        ("underset", "\\underset{a}{", "}"), ("overset", "\\overset{a}{", "}"),
        ("binom", "\\binom{a}{", "}"), ("cfrac", "\\cfrac{a}{", "}"), ("genfrac", "\\genfrac(){1pt}{0}{a}{", "}"),
        ("sfrac", "\\sfrac{a}{", "}"), ("sqrt-index", "\\sqrt[3]{", "}"), ("sup", "x^{", "}"),
        ("sub-sup", "x_{a}^{", "}"), ("sum", "\\sum_{i}^{", "}"), ("mathop", "\\mathop{", "}\\limits^a"),
        ("mathbf", "\\mathbf{", "}"), ("mathbb", "\\mathbb{", "}"), ("textit", "\\textit{", "}"),
        ("matrix", "\\begin{matrix}", "\\end{matrix}"), ("pmatrix", "\\begin{pmatrix}a&", "\\end{pmatrix}"),
        ("array", "\\begin{array}{|c|}\\hline ", "\\\\\\hline\\end{array}"),
        ("aligned", "\\begin{aligned}a&=", "\\end{aligned}"), ("cases", "\\begin{cases}a&", "\\end{cases}"),
        ("multicolumn", "\\begin{array}{cc}\\multicolumn{2}{c}{", "}\\end{array}"),
        ("displaystyle", "{\\displaystyle ", "}"), ("over", "{a\\over ", "}"), ("choose", "{a\\choose ", "}"),
        ("vrow", "{a\\\\", "}"), ("tiny", "{\\tiny ", "}"), ("st", "\\st{", "}"), ("sideset", "\\sideset{_a}{^b}{", "}"),
    )
    for name, open_, close in wrappers:
        sweep(f"depth/{name}", (23, 24, 46, 47, 48), lambda n, o=open_, c=close: nested(o, c, n))


def gen_chains():
    # #36, #101: a chain of scripts wrappers has at most 16 links.
    sweep("101/sup", range(13, 20), lambda n: "x" + "^1" * n)
    sweep("101/sub-sup", range(13, 20), lambda n: "x" + "_1^2" * n)
    sweep("101/bigop", range(13, 20), lambda n: "\\sum" + "^1" * n)
    sweep("101/primes", range(13, 20), lambda n: "x" + "'" * n)
    sweep("101/limits", range(13, 20), lambda n: "\\sum\\limits" + "_1" * n)
    # #102: chains nested in groups add up in layout, at most 96 deep.
    sweep("102/groups", range(28, 36),
          lambda n: "{" * n + "x^1^2^3" + "}^1^2^3" * n)
    sweep("102/groups-bigop", range(6, 20),
          lambda n: "{" * n + "\\sum^1^2^3" + "}^1^2^3" * n)
    sweep("102/twos", range(44, 50),
          lambda n: "{" * n + "x^1^2" + "}^1^2" * n)


def gen_overs():
    # #106: at most four commands of the \over family in one group. Since
    # #105 a denominator runs to the end of the group or to a row end, so
    # these chains nest instead and stay under the bound.
    for command in ("over", "atop", "choose", "brack", "brace"):
        sweep(f"106/{command}", range(2, 8),
              lambda n, c=command: "a" + f"\\{c} b" * n)
    sweep("106/groups", range(2, 8),
          lambda n: "{a\\over b}" * n + "\\over c")
    sweep("106/cells", range(2, 8),
          lambda n: "\\begin{matrix}" + "a" + "\\over b" * n + "&"
          + "c" + "\\over d" * n + "\\\\" + "e" + "\\over f" * n
          + "\\end{matrix}")
    sweep("106/above", range(2, 8), lambda n: "a" + "\\above 1pt b" * n)
    # #105: \cr ends an \over argument only when no letter follows.
    add("105/cr", "a\\over b\\cr c")
    add("105/crx", "a\\over b\\crx c")
    add("105/cr-brace", "a\\over b\\cr{c}")
    add("105/cr-at", "\\makeatletter a\\over b\\cr@ c")
    add("105/cr-end", "a\\over b\\cr")


def gen_expansion():
    # #16: at most 10000 expansions per parser.
    add("16/self", "\\newcommand{\\a}{\\a}\\a")
    add("16/grow", "\\newcommand{\\a}{x\\a}\\a")
    add("16/double", "\\newcommand{\\a}{\\a\\a}\\a")
    # Many expansions of a long text run into the 32M work budget first.
    sweep("16/work", range(4400, 4850, 50),
          lambda n: "\\newcommand{\\a}{ }" + "\\a" * n)
    # #59: 64K of text per parser, 32M of work.
    for unit in (1000, 4000):
        body = " " * unit
        sweep(f"59/length-{unit}", range(60, 70) if unit == 1000 else range(14, 19),
              lambda n, b=body: "\\newcommand{\\a}{" + b + "}" + "\\a" * n)
    # #108, #109: a shared total of 64K + 16 per source unit, across parsers.
    body = " " * 1000
    sweep("109/shared", range(60, 90, 3),
          lambda n: "\\newcommand{\\a}{" + body + "}" + "\\mathrm{\\a}" * n)
    sweep("109/shared-long-source", range(60, 90, 3),
          lambda n: "\\newcommand{\\a}{" + body + "}" + "\\mathrm{\\a}" * n
          + " " * 2000)
    # #108: @{} charges rows times columns of work.
    def at_columns(rows, ats):
        spec = "c" + "@{x}c" * ats
        row = "&".join("a" for _ in range(ats + 1))
        return f"\\begin{{array}}{{{spec}}}" + "\\\\".join([row] * rows) + "\\end{array}"
    for rows, ats in ((10, 10), (100, 50), (300, 100)):
        add(f"108/at-work/{rows}x{ats}", at_columns(rows, ats))


def gen_columns():
    # #12, #93: a repeat count of at most 1000, and the product checked first.
    sweep("12/repeat", (999, 1000, 1001, -1), lambda n: f"\\begin{{array}}{{*{{{n}}}{{c}}}}a\\end{{array}}")
    sweep("93/product", (9, 10, 11), lambda n: f"\\begin{{array}}{{*{{1000}}{{{'c' * n}}}}}a\\end{{array}}")
    # #53: a spec of at most 10000 units after expansion.
    sweep("53/length", (9990, 10000, 10001), lambda n: "\\begin{array}{" + "c" * n + "}a\\end{array}")
    # #17, #93: 100000 steps, probes of registered names included.
    name = "abcdefghijklmnop"
    sweep("17/probes", range(5860, 5905, 5),
          lambda n: f"\\newcolumntype{{{name}}}{{c}}\\begin{{array}}{{" + "z" * n + "}a\\end{array}")
    # #95: a column type name of at most 16 units.
    add("95/name-16", "\\newcolumntype{abcdefghijklmnop}{l}\\begin{array}{abcdefghijklmnop}a\\end{array}")
    add("95/name-17", "\\newcolumntype{abcdefghijklmnopq}{l}\\begin{array}{c}a\\end{array}")


def gen_matrices():
    # #18, #85: at most 100000 cells.
    sweep("85/cells", (99, 100, 101),
          lambda n: "\\begin{array}{c}" + "\\multicolumn{1000}{c}{}\\\\" * n + "\\end{array}")
    # #13: column spans 1..1000; #20: row spans of at most 1000 either way.
    sweep("13/multicolumn", (0, 1, 1000, 1001), lambda n: f"\\begin{{array}}{{cc}}\\multicolumn{{{n}}}{{c}}{{x}}&y\\end{{array}}")
    sweep("13/hdotsfor", (0, 1, 1000, 1001), lambda n: f"\\begin{{matrix}}a&b\\\\\\hdotsfor{{{n}}}\\end{{matrix}}")
    sweep("20/multirow", (-1001, -1000, 1000, 1001), lambda n: f"\\begin{{array}}{{cc}}\\multirow{{{n}}}{{*}}{{x}}&y\\\\a&b\\end{{array}}")
    # #9: alignat takes fewer columns than declared.
    sweep("9/alignat", (1, 2, 3), lambda n: f"\\begin{{alignat}}{{{n}}}a&=b&c&=d\\end{{alignat}}")
    # #60: \\ with nothing before it.
    add("60/leading-cr", "\\\\x")
    add("60/only-cr", "\\\\")
    add("60/double-cr", "\\\\\\\\")


def gen_values():
    # #41: argc 0..9.
    sweep("41/argc", (-1, 0, 9, 10), lambda n: f"\\newcommand{{\\a}}[{n}]{{x}}\\a")
    sweep("41/env-argc", (-1, 0, 9, 10), lambda n: f"\\newenvironment{{e}}[{n}]{{(}}{{)}}\\begin{{e}}x\\end{{e}}")
    # #42: genfrac style 0..3.
    sweep("42/genfrac", (-1, 0, 3, 4), lambda n: f"\\genfrac(){{1pt}}{{{n}}}{{a}}{{b}}")
    # #44: roman numerals up to 100000.
    sweep("44/roman", (0, -5, 3999, 100000, 100001), lambda n: f"\\roman{{{n}}}")
    add("44/roman-text", "\\Roman{xyz}")
    # #45: longdiv operands up to 1e9 either way, divisor not 0.
    sweep("45/longdiv", ("1000000000/7", "1000000001/7", "-1000000000/7", "100/0", "-100/7",
                         "100/-7", "-100/-7", "5/1000000000", "123456789/-1000000000"),
          lambda v: "\\longdiv{%s}{%s}" % tuple(v.split("/")),
          key=lambda v: v.replace("/", "_"))
    # #100: \char takes values in int range only.
    sweep("100/char", ("65", "0x41", "x41", "0101", "2147483647", "2147483648", "-1", "-2147483649",
                       "0x7FFFFFFF", "0x80000000", "0x", "", " 65", "+65"),
          lambda v: f"\\char{{{v}}}",
          key=lambda v: v.replace(" ", "space").replace("+", "plus") or "empty")
    # #24, #25: a missing dimension.
    for i, tex in enumerate(("a\\above b", "a\\above", "a\\abovewithdelims()b", "a\\kern",
                             "a\\kern b", "a\\above 1em b", "a\\abovewithdelims() 2pt b")):
        add(f"25/{i}", tex)
    # valueof() reads floats as MSVC's stream does, hexadecimal ones too:
    # correctly rounded, with an exponent marker and no digits failing to 0.
    hex_values = ("0x2", "0x1.8p1", "0x1e1", "0x0.1", "-0x2", " 0x3", "+0x3", "0x", "0xg", "0x.",
                  "0x1p", "0x1p+", "0x1.000001p0", "0x1.000003p0", "0x1.0000011p0",
                  "0x1.0000010000000000000000000000001p0", "0x1p-149", "0x1p-150", "0x1.8p-149",
                  "0x1.fffffep127", "0x1.ffffffp127", "0x1p99999999999999", "0x1p-99999999999",
                  "0x" + "f" * 40 + "p-100", "0x1.fffffe8p-127", "1e", "1e+", "1e-")
    for i, value in enumerate(hex_values):
        add(f"valueof/scale-{i}", "\\scalebox{" + value + "}{x}")
        add(f"valueof/color-{i}", "\\textcolor{" + value + ",0.5,0}{x}")
        add(f"valueof/hdots-{i}", "\\begin{matrix}aaaa&bbbb\\\\\\hdotsfor[" + value + "]{2}\\end{matrix}")


def gen_finite():
    # #37: an infinite length is 0; #50: an unknown unit has factor 0.
    for i, tex in enumerate(("\\kern1e39pt x", "\\hspace{1e39em}x", "\\hspace{-1e39em}x",
                             "\\rule{1e39pt}{1pt}", "\\rule{1pt}{1e39pt}", "\\raisebox{1e39pt}{x}",
                             "\\kern 3furlong x", "\\rule{3qq}{1pt}", "x\\hspace{1e38em}y")):
        add(f"37/{i}", tex)
    # #49, #99: sizes saturate; #3, #89: ceil and a double baseline.
    for i, tex in enumerate(("\\rule{1pt}{3e37pt}", "\\rule{3e37pt}{1pt}", "\\rule{1pt}{1e30pt}",
                             "\\scalebox{1e30}{x}", "\\scalebox{-1e30}{x}", "\\scalebox{1e-30}{x}",
                             "\\rule[-1e30pt]{1pt}{1pt}")):
        add(f"49/{i}", tex)
    # #66: an infinite angle is 0.
    for i, angle in enumerate(("1e39", "-1e39", "360", "1e30", "45")):
        add(f"66/{i}", f"\\rotatebox{{{angle}}}{{xy}}")
    # #67, #80: the hdots coefficient and count.
    for i, coeff in enumerate(("1e39", "-1e39", "1e-30", "0", "-1", "1e30")):
        add(f"80/{i}", f"\\begin{{matrix}}aaaaaaaa&bbbbbbbb\\\\\\hdotsfor[{coeff}]{{2}}\\end{{matrix}}")
    # #39: arrow shafts of at most 4096 pieces.
    sweep("39/arrow", (100, 1000, 2000, 3000, 4000),
          lambda n: f"\\xrightarrow{{\\hspace{{{n}em}}}}")
    sweep("39/double-arrow", (100, 1000, 3000, 4000),
          lambda n: f"\\overleftrightarrow{{\\hspace{{{n}em}}}}")
    # #29: delimiters of at most 4096 pieces.
    sweep("29/delimiter", (100, 1000, 3000, 5000),
          lambda n: f"\\left(\\rule{{1pt}}{{{n}em}}\\right)")
    # #34, #97: \DeclareMathSizes and \magnification ignore bad values.
    for i, args in enumerate(("{0}{1}{2}{3}", "{1}{0}{1}{1}", "{1e-38}{3.4e38}{1}{1}", "{10}{10}{7}{5}",
                              "{}{}{}{}", "{-10}{10}{7}{5}", "{1e39}{1}{1}{1}")):
        add(f"34/{i}", f"\\DeclareMathSizes{args}x^{{y^z}}")
    for i, value in enumerate(("0", "-1", "1e39", "2000", "500", "x")):
        add(f"97/{i}", f"\\magnification{{{value}}}x+y")
    # #48, #82: color components clamp.
    for i, color in enumerate(("1e39,0,0", "-5,300,2", "0.5,1e39,0", "300,300,300", "1,1,1,1e39",
                               "nan,0,0", "#GGGGGG", "1.5")):
        add(f"48/{i}", f"\\textcolor{{{color}}}{{x}}")
    # #94: nothing is drawn at a size that is not positive.
    for size in (0, -16, 1):
        add(f"94/size-{size}", "x+\\text{ab}", size=size)


def gen_state():
    # #103: predefined formulas built at start-up, whatever the formula did.
    add("103/redefined-symbol", "\\newcommand{\\cdotp}{X}\\cdots")
    add("103/renewed-symbol", "\\newcommand{\\ldotp}{Y}\\ldots\\dots")
    add("103/deep", nested("{", "}", 46, "\\cdots"))
    add("103/deeper", nested("{", "}", 47, "\\cdots"))
    add("103/deep-row", nested("{", "}", 46, "\\iff"))
    add("103/ddots-deep", nested("{", "}", 46, "\\ddots"))
    # #19, #86: built-in commands cannot be redefined.
    add("86/renew-builtin", "\\renewcommand{\\frac}{x}\\frac{a}{b}")
    add("86/new-builtin", "\\newcommand{\\frac}{x}\\frac{a}{b}")
    add("86/conflict-off", "\\fatalIfCmdConflict{false}\\newcommand{\\a}{x}\\newcommand{\\a}{y}\\a")
    add("86/conflict-on", "\\newcommand{\\a}{x}\\newcommand{\\a}{y}\\a")
    # #22, #61, #79: formula-wide state is read at layout, last value wins.
    add("79/cornersize", "\\ovalbox{x}\\cornersize{0.1}")
    add("79/cornersize-nan", "\\cornersize{x}\\ovalbox{x}")
    add("22/rulecolor", "\\begin{array}{|c|}\\hline a\\\\\\hline\\end{array}\\arrayrulecolor{red}\\arrayrulecolor{blue}")
    add("22/break", "a+b+c+d\\breakEverywhere{true}", width=200)
    add("84/font", "\\text{ab}\\externalFont{Monospace}\\text{cd}")
    # #63: ASCII-only classes; C isspace includes \v.
    add("63/vtab-origin", "\\rotatebox[origin=\x0bcc]{90}{x}")
    add("63/unicode-command", "\\newcommand{\\\u00e9}{x}\\\u00e9")
    add("63/unicode-roman", "\\roman{\u0661\u0662}")
    # \debug and \undebug exist only in GRAPHICS_DEBUG builds, which tdesktop
    # does not make, so they are unknown commands.
    add("debug/on", "\\debug x^2+\\frac{a}{b}")
    add("debug/on-then-off", "\\debug x\\undebug")
    add("debug/off", "\\undebug y")


def gen_budget():
    # #38: at most 100000 boxes, counted at every construction and append.
    ten = "\\newcommand{\\x}{xxxxxxxxxx}\\newcommand{\\y}{\\x\\x\\x\\x\\x\\x\\x\\x\\x\\x}"
    sweep("38/chars", range(240, 305, 5), lambda n: ten + "\\y" * n)


def main():
    output = sys.argv[1]
    gen_depth()
    gen_chains()
    gen_overs()
    gen_expansion()
    gen_columns()
    gen_matrices()
    gen_values()
    gen_finite()
    gen_state()
    gen_budget()
    Path(output).parent.mkdir(parents=True, exist_ok=True)
    with open(output, "w", encoding="utf-8", newline="\n") as f:
        for record in records:
            f.write(json.dumps(record, ensure_ascii=True) + "\n")
    print(f"{len(records)} records -> {output}")


if __name__ == "__main__":
    main()
