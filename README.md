# wonderswan-emu

[![CI](https://github.com/PandaCatz/PandaDebugging/actions/workflows/ci.yml/badge.svg)](https://github.com/PandaCatz/PandaDebugging/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/rust-1.96.0-informational?logo=rust)](rust-toolchain.toml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/PandaCatz/PandaDebugging?label=release&color=blueviolet)](https://github.com/PandaCatz/PandaDebugging/releases)

A deterministic, independently testable **WonderSwan / WonderSwan Color** emulator
core in Rust. Its purpose is narrow: **fix the accuracy bugs that recur across
every existing WonderSwan emulator**, and prove each fix against a hardware test
ROM or a reference-emulator oracle — never by "it looks right."

> **Not a playable emulator yet.** The CPU is complete and hardware-validated and
> the machine boots from cartridge ROM, but the subsystems are not yet wired into
> a full running console. Honest status is below.

---

## Why this exists

A handful of WonderSwan behaviours are emulated wrong the *same way* across
Mednafen/Beetle, ares, Oswan, and Swan·emu. This project reimplements the core
from primary-source hardware documentation (clean-room — no emulator code is
copied) and fixes exactly those bugs, each with a regression test that pins the
behaviour the others get wrong.

- **The catalogue + per-bug status:** [`docs/COMMUNITY-BUGS.md`](docs/COMMUNITY-BUGS.md)
- **A plain-language walkthrough** of what each bug looked like, how it was fixed,
  and how it was proved: [`docs/COMMUNITY-BUG-FIXES.md`](docs/COMMUNITY-BUG-FIXES.md)

## Community-bug scorecard

For each documented bug, the **correct behaviour is implemented and pinned by a
unit test** (adversarially researched against WSMan / WSdev / ares — the audit
corrected several; see the ledger's *Audit-corrected* notes). The ✅ marks that
bar. It does **not** mean the bug is demonstrated fixed in a running game.

> **What this does *not* yet claim.** The core is **not playable**, and **none of
> these has been validated against a hardware test ROM or a reference-emulator
> oracle diff** — the project's own gold standard, still pending (no WonderSwan
> test ROMs acquired). The right-hand column shows how far each fix is actually
> integrated; several run only as isolated, unit-tested logic.

| # | Documented bug | Logic + test | Integrated in the machine |
|--:|----------------|:--:|----|
| 2 | Sprite DMA → **tearing** | ✅ `core-ws::ppu` | ⬜ sprite unit not wired in |
| 4 | Noise LFSR → *Clock Tower* **hangs** | ✅ `core-ws::apu` | ◑ registers wired; **LFSR not clocked yet** |
| 5 | Mono palette pool → **wrong shading** | ✅ `core-ws::palette` | ◑ wired to I/O; no PPU rendering |
| 6 | Color-zero (by bit depth) | ✅ `core-ws::palette` | ◑ wired to I/O; no PPU rendering |
| 8 | Internal EEPROM size | ✅ `core-ws::eeprom` | ✅ size-aliasing runs via `$BA`–`$BE` (write-protect protocol deferred) |
| 9 | 8-bit ROM bus width | ✅ `format-ws` footer | ◑ decoded + exposed; bus behaviour not acted on |
| 1 | V30MZ interrupt handling | 🔨 behaviour + V20-validated | ✅ delivered before each step |
| 3 | UART disable → **lockups** | 🔨 lockup path | ✅ disable→clear-IRQ runs via `$B3` |
| 7 | I/O port timing (`IN`/`OUT`) | 🔨 data validated | ⬜ no timing yet |

## Status

- **CPU** — the NEC V30MZ runs the full documented 8086/80186 instruction set and
  is validated against the **V20 single-step hardware oracle** (612k runnable
  cases, **zero defined-behaviour bugs**; [`docs/VALIDATION.md`](docs/VALIDATION.md)).
- **Machine** — a real address-routing memory map (internal RAM per model,
  `$C0`–`$C3` cartridge ROM/SRAM bank windows, the I/O three-way decode, `$A0`
  system control) with hardware-IRQ delivery. It **boots from cartridge ROM via
  the reset vector**.
- **Timing** — the cycle-unit question that blocked all instruction timing is
  **resolved**: measured timings are CPU cycles at 3.072 MHz (no hidden 4×; see
  the [CPU-spec preamble](docs/hardware/01-cpu-v30mz.md)). Per-instruction timing
  is not implemented yet.
- **Not done** — the fixed subsystems aren't wired to the machine's I/O dispatch;
  there is no PPU rendering, DMA, boot-ROM overlay, save/state, or frontend.

Verified on Rust/Cargo 1.96.0: `cargo fmt --check`, `cargo clippy -- -D warnings`,
and **153 tests** pass in debug and release (no `unsafe`; warnings are errors).
The same gate runs in [CI](https://github.com/PandaCatz/PandaDebugging/actions/workflows/ci.yml)
on every push. Milestones are tagged as
[Releases](https://github.com/PandaCatz/PandaDebugging/releases) (latest: the real
memory map — boots from cartridge ROM).

### Build order (phases)

| Phase | What | Status |
|------:|------|--------|
| 0 | Charter, toolchain, fixtures, provenance | 🔨 toolchain + provenance + verified ROM-header decode done; test-ROM acquisition pending |
| 1 | Headless skeleton (contracts, parser, testkit, CLI) | ✅ complete |
| 2 | V30MZ CPU + memory map + interrupt/bus timing | 🔨 full instruction set, V20-validated, real memory map (boots), cycle-unit resolved; per-instruction timing + WSCpuTest remain |
| 3 | PPU / display (sprite DMA, palettes, color-zero) | 🔨 palettes (#5/#6) + sprite-DMA latch (#2) done; pixel rendering pending |
| 4 | APU / sound (mixing, LFSR-in-wave, sweep) | 🔨 noise LFSR (#4) done; mixing, sweep, HyperVoice pending |
| 5 | DMA / SDMA | ⬜ planned |
| 6 | Cartridge / EEPROM / RTC / serial / bus width / input | 🔨 EEPROM (#8), serial (#3), 8-bit bus (#9) done; RTC + input pending |
| 7 | BIOS boot path | ⬜ planned |

Detailed exit gates: [`ROADMAP.md`](ROADMAP.md). Running state + evidence:
[`CLAUDE.md`](CLAUDE.md).

## Architecture

| Crate | Role | State |
|-------|------|-------|
| `ws-contracts` | Deterministic API: integer emulated time, typed video/audio/input packets, the `Core`/`OutputSink` traits, non-panicking errors. | ✅ |
| `format-ws` | Defensive, borrowed `.ws` / `.wsc` parser. Bytes in, validated view out; typed cartridge-footer decode (`CartHeader`). | ✅ |
| `cpu-v30mz` | NEC V30MZ core: registers, flags, addressing, reset, `CpuBus`, ModR/M decode, ALU, and a `step()` executor. | 🔨 instruction set complete; timing pending |
| `core-ws` | Machine core: cartridge boundary, the real memory map, interrupt controller, machine + IRQ delivery (boots from ROM), and the fixed bug subsystems (`apu`, `serial`, `palette`, `eeprom`, `ppu`). | 🔨 map + boot done; subsystems not yet wired |
| `ws-testkit` | Deterministic synthetic core + capture sink + stable hashing. | ✅ |
| `ws-cli` | Headless runner and `--rom` inspector (no ROM bytes logged). | ✅ |
| `v20-harness` | Runs the V20 single-step oracle against `cpu-v30mz`. | ✅ |

## Build & verify

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo test --release --workspace
cargo run --release -p ws-cli          # deterministic synthetic run
cargo run --release -p ws-cli -- --rom <your-dump.ws>   # inspect a cartridge header
```

Rust 1.96.0 is pinned via `rust-toolchain.toml`. No `unsafe`; warnings are errors.

## Documentation

- **Start here:** [`docs/hardware/00-overview.md`](docs/hardware/00-overview.md) —
  SoC family, the bug map, and the priority fix order.
- **Hardware specs:** CPU [`01-cpu-v30mz.md`](docs/hardware/01-cpu-v30mz.md),
  interrupts [`02-interrupts.md`](docs/hardware/02-interrupts.md), cartridge
  footer [`06-cartridge.md`](docs/hardware/06-cartridge.md).
- **Validation:** [`docs/VALIDATION.md`](docs/VALIDATION.md) (the V20 oracle).
- **Roadmap & architecture:** [`ROADMAP.md`](ROADMAP.md),
  [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).
- **Contributors:** [`CLAUDE.md`](CLAUDE.md) — working rules and cross-session state.

## License

Licensed under the **GNU General Public License v3.0 or later** — see
[`LICENSE`](LICENSE). You may use, study, modify, and redistribute this code;
any distributed derivative must remain under the GPL and make its source
available.

## Legal

This is a **clean-room reimplementation** from public documentation. The
WonderSwan name, the console BIOS, and all game software are the property of
their respective owners and are **not** included in this repository — no ROMs,
BIOS dumps, or copyrighted test binaries are committed (see
[`docs/LEGAL_PROVENANCE.md`](docs/LEGAL_PROVENANCE.md)). Use the emulator only
with dumps you are legally entitled to use.
