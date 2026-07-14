# Cartridge footer (internal header)

The WonderSwan cartridge's internal header is the **last 16 bytes** of the ROM
image (a "footer"). Because the final ROM bank maps to the CPU reset vector
`FFFF:0000`, footer byte `0x00` sits at physical `0xFFFF0`, so the first five
footer bytes are the boot far-jump the V30MZ runs at power-on.

This layout was transcribed field-by-field and **adversarially verified** across
five independent research passes (WSMan, the WSdev wiki, ares, Mednafen, and the
Sacred Tech Scroll), then each high-stakes field was re-checked by a separate
agent tasked to refute it. Decoded in `format-ws` (`CartHeader`); every
undocumented code becomes an explicit `Other`/`Unknown`/`None`, never a guess.

## Field layout

Offsets are within the 16-byte footer.

| Offset | Size | Field | Meaning |
|-------:|:----:|-------|---------|
| `0x00` | 5 | Boot far-jump | `EA off_lo off_hi seg_lo seg_hi` — `JMP FAR seg:off` at the reset vector. `0xEA` or the console won't boot. |
| `0x05` | 1 | Maintenance | Must be zero (low bits gate execution). On Color, bit 7 = splash/boot bypass. |
| `0x06` | 1 | Publisher ID | Developer code (registry lookup, e.g. `01`=Bandai). |
| `0x07` | 1 | System | `0` = WonderSwan (mono), `1` = WonderSwan Color. BIOS reportedly ignores it. |
| `0x08` | 1 | Game ID | Cartridge/game number (BCD, last two SKU digits). |
| `0x09` | 1 | Version | Game revision. (bit 7 = internal-EEPROM write-protect per WSdev/STSWS, *assumed/unverified*.) |
| `0x0A` | 1 | ROM size code | See table below. Emulators derive real size from file length. |
| `0x0B` | 1 | Save type code | Save-memory type + size. See table below. |
| `0x0C` | 1 | Flags | Orientation / bus width / ROM speed. See table below. |
| `0x0D` | 1 | Mapper / RTC | Low nibble = mapper (`0`=Bandai 2001/KARNAK, `1`=Bandai 2003 = RTC). High nibble must be zero. |
| `0x0E` | 2 | Checksum | 16-bit little-endian; see algorithm below. |

## ROM size code (`0x0A`)

Unanimous across sources for `02`–`09`; `00`/`01` are inferred and `0A`/`0B` are
WSdev-only extensions.

| Code | Size | Code | Size |
|-----:|------|-----:|------|
| `00` | 128 KiB (1 Mbit) *inf.* | `06` | 4 MiB (32 Mbit) |
| `01` | 256 KiB (2 Mbit) *inf.* | `07` | 6 MiB (48 Mbit) |
| `02` | 512 KiB (4 Mbit) | `08` | 8 MiB (64 Mbit) |
| `03` | 1 MiB (8 Mbit) | `09` | 16 MiB (128 Mbit) |
| `04` | 2 MiB (16 Mbit) | `0A` | 32 MiB (256 Mbit) *WSdev only* |
| `05` | 3 MiB (24 Mbit) | `0B` | 64 MiB (512 Mbit) *WSdev only* |

## Save type code (`0x0B`)

| Code | Type | Size |
|-----:|------|------|
| `00` | none | 0 |
| `01` | SRAM | **32 KiB** (see dispute) |
| `02` | SRAM | 32 KiB (256 Kbit) |
| `03` | SRAM | 128 KiB (1 Mbit) |
| `04` | SRAM | 256 KiB (2 Mbit) |
| `05` | SRAM | 512 KiB (4 Mbit) |
| `10` | EEPROM | 128 B (1 Kbit) |
| `20` | EEPROM | 2 KiB (16 Kbit) |
| `50` | EEPROM | 1 KiB (8 Kbit) *`?` in WSMan* |

## Flags byte (`0x0C`)

| Bit | Meaning |
|----:|---------|
| 0 | Display orientation: `0` = horizontal, `1` = vertical. **Unanimous.** |
| 1 | Unknown in the bit-2 reading (see dispute). |
| 2 | External ROM bus width: **`0` = 8-bit, `1` = 16-bit** (bug #9). |
| 3 | ROM access speed / wait state (semantics source-dependent; timing deferred). |
| 4–7 | Unknown. |

Bits 2–3 mirror the runtime `REG_HW_FLAGS` / System-Control register at port
`$A0`. This footer flags byte is **distinct from** that register.

## Checksum (`0x0E`, little-endian)

A per-byte sum, mod `0x10000`, over **every byte of the image except the final
two checksum bytes** — padding included. Plain summation (no CRC, no
complement). Stored low byte at `0x0E`, high byte at `0x0F`. It is **not**
required to be correct to boot (WonderWitch stores `0x0000`).

Verified from Mednafen's executed loader:
`for i in 0..rom_size-2 { crc = crc.wrapping_add(rom[i]) }` with a `u16`
accumulator. `format-ws`'s `computed_checksum` matches this exactly.

## Resolved source disputes

- **Bus-width bit (highest-stakes).** WSMan's *cartridge-metadata* table and the
  Sacred Tech Scroll place bus width at **bit 1** with polarity `0`=16-bit /
  `1`=8-bit. The WSdev wiki (raw diagram), the ares implementation
  (`metadata[12] & 4`), and WSMan's **own** `REG_HW_FLAGS` register all place it
  at **bit 2** with polarity `0`=8-bit / `1`=16-bit. We take **bit 2, `0`=8/`1`=16**:
  WSMan self-contradicts (its register table disagrees with its metadata table),
  and the bit-2 reading is the one an accuracy-focused emulator (ares) runs
  successfully against the commercial library. This is documentary convergence,
  **not** a physical cart-dump confirmation — see open gaps.
- **Save code `0x01` size.** Historically documented as 8 KB (64 Kbit), but WSdev
  and ares both allocate **32 KiB** (256 Kbit) because every known `0x01` cart
  ships a 256 Kbit chip; Mednafen still uses 8 KB. Under-allocating corrupts
  saves, so we use 32 KiB.
- **Checksum wording.** WSMan says "sum of ROM words"; STSWS and both emulators
  confirm a per-**byte** sum. Byte-wise is authoritative.

## Open gaps (do not harden into facts)

- Bus-width bit position/polarity is not settled by any single authority; the
  deciding "footer bits 2–3 mirror System-Control bits 2–3" claim is
  single-source (WSdev). Confirm by decoding a known 8-bit-bus cart (e.g. Pocket
  Challenge V2) and a known 16-bit cart.
- Save code `0x01` true size (WSdev "under investigation").
- Flags byte bits 1 and 4–7; the maintenance byte's non-bit-7 "must be zero"
  purpose.
- ROM-size codes `00`/`01`/`0A`/`0B` are inferred/unofficial.
- Checksum endianness inferred from Mednafen, not stated by any hardware doc.
- Whether the canonical unit is the 16-byte footer or WSMan's documented 10–11
  trailing bytes.

## Sources

WSMan <http://daifukkat.su/docs/wsman/> (HTTP-only) · WSdev ROM header
<https://ws.nesdev.org/wiki/ROM_header> · ares `mia/medium/wonderswan.cpp` ·
Mednafen `wswan/main.cpp` · Sacred Tech Scroll <http://perfectkiosk.net/stsws.html>.
