# wonderswan-emu

A deterministic, independently testable WonderSwan / WonderSwan Color emulator
core in Rust, built to be *accurate first*: every hardware behaviour is measured
or cited before it is claimed to work.

The project exists to fix the specific accuracy bugs that recur across existing
WonderSwan emulators — V30MZ CPU/interrupt timing, sprite DMA tearing, the
monochrome palette pool, color-zero handling, the noise LFSR, sound mixing, DMA
cycle cost, EEPROM/RTC/serial edge cases — with each fix backed by a hardware
test ROM or a reference-emulator oracle diff, never by "it looks right."

## Layout

| Crate | Role |
|-------|------|
| `ws-contracts` | Deterministic API: integer emulated time, typed video/audio/input packets, the `Core`/`OutputSink` traits, non-panicking errors. |
| `format-ws` | Defensive, borrowed parser for `.ws` / `.wsc` cartridge images. Bytes in, validated view out. |
| `core-ws` | The WonderSwan machine core. Today: cartridge boundary, I/O map, interrupt-controller model. Next: V30MZ CPU, PPU, APU, DMA. |
| `ws-testkit` | Deterministic synthetic core + capture sink + stable hashing. Home of the hardware-test-ROM runner (arrives with the CPU). |
| `ws-cli` | Headless runner and ROM inspector. |

## Where to start

Read [`CLAUDE.md`](CLAUDE.md), then [`ROADMAP.md`](ROADMAP.md) and
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md). Hardware specs live in
[`docs/hardware/`](docs/hardware/). Test-ROM and BIOS provenance is in
[`docs/LEGAL_PROVENANCE.md`](docs/LEGAL_PROVENANCE.md); nothing copyrighted is
committed.

## Build & verify

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo run --release -p ws-cli
```

Rust 1.96.0 is pinned via `rust-toolchain.toml`. No `unsafe`, warnings are errors.

## License

PolyForm Noncommercial 1.0.0 (see `Cargo.toml`). The WonderSwan name, BIOS, and
all game software are the property of their respective owners and are not
included in this repository.
