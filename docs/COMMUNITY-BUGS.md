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
| 1 | **V30MZ interrupt handling** | wrong priority order; treating level lines as edge (serial deadlocks); hardcoding the IVT base instead of re-reading `REG_INT_BASE` | 🔨 controller model — bit-priority (HBLANK_TMR highest), edge/level table, ack-clears-edge-only, relocatable base — done + tested (`core-ws::interrupt`); CPU delivery done + V20-validated. *Audit-corrected:* vector is now `(base & 0xF8) \| line` (was `base + line`), `raise()` is enable-gated, and the machine no longer auto-acks (the ISR writes `$B6`). Cycle-accurate IRQ *timing* still pending (cycle-unit open). |
| 2 | **Sprite DMA (tearing)** | rendering live OAM / single-buffer same-frame → tearing or one-frame-early sprite updates | ✅ `core-ws::ppu::SpriteUnit` — **double-buffered** OAM snapshot used for the *next* frame (mid-frame writes can't tear); tested. *Audit-corrected:* was single-buffer/same-frame with a fabricated `5+2n`=517 duration. Source is internal RAM (not cart SRAM); copy line is 142 (WSMan/Mednafen) vs 144 (WSdev/ares), and WSMan says the copy *timing is unknown* — so exact line/duration + the CPU-pause are left to the PPU scheduler, not guessed. |
| 3 | **UART IRQ clear on disable** | disabling the serial port leaves TX/RX IRQs pending → spurious IRQs / startup lockups | 🔨 `core-ws::serial` — disabling the port lowers the level-triggered SER_TX/SER_RX lines, addressing the documented lockup; tested. *Audit note:* whether hardware clears the status *latch* vs only the *level* on disable is an open primary-source question, and the serial data path (`REG_SER_DATA`, rxFull persistence) is latent until wired — so not yet a fully-settled ✅. |
| 4 | **Noise LFSR runs in wave mode** | freezing the LFSR when the *output-select* is not noise → *Clock Tower* (and others) hang, since they read the running LFSR as a PRNG | ✅ `core-ws::apu` — LFSR advance is **independent of the wave/noise output-select bit** (the fix), but gated by channel-4-enable + the noise-update bit; tested. *Audit-corrected:* was unconditional (would run while the channel is disabled). |
| 5 | **Monochrome palette pool** | mapping palettes straight to 16 greys instead of the two-stage pool → wrong shading | ✅ `core-ws::palette` — two-stage pool→palette→shade indirection; a pool edit reshades every referencing palette; **colour 0 write-protected for palettes 4–7/12–15** (audit-added, per ares); tested |
| 6 | **Color-zero palette behaviour** | forcing palette index 0 transparent for every colour-mode palette → mis-rendered WSC backgrounds | ✅ `core-ws::palette::color_zero_transparent` — keyed on **bit depth**: 2bpp → palettes 0–3/8–11 opaque, 4–7/12–15 transparent; 4bpp → all transparent (except `REG_BACK_COLOR`); tested. *Audit-corrected:* was keyed on mono-vs-colour, which broke 2bpp *colour* backgrounds (ares gates on `depth==2 && !palette.bit2`). |
| 7 | **I/O port access timing** | `IN`/`OUT` completing in too few cycles (should be ~12, not 10) | 🔨 `IN`/`OUT` data behaviour done + V20-validated; the cycle *cost* is pending (cycle-unit question open — see `docs/hardware/01-cpu-v30mz.md`) |
| 8 | **Internal EEPROM size detection** | always presenting one EEPROM size → games mis-detect WS vs WSC | ✅ `core-ws::eeprom` — **1 Kbit / 128-byte (WS)** vs 16 Kbit / 2048-byte (WSC) sizing; tested. *Audit-corrected:* the deep-dive's 64-byte figure was wrong (ares allocates 128 for ASWAN). Sized because software depends on the real capacity; system detection is via the colour/system register, not size-probing. |
| 9 | **8-bit ROM bus width** | hardcoding a 16-bit bus → corrupt data on Pocket Challenge V2 and early carts | ⬜ pending (`format-ws` decodes `REG_HW_FLAGS` bit 2; cart bus model) |

## How to read this

The build order in `ROADMAP.md` is chosen so each phase *lands one or more of
these fixes with a test that demonstrates the corrected behaviour*. When you
implement a subsystem, wire the specific documented bug into a named test (e.g.
`lfsr_keeps_running_in_wave_mode` for #4) and flip the row here to ✅.
