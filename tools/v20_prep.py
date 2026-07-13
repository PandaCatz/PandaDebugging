#!/usr/bin/env python3
"""Flatten SingleStepTests/v20 JSON into a compact TSV the Rust harness reads.

Reads every fixtures/v20-tests/*.json.gz and writes fixtures/v20-tests/prepared/
<opcode>.tsv, one test per line:

    <14 initial regs> R <k> <addr val>*k  X <14 expected regs>  E <m> <addr val>*m

Register order: ax bx cx dx cs ss ds es sp bp si di ip flags. Expected regs are
the initial set overridden by the test's `final.regs` (which lists only changed
registers). Instruction bytes live in the initial RAM at CS:IP, so the harness
executes straight from memory and ignores the prefetch queue and cycle data.
"""
import gzip
import json
import os
import glob

SRC = os.path.join(os.path.dirname(__file__), "..", "fixtures", "v20-tests")
OUT = os.path.join(SRC, "prepared")
REGS = ["ax", "bx", "cx", "dx", "cs", "ss", "ds", "es", "sp", "bp", "si", "di", "ip", "flags"]


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    files = sorted(glob.glob(os.path.join(SRC, "*.json.gz")))
    total = 0
    for path in files:
        name = os.path.basename(path)[: -len(".json.gz")]
        with gzip.open(path) as fh:
            data = json.load(fh)
        lines = []
        for t in data:
            init = t["initial"]["regs"]
            merged = dict(init)
            merged.update(t["final"]["regs"])
            iram = t["initial"].get("ram", [])
            eram = t["final"].get("ram", [])
            parts = [str(init[r]) for r in REGS]
            parts += ["R", str(len(iram))]
            for a, v in iram:
                parts += [str(a), str(v)]
            parts.append("X")
            parts += [str(merged[r]) for r in REGS]
            parts += ["E", str(len(eram))]
            for a, v in eram:
                parts += [str(a), str(v)]
            lines.append(" ".join(parts))
        with open(os.path.join(OUT, name + ".tsv"), "w") as f:
            f.write("\n".join(lines))
        total += len(data)
        print(f"{name}: {len(data)}")
    print("TOTAL cases:", total)


if __name__ == "__main__":
    main()
