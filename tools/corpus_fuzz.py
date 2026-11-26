"""Generates random formulas for differential fuzzing (C4 in PLAN.md).

Formulas nest built-in commands, symbols, predefined formulas, environments,
scripts, groups, colors and lengths drawn from the oracle's table dump, with
arguments of every shape, so that the oracle and the engine can be compared on
inputs no hand-written corpus has. The same seed always gives the same file.

Usage: python -I corpus_fuzz.py <tables.txt> <MicroTeX checkout> <seed> <count> <output.jsonl>
"""

import json
import random
import re
import sys
from pathlib import Path


def units(text):
    return "".join(chr(int(h, 16)) for h in text.strip("[]").split())


def load_tables(path):
    macros, symbols, formulas, colors = [], [], [], []
    for line in open(path, encoding="utf-8"):
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "macro":
            j = line.index("]")
            name = units(line[line.index("["):j + 1])
            rest = line[j + 1:].split()
            macros.append((name, int(rest[0]), int(rest[1])))
        elif parts[0] == "symbol":
            symbols.append(parts[1])
        elif parts[0] == "formula":
            j = line.index("]")
            formulas.append(units(line[line.index("["):j + 1]))
        elif parts[0] == "color":
            colors.append(parts[1])
    return macros, symbols, formulas, colors


def load_environments(root):
    text = (Path(root) / "src/core/macro_def.cpp").read_text(encoding="utf-8")
    return [(m.group(2), int(m.group(1)))
            for m in re.finditer(r'env\((\d+),\s*L"([^"]+)"', text)]


SKIP = {"debug", "undebug", "begin", "end", "includegraphics"}
UNITS = ["pt", "em", "ex", "mu", "px", "cm", "mm", "in", "sp", "pc", "dd", "cc", "bp", "x8", "pix", "point"]
LETTERS = "abcxyzABXYZ0123456789+-=()[]<>,.;:!?|/' "
UNICODE = "αβж中é∑∫→°’‵١\U0001d400"
SPECS = ["c", "lcr", "|c|c|", "@{}l@{x}r@{}", "*{3}{c}", ">{\\color{red}}c", "p{2em}c", "cc|cc"]


class Gen:
    def __init__(self, rng, tables, environments):
        self.rng = rng
        self.macros, self.symbols, self.formulas, self.colors = tables
        self.macros = [m for m in self.macros if m[0] not in SKIP and "@" not in m[0]]
        self.environments = environments

    def length(self):
        r = self.rng
        value = r.choice(["0", "1", "2.5", "-1", "0.3", "10", "1e3", "-0.5", "", "x", "1e39"])
        return value + r.choice(UNITS + [""])

    def color(self):
        r = self.rng
        return r.choice(self.colors + ["#FF0000", "0.5", "1,0,0", "0.1,0.2,0.3,0.4", "zz", ""])

    def atom(self, depth):
        r = self.rng
        kind = r.random()
        if depth <= 0 or kind < 0.35:
            pick = r.random()
            if pick < 0.6:
                return r.choice(LETTERS)
            if pick < 0.85:
                return "\\" + r.choice(self.symbols) + " "
            if pick < 0.95:
                return r.choice(UNICODE)
            return "\\" + r.choice(self.formulas) + " "
        if kind < 0.75:
            return self.command(depth - 1)
        if kind < 0.85:
            return "{" + self.row(depth - 1) + "}"
        if kind < 0.95:
            base = self.atom(depth - 1)
            scripts = r.choice(["^", "_", "^_", "_^", "'", "''"])
            out = base
            for s in scripts:
                if s == "'":
                    out += "'"
                else:
                    out += s + "{" + self.row(depth - 1) + "}"
            return out
        return self.environment(depth - 1)

    def argument(self, depth):
        r = self.rng
        shape = r.random()
        if shape < 0.1:
            return "{}"
        if shape < 0.2:
            return "{" + self.length() + "}"
        if shape < 0.27:
            return "{" + self.color() + "}"
        if shape < 0.35:
            return r.choice(LETTERS.strip())
        return "{" + self.row(depth) + "}"

    def command(self, depth):
        r = self.rng
        name, argc, opts = r.choice(self.macros)
        out = "\\" + name
        if not name[-1:].isalpha():
            out += ""
        if opts and r.random() < 0.5:
            out += "[" + r.choice([self.length(), self.color(), "l", "r", "c", "t", "b", "origin=cc", "2", ""]) + "]"
        for _ in range(argc):
            out += self.argument(depth)
        if r.random() < 0.05:
            out += self.argument(depth)
        return out + (" " if name[-1:].isalpha() else "")

    def environment(self, depth):
        r = self.rng
        name, argc = r.choice(self.environments)
        out = "\\begin{" + name + "}"
        for _ in range(argc):
            out += "{" + (r.choice(SPECS) if name in ("array", "tabular") else str(r.randint(0, 3))) + "}"
        rows = r.randint(1, 3)
        cols = r.randint(1, 3)
        body = "\\\\".join("&".join(self.row(depth) for _ in range(cols)) for _ in range(rows))
        if r.random() < 0.2:
            body = "\\hline " + body + "\\\\\\hline"
        return out + body + "\\end{" + name + "}"

    def row(self, depth):
        r = self.rng
        return "".join(self.atom(depth) for _ in range(r.randint(0, 4)))

    def formula(self):
        r = self.rng
        prefix = r.choice(["", "\\displaystyle ", "\\textstyle ", "\\scriptstyle ", "$", "\\text{"])
        body = self.row(r.randint(1, 5))
        if prefix == "$":
            body += "$"
        if prefix == "\\text{":
            body += "}"
        return prefix + body


def main():
    tables, root, seed, count, output = sys.argv[1:6]
    rng = random.Random(int(seed))
    gen = Gen(rng, load_tables(tables), load_environments(root))
    with open(output, "w", encoding="utf-8", newline="\n") as f:
        for i in range(int(count)):
            record = {"id": f"c4/{seed}/{i}", "tex": gen.formula()}
            if rng.random() < 0.2:
                record["width"] = rng.choice([500, 2000, 6000, 38400])
            f.write(json.dumps(record, ensure_ascii=True) + "\n")
    print(f"{count} records -> {output}")


if __name__ == "__main__":
    main()
