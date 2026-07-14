# Hardware Specs

Per-subsystem implementation references. Start with the overview, then the
subsystem you are implementing. Each spec must cite WSMan or a hardware test for
every behaviour and flag anything still unverified as an open question.

- [`00-overview.md`](00-overview.md) — SoC family, bug map, priority fix order,
  and a distilled reference for every subsystem. **Read first.**
- [`../VALIDATION.md`](../VALIDATION.md) — how the CPU is validated against the
  V20 single-step oracle, and what it resolved (e.g. the MD flag bit → `0xF002`).
- [`01-cpu-v30mz.md`](01-cpu-v30mz.md) — opcode map, cycle timing, flags/
  arithmetic/exceptions, memory & I/O map, and the CPU test-ROM validation plan.
  Web-enriched and adversarially fact-checked; unverified items are in its
  appendix. Its preamble records the **resolved** cycle-unit question (measured
  timings are CPU cycles at 3.072 MHz — no 4×).
- [`02-interrupts.md`](02-interrupts.md) — line table, priority dispatch, edge/
  level semantics, `REG_INT_*`, and IRQ-timing watch items.
- [`06-cartridge.md`](06-cartridge.md) — verified cartridge footer layout (field
  offsets, ROM/save code tables, flags & bus width, mapper/RTC, checksum),
  resolved source disputes, and open gaps.
- [`07-io-registers.md`](07-io-registers.md) — verified I/O-register maps for the
  sound noise channel, serial/UART, and internal EEPROM (addresses + bit layouts),
  the EEPROM Microwire command protocol, resolved disputes, and open gaps.

Planned (added as each phase begins):

- `03-ppu-display.md` — scanline timing, sprite DMA @142, palette pool,
  color-zero rules, `REG_LCD_VTOTAL`, WSC tile bank.
- `04-apu-sound.md` — channels, unsigned mixing, sweep, noise taps, startup
  latency, HyperVoice.
- `05-dma.md` — general DMA + SDMA, CPU halt, cart-SRAM source, `5+2n`.
- `06-cartridge.md` (extend) — EEPROM sizes, S-3511A RTC, UART IRQ clearing,
  keypad pull-down (footer layout + bus width already landed).
- `08-bios-boot.md` — boot ROM, `REG_HW_FLAGS` bank-out timing, HLE fallback.
