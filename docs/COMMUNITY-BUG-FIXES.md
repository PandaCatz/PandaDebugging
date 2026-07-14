# How the Community Bugs Were Fixed

This is the plain-language companion to the terse
[bug ledger](COMMUNITY-BUGS.md). It explains, for a general reader: what each
bug looked like in real games, what the existing WonderSwan emulators get wrong,
what the hardware actually does, and — most importantly — **how we proved our
fix is correct rather than just plausible.**

The whole project exists for one reason: a handful of WonderSwan accuracy bugs
recur across *every* major emulator (Mednafen/Beetle, ares, Oswan, Swan·emu).
We set out to fix exactly those, and to hold each fix to a standard higher than
"it looks right on screen."

> **Honesty note, up front.** "Fixed" here means the corrected behaviour is
> implemented **and** pinned by a test (or a hardware oracle). It does **not**
> mean this is a playable emulator yet. Most of these fixes live in isolated,
> unit-tested modules that are not yet wired into a full running machine. Where a
> fix is only partial, we say so and say why. Nothing on this page is asserted
> from memory or by eye.

---

## How we know a fix is real

Accuracy claims are cheap; most emulator bugs come from a plausible guess that
was never checked against hardware. We used three independent guards against
that.

### 1. Primary-source research, cross-checked adversarially

Before writing a line of code for a behaviour, we pinned the hardware facts
against the primary references — and treated them as hostile until they agreed.
The method was a **multi-agent adversarial research workflow**:

1. **Several independent researchers** each investigate the same question from a
   different angle/source (WSMan, the WSdev wiki, the ares source, the Mednafen
   source, the Sacred Tech Scroll), and report a structured, cited answer.
2. A **reconciler** merges them, marks each field unanimous / majority /
   disputed, and flags every disagreement (conflicting offsets, inverted
   polarities).
3. A separate **skeptic** is then told to *refute* each high-stakes claim using
   an independent source. A claim only survives if it withstands that.

This caught real errors. The cartridge-footer bus-width bit (#9), for example,
is documented two contradictory ways even *within a single source*; the
refutation pass settled it against the runtime hardware register and a
runtime-validated emulator, and even caught a text-extraction misread of a
source diagram. The same pass corrected the save-memory size for one cartridge
code from the stale "8 KB" figure to the real 32 KB. Every such adjudication,
with citations and the surviving *open* questions, is recorded in the
[hardware specs](hardware/).

### 2. A named regression test per bug

Every fix ships with a test whose name is the behaviour the other emulators get
wrong — so a regression re-breaks the exact bug, loudly. The tests are
referenced per fix below.

### 3. Differential validation against a hardware oracle

For the CPU, tests aren't enough — you need to match real silicon instruction by
instruction. We ran the CPU core against the **V20 single-step oracle** (a
per-instruction "given this state, here's the exact result" corpus captured from
a real NEC V20, the V30MZ's close cousin): of ~620,000 captured cases,
**93.49 % exact over the 612,000 runnable ones, with zero defined-behaviour
bugs.** Every divergence is a V20-only instruction or an officially-*undefined*
flag — never a bug in documented behaviour. That run also found and fixed a real
flags-register bug. Details in [VALIDATION.md](VALIDATION.md).

---

## Scorecard

**6 fixed · 3 partial · 0 pending** (of 9). "Partial" always means the *behaviour*
is done but a timing number or a still-open hardware question remains.

| # | Bug | Status |
|--:|-----|--------|
| 2 | Sprite DMA tearing | ✅ fixed |
| 4 | Noise LFSR frozen in wave mode (*Clock Tower* hangs) | ✅ fixed |
| 5 | Monochrome palette pool | ✅ fixed |
| 6 | Colour-zero transparency | ✅ fixed |
| 8 | Internal EEPROM size detection | ✅ fixed |
| 9 | 8-bit ROM bus width | ✅ fixed |
| 1 | V30MZ interrupt handling | 🔨 behaviour + CPU delivery done; cycle timing pending |
| 3 | UART IRQ clear on disable | 🔨 lockup addressed; a latch question + data path open |
| 7 | I/O port access timing | 🔨 data validated; cycle *cost* pending |

---

## The fixes, in plain terms

### ✅ #4 — The noise generator that must never stop (*Clock Tower*)

- **What players saw:** *Clock Tower* and some other games hang at startup on
  many emulators.
- **What emulators get wrong:** the sound chip's channel 4 has a noise generator
  (an LFSR — a hardware pseudo-random number source). Emulators freeze it
  whenever channel 4 isn't currently *outputting* noise. But games read that
  running generator as a cheap random-number source; freeze it and the game
  waits forever for a number that never changes.
- **What the hardware does:** the generator keeps advancing regardless of whether
  channel 4 is outputting noise or a wave — it only stops when the channel or its
  noise-update is switched off.
- **How we proved it:** `lfsr_keeps_running_in_wave_mode` (and its mirror,
  `lfsr_frozen_when_channel_or_update_disabled`, so it stops for the *right*
  reason). An audit tightened an earlier version that would have run even while
  the channel was disabled.

### ✅ #2 — Sprites that tear

- **What players saw:** sprite graphics tear or update a frame early.
- **What emulators get wrong:** they render sprites straight from the live sprite
  table, or copy it into place too late, so a mid-frame write by the game shows
  up half-applied.
- **What the hardware does:** the sprite table is snapshotted (double-buffered)
  and the snapshot is used for the *next* frame, so a mid-frame write can't tear
  the current one.
- **How we proved it:** `sprites_double_buffer_to_the_next_frame` and
  `live_oam_is_independent_of_the_display_buffer`. Notably, the audit
  **removed** a fabricated copy-duration number: the primary source (WSMan) says
  the exact copy timing is *unknown*, so we model the double-buffering (which
  fixes the tearing) and leave the exact cycle timing to the PPU scheduler rather
  than inventing a value.

### ✅ #5 — Monochrome shading through a pool

- **What players saw:** wrong greyscale shading on the original (mono) WonderSwan.
- **What emulators get wrong:** they map the 16 palettes straight onto 16 fixed
  shades of grey.
- **What the hardware does:** it's a **two-stage lookup**. Palettes don't hold
  shades; they hold indices into a separate 8-entry shade *pool*. Editing one
  pool entry re-shades every palette that points at it.
- **How we proved it:** `shade_resolves_through_the_pool` and
  `changing_the_pool_reshades_palettes_that_use_it`. The audit also added the
  hardware's write-protection of colour 0 on certain palettes
  (`color_zero_is_write_protected_for_transparent_palettes`).

### ✅ #6 — Which "colour 0" is transparent

- **What players saw:** mis-rendered backgrounds on the WonderSwan Color.
- **What emulators get wrong:** they treat palette entry 0 as transparent for
  every colour-mode palette.
- **What the hardware does:** transparency depends on the **colour depth**, not on
  mono-vs-colour. At 4 colours-per-tile, half the palettes are opaque and half
  treat colour 0 as transparent; at 16 colours, all treat colour 0 as
  transparent except a dedicated back-colour register.
- **How we proved it:** `two_bpp_color_zero_by_palette_number` and
  `four_bpp_all_transparent_except_back_color`. The audit corrected an earlier
  version keyed on the wrong axis, which broke 4-colour *colour* backgrounds.

### ✅ #8 — Telling a WonderSwan from a WonderSwan Color

- **What players saw:** games mis-detecting which console they're on.
- **What emulators get wrong:** they present a single internal-EEPROM size.
- **What the hardware does:** the original WS carries a 1 Kbit (128-byte) EEPROM
  and the Color a 16 Kbit (2 KB) one; some games infer the console from that size.
- **How we proved it:** `ws_and_wsc_sizes_differ`. The audit corrected the size
  figure the original bug report got wrong (it claimed 64 bytes; the real ASWAN
  allocation is 128).

### ✅ #9 — 8-bit vs 16-bit cartridge bus

- **What players saw:** corrupt data on the Pocket Challenge V2 and some early
  cartridges.
- **What emulators get wrong:** they assume every cartridge uses a 16-bit ROM
  bus.
- **What the hardware does:** the cartridge's own header declares its bus width;
  early carts declare 8-bit, and reading them as 16-bit corrupts the data.
- **How we proved it:** we decoded the full 16-byte cartridge header (`CartHeader`
  in `format-ws`) and surface the width as `WsCartridge::bus_width`, tested both
  ways (`bus_width_bit2_selects_8_or_16_bit`, `exposes_declared_bus_width`). The
  exact header bit was genuinely disputed between sources; the adversarial pass
  settled it, and the full provenance — including the parts that still need a
  real cartridge dump to be certain — is in
  [hardware/06-cartridge.md](hardware/06-cartridge.md).

### 🔨 #1 — Interrupt handling (behaviour done; timing pending)

- **What players saw:** serial-driven games deadlock; audio/raster effects
  glitch.
- **What emulators get wrong:** wrong interrupt priority order; treating
  continuous ("level") interrupt lines as one-shot ("edge") lines; and hardcoding
  the interrupt table's location instead of reading the register that relocates
  it.
- **What we did:** implemented the correct priority ordering, the edge-vs-level
  distinction, acknowledge-clears-edge-only semantics, and the relocatable
  vector base; the CPU's interrupt *delivery* is exercised by the V20-validated
  core. Tests include `highest_bit_wins_priority`,
  `ack_clears_edge_lines_but_not_level_lines`, and
  `vector_masks_base_low_three_bits`.
- **Why it's only partial:** cycle-accurate interrupt *timing* is still pending.
  The cycle-unit question it depended on is now **resolved** (see below), so this
  is a matter of implementing the timing, not an open unknown.

### 🔨 #3 — Serial port interrupts on disable (lockup fixed; a question open)

- **What players saw:** startup lockups / spurious interrupts around the serial
  port.
- **What emulators get wrong:** disabling the serial port leaves its
  transmit/receive interrupts pending.
- **What we did:** disabling the port lowers its (level-triggered) interrupt
  lines, which addresses the documented lockup
  (`disabling_clears_pending_serial_irqs`, `disabled_port_raises_nothing`).
- **Why it's only partial:** whether the hardware clears the status *latch* or
  only the *level* on disable is still an open primary-source question, and the
  serial data path isn't fully wired yet — so we don't claim it fully settled.

### 🔨 #7 — I/O port timing (data done; cost pending)

- **What emulators get wrong:** `IN`/`OUT` port instructions complete in too few
  cycles.
- **What we did:** the data behaviour of `IN`/`OUT` is implemented and
  V20-validated.
- **Why it's only partial:** the exact cycle *cost* is still open — but only its
  *value* (reported as 12 vs 6 by different sources). The *unit* is now settled
  (CPU cycles — see below); confirming which value is right needs a hardware
  measurement.

---

## The timing question behind the two partials (now resolved)

Both remaining partials (#1, #7) need instruction *timing*, and that was blocked
on a single fact: WonderSwan hardware timings were originally measured with a
sound-channel counter, and it wasn't settled whether that counter ticks at the
12.288 MHz master clock or the 3.072 MHz CPU clock — a 4× factor on *every* timing
number.

We resolved it with the same research method described above, and it's a good
example of it working: the measured values are in **CPU cycles (3.072 MHz)** —
there is **no hidden 4×**. The whole confusion was a terminology collision (the
original researcher's word "master clock" refers to the 3.072 MHz clock, not the
12.288 MHz crystal). The answer is corroborated four independent ways: that
author's own definition, two separate emulators (ares and Mednafen both clock the
CPU at 3.072 MHz with no 4× anywhere), an instruction-timing datasheet ratio
(`XCHG` measures 3 vs a known 3 → factor 1.0, not 4), and a physical-limits
argument (the 4× reading would require the hardware to transfer data faster than
its bus allows). No hardware was needed. Details are in the
[CPU spec preamble](hardware/01-cpu-v30mz.md).

What's left for #1/#7 is the ordinary work of implementing the per-instruction
costs on the scheduler, plus a hardware measurement of the exact `IN`/`OUT` value
— neither of which reopens the unit question.

---

## Tools and sources we used

**Language & build**
- **Rust** (2021, pinned to 1.96.0), a small multi-crate workspace.
- `cargo fmt`, `cargo clippy` **with warnings-as-errors**, and `cargo test`.
- `unsafe` is **forbidden** workspace-wide.
- **GitHub Actions CI** runs the whole gate (format, lint, debug + release tests,
  a headless smoke run) on every push.

**Validation**
- A **V20 single-step oracle** harness — differential testing of the CPU against
  a real-hardware instruction corpus (see [VALIDATION.md](VALIDATION.md)).
- A **named regression test per bug**, pinning the exact behaviour other
  emulators get wrong.

**Primary hardware references** (cross-checked against each other)
- **WSMan** (the WonderSwan hardware manual) and the **WSdev wiki**.
- The **ares** and **Mednafen/Beetle** emulator source, used as runtime-validated
  cross-checks — not copied.
- The **Sacred Tech Scroll** and the **NEC V20/V30/8086** datasheets.

**Method**
- A **multi-agent adversarial research workflow**: independent researchers per
  source → reconciliation with explicit disagreement tracking → a separate
  refutation pass that must fail to break a claim before we trust it. Findings,
  citations, and the surviving open questions are written into the
  [hardware specs](hardware/) so nothing has to be re-derived from memory.

---

## Honest limitations

- This is **not a playable emulator**. The fixed subsystems are correct and
  tested in isolation; wiring them into a full running machine (the real memory
  map, PPU rendering, DMA, timing) is still ahead.
- The two timing partials (#1, #7) are unblocked now that the cycle-unit question
  is resolved; what remains is implementing per-instruction timing (and measuring
  the exact `IN`/`OUT` value).
- A few resolved facts (e.g. the cartridge bus-width bit) rest on strong
  documentary convergence rather than a physical hardware dump; those are flagged
  as such in the hardware specs, not hidden.

## Further reading

- [COMMUNITY-BUGS.md](COMMUNITY-BUGS.md) — the terse per-bug ledger with status.
- [hardware/00-overview.md](hardware/00-overview.md) — the hardware bug map.
- [hardware/06-cartridge.md](hardware/06-cartridge.md) — a worked example of the
  research method (the cartridge footer).
- [VALIDATION.md](VALIDATION.md) — how the CPU is validated against the V20 oracle.
