# Architecture Baseline

## System boundary

```text
user-owned .ws/.wsc image -> format-ws -> validated cartridge -> core-ws
                                                       -> timed video/audio events
host input -> timestamp/latch adapter -> core-ws       -> frontend presenters (later)
operator-owned BIOS ------> validated boot image ------> core-ws boot path (Phase 7)
```

## Dependency direction

```text
format-ws ─┐
cpu-v30mz ─┼─> core-ws ─> ws-contracts <─ ws-testkit / ws-cli
devices   ─┘                            <─ frontend (later)
```

Rules:

- `format-ws` takes bytes and returns validated data. No runtime, GPU, audio, or
  windowing dependency.
- `core-ws` owns emulated state and time. It depends on `ws-contracts`, its
  parsed data, and its CPU/device components — never on host devices.
- `ws-testkit` can run any core headless, with no window/GPU/audio device.
- The frontend depends on `ws-contracts`; no core calls into it.

## Ownership model

One top-level machine value owns all mutable state: the V30MZ, the scheduler,
cartridge devices, RAM, the PPU, the APU, DMA/SDMA, timers, EEPROM/RTC, and the
keypad. Devices do not also live inside a separately borrowed bus. Scheduled work
receives a narrow context exposing only the reads, writes, IRQ raise/lower, and
event scheduling it needs. This keeps runtime borrow failures out of
deterministic code.

The cartridge is an active device, not a byte array: it exposes ROM/SRAM access,
optional RTC, bus-width behaviour, and persistent memory — not the NES mapper
vocabulary.

## Time model

The WonderSwan derives everything from one **12.288 MHz master clock**. The core
schedules in integer master-clock ticks; the frontend never changes core clocks.

- The 12.288 MHz clock is divided into **4 memory access slots** shared between
  CPU, DMA, sound wavetable, tile data, and palette RAM. The CPU is **not** paused
  during PPU memory access. Emulating a CPU stall there is wrong.
- Pixel clock 3.072 MHz. Display: HDISP 224 + HBLANK 32 = 256 clocks/line;
  VDISP 144 + VBLANK 15 = 159 lines → ~75.47 Hz.
- DMA transfers cost `5 + 2n` master cycles for `n` words (hardware-measured).
- Sprite (OAM) DMA begins at the start of line 142 and must complete before
  VBlank; it is not instantaneous.
- Scheduled events have explicit tie-breaking order. Long-run tests must prove no
  accumulated drift (the domain is integer, so there is no float to drift).

## Determinism

- No wall clock, host thread timing, or unseeded randomness inside a core.
- The noise LFSR is part of deterministic state and keeps advancing even in wave
  mode (games use it as a PRNG seed).
- Input is latched at defined emulated times.
- Rendering and audio packetisation consume simulation state downstream.
- A replay is identified by core version, cartridge identity, configuration,
  initial state, and the timestamped input stream.

## Save and persistent data

Battery SRAM / internal EEPROM / cartridge-RTC persistence and save-states are
distinct products. Persistent memory is written atomically by its device rules.
Save-states use an explicitly versioned logical schema with strict size limits,
invariant validation, and ROM identity — never raw `serde` over implementation
structs as the durable format.

## Safety and performance

All external formats, states, firmware, and configuration are hostile boundaries:
they return errors, never panic. `unsafe` is allowed only after a release-profile
measurement identifies a material bottleneck, a safe baseline exists, invariants
are documented, and tests/fuzzing exercise the wrapper. Compatibility and
performance are reported from evidence, not from a game opening or a frame
inspected by eye.
