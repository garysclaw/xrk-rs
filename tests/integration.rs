//! Integration tests against a real XRK file.
//! Tests skip gracefully if the fixture file is not present (CI without data).

use xrk::XrkFile;

const XRK_PATH: &str = "tests/fixtures/38_Mobile_In_a_0023.xrk";

fn load() -> Option<XrkFile> {
    if !std::path::Path::new(XRK_PATH).exists() {
        eprintln!("⚠  Skipping: fixture not found at {XRK_PATH}");
        return None;
    }
    Some(XrkFile::open(XRK_PATH).expect("parse failed"))
}

// ─── Lap timing ──────────────────────────────────────────────────────────────

#[test]
fn test_lap_count() {
    let Some(s) = load() else { return };
    assert_eq!(s.laps.len(), 9);
}

#[test]
fn test_lap_numbers_sequential() {
    let Some(s) = load() else { return };
    for (i, lap) in s.laps.iter().enumerate() {
        assert_eq!(lap.number, (i + 1) as u16);
    }
}

#[test]
fn test_best_lap_is_lap7() {
    let Some(s) = load() else { return };
    let best = s.best_lap(5_000).expect("no best lap");
    assert_eq!(best.number, 7);
    assert_eq!(best.time_ms, 18_696);
    assert_eq!(best.time_str(), "0:18.696");
}

#[test]
fn test_lap1_time() {
    let Some(s) = load() else { return };
    let lap1 = s.laps.iter().find(|l| l.number == 1).unwrap();
    assert_eq!(lap1.time_ms, 32_295);
}

// ─── Channel discovery ────────────────────────────────────────────────────────

#[test]
fn test_channels_present() {
    let Some(s) = load() else { return };
    // File must have at least some channels
    assert!(!s.channels.is_empty(), "no channels decoded");
}

#[test]
fn test_channel_names_nonempty() {
    let Some(s) = load() else { return };
    for ch in &s.channels {
        assert!(!ch.name.is_empty(), "channel {} has empty name", ch.id);
    }
}

#[test]
fn test_channel_lookup_by_name() {
    let Some(s) = load() else { return };
    // These channel names are from THIS specific logger config.
    // Other users will have different names — the library just returns what's in the file.
    let names: Vec<&str> = s.channels.iter().map(|c| c.name.as_str()).collect();
    eprintln!("Channels in file: {:?}", names);

    // At minimum, every channel should be accessible by its own name
    for ch in &s.channels {
        assert!(s.channel(&ch.name).is_some(),
            "channel '{}' not findable by name", ch.name);
    }
}

#[test]
fn test_channel_lookup_nonexistent() {
    let Some(s) = load() else { return };
    assert!(s.channel("this_channel_does_not_exist").is_none());
}

// ─── Sample data ─────────────────────────────────────────────────────────────

#[test]
fn test_samples_have_valid_timestamps() {
    let Some(s) = load() else { return };
    for ch in &s.channels {
        for sample in &ch.samples {
            assert!(sample.time_sec >= 0.0,
                "negative timestamp in channel '{}'", ch.name);
            assert!(sample.time_sec < 10_000.0,
                "unreasonably large timestamp in channel '{}'", ch.name);
        }
    }
}

#[test]
fn test_samples_sorted_by_time() {
    let Some(s) = load() else { return };
    for ch in &s.channels {
        let times: Vec<f32> = ch.samples.iter().map(|s| s.time_sec).collect();
        for w in times.windows(2) {
            assert!(w[0] <= w[1],
                "channel '{}' samples not in time order", ch.name);
        }
    }
}

#[test]
fn test_raw_values_in_adc_range() {
    let Some(s) = load() else { return };
    for ch in &s.channels {
        // u16 is always 0–65535 by type, but sanity check non-zero data exists
        let has_nonzero = ch.samples.iter().any(|s| s.raw > 0);
        assert!(has_nonzero, "channel '{}' has all-zero samples", ch.name);
    }
}

// ─── Session info ─────────────────────────────────────────────────────────────

#[test]
fn test_session_duration_reasonable() {
    let Some(s) = load() else { return };
    assert!(s.info.duration_sec > 60.0,  "session too short");
    assert!(s.info.duration_sec < 3600.0, "session too long");
}

#[test]
fn test_track_name_present() {
    let Some(s) = load() else { return };
    assert!(!s.info.track.is_empty(), "track name is empty");
}

// ─── Config + calibration ─────────────────────────────────────────────────────

#[test]
fn test_two_point_calibration() {
    use xrk::Calibration;
    // 0.75V → 0mm, 4.10V → 50mm
    let cal = Calibration::two_point(0.75, 0.0, 4.10, 50.0);
    let raw_at_075v: u16 = (0.75 / 5.0 * 65535.0) as u16;
    let raw_at_410v: u16 = (4.10 / 5.0 * 65535.0) as u16;

    let result_low  = cal.apply(raw_at_075v);
    let result_high = cal.apply(raw_at_410v);

    assert!((result_low  - 0.0).abs()  < 0.5, "expected ~0mm, got {:.3}", result_low);
    assert!((result_high - 50.0).abs() < 0.5, "expected ~50mm, got {:.3}", result_high);
}

#[test]
fn test_logger_config_apply() {
    use xrk::{config::{LoggerConfig, Calibration}};

    let mut cfg = LoggerConfig::new("Test Car");
    cfg.add("MyChannel", Calibration::linear(10.0, -5.0), "mm");

    // At 0.5V: 10 * 0.5 - 5 = 0.0
    let raw_at_05v: u16 = (0.5 / 5.0 * 65535.0) as u16;
    let result = cfg.apply("MyChannel", raw_at_05v).unwrap();
    assert!((result - 0.0).abs() < 0.1, "expected ~0, got {}", result);

    // Unknown channel returns None
    assert!(cfg.apply("NotAChannel", 1000).is_none());
}

#[test]
fn test_per_lap_stats() {
    let Some(s) = load() else { return };
    // Take any channel with enough data
    let ch = s.channels.iter().find(|c| c.samples.len() > 1000);
    let Some(ch) = ch else { return };

    let stats = ch.per_lap_stats(&s.laps);
    assert_eq!(stats.len(), s.laps.len());

    // Stats for laps with samples must be sensible
    for stat in &stats {
        if stat.n_samples > 0 {
            assert!(stat.min <= stat.max);
            assert!(stat.mean >= stat.min as f64);
            assert!(stat.mean <= stat.max as f64);
            assert!(stat.std  >= 0.0);
        }
    }
}
