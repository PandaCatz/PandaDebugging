# Roadmap

Accuracy-first. Every phase has a hard exit gate tied to a hardware test ROM or a
reference-emulator oracle diff. A phase is not complete because a title boots; it
is complete when its stated oracle and adversarial checks pass and the results
are recorded in `CLAUDE.md`.

The phase order follows the deep-dive's **Priority Bug Fix Order** (see
`docs/hardware/00-overview.md`): interrupt timing first, because it affects
almost every game that uses audio or raster effects.

## Phase 0 — charter, toolchain, fixtures (in progress)

- [x] Pin Rust 1.96.0; workspace with `unsafe_code = "forbid"`, warnings-as-errors.
- [x] Legal provenance policy for BIOS / test ROMs / games (`docs/LEGAL_PROVENANCE.md`).
- [x] Transcribe the ROM header layout (WSMan/WSdev/ares/Mednafen, adversarially
  verified); decode fields in `format-ws` (`CartHeader`). See
  `docs/hardware/06-cartridge.md`.
- [ ] Acquire hardware test ROMs into gitignored `fixtures/` (`docs/TEST_ROMS.md`).
- [ ] Write the acceptance matrix and choose the reference oracle (ares / Mednafen).

Exit gate: header fields decoded with oversize/truncation tests; at least the
V30MZ and interrupt/timer test ROMs present locally with recorded provenance.

## Phase 1 — headless skeleton (complete)

Deterministic contracts, defensive parser (structural), interrupt-controller
model, synthetic core + capture, headless CLI. All gates green (see `CLAUDE.md`).

Exit gate met: synthetic core runs identically headless; capture hashes stable
and recorded; 23 tests pass in debug and release.

## Phase 2 — NEC V30MZ CPU + interrupt/bus timing

Fixes deep-dive priority **#1 (interrupt timing)** and **#7 (I/O port timing)**.

- [x] Verified CPU + interrupt spec written: `docs/hardware/01-cpu-v30mz.md`,
  `02-interrupts.md` (adversarially fact-checked; open questions in appendices).
- [x] Resolve the cycle-unit ambiguity (master vs CPU clock) — **resolved**:
  measured timings are CPU cycles (3.072 MHz), no ×4. See the 01 preamble.

1. `cpu-v30mz` crate: 80186-compatible core with the `SALC` opcode, without the
   V20/V30 REPC/REPNC extensions. Trace-first bus interface; generated opcode
   unit tests.
2. Exact instruction cycle counts, including the WonderSwan-specific `IN`/`OUT`
   cost (12 cycles) and the measured deviations from Intel's 80186 figures.
3. Memory bus with the 4-slot access model (CPU/DMA/wavetable/tile/palette share
   the 12.288 MHz master clock; the CPU is *not* stalled during PPU access).
4. Wire `core-ws::InterruptController` to the bus: dynamic `REG_INT_BASE`,
   edge/level handling, priority dispatch, `REG_INT_ACK` semantics.

Exit gate: WSCpuTest and WSTimingTest pass (cycle-accurate); WSHWTest interrupt +
timer checks pass; DMA-cycle formula `5 + 2n` validated by ws-test-suite.

## Phase 3 — PPU / display

Fixes priority **#2 (sprite DMA at line 142)**, **#5 (mono palette pool)**, and
**#6 (color-zero behaviour)**.

- Scanline timing: 224×144 visible, 159 total lines, ~75.47 Hz; HBLANK/VBLANK.
- Sprite DMA triggered at the start of line 142, modelled at `5 + 2n` cycles,
  with sprite-table writes locked out until it completes (kills tearing).
- Monochrome two-stage palette: `REG_PALMONO_POOL` (8-entry shade pool) →
  `REG_PALMONO` (16 palettes × 4 indices) indirection.
- Color-mode palette rules incl. writable color-zero on translucent palettes and
  `REG_BACK_COLOR`.
- `REG_LCD_VTOTAL` writable (blank at 255, glitch below 144, SwanCrystal odd-value
  damage modelled as corrupted scanlines).
- WSC tile bank bit (map entry bit 13) for tiles 512–1023.

Exit gate: ws-test-suite display tests pass; framebuffer oracle diff vs the
chosen reference emulator on a fixed input script matches within tolerance.

## Phase 4 — APU / sound

Fixes priority **#4 (noise LFSR continues in wave mode)**.

- Four 32-sample 4-bit wave channels; channel 2 PCM voice; channel 3 sweep;
  channel 4 configurable-tap noise LFSR (tap table 0–7).
- Unsigned per-channel accumulation into a signed 11-bit result, then clamp.
- LFSR keeps running even when channel 4 is in wave mode (Clock Tower PRNG seed).
- Sweep period in master clocks, signed sweep value; ~16-cycle sound startup
  latency; headphone DAC signed 16-bit @ 24 kHz vs speaker unsigned 8-bit PWM.
- HyperVoice (`REG_HYPER_*`) fed by Sound DMA.

Exit gate: audio-hash oracle diff on a fixed script matches; Clock Tower does not
hang; sweep-rate and noise-tap checks pass.

## Phase 5 — DMA / SDMA

Fixes the DMA half of priority **#2** infrastructure and SDMA audio.

- General-purpose DMA (`REG_DMA_*`), WSC-color-mode only; CPU halted during DMA;
  source may be cartridge SRAM; `5 + 2n` cycle cost.
- Sound DMA (`REG_SDMA_*`), 24-bit length, arbitrary source incl. cart SRAM.

Exit gate: ws-test-suite DMA-timing and cross-region tests pass.

## Phase 6 — cartridge / EEPROM / RTC / serial / bus width / input

Fixes priority **#3 (UART IRQ clear on disable)**, **#8 (EEPROM size detection)**,
and **#9 (8-bit ROM bus)**.

- Internal EEPROM: 512-bit (WS) vs 93C86 16 Kbit (WSC) size detection for system
  ID; RTC month field 1-based.
- Cartridge Seiko S-3511A RTC serial protocol (`REG_RTC_*`).
- UART: TX/RX level-triggered IRQs cleared when the serial port is disabled.
- `REG_HW_FLAGS` bit 2 external bus width (8-bit for Pocket Challenge V2 / early
  carts); 32 KB SRAM for nominal-8 KB cart types.
- Keypad pull-down semantics (unattached lines read 0).

Exit gate: rtctest passes; EEPROM size detection selects the correct system;
8-bit-bus title data reads correctly.

## Phase 7 — BIOS boot path

- Load the operator-supplied boot ROM; `REG_HW_FLAGS` bit 0 (BIOS bank-out)
  transitions 0→1 at the correct time. HLE fallback when no BIOS is provided.

Exit gate: boot ROM hands off to a cartridge; bank-out timing check passes.

## Later — frontend

Only after the cores are gated: a `winit`/`wgpu` presenter and `cpal` audio
adapter kept strictly downstream of the deterministic core.
