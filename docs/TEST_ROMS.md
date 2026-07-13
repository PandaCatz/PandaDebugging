# Hardware Test ROMs & Oracles

These are the authorities we validate against. We consume the ROMs as oracles and
keep their built binaries out of the repo (see `docs/LEGAL_PROVENANCE.md`). Record
the source commit and license for each fixture you actually use.

## Test suites

| Suite | Source | Validates | Phase gate |
|-------|--------|-----------|------------|
| ws-test-suite | https://github.com/asiekierka/ws-test-suite | CPU instruction timing, IRQ/timer handling, DMA `5+2n`, I/O port timing, memory access-slot conflicts | 2, 3, 5 |
| WSCpuTest | https://github.com/FluBBaOfWard/WSCpuTest | V30MZ opcode behaviour & flags vs real hardware | 2 |
| WSTimingTest | (asiekierka / wsdev) | V30MZ instruction cycle timing | 2 |
| WSHWTest | (asiekierka / wsdev) | interrupt & timer handling | 2 |
| rtctest | (wsdev) | 2003 mapper Seiko S-3511 RTC behaviour | 6 |
| ARMV30MZ | https://github.com/FluBBaOfWard/ARMV30MZ | reference V30MZ opcode/flag semantics (read as spec, not run) | 2 |

## Reference docs

| Doc | URL |
|-----|-----|
| WSMan (hardware reference) | http://daifukkat.su/docs/wsman/ |
| WSdev wiki — NEC V30MZ | https://ws.nesdev.org/wiki/NEC_V30MZ |
| WSdev wiki — Display | https://ws.nesdev.org/wiki/Display |
| WonderSwan hardware tests blog | http://daifukkat.su/blog/archives/2015/07/11/wonderswan_hardware_tests/ |
| awesome-wsdev | https://github.com/WonderfulToolchain/awesome-wsdev |

## Reference emulators (behavioural oracles)

- **ares** (https://ares-emu.net) — most accurate WonderSwan core; the primary
  framebuffer/audio oracle for diffing.
- **Mednafen** — secondary oracle.

Both are run **separately by the operator** to produce reference captures. This
project copies no code from them.

## Acquisition

Building from source needs the WonderSwan toolchain (wonderful-toolchain /
`wf-gcc`). Some suites publish prebuilt `.ws` files in their GitHub releases.
`tools/fetch-test-roms.ps1` clones the sources into the gitignored
`fixtures/test-roms/`. Downloading or building is an explicit, operator-run step —
review each project's license first. Nothing here is fetched automatically.
