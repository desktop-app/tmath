"""Builds corpus/adversarial.jsonl: formulas of up to 32K characters
(tdesktop's kMaxFormulaTexLength) at or just under the C++ limits, for the
worst-case time gate in PLAN.md. Usage: python -I corpus_adversarial.py <output.jsonl>
"""

import json
import sys

L = 32 * 1024
records = []


def add(name, tex):
    records.append({"id": name, "tex": "\\displaystyle " + tex[:L]})


add("scripts-32k", "x^{2}_{i}+" * 4000)
add("frac-flat-32k", "\\frac{a}{b}+" * 3000)
add("sqrt-nest-40x", ("\\sqrt{" * 40 + "x" + "}" * 40 + "+") * 300)
add("frac-nest-40x", ("\\frac{1}{" * 40 + "x" + "}" * 40 + "+") * 120)
add("matrix-100x150", "\\begin{matrix}" + ("a&" * 149 + "a\\\\") * 100 + "\\end{matrix}")
add("matrix-frac-60x30", "\\begin{pmatrix}" + ("\\frac{a}{b}&" * 29 + "x\\\\") * 60 + "\\end{pmatrix}")
add("macro-expand", "\\newcommand{\\a}{xyzxyzxyzxyzxyzx}\\newcommand{\\b}{\\a\\a\\a\\a\\a\\a\\a\\a}\\newcommand{\\c}{\\b\\b\\b\\b\\b\\b\\b\\b}\\newcommand{\\d}{\\c\\c\\c\\c\\c\\c\\c\\c}" + "\\d" * 200)
add("text-cyr-32k", "\\text{" + "привет мир " * 2900 + "}")
add("left-right-nest", ("\\left(" * 40 + "x" + "\\right)" * 40 + "+") * 150)
add("bigops", "\\sum_{i=1}^{n}\\prod_{j}\\int_0^1 " * 900)
add("binom-many", "\\binom{a}{b}" * 2600)
add("color-box-many", "\\fcolorbox{red}{blue}{\\rotatebox{30}{x}}" * 800)

with open(sys.argv[1], "w", encoding="utf-8", newline="\n") as f:
    for record in records:
        f.write(json.dumps(record, ensure_ascii=False) + "\n")
print(len(records))
