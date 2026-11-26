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
