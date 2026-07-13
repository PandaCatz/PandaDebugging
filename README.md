# wonderswan-emu

A deterministic, independently testable WonderSwan / WonderSwan Color emulator
core in Rust, built to be *accurate first*: every hardware behaviour is measured
or cited before it is claimed to work.

The project exists to fix the specific accuracy bugs that recur across existing
WonderSwan emulators — V30MZ CPU/interrupt timing, sprite DMA tearing, the
monochrome palette pool, color-zero handling, the noise LFSR, sound mixing, DMA
cycle cost, EEPROM/RTC/serial edge cases — with each fix backed by a hardware
test ROM or a reference-emulator oracle diff, never by "it looks right."

## Community bugs — the scorecard

The point of this project is to fix the
[documented WonderSwan emulation bugs](docs/COMMUNITY-BUGS.md) that recur across
Mednafen/Beetle, ares, Oswan, and Swan.emu. Every fix ships with a named test
pinning the behaviour those emulators get wrong.

**6 fixed · 2 partial · 1 remaining** (of 9)

| # | Documented bug | Status |
|---|----------------|--------|
| 3 | UART disable leaves TX/RX IRQs pending → startup **lockups** | ✅ **fixed** (`core-ws::serial`) |
| 4 | Noise LFSR frozen in wave mode → *Clock Tower* **hangs** | ✅ **fixed** (`core-ws::apu`) |
| 5 | Monochrome palette pool indirection → **wrong shading** | ✅ **fixed** (`core-ws::palette`) |
| 8 | Internal EEPROM size → **WS/WSC mis-detection** | ✅ **fixed** (`core-ws::eeprom`) |
| 1 | V30MZ interrupt handling (priority, edge/level, relocatable IVT) | 🔨 behaviour done + CPU V20-validated; cycle timing pending |
| 7 | I/O port access timing (`IN`/`OUT` ≈ 12 cycles) | 🔨 data validated; cycle cost pending |
| 6 | Color-zero palette behaviour | ✅ **fixed** (`core-ws::palette`) |
| 2 | Sprite DMA at line 142 (`5 + 2n`) → **tearing** | ✅ **fixed** (`core-ws::ppu` — latch/lock timing; pixel rendering separate) |
| 9 | 8-bit ROM bus width (Pocket Challenge V2, early carts) | ⬜ next — needs the cart bus model |

Per-bug detail and the proving tests are in
[`docs/COMMUNITY-BUGS.md`](docs/COMMUNITY-BUGS.md).

## Status

**Current focus:** fixing the documented community bugs (scorecard above). The
V30MZ CPU is complete and hardware-validated; the last self-contained fix is the
8-bit ROM bus (#9), after which the remaining two (#1, #7) need the cycle-unit
question resolved for timing.

Verified on Rust/Cargo 1.96.0 (Windows x86-64): `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, and **124 tests** all pass in debug
and release. No `unsafe`; warnings are errors.

The CPU is additionally validated against the **V20 single-step hardware oracle**
(620k cases): **zero defined-behaviour bugs** — every divergence is a V20-only
instruction or an officially-undefined flag. See
[`docs/VALIDATION.md`](docs/VALIDATION.md).

| Phase | What | Status |
|------:|------|--------|
| 0 | Charter, toolchain, fixtures, provenance | 🔨 in progress (toolchain + provenance done; ROM-header decode & test-ROM acquisition pending) |
| 1 | Headless skeleton (contracts, parser, testkit, CLI) | ✅ complete |
| 2 | V30MZ CPU + interrupt/bus timing | 🔨 CPU runs the **full documented 8086/80186 set** and is **V20-validated** (zero defined-behaviour bugs); machine + hardware-IRQ delivery done. Remaining: real memory map, WSCpuTest, cycle-unit → timing |
| 3 | PPU / display (sprite DMA @142, palettes, color-zero) | 🔨 palettes (#5), color-zero (#6), and sprite-DMA-@142 latch/lock (#2) done; pixel rendering pending |
| 4 | APU / sound (unsigned mixing, LFSR-in-wave-mode, sweep) | 🔨 noise LFSR in wave mode (#4) done; unsigned mixing, sweep, HyperVoice pending |
| 5 | DMA / SDMA (`5 + 2n`, CPU halt, cart-SRAM source) | ⬜ planned |
| 6 | Cartridge / EEPROM / RTC / serial / bus width / input | 🔨 internal EEPROM (#8) and serial/UART (#3) done; RTC, 8-bit bus width (#9), input pending |
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
| `cpu-v30mz` | NEC V30MZ core. Registers, flags, addressing, reset, `CpuBus`, ModR/M decode, ALU, and a `step()` executor running the documented instruction set. | 🔨 instruction set complete; hardware-IRQ wiring and cycle timing pending |
| `core-ws` | The WonderSwan machine core. Cartridge boundary, I/O map, interrupt controller, machine + IRQ delivery, and the fixed subsystems: `apu` (#4), `serial` (#3), `palette` (#5), `eeprom` (#8). | 🔨 subsystems landing bug-first; real memory map / full PPU / DMA pending |
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
