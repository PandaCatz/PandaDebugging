# WonderSwan Interrupt Controller — Implementation Spec (Phase 2)

> **Status:** web-enriched, adversarially fact-checked on 2026-07-13. High
> confidence: the line table, trigger types, priority order, and `REG_INT_*`
> addresses were confirmed against the WSdev wiki and (over plain HTTP) WSMan.
>
> **Corrections folded in:** per nesdev `NEC_V30MZ_interrupts`, the V30MZ implements
> only six of the eight 80186 CPU interrupts — **both** `INT 6` (invalid opcode)
> and `INT 7` (ESC) run as NOPs, not just `INT 7`.
>
> **Watch items (see appendix):** IRQ-entry / IRET-exit cycle counts are
> undocumented (the ARMV30MZ 40/33/10 figures are approximations, not hardware
> truth); the "level line fires continuously and can lock a program" behaviour is
> hedged in WSMan itself; and "`REG_INT_ACK` clears edge lines only" is an
> *inferred* consequence of level lines re-asserting from their live source, not a
> stated rule. The `core-ws::interrupt` model already encodes the edge/level
> priority behaviour; treat these as confirmation targets for the bus wiring.

---


## Interrupt Controller

The WonderSwan exposes an 8-line hardware interrupt controller through four I/O ports in the `$B0`–`$B6` region. All eight lines are OR'd (after enable masking) into the single maskable INT pin of the NEC V30MZ; the controller presents the winning line's **vector number** to the CPU, which then performs a standard x86 real-mode software-interrupt sequence through the Interrupt Vector Table (IVT) at physical `0000:0000h`.

### Register Map (verified from WSMan rev.7 I/O port table)

| Port | Access | Name | Size | Purpose |
|------|--------|------|------|---------|
| `$B0` | R/W | `REG_INT_BASE` | byte | Base vector number for the 8 HW IRQ lines |
| `$B2` | R/W | `REG_INT_ENABLE` | byte | Per-line enable mask (bit *n* enables line *n*) |
| `$B4` | R | `REG_INT_STATUS` | byte | Per-line **raw** asserted mask (not gated by ENABLE) |
| `$B5` | R/W | `REG_KEYPAD` | byte | Keypad multiplex/select + button state (source of line 1) |
| `$B6` | W | `REG_INT_ACK` | byte | Acknowledge/clear edge lines (write 1 to clear) |

`$B1` (`REG_SER_DATA`) and `$B3` (`REG_SER_STATUS`) are interleaved into this block but belong to the serial port; they are the *sources* of lines 0 and 3 (see below). `REG_INT_ACK` is write-only: **reading `$B6` returns 0** (WSMan, verbatim).

### Hardware IRQ Lines (verified verbatim from WSMan)

Bit position **is** priority. Highest bit number wins.

| Bit | Trigger | Name | Source condition | Priority |
|-----|---------|------|------------------|----------|
| 7 | **Edge** | `HWINT_HBLANK_TMR` | H-blank timer underflow | Highest |
| 6 | **Edge** | `HWINT_VBLANK` | V-blank (start) | |
| 5 | **Edge** | `HWINT_VBLANK_TMR` | V-blank timer underflow | |
| 4 | **Edge** | `HWINT_LINE` | Line-compare match (`LINE_CUR == LINE_CMP`) | |
| 3 | **Level** | `HWINT_SER_RX` | Serial "Data Received" (`REG_SER_STATUS.0 == 1`) | |
| 2 | **Level** | `HWINT_CART` | Cartridge IRQ (usually RTC alarm) | |
| 1 | **Edge** | `HWINT_KEY` | Key-press on the enabled `REG_KEYPAD` group | |
| 0 | **Level** | `HWINT_SER_TX` | Serial "Send Buffer Empty" (`REG_SER_STATUS.2 == 1`) | Lowest |

`REG_INT_ENABLE` and `REG_INT_STATUS` use these same bit positions. `REG_INT_STATUS` shows the raw latched/live line state and is **explicitly not gated by `REG_INT_ENABLE`** (WSMan) — a disabled line still sets its status bit; it just cannot reach the CPU.

### Vector-Number Computation (verified: WSMan + ARMV30MZ)

The generated interrupt (vector) number for a hardware line is:

```
vector_number = (REG_INT_BASE & 0xF8) | line          ; line = winning bit index 0..7
```

The **low 3 bits of `REG_INT_BASE` are ignored by the interrupt-generation hardware** (WSMan). This lets a single 8-entry, 8-aligned block of the IVT hold all HW handlers. Read-back of the low bits is model-dependent (WS masks them to `011b`, WSC/SC toward `0`) but does not affect generation — always mask with `& 0xF8`.

The IVT is at physical `0000:0000h`, has `100h` (256) entries, each **4 bytes** = a far pointer `IP:CS`:

```
addr    = vector_number * 4
new_IP  = mem16[addr + 0]        ; offset word first
new_CS  = mem16[addr + 2]        ; segment word second
```

ARMV30MZ confirms this layout: it forms the entry address as `vector<<2`, reads the offset word, then reads the segment word at `+2`.

### CPU / Software Vector Numbers (fixed at IVT start, verified WSMan)

These are locked to the beginning of the IVT and are independent of `REG_INT_BASE`:

| INT# | Name | Cause |
|------|------|-------|
| `00h` | `CPUINT_DIV` | Divide error (`DIV`/`IDIV` overflow) |
| `01h` | `CPUINT_STEP` | Single-step (TF=1) |
| `02h` | `CPUINT_NMI` | Non-maskable interrupt |
| `03h` | `CPUINT_BREAK` | `INT 3` |
| `04h` | `CPUINT_INTO` | `INTO` with OF=1 |
| `05h` | `CPUINT_BOUNDS` | `BOUND` range failure |
| `06h` | `CPUINT_INVALID` | Invalid opcode — **not generated by V30MZ**, runs as NOP (nesdev NEC_V30MZ_interrupts); see open questions |
| `07h` | `CPUINT_ESCAPE` | Escape opcode — **not generated by V30MZ**, runs as NOP (nesdev); WSMan tags 07h 'TODO: exists?' |

NMI uses **vector 2** (ARMV30MZ `NEC_NMI_VECTOR = 2`). Keep the HW `REG_INT_BASE` block from overlapping `00h–07h` unless a game deliberately shares them.

### Dispatch Algorithm

At each instruction boundary (subject to the delay rules below):

```
pending = REG_INT_STATUS & REG_INT_ENABLE      ; 8-bit
if (pending != 0) and (CPU.IF == 1):
    line   = highest_set_bit(pending)          ; 7 wins over 0
    vector = (REG_INT_BASE & 0xF8) | line
    take_interrupt(vector)                      ; see entry sequence
```

Only the single highest-priority enabled+asserted line is dispatched per acknowledge; lower lines stay set in `REG_INT_STATUS` and are re-evaluated at the next boundary. `REG_INT_STATUS` reflects raw sources, so a line masked off in `REG_INT_ENABLE` can be **polled** without ever interrupting.

### Edge vs. Level Acknowledge Semantics (verified WSMan, with author caveat)

- **Edge lines (1,4,5,6,7):** latched in `REG_INT_STATUS` when the event edge occurs. They stay asserted until software writes a 1 to the matching bit of `REG_INT_ACK` (`$B6`). Writing 0 bits has no effect. `REG_INT_ACK` is not gated by `REG_INT_ENABLE`.
- **Level lines (0,2,3):** track the live source condition. Writing `REG_INT_ACK` produces no persistent change — the status bit re-asserts as long as the source holds. To deassert, the program must **resolve the source** (e.g. read/write `REG_SER_DATA` to clear TX-empty / RX-ready; clear the cartridge/RTC-alarm condition).

WSMan documents the trigger column authoritatively but hedges the lock-up wording with a parenthetical *"(TODO: is this true? I don't think so)"*. The practical rule for accurate emulation (matches serial-game behavior): implement lines 0/2/3 as level — assert while source true, ignore ACK for them — and lines 1/4/5/6/7 as edge cleared only by ACK. **Treating all lines as edge (auto-clear on dispatch) starves the level condition and deadlocks serial-transfer games**, because the handler never re-enters to drain the UART.

### CPU Interrupt Entry / Exit Sequence (verified ARMV30MZ; standard x86 real-mode)

Entry (`take_interrupt`), in order:

1. Push `FLAGS` (16-bit).
2. Push `CS`.
3. Push `IP` (address of next instruction; for external IRQ this is the boundary IP).
4. Clear **IF** (interrupt disable) — nested maskable IRQs blocked until the handler re-enables.
5. Clear **TF** (trap/single-step).
6. Load `IP = mem16[vector*4]`, `CS = mem16[vector*4 + 2]`.

ARMV30MZ performs exactly this: `pushFlags`, then push CS, then push IP; `strb …,#v30IF` clears IF and `bic v30cyc,#HALT_FLAG|TRAP_FLAG` clears the halt and trap flags on entry. Taking an interrupt also releases `HLT` (the CPU resumes at the instruction after `HLT`).

Exit — `IRET`:

```
POP IP   ;  POP CS   ;  POP FLAGS      (restores IF/TF)
```

confirmed by both WSMan (`IRET: POP PC, POP CS, POP FLAGS`) and ARMV30MZ (`i_iret` reads IP, then CS, then falls into `POPF`). Because `IRET` restores FLAGS (and thus IF), the controller re-arms immediately; a still-asserted level line will re-fire on the next boundary.

### Interrupt Recognition Timing / Delay Rules (verified NEC_V30MZ_interrupts, nesdev)

Maskable interrupts are recognized only **after the current instruction completes**, and recognition is **deferred by one instruction** after any of:

- an instruction that modifies a **stack segment register** (`MOV SS,*` / `POP SS`),
- a **prefix** instruction (segment-override, `REP`, `LOCK`) — the prefixed instruction is treated as a unit,
- an instruction that **sets IF** (`STI`, `POPF`),
- a change to the single-step (trap) flag.

ARMV30MZ mirrors this with dedicated delayed-check paths for `STI/EI` (`v30DelayIrqCheckTrap`) and `IRET/POPF` (`v30DelayIrqCheck`), so the instruction *following* `STI` executes before the first maskable IRQ can be taken. NMI is not maskable by IF but still honors the instruction-boundary rule.

### IRQ Entry/Exit Cycle Timing

WSMan's V30MZ instruction-timing table lists the cycle counts for `CLI`, `STI`, `HLT`, `INT irqno`, and `IRET` as **`!` (undocumented/unmeasured)**; the daifukkat.su hardware-test blog contains no interrupt-latency measurements. There is therefore **no primary hardware-measured entry/exit cycle count** — see open questions.

For reference only, the ARMV30MZ port charges (implementation approximation, not hardware-verified):

| Event | ARMV30MZ cost |
|-------|---------------|
| Maskable HW IRQ vectoring | `eatCycles 7` (fetch/dispatch) + `eatCycles 33` = **~40 cycles** |
| NMI vectoring | ~33 cycles (skips the FetchIRQ 7) |
| `IRET` | `eatCycles 10` (7 + 3 shared with `POPF`) |

Do not treat these as measured truth; gate exact values behind hardware/reference cross-checks.

### Emulator Pitfalls (per behavior)

- **All-edge shortcut → serial deadlock.** Lines 0/2/3 are level; auto-clearing them on dispatch (or clearing them via `REG_INT_ACK`) causes serial/link games to hang. Assert them while the source condition holds.
- **`REG_INT_ACK` gated by enable.** It is *not* gated by `REG_INT_ENABLE`; a bit can be acked while masked. Also it clears only edge latches — acking a level bit is a no-op.
- **`REG_INT_STATUS` gated by enable.** It is *not*; it shows raw sources for polling. Don't AND it with ENABLE before returning a read.
- **Vector uses `& 0xF8`.** Feeding the raw `REG_INT_BASE` (with dirty low bits) into `base | line` corrupts the vector on WS, where the low bits read back as `011b`.
- **IF/TF must both clear on entry**, and both restore from the stacked FLAGS on `IRET`. Forgetting to clear IF permits spurious re-entry before the handler masks the source.
- **HLT wake:** an enabled+asserted line (with IF=1) must break `HLT`; NMI breaks `HLT` regardless of IF.
- **STI/POPF/IRET one-instruction shadow:** do not sample interrupts on the boundary immediately after these — one further instruction must retire first, or code that does `STI; HLT` will race.
- **UART disable:** per the internal deep-dive, clearing serial-enable (`REG_SER_STATUS.7`) should drop pending level lines 0/3. WSMan does not state this explicitly — treat as a source-resolution consequence and verify (open question).


---

## Appendix — adversarial review record

Generated from an independent verification pass that re-fetched sources. Items under **Open questions** are unverified and MUST NOT be encoded as literals until confirmed on hardware or against a reachable primary source.


### Interrupt Controller: Registers, Dispatch, Edge/Level, Priority, Timing

*Verifier verdict:* `needs_fixes` · *confidence:* `high`

**Corrections (applied inline where definitive):**

- INT 6 / INT 7 implementation: nesdev NEC_V30MZ_interrupts states verbatim that the V30MZ implements only six of eight 80186 CPU interrupts and that 'INT 6 - Unused Opcode ... INT 7 - ESC Opcode' are NOT implemented (executed as NOPs). The spec's CPU-vector table flags only INT 07h CPUINT_ESCAPE as '(existence on V30MZ unconfirmed)' while listing INT 06h CPUINT_INVALID as 'Invalid opcode (see open questions)' without the equivalent caveat. Correct value/source: per nesdev, BOTH INT 6 and INT 7 are not generated by V30MZ hardware; the table should mark INT 6 as not-implemented too. (Note: WSMan lists CPUINT_INVALID/CPUINT_ESCAPE as names and tags only 07h with 'TODO: exists on V30MZ?', so the two primary sources disagree and nesdev is the authoritative one for CPU-exception generation.)

**Open questions (verifier):**

- Hardware-measured IRQ entry and IRET exit cycle counts remain undocumented. CONFIRMED absent: WSMan's V30MZ instruction-timing table shows CLI/STI/HLT/INT/IRET all as '!' with '??????', and nesdev carries only a TODO 'Document how many cycles interrupt processing takes'. The ARMV30MZ figures (maskable 7+33=40, NMI 33, IRET 10) are implementation approximations, not hardware truth; verify against a cycle-accurate reference (ares/mednafen) or logic-analyzer capture before locking.
- Level-line (0/2/3) continuous-fire / program-lockup behavior is hedged in WSMan itself ('(TODO: is this true? I dont think so)'). Trigger-type column is authoritative but the continuous-assert timing/lockup semantics need confirmation on hardware or against ares/mednafen serial handling.
- Exact REG_INT_BASE low-3-bit readback per model: WSMan gives masks (WS '*****011', WSC/SC '*******0') but generation always uses &0xF8; confirm precise readback bits if any game depends on them.
- Claim that disabling the UART (clearing REG_SER_STATUS.7 Serial Enable) drops pending TX/RX level IRQs (lines 0/3): CONFIRMED not stated in WSMan (serial chapter only says clearing Enable closes comms; no mention of interrupt-line side effect). Treat as inferred source-resolution consequence and verify on hardware / reference emulator.
- ACK of a level bit producing no persistent clear: WSMan says 'each set bit will acknowledge the according interrupt' with no carve-out for level lines, so the 'edge-only clear' behavior is inferred from level re-assertion, not stated. Confirm on hardware.
- Precise edge-detection point (scanline/tick) for line 4 (HWINT_LINE, LINE_CUR==LINE_CMP) and timer lines 5/6/7 relative to display/timer counters is not covered in the interrupt chapter; cross-reference the display-controller and timer sections.
- Minor: line descriptions 'H-blank timer underflow', 'V-blank timer underflow', 'V-blank (start)', and 'LINE_CUR == LINE_CMP' are author interpretations; WSMan's interrupt chapter says only 'H-Blank timer interrupt' / 'V-Blank timer interrupt' / 'V-Blank interrupt' / 'Line compare interrupt'. The register-name elaborations (REG_SER_STATUS bits, LINE_CUR/LINE_CMP) are correct but drawn from other chapters, not the interrupt chapter.

**Open questions (author):**

- Hardware-measured IRQ entry and IRET exit cycle counts are undocumented: WSMan's V30MZ interrupt-instruction table lists CLI/STI/HLT/INT/IRET cycles as '!' (unknown) and the daifukkat.su hardware-test blog has no interrupt-latency data. The ARMV30MZ reference impl charges ~40 cycles (7+33) for a maskable HW IRQ and 10 for IRET, but these are approximations. Verify against real-hardware logic-analyzer captures or a cycle-accurate reference (mednafen/ares) before locking numbers into the core.
- Whether level lines (0/2/3) truly fire continuously and can lock a program is hedged in WSMan itself ('TODO: is this true? I don't think so'). The trigger-type column is authoritative but the exact continuous-assert timing needs confirmation on hardware / against ares & mednafen serial handling.
- CPU vectors INT 6 (CPUINT_INVALID / invalid opcode) and INT 7 (CPUINT_ESCAPE) — WSMan tags INT7 with 'TODO: exists on V30MZ?', while the nesdev NEC_V30MZ_interrupts page states only six of the eight 80186 CPU interrupts are implemented and INT 6/7 are not. Confirm which (if any) invalid-opcode/escape exceptions the V30MZ actually raises.
- Exact REG_INT_BASE read-back behavior of the low 3 bits per model (WSMan masks show '*****011' on WS vs '*******0' on WSC/SC). Generation always uses &0xF8, but confirm the precise read-back bits if any game relies on them.
- The internal deep-dive claim that disabling the UART (clearing REG_SER_STATUS.7 Serial Enable) clears pending TX/RX (lines 0/3) IRQs is not explicitly stated in WSMan; it is inferred from the level-trigger + serial-enable gating. Verify on hardware or against a reference emulator that TX-empty/RX-ready level assertions drop when serial is disabled.
- REG_INT_ACK for a level line: WSMan says 'each set bit will acknowledge the according interrupt' without carving out level lines, but level status re-asserts from the live source. Confirm on hardware that acking a level bit produces no persistent clear (i.e., the internal-deep-dive 'clears edge lines only' observable behavior).
- Precise edge-detection point for line 4 (HWINT_LINE / line-compare) and lines 5/6/7 relative to the display/timer counters (which scanline/tick sets the latch) — needed for cycle-accurate raster effects but not covered in the interrupt chapter; cross-reference the display-controller and timer sections.
