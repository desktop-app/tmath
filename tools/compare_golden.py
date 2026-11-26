"""Compares an engine run (oracle_run.py --oracle tmath_compare) with a golden.

A formula matches when the kind (ok / error) is the same and, for formulas
that render, the metrics and the digest of the measure and draw calls are the
same. Error messages are not compared. Formulas the engine reports as
"unsupported" use parts of MicroTeX not ported yet; they are counted by what
they need, so the next milestone can be picked from the list.

Usage: python -I compare_golden.py <golden.tsv> <engine.tsv> [--show N]
"""

import argparse
import collections
from pathlib import Path


def load(path):
    rows = {}
    for line in Path(path).read_text(encoding="utf-8").splitlines():
        fields = line.split("\t")
        fields += [""] * (5 - len(fields))
        rows[fields[0]] = fields
    return rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("golden")
    parser.add_argument("engine")
    parser.add_argument("--show", type=int, default=20)
    options = parser.parse_args()
    golden = load(options.golden)
    engine = load(options.engine)

    matched = 0
    unsupported = collections.Counter()
    mismatches = []
    for id_, g in golden.items():
        e = engine.get(id_)
        if e is None:
            mismatches.append((id_, "missing", g, ["", "", "", "", ""]))
            continue
        if e[1] == "unsupported":
            unsupported[e[2]] += 1
            continue
        if g[1].startswith("oracle-"):
            # The C++ crashed or hung; the engine only has to fail cleanly.
            if e[1] in ("ok", "error"):
                matched += 1
            else:
                mismatches.append((id_, "oracle-failure", g, e))
            continue
        if g[1] != e[1]:
            mismatches.append((id_, "kind", g, e))
        elif g[1] == "ok" and (g[3] != e[3] or g[4] != e[4]):
            mismatches.append((id_, "output", g, e))
        else:
            matched += 1

    total = len(golden)
    print(f"{matched} of {total} match, {sum(unsupported.values())} unsupported, "
          f"{len(mismatches)} differ")
    if mismatches:
        print("\ndiffering:")
        for id_, why, g, e in mismatches[:options.show]:
            print(f"  {id_} [{why}]")
            print(f"    oracle: {g[1]} {g[2]} | {g[3]} | {g[4]}")
            print(f"    engine: {e[1]} {e[2]} | {e[3]} | {e[4]}")
    if unsupported:
        print("\nunsupported, by what they need:")
        for what, count in unsupported.most_common(options.show):
            print(f"  {count:5} {what}")
    return 1 if mismatches else 0


if __name__ == "__main__":
    raise SystemExit(main())
