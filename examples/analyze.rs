//! Example: parse a session and print a summary.
//! Optionally apply a LoggerConfig for calibrated output.
//!
//! Usage:
//!   cargo run --example analyze -- session.xrk [config.json]

use std::env;
use xrk::XrkFile;

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).unwrap_or_else(|| {
        eprintln!("Usage: analyze <file.xrk> [config.json]");
        std::process::exit(1);
    });

    let session = XrkFile::open(path).unwrap_or_else(|e| {
        eprintln!("Error: {e}");
        std::process::exit(1);
    });

    // ── Session info ──────────────────────────────────────────────────────────
    println!("╔═══════════════════════════════════════╗");
    println!("║          SESSION SUMMARY              ║");
    println!("╚═══════════════════════════════════════╝");
    println!("  Track    : {}", session.info.track);
    println!("  Date     : {} {}", session.info.date, session.info.time);
    println!("  Vehicle  : {}", session.info.vehicle);
    println!("  Logger   : {}", session.info.logger);
    println!("  Duration : {:.1}s", session.info.duration_sec);
    println!(
        "  File     : {:.2} MB",
        session.info.file_size as f64 / 1_048_576.0
    );
    println!("  Laps     : {}", session.laps.len());
    println!();

    // ── Lap times ─────────────────────────────────────────────────────────────
    println!("LAP TIMES");
    println!("{:-<40}", "");
    for lap in &session.laps {
        let best_marker = session
            .best_lap(0)
            .map_or(false, |b| b.number == lap.number);
        println!(
            "  Lap {:2}  {}{}",
            lap.number,
            lap.time_str(),
            if best_marker { "  ← best" } else { "" }
        );
    }
    println!();

    // ── All channels found in the file ────────────────────────────────────────
    println!("CHANNELS IN FILE");
    println!("{:-<60}", "");
    println!(
        "  {:>4}  {:<20} {:<12} {:>8}  {:>7}",
        "ID", "Name", "Short", "Samples", "Hz"
    );
    println!("{:-<60}", "");

    let mut channels: Vec<_> = session.channels.iter().collect();
    channels.sort_by_key(|c| c.id);

    for ch in &channels {
        println!(
            "  {:>4}  {:<20} {:<12} {:>8}  {:>6.1}",
            ch.id,
            ch.name,
            ch.short_name,
            ch.samples.len(),
            ch.sample_rate_hz(session.info.duration_sec)
        );
    }
    println!();

    // ── Per-lap stats for every channel ───────────────────────────────────────
    if !session.laps.is_empty() && !session.channels.is_empty() {
        println!("PER-LAP RAW STATISTICS (mean ADC count, 0–65535 = 0–5V)");
        println!("{:-<80}", "");

        // Show a few interesting channels (limit output width)
        for ch in channels.iter().take(6) {
            let stats = ch.per_lap_stats(&session.laps);
            println!("\n  {} ({}):", ch.name, ch.short_name);
            for s in &stats {
                if s.n_samples > 0 {
                    println!(
                        "    Lap {:2}  [{:6.0}–{:6.0}]  mean={:6.0}  std={:5.0}  n={}",
                        s.lap_number, s.min as f64, s.max as f64, s.mean, s.std, s.n_samples
                    );
                }
            }
        }
    }
}
