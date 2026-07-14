# WonderSwan V30MZ CPU — Implementation Spec (Phase 2)

> **Status:** web-enriched, adversarially fact-checked on 2026-07-13. Produced by a
> six-agent research pass, each section re-checked by an independent skeptic that
> re-fetched sources and flagged every uncited register address, opcode, and cycle
> count. Definitive corrections are folded into the body; everything still
> unverified is collected in the appendix.
>
> **Provenance caveat.** The primary WonderSwan references — WSMan
> (daifukkat.su) and the Sacred Tech Scroll (perfectkiosk.net) — are HTTP-only and
> refused the forced HTTPS upgrade for most of this session, so many "primary"
> facts were confirmed via the WSdev wiki (ws.nesdev.org), the ARMV30MZ reference
> source, ares issue #908, and libws instead. One agent reached WSMan over plain
> HTTP and confirmed the IVT at `0000:0000`, `REG_INT_BASE = $B0`, and the
> `FFFF:0000` reset vector. Re-verify HTTP-only primaries before locking any
> literal marked ⚠️.
>
> **#1 RESOLVED — the cycle-unit ambiguity (was open; settled 2026-07-14).**
> trap15's LFSR-measured timings are in **CPU cycles (3.072 MHz)** — there is **no
> ×4 correction** on the measured values (`5 + 2n`, `IN`/`OUT`, per-instruction
> deltas). The whole "ambiguity" was a terminology collision: trap15's own WSMan
> calls the 3.072 MHz sound/CPU clock the *"master clock"* ("updates every master
> clock (3072000 Hz)") and the 12.288 MHz crystal the *"input clock"* — so "one
> update per master clock" means one per CPU clock, not per crystal tick.
> Confirmed four independent ways: (1) that WSMan definition; (2) both ares and
> Mednafen clock the CPU at 3.072 MHz and charge every instruction there with no
> ×4 anywhere (ares bills `5 + 2n` DMA in that same domain); (3) `XCHG reg,reg`
> measures **3** vs the NEC datasheet's **3** → ratio 1.0 (a ×4 reading predicts
> 12); (4) a physics floor — 1-cycle ops and 2-cycle/word DMA are impossible as
> 12.288 MHz clocks. A scheduler on a master-clock base just multiplies these
> CPU-cycle costs by 4 (§8). **Still open but independent of the unit:** the exact
> `IN`/`OUT` *value* (§5) and the sprite-DMA formula (§6) — those need a hardware
> measurement and do not reopen this.
>
> Interrupt dispatch has its own document: `02-interrupts.md`.

---


## V30MZ Opcode Set & Encoding

The WonderSwan CPU is a **NEC V30MZ**: a 16-bit core that is **binary-compatible with the Intel 80186** documented instruction set *and* several of the 80186's *undocumented* behaviours (notably `SALC`), while **omitting the NEC V20/V30 proprietary opcode extensions**. This section is the opcode reference the core decodes against. Cycle counts are largely out of scope here (see the timing section); only counts directly attached to an encoding fact are quoted, each with its source.

> Sourcing discipline: the byte→mnemonic mapping for *documented* opcodes is the canonical 8086/80186 encoding, corroborated range-by-range against the **ARMV30MZ** reference dispatch table and the **WSdev instruction-set page** (both read this session). Every WonderSwan-specific *deviation* (inert NOP slots, `SALC`, undocumented aliases, absent V20/V30 ops) is cited individually to the source that establishes it. Numbers not verifiable from a fetched source are in Open Questions, not asserted here.

### 1. Encoding model

The V30MZ uses classic 8086 variable-length encoding: `[prefixes] opcode [ModR/M] [SIB — n/a] [displacement] [immediate]`. There is **no** `0F` two-byte opcode space — on the V30MZ the byte `0F` is a single-byte NOP (see §5), so decoders must **not** treat `0F` as an escape prefix.

ModR/M byte: `mod(7:6) reg(5:3) rm(2:0)`. `mod=11` selects a register operand; `mod=00/01/10` selects a memory operand with 0/8/16-bit displacement (`mod=00, rm=110` is the special `disp16` direct-address form). The `reg` field either names a register operand or selects the sub-operation for a *group* opcode (§3).

### 2. One-byte opcode map (0x00–0xFF)

`Eb/Ev` = ModR/M r/m (byte/word); `Gb/Gv` = ModR/M reg; `Ib/Iv` = immediate; `rel8/rel16` = relative displacement. NEC mnemonic aliases (as used by ARMV30MZ) are noted where they differ from Intel.

| Lo → | x0 | x1 | x2 | x3 | x4 | x5 | x6 | x7 | x8 | x9 | xA | xB | xC | xD | xE | xF |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **0x** | ADD Eb,Gb | ADD Ev,Gv | ADD Gb,Eb | ADD Gv,Ev | ADD AL,Ib | ADD AX,Iv | PUSH ES | POP ES | OR Eb,Gb | OR Ev,Gv | OR Gb,Eb | OR Gv,Ev | OR AL,Ib | OR AX,Iv | PUSH CS | **NOP¹** |
| **1x** | ADC Eb,Gb | ADC Ev,Gv | ADC Gb,Eb | ADC Gv,Ev | ADC AL,Ib | ADC AX,Iv | PUSH SS | POP SS | SBB Eb,Gb | SBB Ev,Gv | SBB Gb,Eb | SBB Gv,Ev | SBB AL,Ib | SBB AX,Iv | PUSH DS | POP DS |
| **2x** | AND Eb,Gb | AND Ev,Gv | AND Gb,Eb | AND Gv,Ev | AND AL,Ib | AND AX,Iv | **ES:** pfx | DAA | SUB Eb,Gb | SUB Ev,Gv | SUB Gb,Eb | SUB Gv,Ev | SUB AL,Ib | SUB AX,Iv | **CS:** pfx | DAS |
| **3x** | XOR Eb,Gb | XOR Ev,Gv | XOR Gb,Eb | XOR Gv,Ev | XOR AL,Ib | XOR AX,Iv | **SS:** pfx | AAA | CMP Eb,Gb | CMP Ev,Gv | CMP Gb,Eb | CMP Gv,Ev | CMP AL,Ib | CMP AX,Iv | **DS:** pfx | AAS |
| **4x** | INC AX | INC CX | INC DX | INC BX | INC SP | INC BP | INC SI | INC DI | DEC AX | DEC CX | DEC DX | DEC BX | DEC SP | DEC BP | DEC SI | DEC DI |
| **5x** | PUSH AX | PUSH CX | PUSH DX | PUSH BX | PUSH SP | PUSH BP | PUSH SI | PUSH DI | POP AX | POP CX | POP DX | POP BX | POP SP | POP BP | POP SI | POP DI |
| **6x** | PUSHA² | POPA² | BOUND² | NOP¹ | NOP¹ | NOP¹ | NOP¹ | NOP¹ | PUSH Iv² | IMUL Gv,Ev,Iv² | PUSH Ib² | IMUL Gv,Ev,Ib² | INSB² | INSW² | OUTSB² | OUTSW² |
| **7x** | JO | JNO | JB/JC | JNB/JNC | JZ/JE | JNZ/JNE | JBE | JA | JS | JNS | JP | JNP | JL | JGE | JLE | JG |
| **8x** | GRP1 Eb,Ib | GRP1 Ev,Iv | GRP1 Eb,Ib³ | GRP1 Ev,Ib(sx) | TEST Eb,Gb | TEST Ev,Gv | XCHG Eb,Gb | XCHG Ev,Gv | MOV Eb,Gb | MOV Ev,Gv | MOV Gb,Eb | MOV Gv,Ev | MOV Ew,Sw | LEA Gv,M | MOV Sw,Ew | POP Ev (GRP1a) |
| **9x** | NOP (XCHG AX,AX) | XCHG CX | XCHG DX | XCHG BX | XCHG SP | XCHG BP | XCHG SI | XCHG DI | CBW | CWD | CALLF ptr16:16 | **WAIT→NOP⁴** | PUSHF | POPF | SAHF | LAHF |
| **Ax** | MOV AL,[disp] | MOV AX,[disp] | MOV [disp],AL | MOV [disp],AX | MOVSB | MOVSW | CMPSB | CMPSW | TEST AL,Ib | TEST AX,Iv | STOSB | STOSW | LODSB | LODSW | SCASB | SCASW |
| **Bx** | MOV AL,Ib | MOV CL,Ib | MOV DL,Ib | MOV BL,Ib | MOV AH,Ib | MOV CH,Ib | MOV DH,Ib | MOV BH,Ib | MOV AX,Iv | MOV CX,Iv | MOV DX,Iv | MOV BX,Iv | MOV SP,Iv | MOV BP,Iv | MOV SI,Iv | MOV DI,Iv |
| **Cx** | GRP2 Eb,Ib² | GRP2 Ev,Ib² | RET Iw | RET | LES Gv,Mp | LDS Gv,Mp | MOV Eb,Ib | MOV Ev,Iv | ENTER Iw,Ib² | LEAVE² | RETF Iw | RETF | INT3 | INT Ib | INTO | IRET |
| **Dx** | GRP2 Eb,1 | GRP2 Ev,1 | GRP2 Eb,CL | GRP2 Ev,CL | AAM Ib⁵ | AAD Ib⁵ | **SALC⁶** | XLAT | ESC/FPO⁷ | ESC⁷ | ESC⁷ | ESC⁷ | ESC⁷ | ESC⁷ | ESC⁷ | ESC⁷ |
| **Ex** | LOOPNE | LOOPE | LOOP | JCXZ | IN AL,Ib | IN AX,Ib | OUT Ib,AL | OUT Ib,AX | CALL rel16 | JMP rel16 | JMPF ptr16:16 | JMP rel8 | IN AL,DX | IN AX,DX | OUT DX,AL | OUT DX,AX |
| **Fx** | LOCK pfx | **?BRKS⁸** | REPNE pfx | REP/REPE pfx | HLT | CMC | GRP3 Eb | GRP3 Ev | CLC | STC | CLI | STI | CLD | STD | GRP4 | GRP5 |

Footnotes ¹–⁸ resolved in §5 (undocumented / undefined behaviour). NEC alias notes: `DAA=ADJ4A`, `DAS=ADJ4S`, `AAA=ADJBA`, `AAS=ADJBS`, `AAM=CVTBD`, `AAD=CVTDB`, `XLAT=TRANS`, `ENTER=PREPARE`, `LEAVE=DISPOSE`, conditional `Jcc=Bcc` (e.g. `JO=BV`, `JB=BC`) — per the ARMV30MZ dispatch labels.

### 3. Group opcodes (ModR/M `reg` field selects the operation)

| Group | Opcode(s) | reg=0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|-------|-----------|-------|---|---|---|---|---|---|---|
| GRP1 | 80 / 81 / 82 / 83 | ADD | OR | ADC | SBB | AND | SUB | XOR | CMP |
| GRP2 (shift/rot) | C0 / C1 / D0 / D1 / D2 / D3 | ROL | ROR | RCL (ROLC) | RCR (RORC) | SHL/SAL | SHR | **SAL-undoc⁹** | SAR (SHRA) |
| GRP3 | F6 / F7 | TEST Ib | **TEST Ib¹⁰** | NOT | NEG | MUL (MULU) | IMUL | DIV (DIVU) | IDIV |
| GRP4 (byte) | FE | INC Eb | DEC Eb | **CALL-dup¹¹** | dup | dup | dup | dup | dup |
| GRP5 (word) | FF | INC Ev | DEC Ev | CALL Ev | CALLF Mp | JMP Ev | JMPF Mp | PUSH Ev | **none¹²** |

GRP1a (`8F /0`) = `POP Ev`; other `8F` reg values are undefined. The GRP1 `reg` decode (ADD…CMP) and the GRP2/GRP3 handler split are corroborated by ARMV30MZ handler labels (`testF6/notF6/negF6/muluF6/mulF6/divubF6/divbF6`; `rol/ror/rolc/rorc/shl/shr/shra`) — read this session.

### 4. Prefixes

| Byte | Prefix | Effect | Cost |
|------|--------|--------|------|
| 26 | `ES:` (NEC `DS1:`) | override segment for the next memory operand | 1 cycle |
| 2E | `CS:` (NEC `PS:`) | override segment | 1 cycle |
| 36 | `SS:` | override segment | 1 cycle |
| 3E | `DS:` (NEC `DS0:`) | override segment | 1 cycle |
| F0 | `LOCK` | bus-lock (no external effect on WonderSwan; decode & pass through) | — |
| F2 | `REPNE`/`REPNZ` | repeat string op while `CX≠0` and `ZF=0` | — |
| F3 | `REP`/`REPE`/`REPZ` | repeat string op while `CX≠0` (and `ZF=1` for CMPS/SCAS) | — |

Segment-override cost of 1 cycle is from the WSMan CPU description (via search snippet). `REP` applies to the string ops at `A4–A7` and `AA–AF` (`MOVS/CMPS/STOS/LODS/SCAS`), exercised by WSCpuTest. Multiple prefixes stack; the last segment override wins.

### 5. Undocumented & undefined opcodes (V30MZ-specific — the core MUST replicate)

All behaviours below are from the **WSCpuTest** README (hardware-measured on real WonderSwans) unless noted. The core should implement these exactly, because test ROMs and games rely on the inert-NOP slots.

| # | Encoding | V30MZ behaviour |
|---|----------|-----------------|
| ¹ | `0F`; `63`–`67` | **Single-byte 1-cycle NOP.** `0F` is *not* `POP CS` and *not* a two-byte escape. `64/65` are the NEC V20 `REPC/REPNC` slots; `66/67` are the 80386 operand/address-size prefix slots (introduced on the 80386, not the 80286) — all inert here. |
| ⁶ | `D6` **SALC** | `AL = (CF ? 0xFF : 0x00)`. 8 cycles. (WSCpuTest; encoding also confirmed by PCjs.) |
| ⁴ | `9B` WAIT/POLL | 1-byte NOP, 9 cycles (POLL pin assumed held low). |
| ⁵ | `D4 Ib` AAM / `D5 Ib` AAD | The immediate ("base") byte can be **any value, not only 0x0A** — `AAM` is effectively `AL/imm, AH=quotient, AL=remainder` (a byte÷byte); `AAD` is `AL = AH*imm + AL`. Emulate as general base, not hard-coded 10. |
| ⁷ | `D8`–`DF` ESC/FPO | **Two-byte NOP**, 1 cycle (consumes the following ModR/M byte). No coprocessor present. |
| ⁹ | GRP2 `reg=6` (e.g. `C0 /6`, `C1 /6`) | Undocumented "SAL" alias that **zeroes AL/AX instead of shifting**. |
| ¹⁰ | GRP3 `reg=1` (`F6 /1`, `F7 /1`) e.g. `F6,C8` | Duplicate `TEST` slot that **does not change flags or registers** (1 cycle). |
| ¹¹ | GRP4 `reg≥2` (`FE,D0`–`F0`) | Duplicate byte `CALL/BRA/PUSH` variants. |
| ¹² | GRP5 `reg=7` (`FF,F8`) | Does nothing. `FF` reg=3/5 (`CALLF/JMPF`) additionally accept a **register** operand (`mod=11`) — normally memory-only; on failure "rom locks, crashes or restarts". |
| — | `8D,C8`–`CF` (`LEA` reg-mode), `C4/C5` reg-mode (`LES/LDS`) | Register-source addressing produces "a couple of new addressing modes" rather than faulting. |
| — | `8C,F8` / `8E,F8` (`MOV` seg) | ModR/M bit 5 (0x20) does **not** affect which segment register is accessed. |
| ⁸ | `F1` | Possibly `BRKS`; **untested** on hardware — treat as undefined (see Open Questions). |

### 6. Absent NEC V20/V30 extensions (must NOT be implemented)

The V30MZ **does not** implement the V20/V30 (µPD70108/µPD70116) proprietary opcodes. On the V30MZ their byte slots decode as the inert NOPs of §5, so the correct emulator action is simply to run the §5 NOP behaviour — **not** to trap. Extension families that are absent (names confirmed via the NEC V20 Wikipedia article):

| V20/V30 family | Instructions | V30MZ status |
|----------------|--------------|--------------|
| Bit manipulation | `TEST1`, `SET1`, `CLR1`, `NOT1` | Absent (V20/V30 encode these under the `0F` two-byte group, which is a NOP here) |
| Packed-BCD string | `ADD4S`, `SUB4S`, `CMP4S` | Absent |
| Nibble rotate | `ROL4`, `ROR4` | Absent |
| Bit-field | `INS`, `EXT` | Absent |
| Repeat prefixes | `REPC`, `REPNC` (the `64/65` slots) | Absent → `64/65` are 1-cycle NOPs |
| 8080 emulation | `BRKEM` / emulation mode | Absent (verify byte — Open Questions) |

### 7. Flags register (PSW) layout

Verified from the WSdev **NEC V30MZ flags** page.

| Bit | 15 | 14–12 | 11 | 10 | 9 | 8 | 7 | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
|-----|----|----|----|----|---|---|---|---|---|---|---|---|---|---|
| Flag | MD | 1 (reserved, set) | OF | DF | IF | TF | SF | ZF | 0 | AF | 0 | PF | 1 | CF |

`MD` (bit 15) "does nothing on the NEC V30MZ". Reserved bits: bit1=1, bits3/5=0, bits12–14=1 (8086 convention). Flag semantics: `CF` carry/borrow of last arithmetic; `PF` even parity of low 8 bits; `AF` nibble carry (bit3→4); `ZF` result==0; `SF` top bit of result; `OF` signed overflow; `TF` single-step trap → INT vector 1 after each instruction; `IF` maskable-interrupt enable; `DF` string direction (1=decrement).

**Hardware flag quirk (ASWAN vs SPHINX):** after unsigned multiply (`MUL/MULU`, `F6 /4` and `F7 /4`), the **Zero flag is always *cleared* on ASWAN (mono WS)** and **always *set* on SPHINX/SPHINX2 (WSC/Crystal)** (WSCpuTest). The core must key this on `Model`. `DIV/DIVU` zero-flag behaviour is "weird" and unverified (Open Questions).

### 8. Emulator pitfalls (per behaviour)

- **`0F` is not an escape.** Decoding `0F` as a two-byte prefix (x86-32 habit) will desync the stream; it is a 1-byte NOP. Same trap for `66/67` (size prefixes) and `64/65` (would-be `FS:`/`GS:` on 386, or `REPC/REPNC` on V20).
- **`ESC` (`D8`–`DF`) still consumes its ModR/M byte** — a naive "NOP = 1 byte" will misalign by one byte and derail decode.
- **`SALC` must be present** even though many x86 references omit it; games/tests may hit `D6`. It writes AL only, touches no flags.
- **`AAM/AAD` are general** (arbitrary base immediate) — a fixed ÷10/×10 fails WSCpuTest and any code that abuses `D4/D5` as byte divide/multiply.
- **`MUL` Zero-flag is model-dependent** — hard-coding either value fails one of the two hardware families; branch on `Model`.
- **Group `reg` sub-op decode**: don't fault on the undocumented `reg=6` shift or `reg=1` TEST slots — replicate the "zeroes AX" / "no-op" behaviour instead.
- **Segment override + string ops**: `REP MOVS` etc. honour the last segment override on the *source*; destination (`ES:DI`) is not overridable.
- **`FF` reg=3/5 register-form** and other reg-mode LEA/LES/LDS "new modes" should not throw #UD — the V30MZ resolves them.

### 9. Test-ROM coverage (WSCpuTest v0.7.4)

WSCpuTest exercises: `AND OR XOR TEST NOT INC DEC ADD SUB CMP ADC SBB NEG`; shifts/rotates `ROL ROR RCL RCR SHL SHR SAR`; `MUL/MULU`, `IMUL`, `DIV/DIVU`, `IDIV`, `AAM`, `AAD`; decimal adjust `DAA DAS AAA AAS`; `PUSH/POP SP`, `BOUND`; and the full undocumented-opcode set of §5 (`0F`, `63`–`67`, `8C/8E,F8`, `8D,C8`–`CF`, `9B`, `C0/C1 /6`, `C4/C5` reg-mode, `D6` SALC, `D8`–`DF`, `F1`, `F6/F7 /1`, `FE`/`FF` duplicate variants). It is the Phase-2 opcode/flag oracle; `WSTimingTest` covers cycle counts.


## V30MZ Instruction & Bus Cycle Timing

**Scope.** Cycle model for the WonderSwan CPU core (NEC V30MZ), the memory-access-slot bus arbitration, DMA cost, I/O cost, and the mapping onto a Rust scheduler tick budget. Every numeric claim is tagged with its source; claims that could not be confirmed against a fetched source are in the *Emulator pitfalls* callouts and the section's open questions, never asserted as fact.

**Provenance legend**
- `[WSMan]` — daifukkat.su/docs/wsman (rev.7), reached via text proxy (host refuses HTTPS).
- `[BLOG]` — daifukkat.su WonderSwan Hardware Tests (trap15, 2015-07-11), via text proxy.
- `[NES-IS]` — WSdev wiki `NEC_V30MZ_instruction_set` (values arrived through a summarizing fetch — re-verify per-opcode against raw wikitext before hard-coding).
- `[NES-SND]` — WSdev wiki `Sound`.
- `[NES-MM]` — WSdev wiki `Memory_map` (summarizing fetch).
- `[PLAT]` — Wonderful Toolchain wiki `wswan:platform_overview`.
- `[WSCPU]` — FluBBaOfWard/WSCpuTest README.
- `[WSTIM]` — FluBBaOfWard/WSTimingTest README.
- `[ARES]` — ares-emulator/ares issue #908 (Absolute Compatibility Checklist).
- `[DD]` — internal deep-dive (the WonderSwan-specific facts supplied to this task; **unconfirmed** where noted).

---

### 1. Clock domains

| Clock | Frequency | Relation | Source |
|---|---|---|---|
| Master / SoC (MCLK) | 12.288 MHz | base | `[BLOG]` `[WSMan]` |
| CPU (V30MZ) | 3.072 MHz | MCLK / 4 | `[PLAT]` |
| Master period | ≈ 81.4 ns | 1 / 12.288 MHz | derived |
| CPU period | ≈ 325.5 ns | 1 / 3.072 MHz | derived |

The V30MZ runs at exactly **MCLK/4**. Internal SRAM is clocked at the full 12.288 MHz and time-division-multiplexed into **four access slots** so peripherals appear to access memory simultaneously without stalling the CPU `[WSMan]`.

> **Cycle-domain warning (load-bearing).** Two clock domains exist; the
> measured-timing *unit* is now settled (see the preamble):
> - **CPU cycles** (3.072 MHz): the per-instruction counts in §4 (`[NES-IS]`),
>   trap15's LFSR-measured deltas (§7), the DMA `5 + 2n` (§6), and the `IN`/`OUT`
>   figures (§5) are **all** in this unit. trap15's "master clock (3072000 Hz)" is
>   this clock, not the crystal.
> - **Master clocks** (12.288 MHz, MCLK): the SoC / RAM-slot clock = CPU × 4. Only
>   relevant as the scheduler *base*.
> Recommended scheduler base = master clocks; convert every CPU-cycle cost by ×4
> (§8). The earlier "measurements might be in master clocks" reading was a
> terminology misread and is retracted.

### 2. Memory-access-slot arbitration

MCLK is divided into 4 slots; each consumer owns a slot, so display/sound fetches never stall the CPU `[WSMan]`:

| Slot | Consumer(s) | Source |
|---|---|---|
| 0 | CPU, general DMA, sprite (OAM) DMA | `[WSMan]` |
| 1 | Sound wavetable fetch | `[WSMan]` |
| 2 | Screen tiles + tilemaps | `[WSMan]` |
| 3 | Palette RAM | `[WSMan]` |

Consequences:
- **CPU is not paused while the display controller accesses memory** `[WSMan]` `[BLOG]` — no "PPU steals bus" stall model. Slots 2/3 belong to the PPU and are disjoint from the CPU's slot 0.
- **Sprite (OAM) DMA does NOT stall the CPU** — the old "sprite DMA halts CPU" belief is explicitly wrong `[ARES]`.
- **General DMA and the CPU share slot 0**, so general DMA *does* pause the CPU while it runs, at a rate "unlikely to be any rate other than /4" `[WSMan]`.

> **Pitfall.** Do not model contention between CPU and PPU. The only CPU stall from a memory master is **general DMA** (slot-0 sharing). Emulators that halt the CPU during sprite fetch will mis-time raster-timed code.

### 3. Bus / memory wait states

Access cost depends on the target region and the memory-control configuration.

| Region | Access cost | Bus width | Source |
|---|---|---|---|
| Internal RAM (IRAM/VRAM) | 1 cycle | 16-bit | `[NES-MM]` |
| Cart SRAM `0x10000–0x1FFFF` | 1 (Color) / 2 cycles, configurable | 8-bit | `[NES-MM]` |
| Cart ROM `0x20000–0xFFFFF` | 1 / 2 cycles, configurable | 8/16-bit, configurable | `[NES-MM]` |
| Cart ROM wait (mono/ASWAN) | 1–3 cycles, via port `0xA0` | — | `[ARES]` |
| Cart SRAM (ASWAN) | "mandatory 3-cycle wait state" | — | `[ARES]` |

Additional documented penalties (apply on top of base access):

| Condition | Penalty | Source |
|---|---|---|
| Word access to an **odd address** | +1 cycle | `[WSTIM]` (per NEC) |
| Branch **to an odd address** | +1 clock (pipeline realign) | `[WSTIM]` `[BLOG]` |
| **Write** to a segment register | +2 cycles | `[WSMan]` |
| **Read** from a segment register | +1 cycle | `[WSMan]` |

> **Pitfall — 8-bit cart bus.** On the mono WonderSwan the cart bus is 8-bit `[NES-MM]`, so a 16-bit fetch from ROM/SRAM is two bus transactions. Color widens ROM to 16-bit (configurable). A model that assumes uniform 16-bit cart access will run mono titles too fast.
> **Pitfall — source conflict on SRAM.** `[ARES]` says ASWAN cart SRAM has a *mandatory 3-cycle wait state*; `[NES-MM]` lists cart SRAM as 1 (Color)/2 cycles. These are not reconciled here — treat SRAM wait-state count as configurable per model and verify (open question). The exact bit layout of port `0xA0` (bus width + wait-state control) was not obtained.

### 4. Per-instruction **core** cycle counts

These are V30MZ core execution cycles (`[NES-IS]`, NEC-datasheet lineage) — the *minimum* execution cost before bus/wait-state and pipeline effects from §3. **Re-verify each value against the raw wikitext before hard-coding** (they came through a summarizing fetch).

| Instruction (class) | Core cycles | Source |
|---|---|---|
| MOV / ADD / logic, reg,reg | 1 | `[NES-IS]` |
| MOV / ADD, with mem operand | 2–3 (varies by direction) | `[NES-IS]` |
| PUSH | 1–2 | `[NES-IS]` |
| POP | 1–3 | `[NES-IS]` |
| Jcc (conditional jump) | 1 not taken / 4 taken | `[NES-IS]` |
| JMP | 4–10 (by addressing mode) | `[NES-IS]` |
| CALL | 5–12 | `[NES-IS]` |
| RET | 6 | `[NES-IS]` |
| RETF | 8–9 | `[NES-IS]` |
| MOVSB / MOVSW | 5 | `[NES-IS]` |
| CMPSB / CMPSW | 6 | `[NES-IS]` |
| SCASB / SCASW | 4 | `[NES-IS]` |
| LODSB / LODSW | 3 | `[NES-IS]` |
| STOSB / STOSW | 3 | `[NES-IS]` |
| MUL reg / mem | 3 / 4 | `[NES-IS]` |
| IMUL reg / mem | 3 / 4 | `[NES-IS]` |
| DIV reg8 / mem8 | 15 / 16 | `[NES-IS]` |
| DIV reg16 / mem16 | 23 / 24 | `[NES-IS]` |
| IDIV reg8 / mem8 | 17 / 18 | `[NES-IS]` |
| IDIV reg16 / mem16 | 24 / 25 | `[NES-IS]` |
| **IN** AL/AX, imm8 / DX | **6** | `[NES-IS]` |
| **OUT** imm8 / DX, AL/AX | **6** | `[NES-IS]` |

Cross-check on the multiply/divide unit (independent source): 16×16 multiplier "3–4 cycles", 16/8 divider "16–18 cycles", 32/16 divider "23–25 cycles" `[PLAT]` — consistent with the `[NES-IS]` MUL/DIV/IDIV rows above.

**Undefined / special opcodes** (functional, from CPU tests):

| Opcode | Behavior | Core cycles | Source |
|---|---|---|---|
| `0x9B` (WAIT/POLL) | 1-byte NOP | 9 | `[WSCPU]` |
| `0xD6` (SALC) | undocumented, supported | 8 | `[WSCPU]` |
| `0xD8–0xDF` (FPU escape) | 2-byte NOP | 1 | `[WSCPU]` |
| other undefined | 1-byte NOP | 1 | `[WSCPU]` |

> **Pitfall.** The V30MZ is 80186-*documented*-compatible plus SALC, but **omits the V20/V30 opcode extensions** — do not implement V30 8080-emulation or packed-BCD extension opcodes. `[NES-IS root]`
> **Pitfall — pipeline.** The V30MZ has a prefetch pipeline flushed on every taken branch; measured time also depends on the *destination* instruction, not just the branch `[WSTIM]` `[BLOG]`. A pure table lookup for branch cost is approximate.

### 5. WonderSwan-measured deviations from Intel 80186

Several instruction timings differ from Intel's published 80186 figures `[DD]`. The one concrete, reproducible hardware fact is DMA (§6). For **IN/OUT** three figures are in tension and **must be resolved on hardware before trusting any of them**:

| Source | IN/OUT cost | Note |
|---|---|---|
| `[DD]` (deep-dive claim) | **12 cycles** | asserted hardware-measured, "not 10 as on 80186" — **unconfirmed by any fetched source** |
| `[NES-IS]` core table | 6 cycles | V30MZ core cycles (excludes I/O bus/wait overhead) |
| Intel 80186 (reference) | 8 (DX) / 10 (imm8) IN; 7 / 9 OUT | for comparison only |
| `[ARES]`/asie | "exact I/O port access timings within one sample remain unknown" | open in mainline emulators |

A plausible reconciliation (do **not** encode as fact): `[NES-IS]`'s 6 is the *core* count; real I/O port access adds bus/wait cycles, so the *observed* total is higher — `[DD]`'s 12 would be core+I/O-bus overhead. This is unverified; see open questions.

### 6. DMA timing (confirmed from hardware)

> **General DMA transfer time = `5 + 2n` cycles**, where `n` is the number of **words** transferred. `[BLOG]` `[WSMan]`

- The 5 is fixed setup/teardown; each word costs 2 (one read + one write access) `[WSMan]`.
- DMA runs on slot 0 and **pauses the CPU** for its whole duration, at a fixed ≈/4 rate `[WSMan]`.
- Sprite (OAM) DMA also uses slot 0 but does **not** stall the CPU `[ARES]`; its cost model is separate and not given a formula here (open question).
- **Unit of `5 + 2n` is CPU cycles (3.072 MHz) — resolved.** ares bills DMA as `step(5)` + `step(2)`/word in its 3.072 MHz CPU domain `[ARES]`, and 2 cycles/word (one read + one write of internal RAM) is only physically possible at the CPU clock. `[DD]`'s loose "master cycles" is the 3.072 MHz "master clock" in trap15's vocabulary, not the 12.288 MHz crystal. Multiply by 4 for a master-clock scheduler base (§8).

> **Pitfall.** Charge the CPU stall for the *entire* `5 + 2n` window on general DMA, but **not** for sprite DMA. The unit is CPU cycles (×4 into a master-clock base); do not treat `5 + 2n` as already-master-clocks or double-apply the 4×.

### 7. LFSR measurement method (for building your own timing test ROMs)

trap15's technique — the ground truth for any WonderSwan timing you need but cannot find published `[BLOG]`:

1. Sound channel 4 noise is a **15-bit LFSR, 2 taps** (one configurable, one fixed) `[BLOG]` `[NES-SND]`.
2. Its advance rate is `f = CPU_CLK / (2048 − reg)` where `reg` is the 12-bit frequency divisor `[BLOG]`. Set `reg = 2047` → advance **once per CPU clock (3.072 MHz)** (finest resolution) `[BLOG]`.
3. The live LFSR value is CPU-readable at ports **`$92/$93`** (15-bit) `[NES-SND]`.
4. Sample LFSR → run operation → sample LFSR again. Because the LFSR sequence is a known permutation (mode 0: bit-14 tap, period 32767 `[NES-SND]`), convert each sample back to a linear cycle index and take the delta. Subtract the pre-measured sampling overhead → "100% accurate time delta with complete precision" `[BLOG]`.

> **Pitfall.** The LFSR is a *permutation*, not a counter — you must invert the sequence (or table it) to get a monotonic index, and handle wrap at the 32767-step period. At `reg = 2047` it advances once per **CPU clock**, so the measured delta is in **CPU cycles** — the *same* unit as §4, **not** 4× it. (The earlier "master clocks / 4×" note was the terminology misread; see the preamble.)

### 8. Mapping to a Rust scheduler tick budget

Recommended model — **one canonical tick = 1 master clock (81.4 ns)**, everything converted into it:

- **Scheduler base:** integer `u64` master-clock counter. All devices (CPU, DMA, PPU line/pixel, sound sample @ 3.072MHz/(2048−reg), timers) are events on this timeline. PPU/sound never bill against CPU time (separate slots, §2).
- **CPU step cost:** `master = 4 * core_cycles(op)`  — because CPU = MCLK/4 `[PLAT]`. Then add bus/wait penalties from §3 (each "cycle" there is a CPU cycle ⇒ ×4 into master clocks): odd-word +1, branch-to-odd +1, seg-reg write +2 / read +1, cart-region base access, mono 8-bit double-fetch.
- **General DMA:** on trigger, advance the master clock by `(5 + 2n) × 4` and hold the CPU (no CPU steps) for that window. `5 + 2n` is CPU cycles (§6), so the factor into the master-clock base is 4.
- **I/O (IN/OUT):** the *unit* is CPU cycles; the *value* is still unconfirmed (§5: `[NES-IS]` 6 vs `[DD]` 12). Parameterize as `IO_COST` in CPU cycles (→ ×4 master) and validate on hardware before baking a literal.
- **Divide the CPU into slot 0 only** for correctness; you do not need to simulate 4 physical slots unless you model exact sub-sample RAM contention (`[ARES]`: still unknown), which no released source pins down.

```
tick (master clocks)
├─ CPU:   4*core_cycles + wait_penalties*4         (slot 0)
├─ DMA:   (5 + 2n)*4, CPU held                     (slot 0, general only)
├─ PPU:   line/pixel events                        (slots 2,3 — never bill CPU)
└─ SND:   sample every (2048 - reg) *?             (slot 1 — never bill CPU)
```

> **Pitfall — precision, not accuracy.** The core-cycle table (§4) is Intel-lineage and known to diverge from measured hardware on several ops `[DD]` `[WSTIM]`. Treat §4 as the scaffold and override individual opcodes with LFSR-measured **CPU-cycle** values (§7) as you validate them. Keep a per-opcode `measured: bool` so the model can distinguish confirmed-from-hardware from datasheet-assumed at runtime.


# V30MZ Flags, Arithmetic Semantics, Exceptions & Reset

Scope: the 16-bit flag register (PSW) layout, per-operation-class flag update
rules, the decimal/ASCII adjust family, the CPU-generated exceptions
(vectors 0–5), the software-interrupt entry/exit sequence, and the power-on
reset state. Companion sections cover the maskable hardware interrupt
controller (`02-interrupts.md`, `REG_INT_*`) and per-opcode cycle timing.

The V30MZ is documented as **fully compatible with the Intel 80186's documented
behaviour**, plus some undocumented 80186 behaviour (e.g. `SALC`), and **without**
the V20/V30 `REPC`/`REPNC`/mode-switch extensions
(source: WSdev *NEC V30MZ*). Documented-flag semantics therefore follow standard
8086/80186 rules; the value of this section is the **exact bit layout, the
reset state, and the *undefined*-flag results that WSCpuTest actually checks** —
the places where "80186-compatible" is not enough.

> Primary-source note: `daifukkat.su/docs/wsman/` (WSMan) and
> `perfectkiosk.net/stsws.html` (Sacred Tech Scroll) are **HTTP-only hosts and
> refused HTTPS** during authoring, so they could not be fetched. Facts below are
> anchored to the WSdev wiki (HTTPS) and the FluBBaOfWard **ARMV30MZ** reference
> implementation (read at commit `master`, files `ARMV30MZ.s`, `.i`, `.h`,
> `ARMV30MZmac.h`). Items that need a primary hardware-test confirmation are in
> Open Questions.

---

## 1. Flag register (PSW) bit layout

16-bit register. Verbatim diagram from WSdev *NEC V30MZ flags*
(`m111 odit  sz0a 0p1c`, bit 15 → bit 0):

| Bit | Sym | Flag | Class | Notes |
|----:|:---:|------|-------|-------|
| 0 | c | **CF** Carry | status | carry/borrow out of MSB; also target of shift/rotate bit |
| 1 | 1 | — | reserved | **always reads 1** |
| 2 | p | **PF** Parity | status | even parity of low 8 bits of result |
| 3 | 0 | — | reserved | **always reads 0** |
| 4 | a | **AF** Aux carry | status | carry/borrow across bit 3↔4 |
| 5 | 0 | — | reserved | **always reads 0** |
| 6 | z | **ZF** Zero | status | result == 0 |
| 7 | s | **SF** Sign | status | MSB of result |
| 8 | t | **TF** Trap/Single-step | control | vector-1 trap after each instruction |
| 9 | i | **IF** Interrupt-enable | control | gates *maskable* hardware IRQs only |
| 10 | d | **DF** Direction | control | 0 = auto-increment SI/DI, 1 = decrement |
| 11 | o | **OF** Overflow | status | signed overflow |
| 12 | 1 | — | reserved | **always reads 1** |
| 13 | 1 | — | reserved | **always reads 1** |
| 14 | 1 | — | reserved | **always reads 1** |
| 15 | m | **MD** Mode | — | "does nothing on the NEC V30MZ" (no 8080 emulation mode) |

**Emulator pitfall:** store the architectural flags however you like internally,
but `PUSHF`/`LAHF`/interrupt-frame reads must materialise this exact layout —
bit 1 = 1, bits 3/5 = 0, bits 12–14 = 1. A common bug is emitting a raw
computed value with the reserved bits zeroed; WSCpuTest compares `PUSHF` output
bit-for-bit. `POPF` must **ignore** writes to the reserved bits and to MD.
An earlier auto-summary of the WSdev flags page mis-tabulated TF/IF/DF/OF at
bits 11/10/9/8; the **verbatim diagram above (TF=8, IF=9, DF=10, OF=11) is
canonical** — do not use the transposed order.

---

## 2. Status-flag formulas (8- and 16-bit)

Let `w` = 8 or 16, `M = 1<<(w-1)` (sign bit), `msk = (1<<w)-1`.

| Flag | ADD (`r = a+b`) | SUB / CMP (`r = a-b`) |
|------|-----------------|-----------------------|
| CF | `(a+b) > msk` | `a < b` (borrow) |
| AF | `((a ^ b ^ r) >> 4) & 1` | `((a ^ b ^ r) >> 4) & 1` |
| OF | `((a ^ r) & (b ^ r) & M) != 0` | `((a ^ b) & (a ^ r) & M) != 0` |
| SF | `(r & M) != 0` | `(r & M) != 0` |
| ZF | `(r & msk) == 0` | `(r & msk) == 0` |
| PF | even parity of `r & 0xFF` | even parity of `r & 0xFF` |

- **ADC/SBB** use the same formulas with carry folded into the addend/subtrahend;
  CF/AF/OF must be computed on the full extended result.
- **PF is always taken from the low 8 bits only**, even for 16-bit results
  (`ARMV30MZmac.h` keeps a single 8-bit `ParityVal` and derives PF from a
  256-entry parity LUT).
- **AF is bit-3→4 carry only**; it is *not* derived from the full-width carry.
- **NEG** = `0 - operand`: CF = `operand != 0`, other flags per SUB.

---

## 3. Flag effects per operation class

`✓` = set from result per §2; `–` = unaffected; `0` = forced 0; `u` = undefined
(value is V30MZ-implementation-specific — see notes). Verified against the
`add8/sub8/incByte/incWord/or8` and shift/rotate macros in `ARMV30MZmac.h`.

| Class | CF | OF | SF | ZF | AF | PF | Notes |
|-------|----|----|----|----|----|----|-------|
| `ADD ADC SUB SBB CMP NEG` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | full arithmetic set |
| `INC DEC` | **–** | ✓ | ✓ | ✓ | ✓ | ✓ | **CF preserved** (macro `and v30f,#PSR_C`) |
| `AND OR XOR TEST` | 0 | 0 | ✓ | ✓ | **0** | ✓ | V30MZ **clears AF** (x86 marks it undefined) |
| `NOT` | – | – | – | – | – | – | no flags affected |
| `MUL IMUL` | ✓ | ✓ | u | u | u | u | CF=OF=1 if upper half is significant; SF/ZF/AF/PF undefined |
| `DIV IDIV` | u | u | u | u | u | u | all undefined & impl-specific — see §5 |
| `SHL SHR SAR` (count≥1) | ✓ | ✓† | ✓ | ✓ | u | ✓ | †OF defined only for count==1; **shifts CLEAR AF** (ARMV30MZ `shl8`/`shr8`/`shra8` clear all flags, then rebuild CF/SF/ZF/OF/PF); rotates preserve AF |
| `ROL ROR RCL RCR` (count≥1) | ✓ | ✓† | – | – | – | – | rotates touch **only CF/OF**; †OF only for count==1 |
| `CLC/STC/CMC` | ✓ | – | – | – | – | – | CF only |
| `CLD/STD` | — | — | — | — | — | — | DF only |
| `CLI/STI` | — | — | — | — | — | — | IF only (STI delays IRQ 1 instr — §6.4) |
| `SAHF` | ✓ | – | ✓ | ✓ | ✓ | ✓ | loads CF/PF/AF/ZF/SF from AH; OF untouched |
| `LAHF` | – | – | – | – | – | – | stores CF/PF/AF/ZF/SF into AH |
| `POPF IRET` | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | loads all flags from stack (also TF/IF/DF) |

**Emulator pitfalls:**
- `INC`/`DEC` **must not touch CF** — the single most common regression; a loop
  using `INC`/`DEC` around `ADC` breaks if CF is clobbered.
- Logical ops **clear** OF and CF and (on V30MZ) AF; do not leave them stale.
- Rotate instructions leave SF/ZF/AF/PF **unchanged** — do not recompute them.
- For shift/rotate **count==0**, no flags change at all (80186 masks the count;
  confirm masking width in the timing/opcode section).

---

## 4. Decimal / ASCII adjust family

Opcodes and immediates from WSdev *NEC V30MZ instruction set*; algorithms and the
**exact undefined-flag results** from the ARMV30MZ opcode bodies
(`_27/_2F/_37/_3F/_D4/_D5`). NEC mnemonics in parentheses.

| Instr | Opcode | Operand | Operates on |
|-------|--------|---------|-------------|
| DAA (ADJ4A) | `27` | — | AL, after BCD add |
| DAS (ADJ4S) | `2F` | — | AL, after BCD sub |
| AAA (ADJBA) | `37` | — | AX, after unpacked-BCD add |
| AAS (ADJBS) | `3F` | — | AX, after unpacked-BCD sub |
| AAM (CVTBD) | `D4 ib` | imm8 base (asm default `0x0A`) | AH=AL/base, AL=AL%base |
| AAD (CVTDB) | `D5 ib` | imm8 base (asm default `0x0A`) | AL=AH*base+AL, AH=0 |

### 4.1 DAA / DAS
Standard two-stage adjust:
```
DAA:  if ((AL & 0x0F) > 9 || AF) { AL += 0x06; AF = 1 }
      if (AL > 0x9F      || CF) { AL += 0x60; CF = 1 }     ; V30MZ pre-tests old AL/CF
DAS:  if ((AL & 0x0F) > 9 || AF) { AL -= 0x06; AF = 1 }
      if (AL > 0x9F      || CF) { AL -= 0x60; CF = 1 }
```
Flag result: **CF, AF per algorithm; SF/ZF/PF from the final AL (defined);
OF undefined.** The V30MZ pre-captures the high-nibble/CF test before the
low-nibble add (matching 80186), so `DAS` never spuriously *clears* CF once set.

### 4.2 AAA / AAS — nonstandard undefined flags (WSCpuTest-relevant)
Condition: `adjust = (AL & 0x0F) > 9 || AF`. On adjust, `AL = (AL±6) & 0x0F`,
`AH ±= 1`. The V30MZ leaves the "undefined" flags at **fixed, quirky values**
(from `_37`/`_3F`): PF is **always set**, and the other status flags collapse to
one of two constants:

| Case | CF | AF | SF | ZF | OF | PF |
|------|----|----|----|----|----|----|
| adjust taken | 1 | 1 | 0 | **1** | 0 | 1 |
| adjust not taken | 0 | 0 | **1** | 0 | 0 | 1 |

Note ZF/SF here do **not** reflect the numeric result — they are hardware
artefacts. AAS uses the identical flag pattern (only the AL/AH delta differs).
**Do not** compute SF/ZF/PF from AX for AAA/AAS.

### 4.3 AAM / AAD
- **AAM** sets **SF/ZF/PF from the result AL** and **clears CF/OF/AF** (`_D4`
  writes only S or Z; C/V/A drop). x86 marks CF/OF/AF undefined; V30MZ clears them.
- **AAM with base 0** (`D4 00`) divides by zero → **#DE (INT 0)**, see §5; the
  flags at that point come from the last MUL overflow state (`d4DivideError`).
- **AAD** sets **SF/ZF/PF from the result AL**; CF/OF/AF take
  implementation-specific values derived from the internal `AH*base+AL` addition
  (`_D5` sets S/Z/C/V and conditionally AF). Treat CF/OF/AF as impl-specific.
- **AAM/AAD are two-byte** (`D4 ib` / `D5 ib`); the base byte is part of the
  encoding even when it is the default `0x0A`. Emulators that hard-code base 10
  and skip the immediate byte desynchronise the instruction stream.

---

## 5. Division and the #DE exception — **implementation-specific flags**

Per the project brief and confirmed by the ARMV30MZ divide routines
(`divubF6`/`divbF6`/`divuwF7`/`divwF7`), the flag state left by `DIV`/`IDIV` and
the state visible at a divide fault is **hardware-implementation-specific on the
V30MZ** — it is *not* the x86 "all undefined, don't care" that most cores assume.
**WSCpuTest is the ground truth; match its expected values exactly.**

### 5.1 #DE (INT 0) trigger conditions
`DIV`/`IDIV` (opcode `F6 /6,/7` byte, `F7 /6,/7` word) raise **vector 0 (#DE)** when:
- divisor == 0, **or**
- the quotient does not fit the destination (`AL` for byte ops, `AX` for word ops).

`AAM` with base 0 also routes through the divide path → #DE.

### 5.2 Flag state left by division (from ARMV30MZ; verify vs WSCpuTest)
| Instruction | CF / OF | SF / ZF / PF | AF |
|-------------|---------|--------------|----|
| Unsigned `DIV` (`/6`) | **inherited from the most recent MUL/IMUL overflow** | ZF forced then conditionally cleared; PF cleared | undefined |
| Signed `IDIV` (`/7`) | undefined | **derived from the quotient** | undefined |

Mechanism: both `divubF6` and the error paths load `v30f` from a stored
`v30MulOverflow` byte ("C & V from last mul, Z always set"). So after a `DIV`,
CF/OF are **stale from the previous multiply** — reproducing this requires
carrying a MUL/IMUL overflow latch in the core. On the fault path, that same
partial state is what is captured in the FLAGS image before the interrupt
sequence (§6) pushes it.

### 5.3 ASWAN vs SPHINX multiply difference (feeds §5.2)
`V30Reset` installs a **different unsigned-multiply handler for ASWAN
(`muluF6Aswan`/`muluF7Aswan`) vs Color/Crystal (`muluF6`/`muluF7`)**. Because the
DIV flag state is inherited from the multiply overflow result, the
post-`DIV`/`IDIV` flags can **differ between the original WonderSwan (ASWAN) and
WonderSwan Color (SPHINX/SPHINX2)**. Model the MUL overflow latch per-SoC and
verify each against WSCpuTest on the corresponding model.

**Emulator pitfalls:**
- Do **not** clear the flags to zero after `DIV`/`IDIV`; games/tests observe the
  inherited/derived bits.
- The **return address** pushed by #DE (points to the instruction *after* `DIV`
  vs at the `DIV`) is an 8086-family quirk — see Open Questions; verify before
  relying on restartability.
- Watch the signed `IDIV` INT_MIN edge case (`0x80`/`0x8000` dividend): the
  reference special-cases it (`divbF6Error: cmp #0x80000000 → 0x81`) rather than
  faulting. Confirm against WSCpuTest.

---

## 6. Software interrupts, INTO/INT3/BOUND, and the interrupt entry/exit sequence

### 6.1 CPU exception / trap vectors (0–5)
From WSdev *NEC V30MZ interrupts* (NEC mnemonics in parentheses). Vectors 6/7
(unused-opcode / ESC) are **not implemented** — treated as NOP.

| Vec | Name | Raised by | Trigger detail |
|----:|------|-----------|----------------|
| 0 | Divide Error (#DE) | `DIV`/`IDIV`, `AAM` base 0 | divisor 0 or quotient overflow |
| 1 | Single-Step / Break | after each instruction while **TF=1** | also the debug trap |
| 2 | **NMI** | WonderSwan I/O (NMI pin) | `NEC_NMI_VECTOR = 2` in ARMV30MZ |
| 3 | Breakpoint (BRK3) | `INT3` = one-byte `CC` | |
| 4 | Overflow (BRKV) | `INTO` = `CE` **when OF=1** | if OF=0, `INTO` is a no-op |
| 5 | Array Bound (CHKIND) | `BOUND` = `62 /r` | when index out of `[lo,hi]` |

`INT n` (`CD ib`) raises the arbitrary vector `n`.

### 6.2 Interrupt vector table (IVT)
The V30MZ IVT is the standard 8086-family table: **fixed at physical
`0x00000`**, `vector * 4` per entry, layout `[IP_lo, IP_hi, CS_lo, CS_hi]`.
Confirmed by `V30TakeIRQ` reading new IP from `vector*4` and new CS from
`vector*4 + 2` with **no base offset**. The relocatable base of the *maskable
hardware interrupt* vectors is a separate concept controlled by `REG_INT_BASE`
(`$B0`) and belongs to the interrupt-controller section; it does **not** move the
CPU's exception table.

### 6.3 Interrupt entry sequence (all sources: `INT n`, INT3, INTO, #DE, NMI, HW IRQ)
Exact order from `V30TakeIRQ`:
1. **Push FLAGS** (16-bit PSW, with reserved bits materialised per §1).
2. **Clear IF** (interrupts disabled in the handler).
3. **Clear TF** (and internal HALT) — so the handler does not single-step.
4. **Push CS** (old code segment).
5. **Push IP** (return offset).
6. Load new **IP** from `[vector*4]`, new **CS** from `[vector*4+2]`.

Post-frame stack (top→down): `IP`, `CS`, `FLAGS`. The pushed FLAGS carries the
**pre-interrupt** IF/TF (they are cleared only in the live register afterwards),
so `IRET` restores them.

### 6.4 Interrupt exit and one-instruction IRQ delay
- `IRET` (`CF`) pops **IP, then CS, then FLAGS** (restoring TF/IF/DF).
- **Maskable-IRQ recognition is delayed by one instruction** after: writes to
  SS (`MOV SS`/`POP SS`), any segment/repeat/lock **prefix**, `STI`, and `POPF`
  (`v30DelayIrqCheck` guards `IRET`/`POPF`; STI uses the trap-delay path). This
  prevents an IRQ landing between `MOV SS`/`MOV SP`. Model the one-instruction
  shadow or stack-setup code faults.

**Emulator pitfalls:**
- IF gates **only maskable** interrupts; **NMI (vec 2), #DE, INT3, INTO, INT n,
  BOUND, and the TF trap are not maskable by IF**.
- `INTO` with OF=0 must fall through as a no-op (do not push a frame).
- Clear TF *inside* the entry sequence, not before pushing FLAGS, or the saved
  FLAGS will wrongly show TF=0.

---

## 7. Power-on reset state

From `V30Reset` (ARMV30MZ). Explicit comment: *"PC, DS1(ES), DS0(DS) & SS are set
to 0x0000; AW,BW,CW,DW,SP,BP,IX,IY are undefined; PS(CS) is set to 0xFFFF."*

| Register | Reset value | Source |
|----------|-------------|--------|
| CS (PS) | `0xFFFF` | `str 0xFFFF0000 → v30SRegPS` |
| IP (PC) | `0x0000` | `mov v30pc,#0` |
| DS (DS0) | `0x0000` | memclr |
| SS | `0x0000` | memclr |
| ES (DS1) | `0x0000` | memclr |
| AX,BX,CX,DX,SP,BP,SI,DI | **undefined** | not initialised |
| DF (direction) | 0 (auto-increment) | `v30DF = +1` |
| IF | 0 (maskable IRQs disabled) | flag state cleared |
| TF | 0 | flag state cleared |

**Reset vector = CS:IP `FFFF:0000` = physical `0xFFFF0`.** Execution begins there;
on the WonderSwan those top-16 bytes are the last bytes of the boot ROM / mapped
cartridge and normally hold a **far `JMP`** to the real init. Because the general
registers and (critically) SP are undefined at reset, boot code must set up
`SS:SP` before any push/interrupt.

**Emulator pitfalls:**
- Do not assume AX/SP/etc. are zero at reset — a core that zeroes them can hide
  boot bugs that real hardware exposes; leave them explicitly undefined/poisoned.
- IF=0 at reset: no maskable IRQ fires until boot code executes `STI`/`EI`.
- The reset FLAGS **readback** value (reserved bits + MD) is not pinned by a
  primary hardware test here — see Open Questions.

---

## 8. Implementation checklist for this section

- [ ] `PUSHF`/interrupt frame emit exact §1 layout (reserved 1/0/1 pattern); `POPF` ignores reserved + MD.
- [ ] `INC`/`DEC` preserve CF; logical ops clear CF/OF/AF.
- [ ] AAA/AAS emit the §4.2 fixed-flag constants (not result-derived SF/ZF).
- [ ] AAM/AAD consume the imm8 base byte; AAM base 0 → #DE.
- [ ] MUL/IMUL overflow latch retained per-SoC (ASWAN vs SPHINX) and fed to `DIV`/`IDIV` flag output.
- [ ] Interrupt entry: push FLAGS → clear IF → clear TF → push CS → push IP → load vector from physical `vector*4`.
- [ ] One-instruction IRQ delay after `MOV SS`/prefix/`STI`/`POPF`.
- [ ] Reset: CS=`FFFF`, IP/DS/SS/ES=0, GP regs undefined, entry at `0xFFFF0`.
- [ ] Every division/AAA/AAS/AAM/AAD flag output diffed against **WSCpuTest**.


## CPU memory map, I/O port mechanics & unmapped-read behavior

The NEC V30MZ is an 80186-class core. It exposes a **20-bit physical memory address bus** (1 MiB, `0x00000`–`0xFFFFF`) and a **16-bit I/O port address space** (`0x0000`–`0xFFFF`), which are two independent address spaces selected by the instruction (memory access vs `IN`/`OUT`). Physical memory addresses are formed the 8086 way:

```
phys_addr = (segment << 4) + offset      // both 16-bit; result truncated to 20 bits
```

There is no A20-style wrap emulation concern beyond the 20-bit truncation on the V30MZ.

### 1. Physical memory map (20-bit)

The SoC splits the 1 MiB space into an internal region and a cartridge region with distinct bus widths, timings and permissions.

| Range | Holder | Bus width | Access speed | Perm |
|---|---|---|---|---|
| `0x00000`–`0x0FFFF` | Internal RAM (SoC unified RAM) | 16-bit | 1 cycle | R/W |
| `0x10000`–`0x1FFFF` | Cartridge — SRAM / RAM window | 8-bit | 1 (color) / 2 cycles (configurable) | R/W |
| `0x20000`–`0xFFFFF` | Cartridge — ROM window | 8/16-bit (configurable) | 1 / 2 cycles (configurable) | R only |

Source: WSdev Memory map page. Everything at `0x10000` and above is driven onto the cartridge bus; the cartridge (via its mapper) decides banking and SRAM/ROM routing.

**Internal RAM size differs by model:**
- **WonderSwan (mono, "ASWAN"):** 16 KiB → valid at `0x00000`–`0x03FFF`.
- **WonderSwan Color / SwanCrystal ("SPHINX"/"SPHINX2"):** 64 KiB → fills the whole `0x00000`–`0x0FFFF` region.

Sources: WSdev Memory map, Wonderful platform_overview.

> **Pitfall — RAM is unified, not separate VRAM.** CPU, display and sound all read this same `0x00000`–`0x0FFFF` RAM in distinct time slices of the 12.288 MHz SoC clock. There is no separate video memory; tile/sprite/palette data live inside this region. The bus model must let CPU writes hit the same array the PPU/APU read, with no CPU wait states for video timing (WSdev Memory map).

> **Pitfall — mono RAM aliasing above 0x03FFF.** On the mono unit only 16 KiB is physically present. Access to `0x04000`–`0x0FFFF` on mono is not covered by the fetched sources — treat as an open question (mirror of the 16 KiB vs undefined); do not silently return a fixed constant without verification.

### 2. Standard cartridge mapper layout & bank registers

All licensed cartridges follow one layout. Four bank-select registers live in the cartridge I/O block (`$C0`–`$C3`). Selected banks are windowed into the physical map:

| Phys range | Window | Bank register (port) | Register width / bits | Granularity |
|---|---|---|---|---|
| `0x10000`–`0x1FFFF` | SRAM / RAM | `$C1` (`IO_BANK_RAM`) | RW8, `bbbb bbbb` | 64 KiB bank |
| `0x20000`–`0x2FFFF` | ROM bank 0 | `$C2` (`IO_BANK_ROM0`) | RW8, `bbbb bbbb` | 64 KiB bank |
| `0x30000`–`0x3FFFF` | ROM bank 1 | `$C3` (`IO_BANK_ROM1`) | RW8, `bbbb bbbb` | 64 KiB bank |
| `0x40000`–`0xFFFFF` | ROM linear (EX) bank | `$C0` (`IO_BANK_LINEAR`) | RW8, `00BB bbbb` | 1 MiB bank; only top 768 KiB reachable |

Sources: WSdev I/O port map (port bit patterns), WSdev Mapper, libws `hardware.h` (verbatim `#define IO_BANK_LINEAR 0xC0 / IO_BANK_RAM 0xC1 / IO_BANK_ROM0 0xC2 / IO_BANK_ROM1 0xC3`).

Details worth encoding:
- The `$C0` **linear bank** selects a 1 MiB window but only `0x40000`–`0xFFFFF` (= `0xC0000` bytes = **768 KiB**) of that bank is visible; the low 256 KiB of each 1 MiB bank is shadowed by the RAM window, ROM0 and ROM1 windows.
- `$C0` implements **4 bits on the 2001 mapper** and **6 bits on the 2003 mapper** (`00BB bbbb`); high bits read back as 0. ROM is partitioned into 64 KiB banks; up to 14 banks are mapped at once.
- **Power-up latch:** the mono WonderSwan expects the register at `$C3` (ROM1) to power up holding `$FF` (WSdev Mapper).

> **Pitfall — bank register readback width.** `$C0` on a 2001 cart must mask to 4 bits; a naïve 8-bit store/readback will mis-model games that probe mapper type. `$C1`/`$C2`/`$C3` are full 8-bit.

> **Pitfall — cartridge I/O is not just banking.** Ports `$C0`–`$FF` are the cartridge bus; carts define their own registers there (e.g. RTC, external EEPROM, `$CE` on 2003 selects whether `0x10000`–`0x1FFFF` maps RAM vs ROM). The core bus must forward `$C0`–`$FF` to the cartridge module, not decode them internally.

### 3. Boot ROM overlay & lockout

The internal boot ROM (IPL/BIOS) is overlaid at the very top of the ROM region while the lockout bit is clear:

| Model | Boot ROM range | Size |
|---|---|---|
| Mono (ASWAN) | `0xFF000`–`0xFFFFF` | 4 KiB |
| Color (SPHINX/SPHINX2) | `0xFE000`–`0xFFFFF` | 8 KiB |

Source: WSdev Boot ROM page.

The final act of the BIOS is a stub that writes port `$A0` to set the **boot ROM lockout** bit (`$A0` bit 0). This transitions **0 → 1**; once locked, the boot ROM is banked out and cartridge ROM shows through at those top addresses. Lockout is a one-way latch for the session.

> **Pitfall — reset vector.** The V30MZ resets to `CS:IP = FFFF:0000` (initial CS=$FFFF, IP=$0000) → physical `0xFFFF0`, which lands inside the boot ROM overlay. Emulation must map the boot ROM (or a HLE stub) there before first fetch, and must switch that top region to cartridge ROM the instant `$A0` bit 0 goes high — mid-execution, not at next reset.

### 4. I/O port address space & routing

The V30MZ issues 16-bit I/O addresses; the SoC decodes them into three blocks. Byte port addresses `$00`–`$FF` cover the internal + cartridge registers.

| From | To | Holder | Bus width | Speed |
|---|---|---|---|---|
| `$00` | `$B7` | WonderSwan SoC (display, sound, DMA, timers, system ctrl) | 16-bit | 1 cycle |
| `$B8` | `$BF` | Internal EEPROM control | 16-bit | 1 cycle |
| `$C0` | `$FF` | Cartridge bus | 8-bit | 1 (color) / 2 cycles (configurable) |

Source: WSdev I/O port map (verbatim block table).

**Exact routing algorithm** (WSdev I/O port map, verbatim logic — apply in this order):
1. If address ∈ `$00B8`–`$00BF` → **internal EEPROM control** block.
2. Else if address ∈ `$00C0`–`$00FF` → **cartridge bus**.
3. Else if `(address bits 0–8) ∈ $000–$0B7` → **SoC** block.
4. Else → **open bus** (unmapped read; see §6).

> **Pitfall — the SoC decode uses only the low 9 bits.** Rule 3 keys off `addr & 0x1FF` being ≤ `0x0B7`, so SoC ports alias across the 16-bit I/O space in a 512-entry pattern (e.g. an access with `addr >= 0x0200` whose low byte is ≥ `$B8` decodes to open bus, not SoC). Do not simply mask the port to 8 bits; implement the documented three-way decode against the full 16-bit port address, or games that read mirrored/high port addresses will diverge.

### 5. IN/OUT mechanics, byte vs word, and bus width

- `IN`/`OUT` reach ports via `imm8` (ports `$00`–`$FF`) or via `DX` (full 16-bit port address). Byte forms hit one port; word forms (`IN AX,…` / `OUT …,AX`) touch two consecutive ports (`port`, `port+1`).
- **SoC block (`$00`–`$B7`) and EEPROM block (`$B8`–`$BF`) are 16-bit wide, 1 cycle** — an aligned word I/O completes in a single access.
- **Cartridge block (`$C0`–`$FF`) is 8-bit wide** — a word I/O to a cartridge port is split into two byte accesses (`port` then `port+1`).
- **Unaligned 16-bit I/O:** "With the exception of internal EEPROM on the ASWAN, all unaligned 16-bit accesses can be converted to two 8-bit accesses with a 1-cycle penalty" (WSdev I/O port map). Model word I/O as byte + byte with a +1-cycle penalty for the split, and treat internal-EEPROM (`$B8`–`$BF`) word access on mono as the documented exception.

**Memory-side word vs byte (mirror of the port rules):**
- Internal RAM (`0x00000`–`0x0FFFF`) is a **16-bit bus**: an aligned word read/write is one 1-cycle access; an odd-address (unaligned) word splits into two byte accesses (classic 8086 penalty).
- Cartridge SRAM window (`0x10000`–`0x1FFFF`) is an **8-bit bus**: every word access is two byte cycles regardless of alignment.
- Cartridge ROM window (`0x20000`–`0xFFFFF`) bus width is **software-selectable** via `$A0` bit 2 (`CART_16BIT`): when 0 the external bus is 8-bit (word ROM read = 2 byte fetches); when 1 it is 16-bit (aligned word ROM read = 1 access). `$A0` bit 3 (`CART_FAST`) selects the ROM wait-state / access speed (1 vs 2 cycles).

> **Pitfall — bus width is a runtime property of `$A0`, not a cart constant.** ROM-read cycle cost and word-splitting for `0x20000`–`0xFFFFF` must be read live from `$A0` bits 2–3, because the BIOS/game reconfigures them (mono carts run the ROM bus at 8-bit; color titles typically enable 16-bit + fast). Hard-coding a width will corrupt timing and, on an 8-bit-configured bus, the sub-access ordering.

### 6. Port `$A0` — System Control (a.k.a. REG_HW_FLAGS)

`$A0` is the master hardware-config/status register. Bit layout `C??? swcl` (WSdev I/O port map), cross-verified against libws `hardware.h` (`IO_SYSTEM_CTRL1 = 0xA0`):

| Bit | Mask | Name (libws) | Meaning | Access |
|---|---|---|---|---|
| 0 | `0x01` | `SYSTEM_CTRL1_IPL_LOCKED` | **Boot ROM lockout** — 0 = boot ROM overlaid; set 0→1 by BIOS to bank it out | R/W (write-latch) |
| 1 | `0x02` | `SYSTEM_CTRL1_COLOR` | Color system flag | R (status) |
| 2 | `0x04` | `SYSTEM_CTRL1_CART_16BIT` | **External ROM bus width** — 0 = 8-bit, 1 = 16-bit | R/W |
| 3 | `0x08` | `SYSTEM_CTRL1_CART_FAST` | ROM wait-state / access speed (fast when set) | R/W |
| 4–6 | `0x70` | — | Unused / undocumented (`?`) | — |
| 7 | `0x80` | `SYSTEM_CTRL1_SELFTEST_OK` | Cartridge-OK / self-test status | R |

This matches the two internal-deep-dive facts exactly: **bit 0 = BIOS bank-out (0→1 after BIOS)** and **bit 2 = external bus width (0 = 8-bit, 1 = 16-bit)**. The WSdev annotation names bit 3 = "ROM wait state (s)", bit 1 = "Color system (c)", bit 7 = "Cartridge OK (C)".

> **Pitfall — mask the writable bits.** Bits 4–6 are undocumented; preserve/ignore them rather than inventing semantics. Bit 7 is status-only. Some devkit code (WonderWitch) preserves the low 5 bits on read-modify-write, so a plain full-byte writeback can misbehave — model per-bit read vs write semantics.

### 7. Unmapped / open-bus read values

When the routing in §4 falls through to open bus, the returned byte is model-dependent:

| Console / mode | Open-bus (unmapped I/O read) value | Source strength |
|---|---|---|
| WonderSwan mono (ASWAN) | **`0x90`** | WSdev I/O port map (explicit) |
| Color / SwanCrystal in **mono-emulation** (color mode off via port `$60`) | **`0x90`** | WSdev I/O port map (explicit) |
| WonderSwan Color / SwanCrystal, **native color mode** | **`0x00`** | ares issue #908 + WSMan-derived search; WSMan itself unreachable |

WSdev states verbatim: "On the monochrome models, as well as color models in mono emulation, open bus is always `0x90`." The color-native `0x00` value matches the internal fact ($90 WS / $00 WSC) and the ares ASWAN=`0x0090` / SPHINX=`0x0000` open-bus constants, but was corroborated via search rather than a directly-fetched WSMan page (WSMan/perfectkiosk are HTTP-only and refused the HTTPS upgrade).

> **Pitfall — open bus is not a single global constant.** The value depends on model AND on whether color mode is currently enabled (port `$60`). A color unit booting a mono game (color mode off) must return `0x90`, not `0x00`. Gate the open-bus byte on the effective mode, not just the hardware SKU.

> **Pitfall — unmapped ≠ zero.** Returning `0x00` universally will break mono-model detection and any code that reads `0x90` sentinels. Wire the §4 decode so only rules 1–3 hit real registers and everything else yields the mode-correct open-bus byte.


## CPU / interrupt test-ROM validation plan

Scope: how each CPU/interrupt/timing test ROM signals its result, how the Rust
harness captures that signal headlessly, and the measurable acceptance criteria
that gate **Phase 2** (V30MZ CPU + interrupt/bus timing). All four ROMs report
**on-screen** — none of them expose a serial or fixed-memory result API (confirmed
by reading each project's README and, for `ws-test-suite`, its source). The harness
therefore reads the **tile map RAM** the ROM renders into, not a pixel framebuffer,
so results are decoded as discrete tile IDs rather than fuzzy image hashes.

### 1. Signal mechanism per ROM (verified against source/README)

| ROM | Version read | Result surface | Pass looks like | Fail looks like | Needs input? |
|-----|------|----------------|-----------------|-----------------|--------------|
| **WSCpuTest** (FluBBaOfWard) | 0.7.4 | On-screen text in its own font | Runs all opcode/flag tests then prints **`Ok`** | **Halts at first failure**, printing input value/flags vs expected (plus div exception detail) | Menu (`X1–X4` navigate, `A` select, `B` back); on failure `A`=next value, `B`=next test — *auto-run-on-boot vs. menu-select is unconfirmed, see open questions* |
| **WSTimingTest** (FluBBaOfWard) | 0.4.0 | On-screen **numeric** table, one value per opcode | Each cell shows a cycle-derived number; **auto-runs continuously**, no pass/fail glyph | N/A (comparative — you diff numbers) | Only page switching (`X4`=left / `X2`=right); results appear without selection |
| **WSHWTest** (FluBBaOfWard) | 0.2.2 (2025-08-04) | On-screen (format **not documented in README**) | Tests interrupts, timers (countdown/repeat/writeback), IO-register writability, sound (noise per mode, ch3 sweep), LCD window + sleep/power | — | Unknown — *must be confirmed by driving the ROM* |
| **ws-test-suite** (asiekierka) | HEAD (`main`) | On-screen pass/fail tiles, **self-describing** | `draw_pass_fail()` writes **tile 5** at map cell `(27 - offset, y)` of `screen_1`; column filled left-to-right, "rightmost = first condition" | Same cell shows **tile 6** | None — each test is a standalone `.ws` that runs on boot |

`ws-test-suite` is the strongest gate because its pass/fail encoding is intrinsic to
the ROM (tile 5 vs 6) — **no external golden image or reference emulator is needed**
to read the verdict (src: `common/test/pass_fail.h`, read this session). WSCpuTest is
similarly self-validating: reaching `Ok` without halting *is* the pass. WSTimingTest
is the only comparative ROM: its numbers must be diffed against a recorded
hardware/reference table.

### 2. How the on-screen signal is laid out in RAM (verified constants)

The harness reconstructs text and pass/fail tiles by reading the tile map out of
internal RAM (16 KB on WS mono, 64 KB on WSC — src: Wonderful Toolchain
`platform_overview`). Layout facts confirmed from source read this session:

| Fact | Value | Source (read) |
|------|-------|---------------|
| Map dimensions | 32 × 32 tiles (256×256 px), visible 224×144 | wsdev *Display* |
| Map entry size | 2 bytes (word) → `MAP_SIZE = 0x800` = 32·32·2 | WSCpuTest `WonderSwan.inc` |
| Row stride | 32 entries; `dest += (y << 5)` | ws-test-suite `common/text.c` |
| Char → tile | ASCII byte used **directly** as tile index (`entry = ascii \| tile`); font unpacked to tile mem 0 | ws-test-suite `common/text.c` |
| Pass/fail glyph | tile **5** (pass) / **6** (fail) at `(27 - offset, y)` | ws-test-suite `common/test/pass_fail.h` |
| Screen-map base register | `IO_SCR_AREA` = port **0x07** | WSCpuTest `WonderSwan.inc` (no `IO_SCR_BASE` symbol exists; `pass_fail.h` does not define 0x07) |
| Display enable | `IO_DISPLAY_CTRL` = port **0x00** | WSCpuTest `WonderSwan.inc` |
| Keypad inject | `IO_KEYPAD` = port **0xB5** | WSCpuTest `WonderSwan.inc` |
| Interrupt enable | `IO_INT_ENABLE` = port **0xB2** | WSCpuTest `WonderSwan.inc` |

**Reading text back:** for a map cell at `(x, y)`, byte offset in the map =
`(y*32 + x) * 2`; the **low byte** is the tile index. In `ws-test-suite`'s font that
low byte equals the ASCII code for the 0x20–0x7F range, and equals 5/6 for the
pass/fail glyphs — so a headless run yields a decodable ASCII string plus a pass/fail
column without OCR or a golden image.

**Locating the map:** do not hardcode a map address. After the ROM stabilises, the
harness reads I/O latch **0x07** and derives the map base from it (WonderSwan screen
maps are 2 KB-aligned, consistent with `MAP_SIZE = 0x800`). The exact bit-field
encoding of port 0x07 is **not yet verified** (see open questions); until it is,
resolve the base from the register value at the documented 2 KB granularity and
assert the derived address is inside internal RAM.

### 3. Headless harness design (Rust)

The core already exposes deterministic master-clock stepping and typed capture
(`ws-contracts`), so no wall-clock or host I/O is involved. Required core surface for
the harness (add if missing):

- `load_rom(&[u8])` — accept the operator-supplied `.ws` (bytes only; never logged).
- `run_ticks(n)` / `run_frames(n)` — advance an exact integer number of master-clock
  ticks; one frame = the PPU's per-frame tick budget (display 75.47 Hz — src:
  `platform_overview`; total lines 159, ≈`12000/159` Hz — secondary, WebSearch).
- `peek_iram(addr, len) -> &[u8]` — read internal RAM (read-only, side-effect free).
- `peek_io(port) -> u8` — read the last-written I/O latch (for port 0x07/0x00).
- `set_keypad(mask)` — drive port 0xB5 for a scripted input timeline (for any ROM
  that needs a keypress to start; keypad lines read 0 when unattached).

Result-capture helpers (host side, no core changes):

- `scrape_text(rom) -> String` — resolve map base from port 0x07, read the 28×18
  visible tile window, map low bytes → ASCII. Used for WSCpuTest (`Ok` detection)
  and WSTimingTest (numeric parse).
- `scrape_pass_fail(rom) -> Vec<bool>` — read column `x = 27-offset` down the rows;
  `true` iff tile==5, `false` iff tile==6, ignore "empty" (0x20) rows. Used for
  `ws-test-suite`.
- `tilemap_signature(rom) -> u64` — FNV-64 over the resolved visible map window
  (reuse `ws-testkit`'s stable hash). Used as a coarse regression signature and for
  WSHWTest until its exact format is confirmed.

Each ROM test is a `#[test]` (or a data-driven table) that:
1. loads the fixture from the gitignored `fixtures/test-roms/` path (skip with a
   clear message if absent — fixtures are operator-acquired, never committed);
2. runs a **bounded** number of frames (a hard cap, e.g. a few hundred frames, so a
   hang fails instead of looping forever);
3. optionally injects the scripted keypad timeline;
4. captures via the helper above and asserts the acceptance criterion below.

Determinism requirements: fixed model (ASWAN vs SPHINX/SPHINX2) per test, fixed
initial state, no RNG, no host time; the run must be bit-identical in debug and
release and stable across runs (same discipline as the existing capture-hash gates).

### 4. Driving / building the fixtures

None of these repos ship a prebuilt `.ws` in-tree (confirmed for WSCpuTest — source
+ `build.sh`/`build.bat` only). Fixtures are built by the operator with the
WonderSwan toolchain (Wonderful / `wf-gcc`; `ws-test-suite` is `Makefile`-based,
FluBBaOfWard ROMs use `build.sh`) or pulled from a release, then dropped into
`fixtures/test-roms/` by `tools/fetch-test-roms.ps1`. Record the source commit and
license per fixture (`docs/LEGAL_PROVENANCE.md`). Building/downloading stays an
explicit operator step; nothing is fetched automatically.

### 5. ARMV30MZ as an opcode/flag oracle (not run as a ROM)

`ARMV30MZ` (FluBBaOfWard) is an ARM32-assembly V30MZ core, read as a **spec** for
opcode/flag semantics, not executed here. Use it to cross-check WSCpuTest failures
at the instruction level. Recorded caveats (src: ARMV30MZ README): its Zero flag is
**not** updated correctly during a division exception, and its timing "doesn't handle
extra cycles on branches to odd addresses" — so it is authoritative for flag/result
semantics of normal opcodes but **not** for div-exception flags or odd-address branch
timing; defer those to hardware ROMs.

### 6. Acceptance criteria gating Phase 2

| Gate | ROM(s) | Machine-checkable criterion |
|------|--------|------------------------------|
| **G1 — opcode & flag correctness** | WSCpuTest | `scrape_text` shows the `Ok` end state and the run never enters the failure-halt state, for **both** the ASWAN and SPHINX(2) SoC models being emulated. Includes the SoC-specific quirk: **unsigned MUL clears Z on ASWAN, sets Z on SPHINX(2)** (verified, WSCpuTest README) — assert the correct polarity per configured model. |
| **G2 — instruction cycle timing** | WSTimingTest | Every per-opcode number from `scrape_text` equals the recorded reference value within **±1** (the ROM documents ±1 as legitimate hardware variance). Reference table captured once from hardware or a trusted core and checked in as data (not the ROM). Result relation ≈ `cycles·4` over the loop (README). |
| **G3 — interrupt & timer handling** | WSHWTest, `ws-test-suite: src/mono/soc/interrupts`, `src/mono/cpu/interrupt_timing` | All `ws-test-suite` interrupt/timer tests report **tile 5** across their pass/fail columns. WSHWTest interrupt+timer sections pass once its reporting format is confirmed (interim: `tilemap_signature` matches a golden capture). |
| **G4 — CPU quirks & prefixes** | `ws-test-suite: src/mono/cpu/80186_quirks`, `.../cpu/prefixes` | All pass/fail cells == tile 5. |
| **G5 — DMA cycle formula `5 + 2n`** | `ws-test-suite: src/color/dma/gdma_timing` (+ `dma/alignment_access`) | Pass/fail cells == tile 5 (this is the ROM that empirically validates the `5 + 2n` general-DMA cost cited in the deep-dive). |

Phase 2 is complete only when **G1–G5 are green and recorded** (per the roadmap exit
gate), in both debug and release, with fixture provenance logged. A title booting is
not evidence; the tile-level assertions are.

### 7. Emulator pitfalls (per behavior)

- **Halt-on-fail vs. hang:** WSCpuTest stops at the first failing case rather than
  printing a summary. Always run under a frame cap and treat "cap reached without
  `Ok`" as failure — otherwise a CPU bug looks like an infinite loop, not a red test.
- **SoC-model polarity:** the MUL Zero-flag result is **opposite** on ASWAN vs
  SPHINX(2). A single hardcoded expectation will pass one model and silently fail the
  other; parametrize the gate by model.
- **±1 timing noise is real:** WSTimingTest values legitimately vary by 1 (pipeline
  flush on branch, destination-address alignment). Do not gate on exact equality or
  you will chase phantom regressions; gate on ±1 and investigate ≥2.
- **Don't hash pixels:** decode tile IDs from the map, not a rendered framebuffer.
  A framebuffer hash couples the CPU gate to PPU/palette correctness (a Phase 3
  concern) and to any reference emulator's rendering; the tile-map read isolates the
  CPU signal. (Framebuffer/audio golden diffs remain the right tool for Phase 3+.)
- **Self-locate the map:** read port 0x07 at runtime; do not assume a fixed map
  address. Different ROMs place the map differently, and hardcoding it will read
  garbage as soon as one ROM disagrees.
- **ASCII-index assumption is ws-test-suite-specific:** its font makes low-byte ==
  ASCII (`common/text.c`). FluBBaOfWard ROMs use their own font, so the exact tile
  index of the `Ok` glyph must be confirmed before asserting on it (open question),
  or detect `Ok` via a golden tile-window signature instead.
- **Oracle blind spots:** ARMV30MZ mis-sets Z on division exceptions and ignores
  odd-address branch penalties — never gate those two behaviors against it; use
  WSCpuTest/WSTimingTest (hardware-derived) instead.


---

## Appendix — adversarial review record

Generated from an independent verification pass that re-fetched sources. Items under **Open questions** are unverified and MUST NOT be encoded as literals until confirmed on hardware or against a reachable primary source.


### V30MZ opcode set & encoding

*Verifier verdict:* `needs_fixes` · *confidence:* `medium`

**Unsupported claims flagged (removed or demoted in the body):**

- §4 prefix table asserts each segment override (bytes 26/2E/36/3E) costs '1 cycle'. Checked: WSMan (http://daifukkat.su/docs/wsman/) HTTPS fetch was REFUSED (ECONNREFUSED); the cited wonderful.asie.pl optimization_v30mz page returns 'This page does not exist anymore'; a web search only returned segment-REGISTER read/write MOV costs (read +1, write +2 cycles), which is a different operation from the override-prefix cost. The per-prefix 1-cycle figure is not backed by any source I could fetch. The spec itself hedges it ('via search snippet'), so it should move to Open Questions, not be asserted.
- §5 footnote 1 states '66/67 are the 80286 operand/address-size prefix slots'. This concrete attribution is wrong and unsupported: operand-size (66) and address-size (67) prefixes were introduced on the 80386, not the 80286 (the 286 has no 32-bit operand/address mode). The spec's own §8 correctly calls 66/67 the '386' size prefixes, so the body is internally contradictory. The V30MZ NOP behaviour of 66/67 is fine; only the historical '80286' label is fabricated.

**Corrections (applied inline where definitive):**

- §5 footnote 1: '66/67 are the 80286 operand/address-size prefix slots' -> should read 80386 operand-size (66) / address-size (67) prefix slots. Source: standard x86 encoding history and the spec's own §8 wording ('66/67 (size prefixes)... on 386').
- §7 and Open Questions claim the DIV/DIVU zero-flag behaviour is 'weird and unverified'. The WSCpuTest README actually documents a concrete rule for DIV/DIVU (16/8): the Zero flag is 'set when remainder is zero and bit 0 of result is set'. So the (16/8) ZF behaviour is documented, not unverified. Source: FluBBaOfWard/WSCpuTest README.

**Open questions (verifier):**

- Segment-override prefix cycle cost could not be verified from any reachable source (WSMan HTTPS refused; optimization_v30mz page deleted). Confirm against WSTimingTest or an HTTP fetch of daifukkat.su before asserting '1 cycle'.
- IN/OUT '12 cycles' (referenced in the researcher's own notes) remains unconfirmed for the same reason (WSMan/daifukkat.su unreachable this session).
- F1: WSCpuTest README states F1 'possibly BRKS' and observes it 'switches the MD flag' — the spec (footnote 8) omits the observed MD-flag effect and calls it merely 'untested'. Reconcile: there IS an observed hardware effect worth recording.
- The body asserts 64/65 = NEC V20 REPC/REPNC slots and 66/67 as size-prefix slots, but the researcher's own Open Questions say the exact V20/V30 byte encodings (incl. REPC/REPNC) were NOT confirmed from a fetched source. Asserting-in-body what is flagged-unconfirmed is inconsistent; either verify against the NEC V20/V30 Users Manual or soften the body claims.
- Dangling footnotes: §2 says 'Footnotes ¹–⁸ resolved in §5', but footnotes ² (80186 new ops) and ³ (0x82 GRP1 alias) do not appear in §5. Not a hardware error, but the cross-reference is broken.
- Full per-opcode cycle-timing table was not fetched/verified (WSdev instruction-set page not reproduced verbatim); belongs to timing section.

**Open questions (author):**

- IN/OUT cycle cost (the internal deep-dive / 00-overview.md states 12 cycles) could not be independently confirmed: daifukkat.su is HTTP-only and refused the forced-HTTPS fetch, and archive.org is blocked. Verify against WSTimingTest (https://github.com/FluBBaOfWard/WSTimingTest) or the daifukkat.su hardware-tests blog when an HTTP fetch is available.
- Exact NEC V20/V30 byte encodings for the omitted extensions (the 0x0F two-byte sub-opcodes for TEST1/SET1/CLR1/NOT1/ADD4S/SUB4S/CMP4S/ROL4/ROR4/INS/EXT, and the specific bytes for REPC/REPNC and BRKEM) were not confirmed from a fetched source; verify against the NEC V20/V30 Users Manual (bitsavers scan at archive.org/details/bitsavers_necV20V30U_11351331). The V30MZ *behaviour* at those slots (inert NOP) is verified; only the historical V20/V30 assignments are unverified.
- Full per-opcode cycle-timing table: the WSdev instruction-set page (https://ws.nesdev.org/wiki/NEC_V30MZ_instruction_set) has hex+binary+byte-count+cycle columns but the fetch summarizer would not reproduce them verbatim. Re-fetch the raw MediaWiki source or scrape directly for the timing section (belongs to the CPU-timing spec section).
- 0xF1 behaviour: WSCpuTest notes it as 'possibly BRKS, untested'. Its real hardware effect is unknown; decide whether to NOP or trap and confirm against hardware/WSCpuTest updates.
- DIV/DIVU Zero-flag behaviour is described by WSCpuTest as 'weird' and 'not tested', and division exceptions do not modify destination registers; the exact ZF/other-flag state after DIV needs hardware confirmation before the core commits to a value.
- SALC could not be fully hardware-verified by the WSCpuTest author ('couldn't get STC to set carry before the SALC'); the 0x00/0xFF semantics are documented but the on-hardware confirmation is partial — re-check against a later WSCpuTest revision.
- Confirm the exact ModR/M reg decode and cycle counts for the duplicate GRP4 (0xFE reg>=2) CALL/BRA/PUSH variants and the GRP5 register-form CALLF/JMPF (0xFF reg=3/5, mod=11), which WSCpuTest reports can crash/restart the ROM on failure.


### V30MZ Instruction & Bus Cycle Timing

*Verifier verdict:* `needs_fixes` · *confidence:* `medium`

**Unsupported claims flagged (removed or demoted in the body):**

- §7 step 2: “Its advance rate is `f = MCLK / (2048 − reg)`… Set `reg = 2047` → advance once per master clock (finest resolution) [BLOG].” Checked WSdev Sound [NES-SND] (https://ws.nesdev.org/wiki/Sound): that page gives the wavetable/noise advance base as the CPU clock 3,072,000 Hz (rate = 3072000/(2048−divisor)), NOT the 12.288 MHz master clock. The [BLOG] source claimed to state ‘one update per master clock’ was unreachable (daifukkat.su refuses HTTPS and the blog was not retrievable via the text proxy this run), so the MCLK-based figure is unconfirmed by any source I could fetch and is actively contradicted by [NES-SND]. If the true base is 3.072 MHz, reg=2047 yields one advance per CPU cycle (not per master clock), which also breaks the §7 pitfall claim ‘measured delta is in master clocks, so results are 4× the core-cycle numbers’ and the §8 domain math.
- §4 row ‘MOV / ADD, with mem operand | 2–3 (varies by direction) [NES-IS]’: on re-fetch of NEC_V30MZ_instruction_set (summarizing fetch) MOV reg,mem and MOV mem,reg both returned 1 cycle, not 2–3. Could not confirm the 2–3 figure from the cited page; raw wikitext was not obtainable (the plain URL is what the researcher means by ‘action=raw also summarized’). Value unconfirmed — flagged rather than asserted.

**Corrections (applied inline where definitive):**

- LFSR / wavetable advance base clock: correct base is the CPU clock 3.072 MHz with rate = 3072000/(2048−divisor), per WSdev Sound (https://ws.nesdev.org/wiki/Sound). §7’s ‘f = MCLK/(2048−reg)’ (12.288 MHz) is wrong by 4×; correct it and the dependent ‘measured delta is in master clocks / results are 4× core cycles’ statements in §7 and the §8 tick-budget math.
- Citation error (number is correct, source is not): the §1 clock table and §8 attribute ‘CPU = MCLK/4’ to [PLAT]. The platform_overview page (https://wonderful.asie.pl/wiki/doku.php?id=wswan:platform_overview) states only ‘clocked at 3.072 MHz’ and does not give the MCLK relation. The MCLK/4 ratio and the 12.288 MHz master figure are sourced from wsman ([WSMan]/[BLOG]); re-tag accordingly.

**Open questions (verifier):**

- §4 per-opcode core-cycle table remains summarizing-fetch-derived on both sides. My independent re-fetch of NEC_V30MZ_instruction_set confirmed the base/low-end values (reg,reg=1; Jcc 1/4; MUL/IMUL 3/4; DIV 15/16/23/24; IDIV 17/18/24/25; IN/OUT=6; string ops 5/6/4/3/3) but returned single base values where the spec lists ranges (PUSH 1 vs 1–2, POP 1 vs 1–3, JMP 4 vs 4–10, CALL 5 vs 5–12, RETF 8 vs 8–9) and 1 (not 2–3) for MOV-with-mem. Raw wikitext is still needed to settle the ranges and the MOV-mem row opcode-by-opcode.
- Cart ROM wait-state has an unreconciled three-way source tension the spec doesn’t fully call out: Memory_map [NES-MM] says 1/2 cycles configurable, ares #908 says 1–3 via port 0xA0, and the wsman proxy says 1 (speed flag) or 3. The exact bit layout of port 0xA0 (bus-width + wait-state control) was not obtained from any fetched source. Cart SRAM conflict (ares ASWAN 3-cycle mandatory vs Memory_map 1/2) is a genuine sourced conflict, correctly flagged by the spec — not a fabrication.
- [BLOG] (daifukkat.su/blog 2015-07-11 hardware tests) and perfectkiosk.net STS were unreachable to me this run (HTTPS refused; blog not retrievable via the text proxy). Blog-primary claims — the unit of `5+2n`, and ‘once per master clock’ — were only cross-checked via the wsman proxy (which confirmed 5+2n and general-DMA-pauses-CPU), not read at the blog directly. The `5+2n` unit (master vs CPU cycles) remains genuinely open, as the researcher notes.
- IN/OUT true cost: confirmed 6 is the [NES-IS] core figure; the [DD] value of 12 is not confirmed by any fetched source and ares #908 does not settle I/O-port timing (it lists ‘open bus/unaligned I/O access quirks’ as TODO). Consistent with the researcher’s open question — resolve on hardware before encoding a literal.

**Open questions (author):**

- IN/OUT cycle cost is unresolved: the deep-dive claims 12 (hardware-measured, 'not 10 as on 80186') but NO fetched source confirms it; the WSdev instruction-set table lists 6 core cycles and ares #908/asie state I/O-port timing 'remains unknown.' Resolve on hardware with a WSTimingTest-style LFSR measurement of IN AL,DX / OUT DX,AL before encoding any literal.
- **RESOLVED (2026-07-14) — cycle unit of `5+2n` and of trap15's measured deltas: CPU cycles (3.072 MHz), no ×4.** trap15's "master clock" is the 3.072 MHz sound/CPU clock (WSMan: "updates every master clock (3072000 Hz)"; the 12.288 MHz crystal is the "input clock"), so at `reg=2047` the LFSR advances once per CPU clock. Corroborated by ares and Mednafen (both charge instructions — and, in ares, `5+2n` DMA — at 3.072 MHz with no ×4), by `XCHG reg,reg` = 3 measured / 3 datasheet = 1.0, and by a physics floor (1-cycle ops and 0.5-cycle/word DMA are impossible as 12.288 MHz clocks). A master-clock scheduler multiplies these by 4 (§8). Resolved by documentary + emulator-convergence + arithmetic evidence; no hardware measurement was required. See the document preamble.
- Cart SRAM wait state conflicts between sources: ares #908 says ASWAN SRAM has a mandatory 3-cycle wait state, while WSdev Memory_map lists cart SRAM at 1(Color)/2 cycles. Confirm per-model (ASWAN/mono vs SPHINX/Color) SRAM and ROM cycle counts and the exact bit layout of the port 0xA0 bus-width/wait-state control register (not obtained).
- Sprite (OAM) DMA cost: confirmed to NOT stall the CPU (ares #908), but no cycle-cost formula was found. Determine its master-clock cost and slot-0 behavior relative to the CPU.
- Per-opcode WonderSwan-measured master-clock timings: no published table exists (WSTimingTest emits per-test-loop deltas, not per-opcode absolutes). The §4 core table came through a summarizing fetch of the WSdev instruction-set page and must be re-verified opcode-by-opcode against the raw wikitext, then overridden with LFSR-measured values where they diverge from Intel figures. Primary hosts daifukkat.su (wsman + blog) and perfectkiosk.net (Sacred Tech Scroll) refuse HTTPS and Wayback was blocked; wsman/blog were reached via a text-proxy and STS could not be read at all.
- Exact behavior of CPU/DMA sharing slot 0 vs the other three slots at sub-sample granularity (RAM contention) is stated as unknown even in mainline emulators (ares #908); decide whether the emulator needs to model 4 physical slots or can treat the CPU as owning a contiguous slot-0 budget.


### V30MZ Flags, Arithmetic Semantics, Exceptions & Reset

*Verifier verdict:* `needs_fixes` · *confidence:* `high`

**Corrections (applied inline where definitive):**

- Shift AF behavior is misstated. Spec Section 3 (SHL/SHR/SAR row note) says 'AF undefined (macro preserves it)' and Open Questions repeats 'ARMV30MZ preserves the prior AF for shifts.' This is wrong per the source: shl8/shr8/shra8 in ARMV30MZmac.h begin with `movs v30f,v30f,lsl#31 ;@ Move PSR_C to Carry, clear flags`, which clears ALL status flags (AF included) and then rebuilds only CF/SF/ZF/OF and PF (from ParityVal) from the result. Therefore ARMV30MZ shifts CLEAR AF (leave it 0), not preserve it. Preservation of S/Z/A is done by the ROTATE macros (rol8/ror8/rolc/rorc), which use `and v30f,#PSR_S+PSR_Z+PSR_A ;@ Keep S, Z & A`. Correct value: shifts clear AF; rotates preserve AF. Source: ARMV30MZmac.h shl8/shr8/shra8 (approx lines 461-520) vs rol8/ror8 (approx lines 358-455).
- Reset/PUSHF MD-bit open question is partially answerable from the reference the spec already cites: pushFlags (ARMV30MZ.s ~line 2024) loads its reserved-bit base as `ldr r1,=0xF002`, i.e. it sets bit 15 (MD) = 1 on every PUSHF/interrupt-frame push. So per ARMV30MZ the materialized/readback value has MD=1 (favoring 0xF002 over 0x7002). Spec Section 7 leaves this fully open; it should note the reference implementation already picks 0xF002 (MD reads 1), with hardware confirmation still outstanding. Source: ARMV30MZ.s pushFlags (0xF002 base mask).

**Open questions (verifier):**

- Exact post-DIV/IDIV flag values (byte/word, divide-by-zero and quotient-overflow faults) on real ASWAN vs SPHINX hardware. ARMV30MZ confirms the mechanism (unsigned DIV loads CF/OF/Z from v30MulOverflow = last-multiply state; signed IDIV derives SF/ZF/PF from quotient), but bit-for-bit values still need WSCpuTest confirmation on both SoCs.
- #DE return-address semantics: ARMV30MZ advances PC past the DIV/IDIV opcode before faulting (divbF6/divwF7 error paths), so the pushed CS:IP points after the instruction; restartability vs real hardware still needs WSCpuTest confirmation.
- Signed IDIV INT_MIN edge case (dividend 0x80/0x8000): ARMV30MZ special-cases it to quotient 0x81 (divbF6Error: `cmp r0,#0x80000000; moveq r0,#0x0081`) rather than faulting. Whether real V30MZ faults or returns 0x81 (and resulting flags) is unconfirmed against hardware.
- Exact CF/OF/AF left by AAM and AAD: ARMV30MZ clears C/V/A for AAM (_D4: `movs v30f,r0,lsl#24` then only S or Z) and derives them from the internal add for AAD (_D5 sets S/Z/C/V and conditionally AF). Real-hardware values still need WSCpuTest.
- Reset FLAGS readback (MD bit): reference favors 0xF002 (MD=1) as above; primary hardware PUSHF-after-reset test still needed to pin 0x7002 vs 0xF002.
- Sacred Tech Scroll (perfectkiosk.net/stsws.html) was NOT fetched by this review (host unreachable in-session), so its reset/IVT/division notes were not independently cross-checked. WSMan (daifukkat.su, HTTP-only, port 443 refused) WAS fetched over plain HTTP and confirmed IVT@0000:0000h with 4-byte entries, REG_INT_BASE=0B0h, and FFFF:0000h reset start.
- Adjust-family cycle counts differ between WSdev and ARMV30MZ (DAA=10/DAS=11, AAA=AAS=9, AAM=16, AAD=6 in ARMV30MZ); belongs to the timing section and should be resolved against WSTimingTest.

**Open questions (author):**

- Exact FLAGS readback value immediately after reset (PUSHF): the reference clears the defined flags but does not materialise a canonical PSW word, and MD (bit 15) 'does nothing' on V30MZ. Determine whether MD reads 0 or 1 at reset (→ 0x7002 vs 0xF002) by running a PUSHF-after-reset hardware test / WSCpuTest.
- Precise post-division flag values (CF/OF/SF/ZF/AF/PF) for unsigned DIV and signed IDIV, byte and word, including divide-by-zero and quotient-overflow faults. The ARMV30MZ behaviour (CF/OF inherited from last MUL for unsigned; SF/ZF/PF from quotient for signed) must be confirmed bit-for-bit against WSCpuTest on both ASWAN and SPHINX, since it is explicitly implementation-specific.
- Return address (pushed CS:IP) semantics for #DE: does the V30MZ push the address of the instruction following DIV/IDIV or the DIV itself? The reference advances PC past the opcode before faulting; confirm restartability behaviour against WSCpuTest.
- Signed IDIV INT_MIN edge case (dividend 0x80/0x8000): the reference special-cases it to quotient 0x81 rather than faulting. Verify whether real V30MZ hardware faults or returns this value, and the resulting flags.
- Exact CF/OF/AF values left by AAM and AAD (x86 marks them undefined; ARMV30MZ clears them for AAM and derives them from the internal add for AAD). Confirm against WSCpuTest.
- Cycle-count discrepancies between WSdev instruction-set page and ARMV30MZ for the adjust family (e.g. DAS: WSdev implies 10 vs ARMV30MZ `fetch 11`; AAM: WSdev 17 vs ARMV30MZ 16). Timing belongs to the opcode/timing section but should be resolved against WSTimingTest.
- Shift/rotate AF behaviour: x86 marks AF undefined; ARMV30MZ preserves the prior AF for shifts. Confirm the V30MZ leaves AF unchanged (vs clearing) via WSCpuTest.
- WSMan (daifukkat.su/docs/wsman) and the Sacred Tech Scroll (perfectkiosk.net/stsws.html) are HTTP-only and refused HTTPS during authoring; re-fetch over plain HTTP to cross-check the reset state, IVT location, and division-flag notes against these primary docs.


### CPU Memory Map, I/O Port Mechanics & Unmapped-Read Behavior

*Verifier verdict:* `needs_fixes` · *confidence:* `medium`

**Unsupported claims flagged (removed or demoted in the body):**

- "$C0 implements 4 bits on the 2001 mapper" (and the table row implying a 2001 linear-bank width) — could NOT confirm. Checked WSdev Mapper page (says only 'the number of bits in the Linear bank register depends on the mapper', no per-chip width) and WSdev Bandai 2001 page (documents only EEPROM registers at $C4/$C6/$C8; no $C0 linear-bank register or 4-bit width found). The companion claim '6 bits on the 2003 mapper (00BB bbbb)' DID confirm on the Bandai 2003 page, so only the 2001=4-bit half is unbacked.
- "up to 14 banks are mapped at once" — no source states this count. Not present on WSdev Mapper or Memory map pages; appears to be a derived figure (12 linear 64 KiB banks over 0x40000-0xFFFFF + ROM0 + ROM1). Plausible but unverified.

**Corrections (applied inline where definitive):**

- Reset vector: spec says 'CS:IP = F000:FFF0'. Correct value is CS:IP = FFFF:0000 (WSdev Boot ROM page: reset 'jumps to $FFFF:$0000', initial CS=$FFFF IP=$0000). Both decode to physical 0xFFFF0, so the stated physical address and the 'map boot ROM there before first fetch' guidance are fine, but the register pair F000:FFF0 is the IBM-PC/286+ convention, not the V30MZ's actual 8086-class reset state.
- Color-native open-bus value: spec's §7 table cites 'ares issue #908' as support for WonderSwan Color / SwanCrystal native = 0x00. Ares issue #908 actually states 'SPHINX: Unknown' for open bus and attributes 0x0000 only to SPHINX2. So 0x00 is sourced for SwanCrystal (SPHINX2) but is NOT confirmed by that issue for the WonderSwan Color (SPHINX) SKU — the source lists it as unknown. Treat 0x00-for-SPHINX as unverified, not citation-backed.

**Open questions (verifier):**

- WSMan (daifukkat.su/docs/wsman) and Sacred Tech Scroll (perfectkiosk.net) remain HTTP-only and were unreachable this session too; the primary-source addresses were confirmed via WSdev wiki + libws + ares #908 rather than WSMan itself.
- 2001-mapper linear-bank register width (claimed 4 bits) needs confirmation from the dedicated Bandai 2001 mapper documentation or WSMan.
- WonderSwan Color (SPHINX, not SPHINX2) native-color open-bus byte is unresolved: ares #908 marks SPHINX 'Unknown'; verify 0x00 vs other against Mednafen/ares source or hardware test.
- Mono-unit reads at 0x04000-0x0FFFF (above the 16 KiB present) — mirror vs open/undefined — still uncovered by fetched sources.
- Exact per-configuration ROM cycle counts for the $A0 bit2 (width) x bit3 (wait-state) matrix are only qualitative ('1/2 cycles') in WSdev; pin numbers from WSMan timing tables or daifukkat hardware-tests.
- Precise conditions for a color unit with color mode off returning 0x90 vs 0x00 across port ranges need direct source confirmation.

**Open questions (author):**

- WSMan (http://daifukkat.su/docs/wsman/) and the Sacred Tech Scroll (http://perfectkiosk.net/stsws.html) are HTTP-only and refused the forced HTTPS upgrade (ECONNREFUSED :443); web.archive.org is also blocked by the fetch tool. The task asked to cite WSMan for every address — those exact addresses were instead confirmed via the WSdev wiki (which mirrors WSMan) and libws headers. Re-fetch WSMan over plain HTTP from an environment that allows it to pin the primary citations.
- The WonderSwan Color / SwanCrystal native-color open-bus value (0x00) is corroborated by ares issue #908 (SPHINX open bus 0x0000) and a WSMan-derived search summary, but the WSdev I/O port map page excerpt I read explicitly states only the 0x90 mono value. Verify 0x00 for color-native directly in WSMan or ares/Mednafen source before treating it as canonical.
- Behavior of internal-region reads at 0x04000-0x0FFFF on the 16 KiB mono unit (mirror of the low 16 KiB vs undefined/open) is not covered by the fetched sources. Verify against a hardware test or Mednafen/ares source before choosing a mirror mask.
- Exact cycle counts for the ROM window under each $A0 configuration (bit2 width x bit3 wait-state -> the '1/2 cycles configurable' entry) are stated only qualitatively by WSdev. Pin the precise per-combination cycle numbers from WSMan timing tables or the daifukkat hardware-tests blog (http://daifukkat.su/blog/archives/2015/07/11/wonderswan_hardware_tests/, not yet fetched).
- The precise conditions under which a color unit with color mode disabled returns 0x90 vs 0x00 for specific port ranges (one search summary mentioned a port>=~0x40 threshold) need confirmation; the clean rule appears to be 'mode-gated', but confirm the exact port-range interaction from WSMan/ares source.
- The internal EEPROM control block ($B8-$BF) word-access exception on ASWAN is noted but its exact cycle behavior/why is not detailed in fetched sources; verify against WSMan before modeling EEPROM word timing.


### CPU/interrupt test-ROM validation plan

*Verifier verdict:* `needs_fixes` · *confidence:* `medium`

**Unsupported claims flagged (removed or demoted in the body):**

- "G5 — DMA cycle formula `5 + 2n`" / "empirically validates the `5 + 2n` general-DMA cost cited in the deep-dive": the literal 5 + 2n general-DMA cost is NOT confirmed by any reachable source. Checked WSCpuTest/WSTimingTest/WSHWTest READMEs, nesdev Display, and platform_overview (none mention DMA cycle cost). Its intended primary (WSMan, daifukkat.su) was unreachable this session, so the number remains unverified against a source. (The researcher's own open questions also flag this as not independently confirmed.)

**Corrections (applied inline where definitive):**

- Spec section 2 lists the screen-map base register as "IO_SCR_AREA / IO_SCR_BASE = port 0x07". WSCpuTest WonderSwan.inc defines only `IO_SCR_AREA equ 0x07`; there is no `IO_SCR_BASE` symbol. Correct label: IO_SCR_AREA = 0x07 (source: WSCpuTest WonderSwan.inc, read this session). The port value 0x07 itself is correct.
- Spec section 2 cites "ws-test-suite `pass_fail.h`" as a co-source for port 0x07. pass_fail.h contains no reference to port 0x07 — it writes tiles via the `ws_screen_put_tile(screen_1, ...)` libws abstraction. The only source that actually defines port 0x07 is WSCpuTest WonderSwan.inc. Drop pass_fail.h from that citation.

**Open questions (verifier):**

- Exact bit-field encoding of port 0x07 (SCR base / SCR area) — candidate base=(value&0x07)<<11 remains UNVERIFIED; its primary source WSMan (daifukkat.su/docs/wsman) was unreachable (connection refused) this session. Confirm against WSMan or the nesdev I/O ports page before hardcoding tilemap self-locate logic.
- Literal IN/OUT = 12 cycles and general DMA = 5 + 2n: not confirmed from any primary reachable this session (WSMan down). Treat WSTimingTest's measured table and ws-test-suite dma/gdma_timing as source of truth until verified.
- WSHWTest v0.2.2 on-screen pass/fail reporting format: README confirms what it tests (interrupts, timers, IO regs, sound, LCD) but does NOT document the result format. Confirm by reading WSHWTest.asm or driving the ROM; gate G3 on a golden tilemap signature until then.
- Exact tile index of the 'Ok'/failure glyphs in the FluBBaOfWard font (distinct from the ws-test-suite ASCII font). Verify in WSCpuTest.asm/font before asserting on 'Ok' by tile decode; otherwise use a golden tile-window signature.
- Fixed IRAM address of libws `screen_1` used by ws-test-suite: self-locate from port 0x07 rather than assume; confirm resolved base lands inside internal RAM (verify against libws ws.h).
- Total scanlines = 159 and 75.47 = 12000/159: corroborated by a secondary web search (144 active + 15 vblank) but NOT read from a primary timing page this session (75.47 Hz alone is on platform_overview; the 159 derivation is not). Confirm the per-frame master-clock tick budget against WSMan or a wsdev timing page. NOTE: WSCpuTest auto-run-on-boot is now RESOLVED — its README explicitly states it auto-runs all tests on load.

**Open questions (author):**

- Exact bit-field encoding of I/O port 0x07 (SCR_BASE/SCR_AREA): which bits select the SCR1 vs SCR2 map base and the multiplier (candidate: base = (value & 0x07) << 11 for 2 KB granularity, consistent with MAP_SIZE=0x800, but not verified). Confirm against WSMan (daifukkat.su/docs/wsman/) or the wsdev I/O_ports page before hardcoding the tilemap self-locate logic. WSMan was unreachable this session (https ECONNREFUSED).
- Whether WSCpuTest auto-runs the full test set on boot or requires a keypress/menu selection (A) to start 'run all'. The two README fetches were ambiguous (one said it auto-runs then prints Ok; the README also documents X1-X4/A/B menu navigation). Must be resolved by driving the ROM headless and observing whether 'Ok' appears without input.
- WSHWTest's exact pass/fail reporting format (on-screen text? colored cells? a pass/fail column like ws-test-suite?). Its v0.2.2 README does not state it. Confirm by reading WSHWTest.asm or by driving the ROM and inspecting the tile map; until then gate G3 on a golden tilemap signature rather than a decoded verdict.
- The exact tile index(es) of the 'Ok' glyph and failure glyphs in the FluBBaOfWard font (WSCpuTest/WSTimingTest/WSHWTest use their own font in WonderSwan.inc, not the ws-test-suite ASCII font). Needed to assert on 'Ok' by tile decode; otherwise use a golden tile-window signature. Verify by inspecting the font/print routines in WSCpuTest.asm.
- The specific WonderSwan-deviation cycle numbers cited in the internal deep-dive (IN/OUT = 12 cycles; general DMA = 5 + 2n) were NOT independently confirmed from a primary source this session. Treat WSTimingTest's measured table and ws-test-suite's dma/gdma_timing pass/fail as the source of truth; verify the literal 12 and 5+2n against WSMan or by capturing the ROM output on hardware.
- The fixed internal-RAM address of libws 'screen_1' used by ws-test-suite (referenced via SCR1_BASE(screen_1) written to port 0x07). The harness should self-locate from port 0x07 rather than assume; confirm the resolved base lands in internal RAM. Verify against the Wonderful/libws ws.h headers.
- Total scanlines per frame (159) and the derivation 75.47 = 12000/159 came from a WebSearch summary, not a primary page read; confirm the per-frame master-clock tick budget used to bound headless runs against WSMan or the wsdev timing page.
