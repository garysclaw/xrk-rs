//! Integration tests — run against the real XRK file.
//!
//! These tests use the actual session file. Skip gracefully if not present
//! (CI won't have the file; developers run with the real data).

use xrk::{XrkFile, CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK,
          CH_VERTICAL_ACC};

const XRK_PATH: &str = "tests/fixtures/38_Mobile_In_a_0023.xrk";

fn load() -> Option<XrkFile> {
    if !std::path::Path::new(XRK_PATH).exists() {
        eprintln!("⚠  Skipping: fixture file not found at {XRK_PATH}");
        return None;
    }
    Some(XrkFile::open(XRK_PATH).expect("failed to parse XRK"))
}

#[test]
fn test_lap_count() {
    let Some(s) = load() else { return };
    assert_eq!(s.laps.len(), 9, "expected 9 laps");
}

#[test]
fn test_lap_numbers_sequential() {
    let Some(s) = load() else { return };
    for (i, lap) in s.laps.iter().enumerate() {
        assert_eq!(lap.number, (i + 1) as u16);
    }
}

#[test]
fn test_best_lap() {
    let Some(s) = load() else { return };
    let best = s.best_lap(5_000).expect("no best lap");
    assert_eq!(best.number, 7);
    // 18.696s = 18696ms
    assert_eq!(best.time_ms, 18_696);
    assert_eq!(best.time_str(), "0:18.696");
}

#[test]
fn test_lap1_time() {
    let Some(s) = load() else { return };
    let lap1 = s.laps.iter().find(|l| l.number == 1).unwrap();
    assert_eq!(lap1.time_ms, 32_295);
}

#[test]
fn test_all_four_shock_channels_present() {
    let Some(s) = load() else { return };
    for id in [CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK] {
        assert!(
            s.channel_by_id(id).is_some(),
            "missing shock channel id={id}"
        );
    }
}

#[test]
fn test_shock_sample_counts() {
    let Some(s) = load() else { return };
    // Each shock should have 50,000+ samples over the session
    for id in [CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK] {
        let ch = s.channel_by_id(id).unwrap();
        assert!(
            ch.samples.len() > 40_000,
            "channel {id} has only {} samples",
            ch.samples.len()
        );
    }
}

#[test]
fn test_shock_voltage_range() {
    let Some(s) = load() else { return };
    // All shocks should span at least 2V (active suspension on a real course)
    for id in [CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK] {
        let ch = s.channel_by_id(id).unwrap();
        let v_min = ch.min_raw().unwrap() as f32 / 65535.0 * 5.0;
        let v_max = ch.max_raw().unwrap() as f32 / 65535.0 * 5.0;
        assert!(
            v_max - v_min > 2.0,
            "channel {id} voltage range too small: {:.3}V",
            v_max - v_min
        );
    }
}

#[test]
fn test_vertical_accel_mean_near_1g() {
    let Some(s) = load() else { return };
    // VerticalAcc mean should be ~1G (car stayed on the ground)
    // Using known calibration: 0G=2.5V, 1G=1.185V
    let ch = s.channel_by_id(CH_VERTICAL_ACC).expect("no VerticalAcc");
    let mean_v = ch.mean_voltage().unwrap();
    let mean_g = (mean_v - 2.5) / 1.185;
    assert!(
        (mean_g - 1.0).abs() < 0.1,
        "VerticalAcc mean G = {mean_g:.3}, expected ~1.0G"
    );
}

#[test]
fn test_channel_by_name() {
    let Some(s) = load() else { return };
    assert!(s.channel_by_name("LF_Shock").is_some());
    assert!(s.channel_by_name("RF_Shock").is_some());
    assert!(s.channel_by_name("nonexistent").is_none());
}

#[test]
fn test_per_lap_stats_shape() {
    let Some(s) = load() else { return };
    let ch = s.channel_by_id(CH_LF_SHOCK).unwrap();
    let stats = ch.per_lap_stats(&s.laps);
    assert_eq!(stats.len(), 9, "should have stats for all 9 laps");
    // Every race lap (2–7) should have > 500 samples
    for stat in stats.iter().filter(|s| s.lap_number >= 2 && s.lap_number <= 7) {
        assert!(
            stat.n_samples > 500,
            "lap {} has only {} samples",
            stat.lap_number, stat.n_samples
        );
    }
}

#[test]
fn test_samples_sorted_by_time() {
    let Some(s) = load() else { return };
    let ch = s.channel_by_id(CH_LF_SHOCK).unwrap();
    let times: Vec<f32> = ch.samples.iter().map(|s| s.time_sec).collect();
    for w in times.windows(2) {
        assert!(w[0] <= w[1], "samples not in time order: {} > {}", w[0], w[1]);
    }
}

#[test]
fn test_session_info_metadata() {
    let Some(s) = load() else { return };
    assert!(!s.info.track.is_empty(), "track should not be empty");
    assert!(s.info.duration_sec > 600.0, "session should be >600s");
    assert!(s.info.duration_sec < 800.0, "session should be <800s");
}
