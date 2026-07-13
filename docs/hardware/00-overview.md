# WonderSwan Hardware Overview & Bug Map

Distilled from the "WonderSwan Emulation Bug Deep Dive" plus the cited references
(WSMan, WSdev wiki, daifukkat.su hardware tests). This is the project's working
reference; per-subsystem specs are added as `01-cpu.md`, `02-ppu.md`, etc. Every
claim here must trace to WSMan or a hardware test before code depends on it.

## SoC family

| SoC | Console | RAM | Master clock |
|-----|---------|-----|--------------|
| ASWAN | WonderSwan / Pocket Challenge V2 | 16 KB | 12.288 MHz (÷ to 3.072 MHz) |
| SPHINX | WonderSwan Color | 64 KB | 12.288 MHz |
| SPHINX2 | SwanCrystal | 64 KB | 12.288 MHz |

CPU: **NEC V30MZ**, 80186-compatible, 16-bit, 20-bit segmented address space.
Display: 224×144 @ ~75.47 Hz (256 clocks/line, 159 lines incl. blanking).
Unmapped I/O reads return `$90` on WS and `$00` on WSC.

## Priority bug-fix order (drives the roadmap)

1. V30MZ interrupt timing — affects nearly every audio/raster game.
2. Sprite DMA double-buffered near line 142/144 — eliminates tearing (exact
   copy timing is unknown per WSMan).
3. UART IRQ clearing on serial disable — prevents startup lockups.
4. Noise LFSR continues in wave mode — fixes Clock Tower and others.
5. Monochrome palette pool indirection — correct WS visuals.
6. Color-zero palette behaviour — correct WSC visuals.
7. I/O port access timing (12 cycles for IN/OUT).
8. Internal EEPROM size detection (WS 512-bit vs WSC 16 Kbit).
9. 8-bit ROM bus support (Pocket Challenge V2, early carts).

## 1. V30MZ CPU

- 80186-compatible **plus** the undocumented `SALC` opcode; **without** the
  V20/V30 REPC/REPNC extensions. Some instruction timings differ from Intel's.
- `IN`/`OUT` cost **12** cycles (not 10). DMA cost `5 + 2n` words.
- Real timings were measured using the noise LFSR at max frequency as a cycle
  counter (sample before/after, take the delta).
- Division exception flag state is implementation-specific.

## 2. Interrupts (modelled in `core-ws::interrupt`)

Eight lines; bit position = priority (bit 7 highest). `REG_INT_BASE` (`$B0`)
relocates the IVT anywhere in the first 64 KB. `REG_INT_ACK` (`$B6`) clears only
edge lines.

| Bit | Line | Trigger |
|-----|------|---------|
| 0 | HWINT_SER_TX | Level |
| 1 | HWINT_KEY | Edge |
| 2 | HWINT_CART | Level |
| 3 | HWINT_SER_RX | Level |
| 4 | HWINT_LINE | Edge |
| 5 | HWINT_VBLANK_TMR | Edge |
| 6 | HWINT_VBLANK | Edge |
| 7 | HWINT_HBLANK_TMR | Edge (highest priority) |

Level lines fire continuously until the source clears; treating them as edge
deadlocks serial-driven games.

## 3. PPU / display

- 12.288 MHz split into 4 memory access slots (CPU/DMA/wavetable/tile/palette).
  CPU is **not** stalled during PPU access.
- `HDISP 224 + HBLANK 32 = 256`; `VDISP 144 + VBLANK 15 = 159` → ~75.47 Hz.
- **Sprite DMA**: OAM (in **internal work RAM**, not cart SRAM) is copied to the
  internal sprite RAM and **double-buffered for the next frame**. Copy scanline
  is a source split — WSMan/Mednafen say 142, WSdev/ares say 144 — and WSMan says
  the copy **timing is unknown** (`5+2n`=517 was a bad transcription; WSdev/ares
  estimate ~256 dot-clocks). It appears to pause the CPU during the copy.
- **Color-zero** (rule keys on **bit depth**, not mono/colour): at 2bpp, palettes
  0–3 & 8–11 are opaque (color 0 not transparent), 4–7 & 12–15 use color 0 as
  transparent; at 4bpp (16-colour) all palettes treat color 0
  as transparent **except** via `REG_BACK_COLOR`. Color 0 writable on translucent
  palettes (ares v144 fix).
- **`REG_LCD_VTOTAL` (`$016`)** writable: 255 blanks display; <144 stops VBlank
  IRQs; odd value on SwanCrystal physically damages the LCD (model as corruption).
- **Mono palette pool**: `REG_PALMONO_POOL` (`$1C–$1F`, 8-entry 4-bit shade pool,
  `$0` brightest…`$F` darkest) → `REG_PALMONO` (`$20–$3F`, 16 palettes × 4
  indices). `final_shade = POOL[palette[idx][color]]`.
- **WSC tile bank bit** (map entry bit 13) selects tiles 512–1023; backgrounds
  only, sprites limited to 0–511; ignored in mono mode.

## 4. APU / sound

- 4 wave channels (32-sample, 4-bit wavetable). Ch2 → PCM voice; ch3 → sweep;
  ch4 → noise LFSR. Runs at the 3.072 MHz clock.
- Speaker: unsigned 8-bit PWM mono (`REG_SND_OUTPUT` volume shift). Headphones:
  signed 16-bit stereo DAC (Rohm BU9480F) @ 24 kHz.
- **Unsigned accumulation**: per-channel samples are unsigned; L/R accumulate via
  unsigned addition into a signed 11-bit result, then clamp.
- **~16-cycle startup latency** after enabling channels.
- **Sweep** (ch3): period `(REG_SND_SWEEP_TIME + 1) * 8192` master clocks; sweep
  value is a **signed** 8-bit delta added to pitch each tick.
- **Noise LFSR**: 15-bit, tap table {0:14, 1:10, 2:13, 3:4, 4:8, 5:6, 6:9, 7:11};
  `bit = 1 ^ (ctr>>7) ^ (ctr>>tap); ctr = ((ctr<<1)|bit) & 0x7FFF`. **Keeps
  updating even in wave mode** — Clock Tower seeds its PRNG from it.
- HyperVoice (WSC PCM) via `REG_HYPER_CTRL` / `REG_HYPER_CHAN_CTRL`, fed by SDMA.

## 5. DMA

- General DMA `REG_DMA_*` (`$040–$048`): **WSC color mode only**; source may be
  cart SRAM; CPU halted during transfer; `5 + 2n` cost.
- Sound DMA `REG_SDMA_*` (`$04A–$052`): 24-bit length (`$04E–$050`); arbitrary
  source incl. cart SRAM; feeds the PCM voice channel.

## 6. Cartridge / EEPROM / RTC / serial / input

- ROM header is the **last 16 bytes**; checksum is the final 2 bytes. Exact field
  layout: verify against WSMan (Phase 0). `REG_HW_FLAGS` bit 0 = BIOS bank-out
  (0→1 after boot), bit 2 = external bus width (0 = 8-bit, 1 = 16-bit).
- Internal EEPROM: 512-bit (WS) vs 93C86 16 Kbit (WSC) — games detect the system
  by size. RTC month field is 1-based (Mednafen bug was 0-based).
- Cartridge RTC: Seiko S-3511A serial via `REG_RTC_STATUS/CMD` (`$CA`),
  `REG_RTC_DATA` (`$CB`).
- Serial (EXT): TX/RX both level-triggered; disabling the port must clear pending
  TX/RX IRQs (else spurious IRQs / lockups).
- Keypad `REG_KEYPAD` (`$B5`): 4×4 matrix (3 groups used); **pull-down** —
  unattached lines read 0; some games refuse to boot if they read 1.

## Sources

WSMan <http://daifukkat.su/docs/wsman/> · WSdev NEC V30MZ
<https://ws.nesdev.org/wiki/NEC_V30MZ> · WSdev Display
<https://ws.nesdev.org/wiki/Display> · daifukkat.su hardware tests
<http://daifukkat.su/blog/archives/2015/07/11/wonderswan_hardware_tests/>.
