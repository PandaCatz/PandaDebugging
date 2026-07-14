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
  extraction, verified checksum, and a fully-decoded typed footer (`CartHeader`):
  publisher, system, game id, version, ROM-size/save-type code tables, flags
  (orientation + bus width, bug #9), mapper/RTC, boot far-jump, checksum. Layout
  adversarially verified vs WSMan/WSdev/ares/Mednafen; see
  `docs/hardware/06-cartridge.md`. Undocumented codes decode to explicit
  `Other`/`Unknown`/`None`, never guessed.
- `cpu-v30mz`: register file, flags (defined bits; MD read-back left open),
  20-bit segmented addressing, `CS:IP = FFFF:0000` reset, the trace-first
  `CpuBus`, instruction fetch, ModR/M decode (all 16-bit modes + segment
  override), the ALU (8/16-bit, full flag semantics), and a `step()` executor
  running the **full documented 8086/80186 instruction set** as used on the
  V30MZ: ALU (+ GRP1/GRP3), MOV/XCHG/LEA, INC/DEC, TEST, CBW/CWD, SALC,
  MUL/IMUL/DIV/IDIV, GRP2 shifts/rotates, GRP4/5 (indirect CALL/JMP/PUSH), stack,
  string ops + REP, control flow, IN/OUT, INT/INTO/IRET with the interrupt-
  delivery sequence (service_interrupt, IVT at physical vector*4), and
  flag/NOP/HLT.
  Remaining: hardware IRQ delivery (the machine must consult
  core-ws::InterruptController before each step), a few V30MZ-undocumented slots
  (e.g. 0xF1), and **all cycle timing** (blocked on the cycle-unit question).
- `core-ws`: cartridge boundary + `memory::MemoryMap` (the **real** address-
  routing map) + interrupt-controller model + a `Machine` (CPU + bus + interrupt
  controller) with hardware-IRQ delivery that **boots from cartridge ROM via the
  reset vector**, plus the audited community-bug subsystems: `apu` (noise LFSR,
  #4), `serial` (UART, #3), `palette` (pool + color-zero, #5/#6), `eeprom` (#8),
  `ppu::SpriteUnit` (sprite double-buffer, #2). The memory map routes internal
  RAM (16 KiB mono / 64 KiB colour), the cartridge ROM/SRAM bank windows
  (`$C0`–`$C3`), the I/O three-way decode, `$A0` system control, and open-bus
  reads — from the decoded `CartHeader`. **The community-bug subsystems are still
  isolated and unit-tested — not yet wired to the map's I/O dispatch.**
- `ws-testkit`: deterministic synthetic core, capture sink, stable FNV-64 hashes.
- `ws-cli`: headless synthetic run + `--rom` inspector (no ROM bytes logged).
- `v20-harness`: runs the V20 single-step oracle against `cpu-v30mz`.

Not implemented yet: wiring the fixed subsystems to the memory map's I/O
dispatch, the boot-ROM overlay, the internal-EEPROM/RTC register protocols, the
DMA engines, pixel rendering, save/state, timing, and any frontend. This is
**not** a playable emulator yet.

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
- Milestones are tagged `v0.MINOR.0` (annotated) and cut as GitHub Releases:
  v0.1.0 skeleton, v0.2.0 V30MZ set, v0.3.0 machine+IRQ, v0.4.0 V20-validated,
  v0.5.0 community-bug fixes. Tag the next milestone the same way.
- CI: `.github/workflows/ci.yml` runs the full required-commands gate (fmt,
  clippy, debug+release tests, ws-cli smoke) on every push/PR on Rust 1.96.0.
  Keep it green; a red CI is a red gate.

## Key open questions before writing CPU timing

- **Cycle-unit ambiguity — RESOLVED (2026-07-14):** measured timings are CPU
  cycles (3.072 MHz), no ×4. trap15's "master clock" = 3.072 MHz (WSMan), not the
  12.288 MHz crystal; corroborated by ares/Mednafen, `XCHG` 3/3=1.0, and a physics
  floor. A master-clock scheduler ×4's them. Resolved by a 9-agent research
  workflow — no hardware needed. See `01-cpu-v30mz.md` preamble. (Still open,
  *independent of the unit:* the `IN`/`OUT` value and the sprite-DMA formula.)
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

Verified on Windows x86-64 with Rust/Cargo 1.96.0 on 2026-07-14:

- `cargo fmt --all -- --check` — pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — pass.
- `cargo test --workspace --all-targets --all-features` — 153 passed, 0 failed
  (cpu-v30mz 85, core-ws 46, format-ws 14, ws-testkit 5, ws-contracts 3, ws-cli 0).
- `cargo test --release --workspace` — 153 passed, 0 failed.
- Real memory map (`core-ws::memory`): internal RAM per model, cartridge ROM/SRAM
  bank windows, I/O three-way decode, `$A0`, open-bus. `Machine::with_cartridge`
  boots from ROM via the reset vector (test `boots_from_cartridge_rom_via_the_reset_vector`).
  Adversarial review caught + fixed a `$C0` power-up bug (reset vector missed the
  footer on ROMs > 1 MiB); locked with a multi-size test.
- Cartridge footer layout (bug #9) verified by a 12-agent research workflow
  (5 source finders → reconcile → 6 adversarial verifiers over WSMan/WSdev/ares/
  Mednafen/STSWS): fixed the bus-width bit to footer `0x0C` bit 2 (`0`=8/`1`=16)
  and corrected save-code `0x01` to 32 KiB. `ws-cli --rom` on a crafted footer
  decodes every field and validates the checksum. See docs/hardware/06-cartridge.md.
- Community-bug fixes adversarially audited vs WSMan/WSdev/ares (14-agent
  workflow); corrected EEPROM size (128B), color-zero (bit-depth axis), the
  interrupt vector mask + enable-gated raise, LFSR enable-gating, sprite-DMA
  double-buffering, and the palette colour-0 write-protect. See docs/COMMUNITY-BUGS.md.
- `cargo run -p v20-harness --release` — V20 single-step oracle: 93.49% exact
  pass over 612k runnable cases, **zero defined-behaviour bugs** (all divergences
  are V20-only instructions or officially-undefined flags). See
  `docs/VALIDATION.md`. Found + fixed the `Flags::to_word` high-bits bug
  (resolves the MD-bit open question → `0xF002`).
- `cargo run --release -p ws-cli` — synthetic baseline:
  - final tick: `30`
  - video: `3` frames, hash `2d1f1e3d37030229`
  - audio: `7` packets / `28` frames, hash `b47aa59f8351ec23`
  - ordered event stream hash: `f7b04e5e9749b6f8`

## Next tasks, in order

The driving goal is the community-bug ledger (`docs/COMMUNITY-BUGS.md`): 6 fixed,
3 partial, 0 remaining, all adversarially audited/verified vs WSMan/WSdev/ares.

Done so far: full V30MZ instruction set (V20-validated, zero defined-behaviour
bugs — `docs/VALIDATION.md`); `core-ws::Machine` with hardware-IRQ delivery over
the **real address-routing memory map** (`memory::MemoryMap`) that boots from
cartridge ROM via the reset vector; the verified cartridge footer decode (bug #9)
in `format-ws`; and the cycle-unit ambiguity **resolved** (CPU cycles, no ×4).
The six community-bug subsystems (`apu`, `serial`, `palette`, `eeprom`, `ppu`)
are still **isolated modules, not yet wired to the map's I/O dispatch**.

Next pieces, in order:

1. **Wire the fixed subsystems to the memory map's I/O dispatch** so the
   community-bug fixes run inside the real machine (APU ports, palette/PPU regs,
   serial `$B0`-area regs, internal-EEPROM `$B8`–`$BF` protocol). The map's
   `io_read`/`io_write` currently model `$A0` + the `$C0`–`$C3` banks and leave
   the rest as explicit open-bus gaps.
2. **Per-instruction timing** — now unblocked (unit resolved to CPU cycles).
   Build the master-clock scheduler (CPU cost ×4), closing bugs #1 and #7 once
   the `IN`/`OUT` value is measured.
3. **Acquire WSCpuTest** (wf-toolchain build or a prebuilt `.ws`) and boot it on
   the machine — the authority for the still-open flag questions (shift AF, DIV,
   GRP2 count-mask, `0x0F`/`0x64`–`0x67` inert-NOP) and the #3 serial semantics.
4. Later structural work: the boot-ROM overlay, the DMA engines, PPU pixel
   rendering, RTC.

## Decisions still open

- Whether to build test ROMs from source (wonderful-toolchain) or consume
  prebuilt release binaries. See `docs/TEST_ROMS.md`.
- Reference oracle choice for framebuffer/audio diffs (ares vs Mednafen).
- Whether Linux is a release target or only a CI target.

## Honest limitations

The synthetic core proves the shared contract and headless capture path only —
it is not console emulation. `core-ws` proves the parser-to-runtime boundary and
an isolated interrupt model; it does not execute WonderSwan code.
