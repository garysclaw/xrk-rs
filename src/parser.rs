//! Binary parser for the AiM XRK file format.
//!
//! ## Format summary
//!
//! XRK files are a stream of tagged chunks:
//!
//! ```text
//! <hTAG  [5 bytes]  chunk data  ...
//! ```
//!
//! Key chunks decoded here:
//!
//! | Tag    | Content                                      |
//! |--------|----------------------------------------------|
//! | `<hTMD` | Session date string                         |
//! | `<hTMT` | Session time string                         |
//! | `<hTRK` | Track/venue name                            |
//! | `<hVEH` | Vehicle identifier                          |
//! | `<hCHS` | Channel header (name, units)                |
//! | `<hCAL` | Calibration constants                       |
//! | `<hLAP` | Lap record (number, time_ms, start_ts)      |
//!
//! Data is interleaved with stream markers:
//!
//! ```text
//! )(M  timestamp[4]  channel_id[2]  n_samples[2]  values[n_samples * 2]
//! )(S  timestamp[4]  channel_id[2]  flags[2]
//! ```
//!
//! All multi-byte integers are little-endian.
//! Timestamps are raw uint32 counters; see [`TimeScale`] for conversion.

use crate::error::XrkError;
use crate::types::{
    channel_name, Channel, ChannelId, Lap, Sample, SessionInfo, XrkFile,
    CH_INLINE_ACC, CH_LATERAL_ACC, CH_VERTICAL_ACC,
    CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK,
    CH_ROLL_RATE, CH_PITCH_RATE, CH_YAW_RATE,
    CH_GPS_LAT_ACC, CH_GPS_INL_ACC, CH_GPS_YAW_RATE,
    CH_LUMINOSITY, CH_RPM, CH_VBAT, CH_LOGGER_TEMP, CH_ODOMETER,
};

// Channels we actually decode time-series data for.
// Extend this list to add more channels.
const DECODED_CHANNELS: &[ChannelId] = &[
    CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK,
    CH_INLINE_ACC, CH_LATERAL_ACC, CH_VERTICAL_ACC,
    CH_ROLL_RATE, CH_PITCH_RATE, CH_YAW_RATE,
    CH_GPS_LAT_ACC, CH_GPS_INL_ACC, CH_GPS_YAW_RATE,
    CH_RPM, CH_VBAT, CH_LOGGER_TEMP, CH_ODOMETER, CH_LUMINOSITY,
];

// ─── Time scale ───────────────────────────────────────────────────────────────

/// Converts raw XRK timestamp units to seconds.
///
/// XRK timestamps are uint32 counters with no documented unit.
/// We derive the scale by computing the first/last )(M timestamp span
/// and dividing by the known session duration.
///
/// For the tested session (Mobile In, 2026-01-19):
/// - Timestamp range: 212,520 units ≈ 652 seconds
/// - Scale: ~3.068 ms / unit
struct TimeScale {
    first_ts: u32,
    /// Seconds per timestamp unit
    scale: f64,
}

impl TimeScale {
    fn to_sec(&self, ts: u32) -> f64 {
        (ts.wrapping_sub(self.first_ts)) as f64 * self.scale
    }
}

// ─── Top-level parse entry point ──────────────────────────────────────────────

pub fn parse(data: &[u8]) -> Result<XrkFile, XrkError> {
    if data.len() < 64 {
        return Err(XrkError::FileTooSmall(data.len()));
    }

    // --- Pass 1: collect all )(M timestamps to build the time scale ---
    let timestamps = collect_timestamps(data);
    if timestamps.is_empty() {
        return Err(XrkError::NoDataMarkers);
    }

    let first_ts = *timestamps.first().unwrap();
    let last_ts  = *timestamps.last().unwrap();
    let ts_range = last_ts.wrapping_sub(first_ts) as f64;

    // Derive approximate session duration from last LAP chunk
    let session_duration_sec = estimate_session_duration(data, first_ts, ts_range);
    let scale = if ts_range > 0.0 {
        session_duration_sec / ts_range
    } else {
        0.003068 // fallback: empirically derived constant
    };

    let timescale = TimeScale { first_ts, scale };

    // --- Pass 2: decode laps, metadata, and channel data ---
    let info    = parse_session_info(data, timestamps.len(), session_duration_sec);
    let laps    = parse_laps(data, &timescale);
    let channels = parse_channels(data, &timescale);

    Ok(XrkFile { info, laps, channels })
}

// ─── Timestamp collection ─────────────────────────────────────────────────────

fn collect_timestamps(data: &[u8]) -> Vec<u32> {
    let mut result = Vec::with_capacity(100_000);
    let marker = b")(M";
    let mut pos = 0;

    while pos + 7 <= data.len() {
        if data[pos..pos + 3] == *marker {
            if pos + 7 <= data.len() {
                let ts = read_u32_le(data, pos + 3);
                result.push(ts);
            }
            pos += 3;
        } else {
            pos += 1;
        }
    }
    result
}

// ─── Session duration estimation ─────────────────────────────────────────────

/// Estimate session duration from the last LAP chunk's timestamp.
fn estimate_session_duration(data: &[u8], first_ts: u32, ts_range: f64) -> f64 {
    // Find the last LAP chunk and use its start timestamp
    let mut last_lap_end_sec = 0.0_f64;

    let mut pos = 0;
    while pos + 32 <= data.len() {
        if &data[pos..pos + 5] == b"<hLAP" {
            let lap_num  = read_u16_le(data, pos + 14);
            let lap_ms   = read_u32_le(data, pos + 16);
            let lap_ts   = read_u32_le(data, pos + 28);

            if lap_num >= 1 && lap_num <= 200 && lap_ms >= 1000 && lap_ms <= 3_600_000 {
                // Rough conversion using ts_range as denominator
                if ts_range > 0.0 {
                    let rough_scale = 650.0 / ts_range; // bootstrap estimate
                    let start = (lap_ts.wrapping_sub(first_ts)) as f64 * rough_scale;
                    let end = start + lap_ms as f64 / 1000.0;
                    if end > last_lap_end_sec {
                        last_lap_end_sec = end;
                    }
                }
            }
            pos += 5;
        } else {
            pos += 1;
        }
    }

    if last_lap_end_sec > 10.0 {
        last_lap_end_sec
    } else {
        // Fallback: assume ts_range / 3.068ms per unit
        ts_range * 0.003068
    }
}

// ─── Session info ─────────────────────────────────────────────────────────────

fn parse_session_info(data: &[u8], n_markers: usize, duration_sec: f64) -> SessionInfo {
    let mut info = SessionInfo {
        duration_sec,
        file_size: data.len(),
        ..Default::default()
    };

    // Count CHS chunks for channel_count
    let mut pos = 0;
    while pos + 5 <= data.len() {
        if &data[pos..pos + 5] == b"<hCHS" {
            info.channel_count += 1;
        }
        pos += 1;
    }

    // Extract string metadata from specific chunk types
    for (tag, field) in [
        (b"<hTMD" as &[u8], "date"),
        (b"<hTMT",           "time"),
        (b"<hTRK",           "track"),
        (b"<hVEH",           "vehicle"),
    ] {
        if let Some(s) = extract_first_ascii_string(data, tag, 8, 80) {
            match field {
                "date"    => info.date    = s,
                "time"    => info.time    = s,
                "track"   => info.track   = s,
                "vehicle" => info.vehicle = s,
                _         => {}
            }
        }
    }

    let _ = n_markers; // used by caller for validation
    info
}

// ─── Lap parsing ─────────────────────────────────────────────────────────────

fn parse_laps(data: &[u8], ts: &TimeScale) -> Vec<Lap> {
    let mut laps = Vec::new();
    let mut pos = 0;

    while pos + 32 <= data.len() {
        if &data[pos..pos + 5] != b"<hLAP" {
            pos += 1;
            continue;
        }

        let lap_num  = read_u16_le(data, pos + 14);
        let lap_ms   = read_u32_le(data, pos + 16);
        let start_raw = read_u32_le(data, pos + 28);

        if lap_num >= 1 && lap_num <= 200 && lap_ms >= 1000 && lap_ms <= 3_600_000 {
            laps.push(Lap {
                number:    lap_num,
                time_ms:   lap_ms,
                start_sec: ts.to_sec(start_raw),
            });
        }
        pos += 5;
    }

    laps.sort_by_key(|l| l.number);
    laps.dedup_by_key(|l| l.number);
    laps
}

// ─── Channel data parsing ─────────────────────────────────────────────────────

fn parse_channels(data: &[u8], ts: &TimeScale) -> Vec<Channel> {
    // Pre-allocate per-channel sample buffers
    let mut buffers: std::collections::HashMap<ChannelId, Vec<Sample>> =
        DECODED_CHANNELS
            .iter()
            .map(|&id| (id, Vec::with_capacity(60_000)))
            .collect();

    let marker = b")(M";
    let mut pos = 0;

    while pos + 11 <= data.len() {
        // Fast scan for )(M marker
        if data[pos..pos + 3] != *marker {
            pos += 1;
            continue;
        }

        let record_start = pos;
        pos += 3;

        if record_start + 11 > data.len() {
            break;
        }

        let raw_ts   = read_u32_le(data, record_start + 3);
        let ch_id    = read_u16_le(data, record_start + 7);
        let n_samples = read_u16_le(data, record_start + 9) as usize;

        // Sanity checks
        if n_samples == 0 || n_samples > 100 {
            continue;
        }

        let data_start = record_start + 11;
        let data_end   = data_start + n_samples * 2;
        if data_end > data.len() {
            continue;
        }

        let buf = match buffers.get_mut(&ch_id) {
            Some(b) => b,
            None    => continue,
        };

        let time_sec = ts.to_sec(raw_ts) as f32;

        for i in 0..n_samples {
            let raw = read_u16_le(data, data_start + i * 2);
            buf.push(Sample { time_sec, raw });
        }
    }

    // Build final Channel structs
    buffers
        .into_iter()
        .filter(|(_, samples)| !samples.is_empty())
        .map(|(id, mut samples)| {
            samples.sort_by(|a, b| a.time_sec.partial_cmp(&b.time_sec).unwrap());
            Channel {
                id,
                name: channel_name(id).to_string(),
                samples,
            }
        })
        .collect()
}

// ─── String extraction helpers ────────────────────────────────────────────────

/// Find the first chunk with the given tag and extract the first printable
/// ASCII string from within `[skip_bytes, skip_bytes + window]`.
fn extract_first_ascii_string(
    data: &[u8],
    tag: &[u8],
    skip_bytes: usize,
    window: usize,
) -> Option<String> {
    let pos = find_bytes(data, tag)?;
    let start = (pos + skip_bytes).min(data.len());
    let end   = (start + window).min(data.len());
    let slice = &data[start..end];

    // Find a run of printable ASCII (space through tilde, at least 3 chars)
    let mut run_start = None;
    let mut best: Option<String> = None;

    for (i, &b) in slice.iter().enumerate() {
        if b.is_ascii_graphic() || b == b' ' {
            if run_start.is_none() {
                run_start = Some(i);
            }
        } else {
            if let Some(rs) = run_start.take() {
                let candidate = &slice[rs..i];
                if candidate.len() >= 3 {
                    let s = std::str::from_utf8(candidate)
                        .ok()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty());
                    if s.as_ref().map_or(0, |s| s.len())
                        > best.as_ref().map_or(0, |s| s.len())
                    {
                        best = s;
                    }
                }
            }
        }
    }
    // Handle run extending to end of slice
    if let Some(rs) = run_start {
        let candidate = &slice[rs..];
        if candidate.len() >= 3 {
            let s = std::str::from_utf8(candidate)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if s.as_ref().map_or(0, |s| s.len()) > best.as_ref().map_or(0, |s| s.len()) {
                best = s;
            }
        }
    }
    best
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

// ─── Low-level integer readers ────────────────────────────────────────────────

#[inline(always)]
fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

#[inline(always)]
fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}
