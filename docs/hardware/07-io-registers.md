# I/O registers: sound (noise), serial, internal EEPROM

Verified I/O-register maps for the three subsystems being wired to the machine's
I/O dispatch (community bugs #3, #4, #8). Transcribed and **adversarially
verified** across libws (`hardware.h`), the WSdev wiki, ares, and Mednafen — each
critical address + bit position survived a refutation pass at high confidence.
The constants live in `core-ws/src/io.rs`.

> Two recurring transcription errors were caught and rejected here: putting the
> noise/serial *enable* on bit 6/7 (a WebFetch-summarizer artifact), and a
> low-nibble EEPROM strobe scheme. The values below are from byte-exact headers
> and raw wikitext, cross-checked against running emulator code.

## Sound — channel-4 noise (bug #4)

| Port | Name | Access | Bits |
|------|------|--------|------|
| `$8E` | `SND_NOISE` | R/W | 0–2 tap mode; 3 (`0x08`) LFSR reset (write-1 strobe, reads 0); 4 (`0x10`) noise/LFSR-update enable; 5–7 unused |
| `$90` | `SND_CTRL` | R/W | 0–3 channel 1–4 enable (ch4 = bit 3); 5 ch2 voice; 6 ch3 sweep; 7 (`0x80`) ch4 output select (`0`=wavetable, `1`=noise) |
| `$92`/`$93` | `SND_RANDOM` | R | live 15-bit LFSR (`$92` = bits 0–7, `$93` = bits 8–14; bit 15 reads 0) |

The LFSR advances only while **both** ch4-enable (`$90` bit 3) **and**
noise-update-enable (`$8E` bit 4) are set — independent of the output-select bit
(that independence is the bug-#4 fix). It is driven by the sound clock, so the
advance itself awaits the timing scheduler.

## Serial / UART — EXT link (bug #3)

| Port | Name | Access | Bits |
|------|------|--------|------|
| `$B1` | `SER_DATA` | R/W | read = RX byte, write = TX byte |
| `$B3` | `SER_STATUS`/`SER_CTRL` | R/W | 7 (`0x80`) enable; 6 baud (`0`=9600, `1`=38400); 5 (write) overrun-reset; 2 (read) TX-ready; 1 (read) overrun; 0 (read) RX-ready |

Disabling the port (clearing `$B3` bit 7) must lower the level-triggered
`SER_TX`/`SER_RX` IRQ lines — the bug-#3 fix.

## Internal EEPROM ($B8–$BF block, bug #8)

| Port | Name | Access | Purpose |
|------|------|--------|---------|
| `$BA`/`$BB` | `IEEP_DATA` | R/W | 16-bit data buffer (write before WRITE, read after READ) |
| `$BC`/`$BD` | `IEEP_CMD` | R/W | Microwire command word: start + 2-bit opcode + address |
| `$BE` | `IEEP_CTRL`/`STATUS` | R/W | write = operation strobe; read = status |
| `$BF` | — | — | unused high byte |

`$BE` write strobes: READ `0x10`, WRITE `0x20`, SHORT/ERASE/EWEN/EWDS `0x40`,
PROTECT `0x80`. `$BE` read status: bit 1 (`0x02`) ready/idle, bit 0 (`0x01`)
read-done.

### Microwire command protocol (for the not-yet-built state machine)

93Cxx serial EEPROM: WS mono = 93C46 (64×16), WSC/SwanCrystal = 93C86 (1024×16).
Command word (`$BC`/`$BD`) = START(1) + 2-bit opcode + address; `word_addr =
byte_addr >> 1`; address width 6-bit (mono) / 10-bit (colour).

- Opcodes: `READ`=10, `WRITE`=01, `ERASE`=11, `EXTENDED`=00. Extended sub-ops
  (top address bits): `EWDS`=00, `WRAL`=01, `ERAL`=10, `EWEN`=11.
- **READ:** write cmd → strobe `$BE` bit 4 (`0x10`) → poll bit 0 (`0x01`)=1 →
  read `$BA`/`$BB`.
- **WRITE:** load `$BA`/`$BB` → write cmd → strobe bit 5 (`0x20`) → poll bit 1
  (`0x02`)=1.
- **ERASE / EWEN / EWDS:** write cmd → strobe bit 6 (`0x40`, shared) → poll bit 1.
  EWEN unlocks writes, EWDS re-locks; write/erase are rejected while protected
  (default locked). **EWEN vs EWDS must be distinguished by the command word's
  sub-op, not the strobe** — libws `write_lock`/`write_unlock` are byte-identical
  and both emit EWEN.

The cartridge (external) EEPROM at `$C4`–`$C8` uses the identical protocol/layout.

## Resolved disputes

- **`$BE` bit 6/7 semantics.** Canonical (libws + WSdev raw wikitext + ares) =
  bit 6 SHORT/ERASE strobe, bit 7 PROTECT. A WSTech/Mednafen *legacy* layout
  (bit 6 protect, bit 7 initialize) exists; three authoritative sources back the
  canonical high-nibble layout, so we use it — but confirm per target model
  before shipping the EEPROM.
- **`SND_RANDOM` access.** Read-only per libws/WSdev/ares; Mednafen also allows a
  seeding write. We model it read-only.
- **Serial overrun-reset** is bit 5 (write-1), per libws/WSdev/ares.

## Open gaps

- `$BE` bit 6/7 assignment unconfirmed against physical hardware for a specific
  model.
- The EEPROM write-protect default and the exact `canWrite()` gating need a
  hardware check before the protocol state machine ships.
- `SND_NOISE` bit 3 read-back value (reset strobe) is modelled write-only
  (reads 0), per ares/Mednafen; not explicitly stated by libws/WSdev.

## Sources

libws `hardware.h` (WonderfulToolchain) · WSdev Sound / Serial / Internal EEPROM
<https://ws.nesdev.org/wiki/> · ares `ares/ws/` · Mednafen `wswan/` · WSMan
<http://daifukkat.su/docs/wsman/> (HTTP-only).
