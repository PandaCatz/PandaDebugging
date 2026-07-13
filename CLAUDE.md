# Claude Project Handoff — wonderswan-emu

Canonical cross-session handoff. Read this file, `ROADMAP.md`, and
`docs/ARCHITECTURE.md` before changing code. Update this file at the end of every
implementation session.

## Mission

Build a deterministic, independently testable WonderSwan / WonderSwan Color
(ASWAN / SPHINX / SPHINX2) core in Rust that fixes the accuracy bugs documented
in `docs/hardware/00-overview.md`. Accuracy is proven by hardware test ROMs and
reference-emulator oracle diffs, never asserted by eye.

## Non-negotiable rules

- Run and measure before claiming behaviour works. A command is not evidence
  unless its observed result is recorded here.
- The core simulates in exact integer master-clock ticks. It never sleeps, reads
  wall time, opens host devices, or waits for VSync.
- `format-ws` accepts bytes and returns validated structures. The core consumes
  the validated view and never re-interprets raw images.
- All external data (ROMs, BIOS, save data, config) is hostile: checked
  arithmetic, size limits, non-panicking errors, oversize/truncation tests.
- Keep BIOS, cartridge dumps, test-ROM binaries, save data, and operator paths
  out of the repository and out of logs. See `docs/LEGAL_PROVENANCE.md`.
- No `unsafe` without a measured release bottleneck, documented invariants,
  focused tests/fuzzing, and a safe baseline.
- Warnings are errors. Do not start a new subsystem with a red current gate.
- Do not fabricate hardware facts. An unknown register/offset stays an explicit
  gap until verified against WSMan or a hardware test — never a plausible guess.

## Current state

Phase 1 (headless skeleton) is complete and green. The workspace contains:

- `ws-contracts`: deterministic contracts — `EmulatedTime`, `ClockRate`, typed
  `VideoFrame`/`AudioPacket`, `InputEvent`, the `Core`/`OutputSink` traits, and a
  non-panicking `CoreError`. WonderSwan `Model` (Mono/Color/Crystal) replaces the
  NES region concept.
- `format-ws`: defensive borrowed ROM parser. Structural validation, footer
  extraction, stored/provisional checksum. Exact header-field offsets are
  deliberately *not yet decoded* (Phase 0 task) rather than guessed.
- `cpu-v30mz`: register file, flags (defined bits; MD read-back left open),
  20-bit segmented addressing, `CS:IP = FFFF:0000` reset, the trace-first
  `CpuBus`, instruction fetch, ModR/M decode (all 16-bit modes + segment
  override), the ALU (8/16-bit, full flag semantics), and a `step()` executor
  running a large opcode subset: ALU + GRP1 immediate, MOV (all forms/seg/LEA/
  moffs/imm), XCHG, INC/DEC r16, TEST, CBW/CWD, SALC, stack (PUSH/POP/PUSHF/
  POPF), control flow (Jcc/JMP/CALL/RET/LOOP), and flag/NOP/HLT.
  Remaining: GRP2 shifts, GRP3 MUL/DIV/NOT/NEG, GRP4/5 (indirect CALL/JMP/PUSH),
  string ops + REP, INT/IRET + interrupt delivery, IN/OUT — and **no timing**
  yet (blocked on the cycle-unit question).
- `core-ws`: cartridge ownership boundary + I/O register map (doc-cited
  addresses) + a fully unit-tested interrupt-controller model (8 lines,
  edge-vs-level semantics, bit-priority selection, relocatable vector base).
- `ws-testkit`: deterministic synthetic core, capture sink, stable FNV-64 hashes.
- `ws-cli`: headless synthetic run + `--rom` inspector (no ROM bytes logged).

Not implemented yet: the V30MZ opcode decoder/executor and interrupt-delivery
sequence, the memory bus and access-slot model, the PPU, the APU, the
general-purpose and sound DMA engines, EEPROM/RTC/serial devices, the BIOS boot
path, save/state, and any frontend. This crate set is **not** a running
WonderSwan core.

## Completed work

- Scaffolded a 5-crate Cargo workspace mirroring the `universal-retro-emulator`
  conventions (deterministic time, hostile-input parsers, no-`unsafe`,
  warnings-as-errors).
- Pinned Rust 1.96.0; `unsafe_code = "forbid"` at the workspace.
- Encoded the deep-dive's interrupt facts as executable, tested behaviour:
  bit-priority ordering (HBLANK_TMR highest), edge-vs-level trigger table, and
  ack-clears-edge-only / lower-clears-level-only semantics.
- Wrote the legal provenance policy, the phased roadmap, and the architecture
  baseline.
- Ran a six-agent, adversarially-verified research pass into the Phase-2 CPU
  spec: `docs/hardware/01-cpu-v30mz.md` (opcodes, timing, flags/exceptions,
  memory & I/O map, validation plan) and `02-interrupts.md`. Definitive fixes
  folded into the bodies; unverified items collected in each doc's appendix.
- Git: local `main` pushed to `https://github.com/PandaCatz/PandaDebugging`
  (public). Push major milestones there; no Co-Authored-By / AI trailer;
  identity `PandaCatz <PandaCatz@users.noreply.github.com>`.

## Key open questions before writing CPU timing

- **Cycle-unit ambiguity (blocker for timing literals):** unknown whether the
  LFSR cycle-counter ticks at 12.288 MHz (master) or 3.072 MHz (CPU). Resolve by
  measuring a known-`n` DMA before baking any timing constant.
- `IN`/`OUT` cost: WSdev table says 6 core cycles; the deep-dive says 12. Do not
  hardcode either — parameterize and measure.
- Post-DIV/IDIV flag state and `#DE` return-address semantics: implementation-
  specific; confirm against WSCpuTest on both ASWAN and SPHINX.
- Several WSMan/Sacred-Tech-Scroll facts were confirmed via WSdev/ARMV30MZ/ares
  because the HTTP-only primaries refused HTTPS; re-verify over plain HTTP.

## Required commands

Run from `H:\claaaude\wonderswan-emu`:

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo test --release --workspace
cargo run --release -p ws-cli
```

## Latest verified results

Verified on Windows x86-64 with Rust/Cargo 1.96.0 on 2026-07-13:

- `cargo fmt --all -- --check` — pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — pass.
- `cargo test --workspace --all-targets --all-features` — 79 passed, 0 failed
  (cpu-v30mz 56, core-ws 9, format-ws 6, ws-testkit 5, ws-contracts 3, ws-cli 0).
- `cargo test --release --workspace` — 79 passed, 0 failed.
- `cargo run --release -p ws-cli` — synthetic baseline:
  - final tick: `30`
  - video: `3` frames, hash `2d1f1e3d37030229`
  - audio: `7` packets / `28` frames, hash `b47aa59f8351ec23`
  - ordered event stream hash: `f7b04e5e9749b6f8`

## Next tasks, in order

See `ROADMAP.md` for the full phase plan and exit gates. Immediate Phase 0/2
work:

1. Continue the `cpu-v30mz` opcode table: GRP2 shifts/rotates, GRP3
   (`MUL`/`DIV`/`IMUL`/`IDIV`/`NOT`/`NEG`), GRP4/5 (indirect `CALL`/`JMP`/`PUSH`,
   `INC`/`DEC` r/m), string ops + `REP`, `INT`/`INTO`/`IRET` with the interrupt-
   delivery sequence, and `IN`/`OUT`. Then run WSCpuTest headless. Keep timing
   out until the cycle-unit question is resolved. (ALU+GRP1, MOV/XCHG, INC/DEC,
   TEST, stack, and control flow already done.)
2. Add the interrupt-delivery sequence and wire `core-ws::InterruptController`
   to the CPU (IVT at `REG_INT_BASE`, push flags/CS/IP, clear IF/TF).
3. Acquire the hardware test ROMs into gitignored `fixtures/` (`docs/TEST_ROMS.md`)
   and stand up the headless test-ROM runner in `ws-testkit`; validate opcodes
   against WSCpuTest (auto-runs on boot) and interrupts against WSHWTest.
4. Resolve the cycle-unit ambiguity via an LFSR/DMA measurement, then add timing.
5. Transcribe and decode the ROM header fields in `format-ws` (Phase 0), with
   oversize/truncation tests.

## Decisions still open

- Whether to build test ROMs from source (wonderful-toolchain) or consume
  prebuilt release binaries. See `docs/TEST_ROMS.md`.
- Reference oracle choice for framebuffer/audio diffs (ares vs Mednafen).
- Whether Linux is a release target or only a CI target.

## Honest limitations

The synthetic core proves the shared contract and headless capture path only —
it is not console emulation. `core-ws` proves the parser-to-runtime boundary and
an isolated interrupt model; it does not execute WonderSwan code.
