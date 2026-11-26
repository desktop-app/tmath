"""Runs the C++ oracle over a corpus and writes golden digests.

The oracle prints "@ <id>" before each formula. When the process dies or
stays silent past the timeout, the formula in flight is recorded as
"oracle-crash" or "oracle-timeout" and a fresh oracle continues with the next
one. The C++ still has inputs like that, and for those the Rust engine only has
to fail cleanly.

Output, one tab-separated line per formula:
  id, kind (ok / error / draw-error / oracle-crash / oracle-timeout),
  message, metrics, digest
Messages are informational: identity does not require the same error text.

Usage: python -I oracle_run.py <corpus.jsonl> <golden.tsv>
       [--oracle <exe>] [--timeout <seconds>] [--full <dump.txt>]
"""

import argparse
import queue
import subprocess
import sys
import threading
from pathlib import Path

DEFAULT_ORACLE = Path(__file__).resolve().parent.parent / "out" / "oracle" / "Release" / (
    "microtex_oracle.exe" if sys.platform == "win32" else "microtex_oracle")


def run_batch(oracle, lines, timeout, full):
    """Runs the oracle on lines until it finishes, crashes or hangs.

    Returns (results, consumed): a list of (id, block) for finished formulas,
    a (id, kind) failure if one happened, and how many input lines were used.
    """
    args = [str(oracle)] + ([] if full else ["--digest", "--no-boxes"])
    process = subprocess.Popen(
        args,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL)
    output = queue.Queue()

    def feed():
        try:
            for line in lines:
                process.stdin.write(line.encode("utf-8") + b"\n")
            process.stdin.close()
        except (BrokenPipeError, OSError):
            pass

    def read():
        for raw in process.stdout:
            output.put(raw.decode("utf-8", "replace").rstrip("\r\n"))
        output.put(None)

    threading.Thread(target=feed, daemon=True).start()
    threading.Thread(target=read, daemon=True).start()

    results = []
    current = None
    block = []
    while True:
        try:
            line = output.get(timeout=timeout)
        except queue.Empty:
            process.kill()
            process.wait()
            return results, (current, "oracle-timeout"), len(results) + 1
        if line is None:
            code = process.wait()
            if current is not None:
                return results, (current, f"oracle-crash {code & 0xFFFFFFFF:08x}"), len(results) + 1
            return results, None, len(results)
        if line.startswith("@ "):
            current = line[2:]
            block = []
        elif line == "end":
            results.append((current, block))
            current = None
        else:
            block.append(line)


def summarize(id_, block):
    status = next((l[len("status "):] for l in block if l.startswith("status ")), "")
    kind, _, message = status.partition(" ")
    metrics = next((l[len("metrics "):] for l in block if l.startswith("metrics ")), "")
    digest = next((l[len("digest "):] for l in block if l.startswith("digest ")), "")
    return "\t".join([id_, kind, message.replace("\t", " "), metrics, digest])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("corpus")
    parser.add_argument("golden")
    parser.add_argument("--oracle", default=str(DEFAULT_ORACLE))
    parser.add_argument("--timeout", type=float, default=10.0)
    parser.add_argument("--full", help="also write full dumps to this file")
    options = parser.parse_args()

    lines = [l for l in Path(options.corpus).read_text(encoding="utf-8").splitlines() if l.strip()]
    golden = []
    dumps = []
    failures = 0
    position = 0
    while position < len(lines):
        results, failure, consumed = run_batch(
            options.oracle, lines[position:], options.timeout, full=False)
        for id_, block in results:
            golden.append(summarize(id_, block))
        if failure:
            failures += 1
            id_, kind = failure
            golden.append("\t".join([id_ or f"line{position + consumed}", kind.split(" ")[0], kind, "", ""]))
            print(f"{kind}: {id_}", file=sys.stderr)
        position += consumed
    if options.full:
        # Full dumps skip the inputs that killed the oracle above.
        bad = {row.split("\t")[0] for row in golden if row.split("\t")[1].startswith("oracle-")}
        safe = [l for l in lines if not any(f'"id": "{b}"' in l for b in bad)]
        position = 0
        while position < len(safe):
            results, failure, consumed = run_batch(
                options.oracle, safe[position:], options.timeout, full=True)
            for id_, block in results:
                dumps.append("@ " + id_)
                dumps.extend(block)
                dumps.append("end")
            position += consumed
        Path(options.full).write_text("\n".join(dumps) + "\n", encoding="utf-8", newline="\n")

    Path(options.golden).parent.mkdir(parents=True, exist_ok=True)
    Path(options.golden).write_text("\n".join(golden) + "\n", encoding="utf-8", newline="\n")
    kinds = {}
    for row in golden:
        kind = row.split("\t")[1]
        kinds[kind] = kinds.get(kind, 0) + 1
    print(f"{len(golden)} formulas -> {options.golden}: "
          + ", ".join(f"{k} {v}" for k, v in sorted(kinds.items())))


if __name__ == "__main__":
    main()
