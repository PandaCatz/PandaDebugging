#![forbid(unsafe_code)]

//! Headless entry point.
//!
//! - No arguments: run the deterministic synthetic core and print its capture
//!   summary. This is the reproducible baseline recorded in `CLAUDE.md`.
//! - `--rom <path>`: structurally validate an operator-supplied ROM and print
//!   derived facts only. ROM/game bytes are never echoed to stdout.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match args.first().map(String::as_str) {
        Some("--rom") => {
            let path = args.get(1).ok_or("usage: ws-cli --rom <path-to-rom>")?;
            let bytes = std::fs::read(path)?;
            let image = format_ws::RomImage::parse(&bytes)?;
            println!("rom: {} bytes", image.len());
            println!("bank aligned (64 KiB): {}", image.is_bank_aligned());
            println!("stored checksum:        {:#06x}", image.stored_checksum());
            println!(
                "computed (provisional): {:#06x}",
                image.computed_checksum_provisional()
            );
            // Exercise the owned-cartridge boundary.
            let cart = core_ws::WsCartridge::from_image(image)?;
            println!("owned cartridge: {} bytes", cart.rom().len());
            Ok(())
        }
        _ => {
            let summary = ws_testkit::run_synthetic(30)?;
            println!("synthetic core: deterministic headless capture");
            println!("final tick:        {}", summary.final_time.ticks());
            println!(
                "video frames:      {} (hash {:#018x})",
                summary.video_frames, summary.video_hash
            );
            println!(
                "audio packets:     {} / {} frames (hash {:#018x})",
                summary.audio_packets, summary.audio_frames, summary.audio_hash
            );
            println!("event stream hash: {:#018x}", summary.event_hash);
            Ok(())
        }
    }
}
