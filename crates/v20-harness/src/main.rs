// SPDX-License-Identifier: GPL-3.0-or-later
#![forbid(unsafe_code)]

//! Validate `cpu-v30mz` against the SingleStepTests/v20 oracle.
//!
//! Reads `fixtures/v20-tests/prepared/*.tsv` (produced by `tools/v20_prep.py`),
//! runs each case through one `Cpu::step`, and reports divergences per opcode.
//!
//! The NEC V20 has proprietary extensions the WonderSwan's V30MZ omits, and the
//! two can differ on officially-undefined flag results (e.g. after `MUL`, or
//! multi-bit shifts). So some divergence — especially *flags-only* — is expected
//! and informative, not necessarily a bug. Real state divergences (registers or
//! memory) are the ones to chase.

use cpu_v30mz::{Cpu, CpuBus, Flags};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const ADDR_MASK: u32 = 0x000F_FFFF;
/// Architectural flag bits we model: CF, reserved-1, PF, AF, ZF, SF, TF, IF, DF, OF.
const FLAG_MASK: u16 = 0x0FD7;

const REG_NAMES: [&str; 14] = [
    "ax", "bx", "cx", "dx", "cs", "ss", "ds", "es", "sp", "bp", "si", "di", "ip", "fl",
];
const FLAG_NAMES: [&str; 12] = [
    "CF", "r1", "PF", "-", "AF", "-", "ZF", "SF", "TF", "IF", "DF", "OF",
];

struct Mem {
    bytes: Vec<u8>,
    dirty: Vec<u32>,
}

impl Mem {
    fn new() -> Self {
        Self {
            bytes: vec![0; (ADDR_MASK as usize) + 1],
            dirty: Vec::new(),
        }
    }
}

impl CpuBus for Mem {
    fn read_u8(&mut self, address: u32) -> u8 {
        self.bytes[(address & ADDR_MASK) as usize]
    }
    fn write_u8(&mut self, address: u32, value: u8) {
        let index = address & ADDR_MASK;
        self.bytes[index as usize] = value;
        self.dirty.push(index);
    }
    fn io_read_u8(&mut self, _port: u16) -> u8 {
        0
    }
    fn io_write_u8(&mut self, _port: u16, _value: u8) {}
}

struct Case {
    init: [u16; 14],
    iram: Vec<(u32, u8)>,
    exp: [u16; 14],
    eram: Vec<(u32, u8)>,
}

fn parse_case(line: &str) -> Option<Case> {
    let mut it = line.split_whitespace();
    let mut init = [0u16; 14];
    for slot in &mut init {
        *slot = it.next()?.parse().ok()?;
    }
    (it.next()? == "R").then_some(())?;
    let iram = parse_pairs(&mut it)?;
    (it.next()? == "X").then_some(())?;
    let mut exp = [0u16; 14];
    for slot in &mut exp {
        *slot = it.next()?.parse().ok()?;
    }
    (it.next()? == "E").then_some(())?;
    let eram = parse_pairs(&mut it)?;
    Some(Case {
        init,
        iram,
        exp,
        eram,
    })
}

fn parse_pairs<'a>(it: &mut impl Iterator<Item = &'a str>) -> Option<Vec<(u32, u8)>> {
    let count: usize = it.next()?.parse().ok()?;
    let mut pairs = Vec::with_capacity(count);
    for _ in 0..count {
        let address: u32 = it.next()?.parse().ok()?;
        let value: u8 = it.next()?.parse().ok()?;
        pairs.push((address, value));
    }
    Some(pairs)
}

#[derive(Default)]
struct Report {
    total: u64,
    pass: u64,
    state_fail: u64,
    flag_fail: u64,
    skipped: u64,
    flag_bit_diffs: [u64; 12],
    sample: Vec<String>,
}

fn byte_at(case: &Case, address: u32) -> u8 {
    let addr = address & ADDR_MASK;
    case.iram
        .iter()
        .rev()
        .find(|&&(a, _)| (a & ADDR_MASK) == addr)
        .map_or(0, |&(_, v)| v)
}

/// True if the instruction uses a V20-only prefix/escape absent on the V30MZ
/// (`REPC`/`REPNC` = `0x64`/`0x65`, or the `0x0F` extension escape). Such cases
/// cannot match a V30MZ, where those bytes are inert.
fn uses_v20_only(case: &Case) -> bool {
    let mut phys =
        ((u32::from(case.init[4]) << 4).wrapping_add(u32::from(case.init[12]))) & ADDR_MASK;
    for _ in 0..8 {
        match byte_at(case, phys) {
            0x64 | 0x65 | 0x0F => return true,
            0x26 | 0x2E | 0x36 | 0x3E | 0xF0 | 0xF2 | 0xF3 => phys = (phys + 1) & ADDR_MASK,
            _ => return false,
        }
    }
    false
}

/// Run one case; return the produced register file and whether expected memory
/// matched. Touched memory is restored to zero afterwards.
fn run_case(case: &Case, mem: &mut Mem) -> ([u16; 14], bool) {
    for &(address, value) in &case.iram {
        mem.bytes[(address & ADDR_MASK) as usize] = value;
    }
    mem.dirty.clear();

    let mut cpu = Cpu::new();
    {
        let r = &mut cpu.regs;
        let slots = [
            &mut r.ax, &mut r.bx, &mut r.cx, &mut r.dx, &mut r.cs, &mut r.ss, &mut r.ds, &mut r.es,
            &mut r.sp, &mut r.bp, &mut r.si, &mut r.di, &mut r.ip,
        ];
        for (slot, value) in slots.into_iter().zip(case.init) {
            *slot = value;
        }
        r.flags = Flags::from_word(case.init[13]);
    }

    cpu.step(mem);

    let got = [
        cpu.regs.ax,
        cpu.regs.bx,
        cpu.regs.cx,
        cpu.regs.dx,
        cpu.regs.cs,
        cpu.regs.ss,
        cpu.regs.ds,
        cpu.regs.es,
        cpu.regs.sp,
        cpu.regs.bp,
        cpu.regs.si,
        cpu.regs.di,
        cpu.regs.ip,
        cpu.regs.flags.to_word(),
    ];

    let ram_ok = case
        .eram
        .iter()
        .all(|&(address, value)| mem.bytes[(address & ADDR_MASK) as usize] == value);

    for &(address, _) in &case.iram {
        mem.bytes[(address & ADDR_MASK) as usize] = 0;
    }
    let dirty = std::mem::take(&mut mem.dirty);
    for address in dirty {
        mem.bytes[address as usize] = 0;
    }

    (got, ram_ok)
}

fn describe(got: &[u16; 14], case: &Case, ram_ok: bool) -> String {
    let mut msg = String::new();
    for ((g, e), name) in got.iter().zip(&case.exp).zip(&REG_NAMES).take(13) {
        if g != e {
            let _ = write!(msg, "{name}: got {g:#06x} exp {e:#06x}  ");
        }
    }
    if !ram_ok {
        msg.push_str("ram-mismatch");
    }
    msg
}

fn evaluate(path: &Path, mem: &mut Mem) -> Report {
    let mut report = Report::default();
    let Ok(text) = fs::read_to_string(path) else {
        return report;
    };
    for line in text.lines() {
        let Some(case) = parse_case(line) else {
            continue;
        };
        if uses_v20_only(&case) {
            report.skipped += 1;
            continue;
        }
        report.total += 1;
        let (got, ram_ok) = run_case(&case, mem);
        let regs_ok = got.iter().zip(&case.exp).take(13).all(|(g, e)| g == e);
        let flags_ok = (got[13] ^ case.exp[13]) & FLAG_MASK == 0;

        if regs_ok && ram_ok && flags_ok {
            report.pass += 1;
        } else if !regs_ok || !ram_ok {
            report.state_fail += 1;
            if report.sample.len() < 2 {
                report.sample.push(describe(&got, &case, ram_ok));
            }
        } else {
            report.flag_fail += 1;
            let diff = (got[13] ^ case.exp[13]) & FLAG_MASK;
            for (bit, slot) in report.flag_bit_diffs.iter_mut().enumerate() {
                if diff & (1 << bit) != 0 {
                    *slot += 1;
                }
            }
        }
    }
    report
}

fn main() {
    let dir = Path::new("fixtures/v20-tests/prepared");
    if !dir.is_dir() {
        eprintln!("fixture directory not found: {}", dir.display());
        eprintln!("download the v20 set and run tools/v20_prep.py first.");
        std::process::exit(2);
    }

    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("read prepared dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "tsv"))
        .collect();
    entries.sort();

    let mut mem = Mem::new();
    let mut grand = Report::default();
    let mut offenders: Vec<(String, Report)> = Vec::new();
    let mut flag_offenders: Vec<(String, [u64; 12])> = Vec::new();

    println!(
        "{:<10} {:>8} {:>8} {:>10} {:>10}",
        "opcode", "total", "pass", "state_f", "flag_f"
    );
    for path in &entries {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let report = evaluate(path, &mut mem);
        println!(
            "{:<10} {:>8} {:>8} {:>10} {:>10}",
            name, report.total, report.pass, report.state_fail, report.flag_fail
        );
        grand.total += report.total;
        grand.pass += report.pass;
        grand.state_fail += report.state_fail;
        grand.flag_fail += report.flag_fail;
        grand.skipped += report.skipped;
        for (g, r) in grand.flag_bit_diffs.iter_mut().zip(report.flag_bit_diffs) {
            *g += r;
        }
        if report.flag_fail > 0 {
            flag_offenders.push((name.clone(), report.flag_bit_diffs));
        }
        if report.state_fail > 0 {
            offenders.push((name, report));
        }
    }

    let pct = 100.0 * grand.pass as f64 / grand.total.max(1) as f64;
    println!(
        "\nTOTAL {} run | pass {} ({pct:.2}%) | state_fail {} | flag_fail {} | skipped(V20-only) {}",
        grand.total, grand.pass, grand.state_fail, grand.flag_fail, grand.skipped
    );

    println!("\nflag-only diffs by bit:");
    for (bit, count) in grand.flag_bit_diffs.iter().enumerate() {
        if *count > 0 {
            println!("  {:<3} {count}", FLAG_NAMES[bit]);
        }
    }

    if !offenders.is_empty() {
        println!("\nopcodes with STATE divergences (real-bug candidates):");
        offenders.sort_by_key(|(_, r)| std::cmp::Reverse(r.state_fail));
        for (name, report) in &offenders {
            println!("  {name}: {} / {}", report.state_fail, report.total);
            for s in &report.sample {
                println!("      e.g. {s}");
            }
        }
    }

    if !flag_offenders.is_empty() {
        println!("\nflag-only divergences by opcode (which bits differ):");
        for (name, bits) in &flag_offenders {
            let mut line = String::new();
            for (bit, count) in bits.iter().enumerate() {
                if *count > 0 {
                    let _ = write!(line, "{}={count} ", FLAG_NAMES[bit]);
                }
            }
            println!("  {name}: {line}");
        }
    }
}
