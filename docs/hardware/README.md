# Hardware Specs

Per-subsystem implementation references. Start with the overview, then the
subsystem you are implementing. Each spec must cite WSMan or a hardware test for
every behaviour and flag anything still unverified as an open question.

- [`00-overview.md`](00-overview.md) — SoC family, bug map, priority fix order,
  and a distilled reference for every subsystem. **Read first.**

Planned (added as each phase begins):

- `01-cpu-v30mz.md` — opcode set, `SALC`, cycle timings, IN/OUT cost, flags,
  division exceptions, LFSR-as-cycle-counter method.
- `02-interrupts.md` — line table, priority dispatch, edge/level, `REG_INT_*`.
- `03-ppu-display.md` — scanline timing, sprite DMA @142, palette pool,
  color-zero rules, `REG_LCD_VTOTAL`, WSC tile bank.
- `04-apu-sound.md` — channels, unsigned mixing, sweep, noise taps, startup
  latency, HyperVoice.
- `05-dma.md` — general DMA + SDMA, CPU halt, cart-SRAM source, `5+2n`.
- `06-cartridge-eeprom-rtc-serial.md` — header layout, EEPROM sizes, S-3511A RTC,
  UART IRQ clearing, bus width, keypad pull-down.
- `07-bios-boot.md` — boot ROM, `REG_HW_FLAGS` bank-out timing, HLE fallback.
