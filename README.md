# wonderswan-emu

A deterministic, independently testable WonderSwan / WonderSwan Color emulator
core in Rust, built to be *accurate first*: every hardware behaviour is measured
or cited before it is claimed to work.

The project exists to fix the specific accuracy bugs that recur across existing
WonderSwan emulators — V30MZ CPU/interrupt timing, sprite DMA tearing, the
monochrome palette pool, color-zero handling, the noise LFSR, sound mixing, DMA
cycle cost, EEPROM/RTC/serial edge cases — with each fix backed by a hardware
test ROM or a reference-emulator oracle diff, never by "it looks right."

## Status

**Current focus:** Phase 2 — the NEC V30MZ opcode decoder/executor.

Verified on Rust/Cargo 1.96.0 (Windows x86-64): `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, and **94 tests** all pass in debug
and release. No `unsafe`; warnings are errors.

| Phase | What | Status |
|------:|------|--------|
| 0 | Charter, toolchain, fixtures, provenance | 🔨 in progress (toolchain + provenance done; ROM-header decode & test-ROM acquisition pending) |
| 1 | Headless skeleton (contracts, parser, testkit, CLI) | ✅ complete |
| 2 | V30MZ CPU + interrupt/bus timing | 🔨 in progress — [specs](docs/hardware/) done + verified; `step()` runs most of the instruction set: ALU+GRP1, MOV/`XCHG`, `INC`/`DEC`, `TEST`, `MUL`/`IMUL`, `NOT`/`NEG`, indirect group ops, stack, string ops + `REP`, control flow, and `IN`/`OUT`. Remaining: GRP2 shifts, `DIV`/`IDIV`, `INT`/`IRET` + interrupt delivery, then timing |
| 3 | PPU / display (sprite DMA @142, palettes, color-zero) | ⬜ planned |
| 4 | APU / sound (unsigned mixing, LFSR-in-wave-mode, sweep) | ⬜ planned |
| 5 | DMA / SDMA (`5 + 2n`, CPU halt, cart-SRAM source) | ⬜ planned |
| 6 | Cartridge / EEPROM / RTC / serial / bus width / input | ⬜ planned |
| 7 | BIOS boot path | ⬜ planned |

Detailed exit gates are in [`ROADMAP.md`](ROADMAP.md); the running state and
verified evidence are in [`CLAUDE.md`](CLAUDE.md).

> **Open blocker for timing:** it is unresolved whether the hardware LFSR
> cycle-counter ticks at the 12.288 MHz master clock or the 3.072 MHz CPU clock —
> a 4× factor that scales every measured timing (DMA `5+2n`, `IN`/`OUT`,
> per-instruction). No timing literal is baked until it is measured. See the
> preamble of [`docs/hardware/01-cpu-v30mz.md`](docs/hardware/01-cpu-v30mz.md).

## Layout

| Crate | Role | State |
|-------|------|-------|
| `ws-contracts` | Deterministic API: integer emulated time, typed video/audio/input packets, the `Core`/`OutputSink` traits, non-panicking errors. | ✅ |
| `format-ws` | Defensive, borrowed parser for `.ws` / `.wsc` cartridge images. Bytes in, validated view out. | ✅ structural (header fields deferred) |
| `cpu-v30mz` | NEC V30MZ core. Registers, flags, addressing, reset, `CpuBus`, ModR/M decode, ALU, and a `step()` executor running most of the instruction set. | 🔨 runs ALU/MOV/stack/control-flow/`MUL`/strings/`IN`-`OUT`; shifts, `DIV`, interrupts, and timing pending |
| `core-ws` | The WonderSwan machine core. Cartridge boundary, I/O map, unit-tested interrupt-controller model. | 🔨 boundary + interrupt model |
| `ws-testkit` | Deterministic synthetic core + capture sink + stable hashing. Home of the hardware-test-ROM runner (arrives with the CPU). | ✅ synthetic path |
| `ws-cli` | Headless runner and ROM inspector. | ✅ |

## Where to start

Read [`CLAUDE.md`](CLAUDE.md), then [`ROADMAP.md`](ROADMAP.md) and
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). Hardware specs live in
[`docs/hardware/`](docs/hardware/) — start with
[`00-overview.md`](docs/hardware/00-overview.md), then the CPU
([`01-cpu-v30mz.md`](docs/hardware/01-cpu-v30mz.md)) and interrupt
([`02-interrupts.md`](docs/hardware/02-interrupts.md)) specs. Test-ROM and BIOS
provenance is in [`docs/LEGAL_PROVENANCE.md`](docs/LEGAL_PROVENANCE.md); nothing
copyrighted is committed.

## Build & verify

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo test --release --workspace
cargo run --release -p ws-cli
```

Rust 1.96.0 is pinned via `rust-toolchain.toml`. No `unsafe`, warnings are errors.

## License

PolyForm Noncommercial 1.0.0 (see `Cargo.toml`). The WonderSwan name, BIOS, and
all game software are the property of their respective owners and are not
included in this repository.
