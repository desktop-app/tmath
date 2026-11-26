"""Builds corpus/features.jsonl, the generated feature matrix (C1 in PLAN.md).

Everything is enumerated from MicroTeX's own sources, so a command, symbol,
environment, built-in formula, Unicode mapping, unit or color that exists there
gets records here: primitive commands with each argument shape, symbols in
every style, predefined formulas and commands, environments by shape,
delimiters at every size, units, colors, math alphabets over several scripts,
and line breaking at several widths.

Usage: python -I corpus_features.py <MicroTeX checkout> <output.jsonl>
"""

import json
import re
import sys
from pathlib import Path

SIZE = 1024
WIDTH = 600 * 64
STYLES = ["displaystyle", "textstyle", "scriptstyle", "scriptscriptstyle"]

records = []
seen = set()


def add(id_, tex, display=True, **params):
    if id_ in seen:
        raise SystemExit(f"duplicate id {id_}")
    seen.add(id_)
    record = {"id": id_, "tex": ("\\displaystyle " + tex) if display else tex}
    record.update(params)
    records.append(record)


def read(root, path):
    return (Path(root) / "src" / path).read_text(encoding="utf-8")


def primitives(root):
    text = read(root, "core/macro_def.cpp")
    result = []
    for m in re.finditer(r'mac\((\d+),\s*(?:(\d+),\s*)?\w+,\s*"([^"]+)"\)', text):
        argc, opts, name = int(m.group(1)), m.group(2), m.group(3)
        if "@" in name:
            continue
        result.append((name, argc, int(opts) if opts else None))
    return result


def predefined_commands(root):
    text = read(root, "core/macro_def.cpp")
    return [(m.group(2), int(m.group(1)))
            for m in re.finditer(r'cmd\((\d+),\s*L"([^"]+)"', text)]


def environments(root):
    text = read(root, "core/macro_def.cpp")
    return [(m.group(2), int(m.group(1)))
            for m in re.finditer(r'env\((\d+),\s*L"([^"]+)"', text)]


def symbols(root):
    text = read(root, "res/builtin/tex_symbols.res.cpp")
    result = {}
    for m in re.finditer(r'^\s*(sym|del)\((\w+),\s*(\w+)\)', text, re.M):
        result.setdefault(m.group(3), (m.group(1), m.group(2), m.group(3)))
    return list(result.values())


def predefined_formulas(root):
    text = read(root, "core/formula_def.cpp")
    return sorted(set(m.group(1) for m in re.finditer(r'\{L"([^"]+)",\s*L"', text)))


def unicode_mappings(root):
    text = read(root, "res/builtin/formula_mappings.res.cpp")
    return sorted(set(int(m.group(1)) for m in re.finditer(r'\{(\d+),\s*"', text)))


def units(root):
    text = read(root, "atom/unit_conversion.cpp")
    return [m.group(1) for m in re.finditer(r'\{"(\w+)",\s*UnitType::', text)]


def colors(root):
    text = read(root, "atom/colors_def.cpp")
    names = re.findall(r'\{"(\w+)",\s*\w+\}', text) + re.findall(r'c\("(\w+)"', text)
    return sorted(set(names))


SIMPLE_ARGS = ["x", "y+1", "\\frac{a}{b}", "A", "2", "z_i"]
NESTED_ARGS = ["\\sqrt{x^2+\\frac{1}{y}}", "{a}_{b}^{c}", "\\left(t\\right)"]

TYPED = {
    "rule": ["\\rule{2em}{1pt}", "\\rule[0.5ex]{1em}{0.4pt}", "\\rule{0pt}{3ex}x"],
    "includegraphics": [],
    "sqrt": ["\\sqrt{x}", "\\sqrt[3]{x+1}", "\\sqrt[n]{\\frac{a}{b}}"],
    "rotatebox": ["\\rotatebox{45}{xy}", "\\rotatebox[origin=c]{180}{\\&}", "\\rotatebox{-90}{\\frac{a}{b}}"],
    "scalebox": ["\\scalebox{2}{x}", "\\scalebox{0.5}[2]{y}", "\\scalebox{-1}{z}"],
    "raisebox": ["\\raisebox{2pt}{x}", "\\raisebox{-1ex}{\\frac{a}{b}}"],
    "reflectbox": ["\\reflectbox{abc}"],
    "color": ["\\color{red}{x+y}", "{\\color{blue} a+b}", "\\color{#FF8800}{z}"],
    "textcolor": ["\\textcolor{green}{x}", "\\textcolor{#336699}{\\frac{1}{2}}"],
    "colorbox": ["\\colorbox{yellow}{x}", "\\colorbox{#00FF00}{\\sum_i x_i}"],
    "fcolorbox": ["\\fcolorbox{red}{blue}{x}", "\\fcolorbox{black}{white}{\\int_0^1 f}"],
    "definecolor": [
        "\\definecolor{c}{rgb}{0.1,0.5,0.9}\\color{c}{x}",
        "\\definecolor{c}{RGB}{10,128,250}\\color{c}{x}",
        "\\definecolor{c}{cmyk}{0.1,0.2,0.3,0.4}\\color{c}{x}",
        "\\definecolor{c}{gray}{0.5}\\color{c}{x}",
        "\\definecolor{c}{HTML}{FF8800}\\color{c}{x}",
    ],
    "kern": ["a\\kern 3pt b", "a\\kern{-1em}b", "a\\kern 2mu b"],
    "hspace": ["a\\hspace{1em}b", "a\\hspace{-3pt}b"],
    "vspace": ["a\\vspace{1em}b"],
    "genfrac": ["\\genfrac{(}{)}{0pt}{0}{a}{b}", "\\genfrac{[}{]}{1pt}{1}{a}{b}",
                "\\genfrac{}{}{}{2}{a}{b}", "\\genfrac{\\{}{\\}}{2pt}{3}{a}{b}"],
    "frac": ["\\frac{a}{b}", "\\frac{\\frac{1}{x}}{1+\\frac{1}{y}}", "\\frac12"],
    "cfrac": ["\\cfrac{1}{1+\\cfrac{1}{x}}", "\\cfrac[l]{1}{x}", "\\cfrac[r]{1}{x}"],
    "binom": ["\\binom{n}{k}", "\\binom{n+1}{\\frac{k}{2}}"],
    "stackrel": ["\\stackrel{def}{=}", "\\stackrel[a]{b}{=}"],
    "stackbin": ["\\stackbin{*}{+}"],
    "overset": ["\\overset{a}{b}"],
    "underset": ["\\underset{a}{b}"],
    "xleftarrow": ["\\xleftarrow{abc}", "\\xleftarrow[under]{over}"],
    "xrightarrow": ["\\xrightarrow{abc}", "\\xrightarrow[under]{over}"],
    "newcommand": [
        "\\newcommand{\\f}{x^2}\\f+\\f",
        "\\newcommand{\\g}[2]{#1+#2}\\g{a}{b}",
        "\\newcommand{\\h}[2][z]{#1\\cdot #2}\\h{a}\\h[b]{c}",
    ],
    "renewcommand": ["\\renewcommand{\\alpha}{A}\\alpha", "\\newcommand{\\f}{a}\\renewcommand{\\f}{b}\\f"],
    "newenvironment": ["\\newenvironment{e}{(}{)}\\begin{e}x\\end{e}",
                       "\\newenvironment{e}[1]{[#1}{]}\\begin{e}{a}x\\end{e}"],
    "renewenvironment": ["\\renewenvironment{matrix}{(}{)}\\begin{matrix}x\\end{matrix}"],
    "newcolumntype": ["\\newcolumntype{C}{>{\\bf}c}\\begin{array}{CC}a&b\\end{array}",
                      "\\newcolumntype{Z}{|c|}\\begin{array}{Z}a\\\\b\\end{array}"],
    "multicolumn": ["\\begin{array}{ccc}\\multicolumn{2}{c}{ab}&c\\\\a&b&c\\end{array}",
                    "\\begin{array}{|c|c|}\\multicolumn{2}{|c|}{x}\\\\1&2\\end{array}"],
    "multirow": ["\\begin{array}{cc}\\multirow{2}{*}{x}&a\\\\&b\\end{array}",
                 "\\begin{array}{cc}\\multirow{-2}{*}{x}&a\\\\&b\\end{array}"],
    "hdotsfor": ["\\begin{matrix}a&b&c\\\\\\hdotsfor{3}\\\\d&e&f\\end{matrix}",
                 "\\begin{matrix}a&b\\\\\\hdotsfor[2]{2}\\end{matrix}"],
    "hline": ["\\begin{array}{c}\\hline a\\\\\\hline b\\\\\\hline\\end{array}"],
    "cline": ["\\begin{array}{ccc}a&b&c\\\\\\cline{1-2}d&e&f\\end{array}"],
    "arrayrulecolor": ["\\arrayrulecolor{red}\\begin{array}{|c|}\\hline a\\\\\\hline\\end{array}"],
    "left": ["\\left(\\frac{a}{b}\\right)", "\\left.\\frac{a}{b}\\right|", "\\left\\{x\\middle|y\\right\\}"],
    "middle": ["\\left(a\\middle/b\\right)"],
    "over": ["{a\\over b}", "{a+b\\over c+d}"],
    "atop": ["{a\\atop b}"],
    "choose": ["{n\\choose k}"],
    "brack": ["{n\\brack k}"],
    "brace": ["{n\\brace k}"],
    "above": ["{a\\above 2pt b}"],
    "abovewithdelims": ["{a\\abovewithdelims() 1pt b}"],
    "atopwithdelims": ["{a\\atopwithdelims[] b}"],
    "overwithdelims": ["{a\\overwithdelims\\{\\} b}"],
    "DeclareMathSizes": ["\\DeclareMathSizes{10}{12}{8}{6}x^{y^z}"],
    "magnification": ["\\magnification=1200 x^2"],
    "externalfont": ["\\externalfont{Arial}\\text{abc}"],
    "romannumeral": ["\\romannumeral{1987}", "\\Romannumeral{42}"],
    "Romannumeral": ["\\Romannumeral{3999}"],
    "longdiv": ["\\longdiv{12345}{13}"],
    "char": ["\\char65", "\\char\"263A"],
    "cornersize": ["\\cornersize{0.3}\\ovalbox{x}"],
    "ovalbox": ["\\ovalbox{abc}"],
    "doublebox": ["\\doublebox{abc}"],
    "shadowbox": ["\\shadowbox{abc}"],
    "fbox": ["\\fbox{abc}", "\\fbox{\\frac{1}{2}}"],
    "boxed": ["\\boxed{E=mc^2}"],
    "sideset": ["\\sideset{_a^b}{_c^d}\\sum", "\\sideset{}{'}\\sum_{n<k}"],
    "underaccent": ["\\underaccent{\\tilde}{x}"],
    "accentset": ["\\accentset{*}{x}"],
    "IJ": ["\\IJ"],
    "cancel": ["\\cancel{x+y}"],
    "bcancel": ["\\bcancel{x+y}"],
    "xcancel": ["\\xcancel{x+y}"],
    "smash": ["\\smash{\\frac{a}{b}}", "\\smash[t]{x}", "\\smash[b]{y}"],
    "operatorname": ["\\operatorname{sgn}(x)", "\\operatorname{argmax}_x f"],
    "DeclareMathOperator": ["\\DeclareMathOperator{\\Tr}{Tr}\\Tr A"],
    "substack": ["\\sum_{\\substack{0<i<m\\\\0<j<n}} P(i,j)"],
    "pmod": ["a\\equiv b\\pmod{m}"],
    "breakEverywhere": ["\\breakEverywhere{true}a+b+c"],
    "fatalIfCmdConflict": ["\\fatalIfCmdConflict{false}\\newcommand{\\alpha}{A}\\alpha"],
    "begin": [],
    "end": [],
    "debug": [],
    "undebug": [],
}

ALPHABETS = {
    "latin": "AaBbZz019",
    "greek": "\u03b1\u03b2\u03b3\u03a9\u0394",
    "cyrillic": "\u0416\u0438\u0437\u043d\u044c",
    "cjk": "\u6570\u5b66",
    "emoji": "\U0001F600",
    "accented": "\u00e9\u00e0\u00fc\u00df",
}

FONT_COMMANDS = ["mathbf", "mathit", "mathrm", "mathsf", "mathtt", "mathcal",
                 "mathbb", "mathfrak", "mathscr", "boldsymbol", "text", "mbox",
                 "textbf", "textit", "textsf", "texttt", "textrm", "bm",
                 "mathds", "mathbfit", "mathbfcal", "mathsfit", "Bbb"]

DELIMITERS = ["(", ")", "[", "]", "\\{", "\\}", "|", "\\|", "/", "\\backslash",
              "\\langle", "\\rangle", "\\lfloor", "\\rfloor", "\\lceil", "\\rceil",
              "\\uparrow", "\\downarrow", "\\updownarrow", "\\Uparrow",
              "\\Downarrow", "\\Updownarrow", "\\lgroup", "\\rgroup",
              "\\lmoustache", "\\rmoustache", "\\vert", "\\Vert", "\\lbrack",
              "\\rbrack", "\\lbrace", "\\rbrace", "."]

SIZES = ["big", "Big", "bigg", "Bigg", "bigl", "bigr", "Bigl", "Bigr", "biggl",
         "biggr", "Biggl", "Biggr", "bigm", "Bigm"]


def args_for(argc, pool):
    return "".join("{" + pool[i % len(pool)] + "}" for i in range(argc))


def gen_primitives(root):
    for name, argc, opts in primitives(root):
        base = f"c1/primitive/{name}"
        if name in TYPED:
            for i, tex in enumerate(TYPED[name]):
                add(f"{base}/typed{i}", tex)
            if name in ("begin", "end", "debug", "undebug", "includegraphics"):
                continue
        add(f"{base}/bare", f"\\{name}")
        add(f"{base}/bare-inline", f"\\{name}", display=False)
        if argc:
            add(f"{base}/simple", f"\\{name}{args_for(argc, SIMPLE_ARGS)}")
            add(f"{base}/nested", f"\\{name}{args_for(argc, NESTED_ARGS)}")
            add(f"{base}/empty", f"\\{name}" + "{}" * argc)
            add(f"{base}/short", f"\\{name}" + "{x}" * max(argc - 1, 0))
            add(f"{base}/scripts", f"{{\\{name}{args_for(argc, SIMPLE_ARGS)}}}^2_i")
            add(f"{base}/inline", f"a\\{name}{args_for(argc, SIMPLE_ARGS)}b", display=False)
        else:
            add(f"{base}/between", f"a\\{name} b")
            add(f"{base}/scripts", f"\\{name}^2_i")
        if opts:
            mandatory = ["{" + SIMPLE_ARGS[i % len(SIMPLE_ARGS)] + "}" for i in range(argc)]
            position = min(opts - 1, len(mandatory))
            with_opt = mandatory[:position] + ["[2]"] + mandatory[position:]
            add(f"{base}/optional", f"\\{name}" + "".join(with_opt))
            empty_opt = mandatory[:position] + ["[]"] + mandatory[position:]
            add(f"{base}/optional-empty", f"\\{name}" + "".join(empty_opt))


def gen_predefined_commands(root):
    for name, argc in predefined_commands(root):
        base = f"c1/command/{name}"
        if name in TYPED:
            for i, tex in enumerate(TYPED[name]):
                add(f"{base}/typed{i}", tex)
        add(f"{base}/simple", f"\\{name}{args_for(argc, SIMPLE_ARGS)}")
        add(f"{base}/inline", f"a\\{name}{args_for(argc, SIMPLE_ARGS)}b", display=False)
        add(f"{base}/scripts", f"{{\\{name}{args_for(argc, SIMPLE_ARGS)}}}^2_i")


def gen_environments(root):
    bodies = {
        "1x1": "a",
        "2x2": "a&b\\\\c&d",
        "3x3": "1&2&3\\\\4&5&6\\\\7&8&9",
        "ragged": "a&b&c\\\\d\\\\e&f",
        "fractions": "\\frac{1}{2}&\\sqrt{x}\\\\x^2&\\sum_i i",
        "hline": "\\hline a&b\\\\\\hline c&d\\\\\\hline",
        "empty-cells": "&\\\\&",
        "trailing": "a&b\\\\c&d\\\\",
    }
    specs = {
        "array": ["{cc}", "{lcr}", "{|c|c|}", "{@{}l@{x}r@{}}", "{c|c}", "{*{3}{c}}", "{p{2em}c}"],
        "tabular": ["{cc}", "{|l|r|}"],
        "alignat": ["{2}", "{1}"],
        "alignedat": ["{2}", "{1}"],
    }
    for name, argc in environments(root):
        variants = specs.get(name, [""]) if argc else [""]
        for vi, spec in enumerate(variants):
            for bname, body in bodies.items():
                add(f"c1/env/{name}/{vi}/{bname}",
                    f"\\begin{{{name}}}{spec}{body}\\end{{{name}}}")
        add(f"c1/env/{name}/inline", f"x=\\begin{{{name}}}{variants[0]}a&b\\\\c&d\\end{{{name}}}", display=False)
        add(f"c1/env/{name}/unclosed", f"\\begin{{{name}}}{variants[0]}a&b")


ARRAY_CASES = {
    "spec/at-empty": r"\begin{array}{@{}c@{}}a\\b\end{array}",
    "spec/at-many": r"\begin{array}{c@{:}c@{-}c}a&b&c\\d&e&f\end{array}",
    "spec/at-past-cols": r"\begin{array}{c@{x}c@{y}c@{z}}a&b\end{array}",
    "spec/at-short-row": r"\begin{array}{ccc@{x}}\\a&b&c\end{array}",
    "spec/at-group": r"\begin{array}{c@{\quad\frac12}c}a&b\end{array}",
    "spec/star": r"\begin{array}{*{3}{c|}}a&b&c\end{array}",
    "spec/star-zero": r"\begin{array}{*{0}{c}c}a\end{array}",
    "spec/star-negative": r"\begin{array}{*{-1}{c}}a\end{array}",
    "spec/star-too-many": r"\begin{array}{*{1001}{c}}a\end{array}",
    "spec/star-too-long": r"\begin{array}{*{1000}{cccccccccccc}}a\end{array}",
    "spec/star-nested": r"\begin{array}{*{2}{*{2}{l}r}}a&b&c&d&e&f\end{array}",
    "spec/vlines": r"\begin{array}{||c|||c||}a&b\\c&d\end{array}",
    "spec/more-cells": r"\begin{array}{c}a&b&c\end{array}",
    "spec/more-columns": r"\begin{array}{lcrlcr}a&b\end{array}",
    "spec/blanks": "\\begin{array}{ l\tc r }a&b&c\\end{array}",
    "spec/unknown": r"\begin{array}{xyz}a&b&c\end{array}",
    "spec/empty": r"\begin{array}{}a&b\end{array}",
    "spec/gt-colors": r"\begin{array}{>{\columncolor{red}}c>\color{blue}c}a&b\end{array}",
    "spec/gt-braced-color": r"\begin{array}{>{\color{blue}}c c}a&b\end{array}",
    "spec/gt-bare-columncolor": r"\begin{array}{>\columncolor{red}c c}a&b\end{array}",
    "spec/gt-nested": r"\begin{array}{>{{\columncolor{red}\columncolor{green}}}c}a\end{array}",
    "spec/newcolumntype": r"\newcolumntype{C}{>{\columncolor{yellow}}c}\begin{array}{CC|C}a&b&c\end{array}",
    "spec/newcolumntype-long": r"\newcolumntype{abcdefghijklmnopq}{c}\begin{array}{c}a\end{array}",
    "spec/newcolumntype-self": r"\newcolumntype{A}{A}\begin{array}{A}a\end{array}",
    "spec/newcolumntype-grows": r"\newcolumntype{A}{AA}\begin{array}{A}a\end{array}",
    "spec/newcolumntype-longest": r"\newcolumntype{AB}{l}\newcolumntype{A}{r}\begin{array}{ABA}a&b\end{array}",
    "spec/newcolumntype-after": r"\begin{array}{C}a\end{array}\newcolumntype{C}{r}",
    "hline/double": r"\begin{array}{cc}\hline\hline a&b\\\hline c&d\\\hline\hline\end{array}",
    "hline/in-cell": r"\begin{array}{cc}a\hline&b\end{array}",
    "hline/only": r"\begin{array}{c}\hline\end{array}",
    "hline/outside": r"a\hline b",
    "hline/in-group": r"\begin{array}{c}{\hline}a\end{array}",
    "hline/rule-color": r"\arrayrulecolor{red}\begin{array}{|c|}\hline a\\\hline\end{array}",
    "hline/rule-color-after": r"\begin{array}{|c|}\hline a\\\hline\end{array}\arrayrulecolor{blue}",
    "color/cell": r"\begin{array}{cc}\cellcolor{red}a&b\\c&\cellcolor{green}d\end{array}",
    "color/row": r"\begin{array}{cc}\rowcolor{yellow}a&b\\c&d\end{array}",
    "color/in-cell": r"\begin{array}{cc}\color{red}a&b\end{array}",
    "color/cell-outside": r"\cellcolor{red}a",
    "color/row-outside": r"\rowcolor{red}a",
    "color/column-outside": r"\columncolor{red}a",
    "multicolumn/rules": r"\begin{array}{|c|c|c|}\multicolumn{2}{|c|}{wide text here}&x\\a&b&c\end{array}",
    "multicolumn/left": r"\begin{array}{ccc}\multicolumn{3}{l}{x}\\a&b&c\end{array}",
    "multicolumn/right": r"\begin{array}{ccc}\multicolumn{3}{r}{x}\\a&b&c\end{array}",
    "multicolumn/past-end": r"\begin{array}{cc}a&\multicolumn{5}{c}{x}\\c&d\end{array}",
    "multicolumn/zero": r"\begin{array}{cc}\multicolumn{0}{c}{x}\end{array}",
    "multicolumn/too-many": r"\begin{array}{cc}\multicolumn{1001}{c}{x}\end{array}",
    "multicolumn/outside": r"\multicolumn{2}{c}{x}",
    "multicolumn/own-rules": r"\begin{array}{|c|c|}\multicolumn{1}{||c||}{x}&y\end{array}",
    "multicolumn/narrower": r"\begin{array}{cc}\multicolumn{2}{c}{x}\\abcdef&ghijkl\end{array}",
    "multicolumn/wider": r"\begin{array}{cc}\multicolumn{2}{c}{a long multicolumn}\\a&b\end{array}",
    "multicolumn/empty": r"\begin{array}{cc}\multicolumn{2}{c}{}\\c&d\end{array}",
    "hdotsfor/basic": r"\begin{matrix}a&b&c&d\\\hdotsfor{4}\\e&f&g&h\end{matrix}",
    "hdotsfor/coefficient": r"\begin{matrix}a&b&c\\\hdotsfor[3]{3}\end{matrix}",
    "hdotsfor/infinite": r"\begin{matrix}a&b\\\hdotsfor[1e99]{2}\end{matrix}",
    "hdotsfor/zero": r"\begin{matrix}a&b\\\hdotsfor[0]{2}\end{matrix}",
    "hdotsfor/negative": r"\begin{matrix}a&b\\\hdotsfor[-2]{2}\end{matrix}",
    "hdotsfor/text": r"\begin{matrix}a&b\\\hdotsfor[x]{2}\end{matrix}",
    "hdotsfor/one": r"\begin{matrix}a\\\hdotsfor{1}\end{matrix}",
    "hdotsfor/wide": r"\begin{matrix}aaaaaaaaaaaa&bbbbbbbbbbbbbbbbb\\\hdotsfor{2}\end{matrix}",
    "hdotsfor/outside": r"\hdotsfor{2}",
    "multirow/down": r"\begin{array}{cc}\multirow{2}{*}{x}&a\\&b\end{array}",
    "multirow/up": r"\begin{array}{cc}&a\\\multirow{-2}{*}{x}&b\end{array}",
    "multirow/up-over-rules": r"\begin{array}{cc}\hline\hline\multirow{-3}{*}{x}&b\end{array}",
    "multirow/down-over-rules": r"\begin{array}{cc}\multirow{3}{*}{x}&a\\\hline&b\\\hline&c\end{array}",
    "multirow/past-end": r"\begin{array}{cc}\multirow{20}{*}{x}&a\end{array}",
    "multirow/tall": r"\begin{array}{cc}\multirow{2}{*}{\frac{\frac{a}{b}}{\frac{c}{d}}}&a\\&b\end{array}",
    "multirow/zero": r"\begin{array}{cc}\multirow{0}{*}{x}&a\end{array}",
    "multirow/too-many": r"\begin{array}{cc}\multirow{1001}{*}{x}&a\end{array}",
    "multirow/outside": r"\multirow{2}{*}{x}",
    "multirow/under-multicolumn": r"\begin{array}{cc}\multicolumn{2}{c}{y}\\\multirow{-2}{*}{x}&b\end{array}",
    "multirow/empty": r"\begin{array}{cc}\multirow{2}{*}{}&b\\c&d\end{array}",
    "multirow/two": r"\begin{array}{ccc}\multirow{2}{*}{x}&a&\multirow{2}{*}{\sum}\\&b&\end{array}",
    "intertext/align": r"\begin{align}a&=b\intertext{and also}c&=d\end{align}",
    "intertext/primes": r"\begin{align}a&=b\intertext{f^{\prime} g^{\prime\prime}}c&=d\end{align}",
    "intertext/outside": r"\intertext{x}",
    "shove/multline": r"\begin{multline}a+b\\\shoveleft{c+d}\\\shoveright{e}\\f\end{multline}",
    "shove/empty": r"\shoveleft{}",
    "cr/top": r"a+b\\c+d\\e",
    "cr/group": r"x+{a\\b}+y",
    "cr/nested": r"{a\\{b\\c}}",
    "cr/after-over": r"a\over b\\c",
    "cr/in-script": r"\int^{a\\b}_0",
    "cr/unbalanced": r"{a\\b",
    "cr/cr": r"a\cr b",
    "cr/text": r"\text{a\\b}",
    "cr/in-cell-group": r"\begin{array}{cc}{a\\b}&c\end{array}",
    "cr/trailing": r"a\\",
    "alignat/negative": r"\begin{alignat}{-1}a&=b&c&=d&e\end{alignat}",
    "alignat/max": r"\begin{alignat}{18446744073709551615}a&=b&c&=d&e\end{alignat}",
    "alignat/overflow": r"\begin{alignat}{18446744073709551616}a&=b&c&=d&e\end{alignat}",
    "alignat/half": r"\begin{alignat}{9223372036854775808}a&=b\end{alignat}",
    "alignat/half-plus": r"\begin{alignat}{9223372036854775809}a&=b&c\end{alignat}",
    "alignat/empty": r"\begin{alignat}{}a&=b\end{alignat}",
    "alignat/hex": r"\begin{alignat}{0x2}a&=b\end{alignat}",
    "alignat/minus-zero": r"\begin{alignat}{-0}a&=b\end{alignat}",
    "alignat/blank": r"\begin{alignedat}{ 2}a&=b&c&=d\end{alignedat}",
    "multline/two-columns": r"\begin{multline}a&b\end{multline}",
    "multline/empty": r"\begin{multline}\end{multline}",
    "gather/empty-rows": r"\begin{gather}\\\end{gather}",
    "align/eats-next": r"\begin{align}a&=b\end{align}c+d",
    "smallmatrix/script": r"x^{\begin{smallmatrix}a&b\\c&d\end{smallmatrix}}",
    "matrix/in-fraction": r"\frac{\begin{matrix}a&b\\c&d\end{matrix}}{\begin{array}{|c|}\hline x\\\hline\end{array}}",
}

WIDTH_ENVIRONMENTS = {
    "align": r"\begin{align}a&=b+c&d&=e\\f&=g&h&=i\end{align}",
    "flalign": r"\begin{flalign}a&=b+c&d&=e\\f&=g&h&=i\end{flalign}",
    "flalign-pair": r"\begin{flalign}a&=b\end{flalign}",
    "alignat": r"\begin{alignat}{2}a&=b+c&d&=e\\f&=g&h&=i\end{alignat}",
    "aligned": r"\begin{aligned}a&=b+c&d&=e\\f&=g&h&=i\end{aligned}",
    "alignedat": r"\begin{alignedat}{2}a&=b+c&d&=e\end{alignedat}",
    "multline": r"\begin{multline}a+b+c\\d+e\\f+g+h\end{multline}",
    "gather": r"\begin{gather}a+b+c\\d\\\shoveright{f}\end{gather}",
    "gathered": r"\begin{gathered}a+b+c\\d\end{gathered}",
    "gather-empty": r"\begin{gather}\\a\\\end{gather}",
    "array": r"\begin{array}{cc}a&b\\\intertext{between}c&d\end{array}",
    "align-intertext": r"\begin{align}a&=b\intertext{and}c&=d\end{align}",
}


def gen_arrays():
    for name, tex in ARRAY_CASES.items():
        add(f"c1/array/{name}", tex)
    for name, tex in WIDTH_ENVIRONMENTS.items():
        for width in (4000, WIDTH):
            add(f"c1/array/width/{name}/{width}", tex, width=width)
            add(f"c1/array/width/{name}/{width}/inline", tex, display=False, width=width)


def gen_symbols(root):
    for kind, atom, name in symbols(root):
        base = f"c1/symbol/{name}"
        for style in STYLES:
            add(f"{base}/{style}", f"\\{style}\\{name}", display=False)
        add(f"{base}/scripts", f"\\{name}_a^b")
        add(f"{base}/between", f"x\\{name} y")
        add(f"{base}/inline", f"x\\{name}_1 y", display=False)


def gen_predefined_formulas(root):
    for i, name in enumerate(predefined_formulas(root)):
        tex = "\\" + name
        add(f"c1/formula/{i:03d}/display", tex)
        add(f"c1/formula/{i:03d}/inline", f"a{tex}b", display=False)
        add(f"c1/formula/{i:03d}/scripts", f"{tex}_a^b")


def gen_unicode(root):
    for code in unicode_mappings(root):
        ch = chr(code)
        add(f"c1/unicode/{code:04x}/math", ch)
        add(f"c1/unicode/{code:04x}/text", f"\\text{{a{ch}b}}")


def gen_delimiters():
    tall = "\\frac{\\frac{a}{b}}{\\frac{c}{d}}"
    for i, d in enumerate(DELIMITERS):
        base = f"c1/delimiter/{i:02d}"
        for size in SIZES:
            add(f"{base}/{size}", f"\\{size}{d} x")
        add(f"{base}/left", f"\\left{d}{tall}\\right.")
        add(f"{base}/right", f"\\left.{tall}\\right{d}")
        add(f"{base}/middle", f"\\left({tall}\\middle{d}{tall}\\right)")
        add(f"{base}/huge", f"\\left{d}\\begin{{matrix}}a\\\\b\\\\c\\\\d\\\\e\\\\f\\\\g\\end{{matrix}}\\right.")


def gen_units(root):
    for unit in units(root):
        base = f"c1/unit/{unit}"
        add(f"{base}/kern", f"a\\kern 2{unit} b")
        add(f"{base}/hspace", f"a\\hspace{{1.5{unit}}}b")
        add(f"{base}/rule", f"\\rule{{3{unit}}}{{0.5{unit}}}")
        add(f"{base}/raisebox", f"\\raisebox{{1{unit}}}{{x}}")
        add(f"{base}/negative", f"a\\kern -1{unit} b")


def gen_colors(root):
    for name in colors(root):
        add(f"c1/color/{name}/color", f"\\color{{{name}}}{{x}}")
        add(f"c1/color/{name}/colorbox", f"\\colorbox{{{name}}}{{x}}")


def gen_alphabets():
    for command in FONT_COMMANDS:
        for script, text in ALPHABETS.items():
            add(f"c1/alphabet/{command}/{script}", f"\\{command}{{{text}}}")
    for script, text in ALPHABETS.items():
        add(f"c1/alphabet/plain/{script}", text)
        add(f"c1/alphabet/scripts/{script}", f"x_{{{text}}}^{{{text}}}")
    add("c1/alphabet/lone-surrogate/math", "a\ud800b")
    add("c1/alphabet/lone-surrogate/text", "\\text{a\udc00b}")
    add("c1/alphabet/nul/text", "\\text{a\u0000b}")


def gen_breaking():
    long_sum = "+".join(f"a_{{{i}}}" for i in range(60))
    long_text = "\\text{" + " ".join(["word"] * 40) + "}"
    for width in (2000, 6000, 20000, WIDTH):
        add(f"c1/break/sum/{width}", long_sum, width=width)
        add(f"c1/break/sum-inline/{width}", long_sum, display=False, width=width)
        add(f"c1/break/text-inline/{width}", long_text, display=False, width=width)
        add(f"c1/break/everywhere/{width}", "\\breakEverywhere{true}" + long_sum, display=False, width=width)
        add(f"c1/break/newline/{width}", "a+b\\\\c+d\\\\\\text{e f g}", display=False, width=width)


def gen_sizes():
    sample = "x_{1,2}=\\frac{-b\\pm\\sqrt{b^2-4ac}}{2a}"
    for size in (64, 768, 1024, 1536, 4096):
        add(f"c1/size/{size}/display", sample, size=size, line=size / 4)
        add(f"c1/size/{size}/inline", sample, display=False, size=size, line=size / 4)
    add("c1/size/color/transparent", sample, fg=0)
    add("c1/size/color/red", sample, fg=0xFFFF0000)


def main():
    root, output = sys.argv[1], sys.argv[2]
    gen_primitives(root)
    gen_predefined_commands(root)
    gen_environments(root)
    gen_arrays()
    gen_symbols(root)
    gen_predefined_formulas(root)
    gen_unicode(root)
    gen_delimiters()
    gen_units(root)
    gen_colors(root)
    gen_alphabets()
    gen_breaking()
    gen_sizes()
    Path(output).parent.mkdir(parents=True, exist_ok=True)
    with open(output, "w", encoding="utf-8", newline="\n") as f:
        for record in records:
            try:
                line = json.dumps(record, ensure_ascii=False)
                line.encode("utf-8")
            except UnicodeEncodeError:
                line = json.dumps(record, ensure_ascii=True)
            f.write(line + "\n")
    families = {}
    for record in records:
        family = record["id"].split("/")[1]
        families[family] = families.get(family, 0) + 1
    print(f"{len(records)} records -> {output}")
    for family, count in sorted(families.items()):
        print(f"  {family}: {count}")


if __name__ == "__main__":
    main()
