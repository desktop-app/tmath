"""Builds corpus/samples.jsonl from MicroTeX's res/SAMPLES.tex and the
formula editor samples, prepared the way tdesktop prepares a display formula
(iv_markdown_microtex.cpp, PreparedTeX): full-width environments rewritten to
their inner variants and a "\\displaystyle " prefix.

Usage: python -I corpus_samples.py <MicroTeX root> <output.jsonl>
"""

import json
import sys
from pathlib import Path

REWRITES = [
    ("align*", "aligned"),
    ("align", "aligned"),
    ("flalign*", "aligned"),
    ("flalign", "aligned"),
    ("alignat*", "alignedat"),
    ("alignat", "alignedat"),
    ("gather*", "gathered"),
    ("gather", "gathered"),
    ("multline*", "gathered"),
    ("multline", "gathered"),
    ("equation*", "gathered"),
    ("equation", "gathered"),
]

EDITOR_SAMPLES = [
    r"e^{i\pi}=-1",
    r"x^n+y^n=z^n",
    r"\sin^2\alpha+\cos^2\alpha=1",
    r"x_{1,2}=\frac{-b\pm\sqrt{b^2-4ac}}{2a}",
]


def prepared(tex, display=True):
    tex = tex.strip()
    if "\\begin{" in tex:
        for source, target in REWRITES:
            tex = tex.replace("\\begin{" + source + "}", "\\begin{" + target + "}")
            tex = tex.replace("\\end{" + source + "}", "\\end{" + target + "}")
    return ("\\displaystyle " + tex) if display else tex


def samples(root):
    text = (Path(root) / "res" / "SAMPLES.tex").read_text(encoding="utf-8")
    result = []
    current = []
    title = ""
    for line in text.splitlines():
        if line.startswith("%%%%"):
            if current:
                result.append((title, "\n".join(current)))
            current = []
            title = ""
        elif line.startswith("%") and not current and not title:
            title = line[1:].strip()
        else:
            current.append(line)
    if any(line.strip() for line in current):
        result.append((title, "\n".join(current)))
    return result


def main():
    root, output = sys.argv[1], sys.argv[2]
    records = []
    for index, (title, tex) in enumerate(samples(root)):
        slug = "".join(c if c.isalnum() else "-" for c in title.lower()).strip("-")
        for display in (True, False):
            records.append({
                "id": f"samples/{index:02d}-{slug}/{'display' if display else 'inline'}",
                "tex": prepared(tex, display),
            })
    for index, tex in enumerate(EDITOR_SAMPLES):
        for display in (True, False):
            records.append({
                "id": f"editor/{index}/{'display' if display else 'inline'}",
                "tex": prepared(tex, display),
            })
    Path(output).parent.mkdir(parents=True, exist_ok=True)
    with open(output, "w", encoding="utf-8", newline="\n") as f:
        for record in records:
            f.write(json.dumps(record, ensure_ascii=False) + "\n")
    print(f"{len(records)} records -> {output}")


if __name__ == "__main__":
    main()
