# CPU Validation — V20 single-step oracle

The `cpu-v30mz` core is validated against the **SingleStepTests/v20** suite
(MIT-licensed, <https://github.com/SingleStepTests/v20>): thousands of
per-instruction cases, each with an initial machine state, the expected final
state, and the bus activity. The NEC V20 is the closest widely-tested relative
of the WonderSwan's V30MZ.

This is a real hardware-derived oracle — it replaces "tests I wrote assert what I
believe" with "tests a real chip's behaviour was captured into."

## Pipeline

1. Download a curated subset of `v1_native/*.json.gz` into `fixtures/v20-tests/`
   (see `fixtures/README.md`; the set is not committed).
2. `tools/v20_prep.py` decompresses and flattens each file to a compact `.tsv`
   (initial regs, initial RAM, expected regs, expected RAM). Instruction bytes
   live in the initial RAM at `CS:IP`, so the core executes straight from memory;
   the prefetch queue and cycle data are ignored (we don't model timing yet).
3. `crates/v20-harness` runs every case through one `Cpu::step` and categorises
   each as pass / state-divergence / flag-only-divergence / skipped.

Run it:

```
python tools/v20_prep.py
cargo run -p v20-harness --release
```

## Result (2026-07-13, curated ~71-opcode subset, 620k cases)

- **93.49%** exact pass over 612k runnable cases (7,595 V20-only cases skipped).
- **Zero defined-behaviour bugs.** Every divergence is one of:
  - **V20-only instructions** — `REPC`/`REPNC` (`0x64`/`0x65`) and the `0x0F`
    extension escape. These do not exist on the V30MZ (the bytes are inert), so
    the harness skips them. (Confirmed exactly: e.g. `A4`'s 1,249 skipped cases
    equal its former 1,249 divergences.)
  - **Officially-undefined flags.** Per the per-opcode flag histogram: shifts
    diverge *only* in AF (undefined for shifts) and, for multi-bit counts, OF
    (undefined for count>1); rotates diverge *only* in OF for count>1; `DIV`
    diverges in all flags (all undefined after `DIV`). Every *defined* flag
    (CF/OF-at-count-1/SF/ZF/PF and the rotate CF) matches across the board.

We deliberately do **not** match the V20's undefined-flag garbage: doing so would
assume the V30MZ behaves identically to the V20 for undefined behaviour, which is
exactly what the WonderSwan-specific WSCpuTest is for. The V30MZ's real
undefined-flag values remain an open question until then.

## Bug found and fixed

`Flags::to_word` omitted the always-1 high bits. Real 8086/V20 hardware reads
FLAGS bits 1 and 12–15 as 1 (`0xF002` in native mode), and `PUSHF`/interrupt
entry write that word to the stack. Every `INT`/`IRET` and every `DIV` `#DE`
push therefore mismatched the oracle in memory. Fixed to `0xF002`; this also
**resolves the spec's open MD-bit question** (`0x7002` vs `0xF002`) in favour of
`0xF002`, matching both the V20 oracle and the ARMV30MZ reference.

## Caveats

The V20 is not the V30MZ. It has extensions the V30MZ lacks and may differ on
undefined behaviour. This oracle rigorously validates the **documented** V30MZ
instruction set; WSCpuTest on real WonderSwan hardware remains the authority for
WonderSwan-specific timing and undefined-flag behaviour.
