//! Example: analyze a session and print a summary report.
//!
//! Usage:
//!   cargo run --example analyze -- data/38_Mobile_In_a_0023.xrk

use std::env;
use xrk::{XrkFile, CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK};

fn main() {
    let path = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: analyze <file.xrk>");
        std::process::exit(1);
    });

    println!("Parsing {path} ...\n");

    let session = XrkFile::open(&path).unwrap_or_else(|e| {
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
    println!("  Duration : {:.1}s", session.info.duration_sec);
    println!("  File     : {:.2} MB", session.info.file_size as f64 / 1_048_576.0);
    println!("  Channels : {}", session.info.channel_count);
    println!("  Laps     : {}", session.laps.len());
    println!();

    // ── Lap times ─────────────────────────────────────────────────────────────
    println!("LAP TIMES");
    println!("{:-<40}", "");
    for lap in &session.laps {
        let marker = if session.best_lap(5_000)
            .map_or(false, |b| b.number == lap.number) { " ← best" } else { "" };
        println!("  Lap {:2}  {}{}", lap.number, lap.time_str(), marker);
    }
    println!();

    // ── Shock channels ────────────────────────────────────────────────────────
    let shock_ids   = [CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK];
    let shock_names = ["LF_Shock", "RF_Shock", "LR_Shock", "RR_Shock"];

    println!("SHOCK POTENTIOMETER SUMMARY");
    println!("{:-<40}", "");
    for (&id, &name) in shock_ids.iter().zip(shock_names.iter()) {
        if let Some(ch) = session.channel_by_id(id) {
            let rate = session.sample_rate_hz(id).unwrap_or(0.0);
            println!(
                "  {name:<12} {:6} samples  {:.0} Hz  {:.3}V–{:.3}V  mean={:.3}V",
                ch.samples.len(),
                rate,
                ch.min_raw().unwrap_or(0) as f32 / 65535.0 * 5.0,
                ch.max_raw().unwrap_or(0) as f32 / 65535.0 * 5.0,
                ch.mean_voltage().unwrap_or(0.0),
            );
        }
    }
    println!();

    // ── Per-lap shock averages ────────────────────────────────────────────────
    println!("PER-LAP SHOCK AVERAGES (mean voltage)");
    println!("{:-<72}", "");
    println!("  {:>4}  {:>8}  {:>8}  {:>8}  {:>8}  {:>8}", "Lap", "Time", "LF_V", "RF_V", "LR_V", "RR_V");
    println!("{:-<72}", "");

    for lap in &session.laps {
        let mut row = format!("  {:>4}  {:>8}", lap.number, lap.time_str());
        for &id in &shock_ids {
            if let Some(ch) = session.channel_by_id(id) {
                let stats = ch.per_lap_stats(&session.laps);
                if let Some(s) = stats.iter().find(|s| s.lap_number == lap.number) {
                    row.push_str(&format!("  {:>8.3}V", s.mean_voltage()));
                } else {
                    row.push_str("       N/A");
                }
            }
        }
        println!("{row}");
    }
    println!();

    // ── Channels present ──────────────────────────────────────────────────────
    println!("CHANNELS DECODED");
    println!("{:-<40}", "");
    let mut channels: Vec<_> = session.channels.iter().collect();
    channels.sort_by_key(|c| c.id);
    for ch in channels {
        let rate = session.sample_rate_hz(ch.id).unwrap_or(0.0);
        println!("  [{:>3}] {:<18} {:>7} samples  {:>5.1} Hz",
            ch.id, ch.name, ch.samples.len(), rate);
    }
}
