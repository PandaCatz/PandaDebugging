# Community Bug Ledger

This project exists to **fix the specific WonderSwan emulation bugs that recur
across the existing emulators** (Mednafen/Beetle, ares, Oswan, Swan.emu) — the
ones catalogued in the deep-dive (`docs/hardware/00-overview.md`). This file
tracks each one: what other emulators get wrong, what the correct behaviour is,
and whether we implement it *and prove it*.

A fix only counts when it is implemented **and** covered by a test (or, for
timing/undefined behaviour, validated against a hardware oracle). "Looks right"
is not a fix.

Status: ✅ implemented + tested · 🔨 partial · ⬜ pending

| # | Documented community bug | What emulators get wrong | Our status |
|---|--------------------------|--------------------------|-----------|
| 1 | **V30MZ interrupt handling** | wrong priority order; treating level lines as edge (serial deadlocks); hardcoding the IVT base instead of re-reading `REG_INT_BASE` | 🔨 controller model — bit-priority (HBLANK_TMR highest), edge-vs-level table, ack-clears-edge-only, relocatable base — done + tested (`core-ws::interrupt`); CPU delivery done + V20-validated. Cycle-accurate IRQ *timing* pending (cycle-unit open). |
| 2 | **Sprite DMA at line 142** | instant DMA at line 142 → visible tearing; or copying at VBlank and missing raster updates | ⬜ pending (needs the PPU scanline model; `5 + 2n` timing) |
| 3 | **UART IRQ clear on disable** | disabling the serial port leaves TX/RX IRQs pending → spurious IRQs / startup lockups | ✅ `core-ws::serial` — disabling the port lowers the level-triggered SER_TX/SER_RX lines; tested (`disabling_clears_pending_serial_irqs`) |
| 4 | **Noise LFSR runs in wave mode** | freezing the LFSR when channel 4 is not in noise mode → *Clock Tower* (and others) hang, because they seed a PRNG from it | ✅ `core-ws::apu` — LFSR advances every tick regardless of mode; tested |
| 5 | **Monochrome palette pool** | mapping palettes straight to 16 greys instead of the two-stage pool → wrong shading | ⬜ pending (needs the PPU palette model) |
| 6 | **Color-zero palette behaviour** | forcing palette index 0 transparent in colour mode; missing the writable translucent-palette / `REG_BACK_COLOR` cases | ⬜ pending (needs the PPU palette model) |
| 7 | **I/O port access timing** | `IN`/`OUT` completing in too few cycles (should be ~12, not 10) | 🔨 `IN`/`OUT` data behaviour done + V20-validated; the cycle *cost* is pending (cycle-unit question open — see `docs/hardware/01-cpu-v30mz.md`) |
| 8 | **Internal EEPROM size detection** | always presenting one EEPROM size → games mis-detect WS vs WSC | ⬜ pending (needs the internal-EEPROM model) |
| 9 | **8-bit ROM bus width** | hardcoding a 16-bit bus → corrupt data on Pocket Challenge V2 and early carts | ⬜ pending (`format-ws` decodes `REG_HW_FLAGS` bit 2; cart bus model) |

## How to read this

The build order in `ROADMAP.md` is chosen so each phase *lands one or more of
these fixes with a test that demonstrates the corrected behaviour*. When you
implement a subsystem, wire the specific documented bug into a named test (e.g.
`lfsr_keeps_running_in_wave_mode` for #4) and flip the row here to ✅.
