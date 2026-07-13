# Project State

Snapshot of what exists and what is honestly *not* yet real. Update alongside
`CLAUDE.md` at the end of each session.

## Exists and verified (2026-07-13)

- 5-crate Cargo workspace; Rust 1.96.0 pinned; `unsafe` forbidden;
  warnings-as-errors. All gates green: fmt, clippy `-D warnings`, 23 tests in
  debug and release.
- `ws-contracts`: deterministic time, typed video/audio/input packets,
  `Core`/`OutputSink` traits, non-panicking `CoreError`.
- `format-ws`: structural ROM validation, footer extraction, stored/provisional
  checksum. Header fields **not yet decoded** (deliberate — Phase 0).
- `core-ws`: cartridge ownership boundary, I/O address map (doc-cited only),
  interrupt-controller model with full edge/level + priority tests.
- `ws-testkit`: synthetic core, capture sink, stable FNV-64 hashes.
- `ws-cli`: synthetic baseline + `--rom` inspector.
- Local git repo on `main`, no remote.

## Not real yet

V30MZ CPU, memory bus / access slots, PPU, APU, DMA/SDMA, EEPROM, cartridge RTC,
serial/UART, keypad, BIOS boot path, save/state, frontend. No commercial game or
hardware test ROM has been executed — there is no CPU to execute it.

## Immediate gaps to close (Phase 0 → 2)

1. Decode the ROM header from WSMan in `format-ws` (with oversize/truncation
   tests).
2. Acquire and record provenance for the V30MZ and interrupt/timer test ROMs.
3. Stand up `cpu-v30mz` with a trace-first bus and generated opcode tests.
