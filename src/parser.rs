//! Binary parser for the AiM XRK file format.
//!
//! ## Format summary
//!
//! XRK files contain tagged chunks interspersed with data stream markers:
//!
//! ```text
//! <hTAG[5]   ...data...                 — tagged chunk
//! )(M ts[4] ch_id[2] n[2] v[n*2]       — measurement samples
//! )(S ts[4] ch_id[2] flags[2]           — section separator
//! ```
//!
//! All integers are little-endian. Timestamps are raw uint32 counters;
//! the time scale is derived from the session's first/last timestamp span.

use crate::error::XrkError;
use crate::types::{Channel, ChannelId, Lap, Sample, SessionInfo, XrkFile};
use std::collections::HashMap;

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn parse(data: &[u8]) -> Result<XrkFile, XrkError> {
    if data.len() < 64 {
        return Err(XrkError::FileTooSmall(data.len()));
    }

    // Pass 1: collect all )(M timestamps for time-scale calibration
    let (first_ts, last_ts) = first_last_timestamps(data)
        .ok_or(XrkError::NoDataMarkers)?;

    let ts_range = last_ts.wrapping_sub(first_ts) as f64;

    // Pass 2: derive session duration from last lap record
    let duration_sec = derive_duration(data, first_ts, ts_range);
    let time_scale = if ts_range > 0.0 { duration_sec / ts_range } else { 0.003068 };

    // Pass 3: parse everything
    let info     = parse_info(data, duration_sec);
    let channels = parse_channel_defs(data); // names/IDs from <hCHS chunks
    let laps     = parse_laps(data, first_ts, time_scale);
    let channels = populate_samples(data, first_ts, time_scale, channels);

    Ok(XrkFile { info, laps, channels })
}

// ─── Timestamp utilities ──────────────────────────────────────────────────────

fn first_last_timestamps(data: &[u8]) -> Option<(u32, u32)> {
    let mut first = None;
    let mut last  = None;
    let mut pos   = 0;

    while pos + 7 <= data.len() {
        if data[pos..pos + 3] == *b")(M" {
            let ts = u32_le(data, pos + 3);
            if first.is_none() { first = Some(ts); }
            last = Some(ts);
            pos += 3;
        } else {
            pos += 1;
        }
    }
    Some((first?, last?))
}

/// Estimate session duration from the last lap's end time.
fn derive_duration(data: &[u8], first_ts: u32, ts_range: f64) -> f64 {
    if ts_range <= 0.0 { return 0.0; }

    // Bootstrap with a rough scale, then find the last lap end
    let rough = ts_range * 0.003068; // ~3ms/unit empirical starting point
    let bootstrap_scale = rough / ts_range;

    let mut latest_end = 0.0_f64;
    let mut pos = 0;

    while pos + 32 <= data.len() {
        if &data[pos..pos + 5] == b"<hLAP" {
            let lap_num = u16_le(data, pos + 14);
            let lap_ms  = u32_le(data, pos + 16);
            let start_ts = u32_le(data, pos + 28);

            if (1..=500).contains(&lap_num) && (500..=3_600_000).contains(&lap_ms) {
                let start = start_ts.wrapping_sub(first_ts) as f64 * bootstrap_scale;
                let end   = start + lap_ms as f64 / 1000.0;
                if end > latest_end { latest_end = end; }
            }
        }
        pos += 1;
    }

    if latest_end > 10.0 { latest_end } else { rough }
}

// ─── Session info ─────────────────────────────────────────────────────────────

fn parse_info(data: &[u8], duration_sec: f64) -> SessionInfo {
    SessionInfo {
        date:         extract_string(data, b"<hTMD"),
        time:         extract_string(data, b"<hTMT"),
        track:        extract_string(data, b"<hTRK"),
        vehicle:      extract_string(data, b"<hVEH"),
        logger:       extract_string(data, b"<hHWN"),
        duration_sec,
        file_size:    data.len(),
    }
}

// ─── Channel definitions from <hCHS chunks ────────────────────────────────────

/// Parse channel definitions in file order.
/// Each <hCHS chunk defines one channel: short name + long name.
/// The channel's numeric ID is its 0-based position in the CHS sequence.
fn parse_channel_defs(data: &[u8]) -> Vec<Channel> {
    let mut channels = Vec::new();
    let mut pos = 0;
    let mut id: ChannelId = 0;

    while pos + 8 <= data.len() {
        if &data[pos..pos + 5] != b"<hCHS" {
            pos += 1;
            continue;
        }

        // Extract printable ASCII strings from the next 100 bytes
        let window = &data[(pos + 8).min(data.len())..(pos + 108).min(data.len())];
        let strings = extract_ascii_strings(window, 2);

        let short_name = strings.first().cloned().unwrap_or_default();
        let long_name  = strings.into_iter().nth(1).unwrap_or_else(|| short_name.clone());

        channels.push(Channel {
            id,
            short_name,
            name: long_name,
            samples: Vec::new(),
        });

        id += 1;
        pos += 5;
    }

    channels
}

// ─── Sample population ────────────────────────────────────────────────────────

/// Read all )(M markers and distribute samples into the correct channels.
fn populate_samples(
    data: &[u8],
    first_ts: u32,
    time_scale: f64,
    mut channels: Vec<Channel>,
) -> Vec<Channel> {
    // Build a lookup from channel_id → index in `channels`
    let mut id_to_idx: HashMap<ChannelId, usize> = channels
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id, i))
        .collect();

    let mut pos = 0;

    while pos + 11 <= data.len() {
        if data[pos..pos + 3] != *b")(M" {
            pos += 1;
            continue;
        }

        let raw_ts    = u32_le(data, pos + 3);
        let ch_id     = u16_le(data, pos + 7);
        let n_samples = u16_le(data, pos + 9) as usize;

        let data_start = pos + 11;
        let data_end   = data_start + n_samples * 2;

        if n_samples == 0 || n_samples > 200 || data_end > data.len() {
            pos += 1;
            continue;
        }

        let time_sec = (raw_ts.wrapping_sub(first_ts) as f64 * time_scale) as f32;

        // If we haven't seen this channel ID before, create an entry for it
        let idx = *id_to_idx.entry(ch_id).or_insert_with(|| {
            let idx = channels.len();
            channels.push(Channel {
                id: ch_id,
                name: format!("Channel_{}", ch_id),
                short_name: format!("Ch{:02}", ch_id),
                samples: Vec::new(),
            });
            idx
        });

        for i in 0..n_samples {
            let raw = u16_le(data, data_start + i * 2);
            channels[idx].samples.push(Sample { time_sec, raw });
        }

        pos += 3;
    }

    // Sort each channel's samples by time (markers can arrive slightly out of order)
    for ch in &mut channels {
        ch.samples.sort_unstable_by(|a, b| a.time_sec.partial_cmp(&b.time_sec).unwrap());
    }

    // Drop channels with zero samples (defined in CHS but never transmitted)
    channels.retain(|c| !c.samples.is_empty());
    channels
}

// ─── Lap parsing ─────────────────────────────────────────────────────────────

fn parse_laps(data: &[u8], first_ts: u32, time_scale: f64) -> Vec<Lap> {
    let mut laps = Vec::new();
    let mut pos  = 0;

    while pos + 32 <= data.len() {
        if &data[pos..pos + 5] != b"<hLAP" {
            pos += 1;
            continue;
        }

        let lap_num  = u16_le(data, pos + 14);
        let lap_ms   = u32_le(data, pos + 16);
        let start_ts = u32_le(data, pos + 28);

        if (1..=500).contains(&lap_num) && (500..=3_600_000).contains(&lap_ms) {
            let start_sec = start_ts.wrapping_sub(first_ts) as f64 * time_scale;
            laps.push(Lap { number: lap_num, time_ms: lap_ms, start_sec });
        }
        pos += 5;
    }

    laps.sort_by_key(|l| l.number);
    laps.dedup_by_key(|l| l.number);
    laps
}

// ─── String extraction helpers ────────────────────────────────────────────────

/// Find the first occurrence of `tag` and extract the longest printable ASCII
/// string from the following 80 bytes.
fn extract_string(data: &[u8], tag: &[u8]) -> String {
    let Some(pos) = find(data, tag) else { return String::new() };
    let window = &data[(pos + 8).min(data.len())..(pos + 80).min(data.len())];
    extract_ascii_strings(window, 3)
        .into_iter()
        .max_by_key(|s| s.len())
        .unwrap_or_default()
}

/// Extract all runs of printable ASCII of length ≥ `min_len` from a byte slice.
fn extract_ascii_strings(data: &[u8], min_len: usize) -> Vec<String> {
    let mut results = Vec::new();
    let mut start: Option<usize> = None;

    for (i, &b) in data.iter().enumerate() {
        if b.is_ascii_graphic() || b == b' ' {
            if start.is_none() { start = Some(i); }
        } else if let Some(s) = start.take() {
            let candidate = &data[s..i];
            if candidate.len() >= min_len {
                if let Ok(text) = std::str::from_utf8(candidate) {
                    let text = text.trim().to_string();
                    if !text.is_empty() { results.push(text); }
                }
            }
        }
    }
    if let Some(s) = start {
        let candidate = &data[s..];
        if candidate.len() >= min_len {
            if let Ok(text) = std::str::from_utf8(candidate) {
                let text = text.trim().to_string();
                if !text.is_empty() { results.push(text); }
            }
        }
    }
    results
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ─── Integer readers ─────────────────────────────────────────────────────────

#[inline(always)] fn u16_le(d: &[u8], o: usize) -> u16 { u16::from_le_bytes([d[o], d[o+1]]) }
#[inline(always)] fn u32_le(d: &[u8], o: usize) -> u32 { u32::from_le_bytes([d[o], d[o+1], d[o+2], d[o+3]]) }
