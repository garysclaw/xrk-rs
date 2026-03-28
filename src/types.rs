use std::path::Path;
use crate::{parser, XrkError};

/// Channel identifier — the numeric ID as stored in the XRK file.
pub type ChannelId = u16;

// ─── Session metadata ─────────────────────────────────────────────────────────

/// Top-level session metadata extracted from XRK header chunks.
#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    /// Recording date as stored in the file (e.g. "01/19/2026")
    pub date: String,
    /// Recording time as stored in the file (e.g. "13:56:02")
    pub time: String,
    /// Track / venue name
    pub track: String,
    /// Vehicle / car identifier
    pub vehicle: String,
    /// Logger device name or serial number
    pub logger: String,
    /// Approximate session duration in seconds (derived from timestamp span)
    pub duration_sec: f64,
    /// Total file size in bytes
    pub file_size: usize,
}

// ─── Lap record ───────────────────────────────────────────────────────────────

/// A single timed lap.
#[derive(Debug, Clone)]
pub struct Lap {
    /// 1-based lap number as recorded by the logger
    pub number: u16,
    /// Lap duration in milliseconds
    pub time_ms: u32,
    /// Session-relative start time in seconds
    pub start_sec: f64,
}

impl Lap {
    /// Format lap time as `M:SS.mmm` (e.g. `"0:18.696"`).
    pub fn time_str(&self) -> String {
        let m = self.time_ms / 60_000;
        let s = (self.time_ms % 60_000) as f64 / 1000.0;
        format!("{}:{:06.3f}", m, s)
    }

    /// Lap duration as floating-point seconds.
    #[inline]
    pub fn time_sec(&self) -> f64 {
        self.time_ms as f64 / 1000.0
    }

    /// Session-relative end time in seconds.
    #[inline]
    pub fn end_sec(&self) -> f64 {
        self.start_sec + self.time_sec()
    }
}

// ─── Sample ───────────────────────────────────────────────────────────────────

/// A single time-stamped measurement.
///
/// Raw values are **uint16 ADC counts** (0–65535).
/// For AiM loggers with a 0–5 V input range this corresponds to 0.0–5.0 V,
/// but the exact mapping depends on the logger hardware and channel config.
///
/// No calibration is applied here — that is the responsibility of the
/// application using this library.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    /// Session-relative time in seconds
    pub time_sec: f32,
    /// Raw ADC value (0–65535)
    pub raw: u16,
}

// ─── Per-lap channel statistics ───────────────────────────────────────────────

/// Descriptive statistics for one channel over one lap.
#[derive(Debug, Clone)]
pub struct LapStats {
    pub lap_number: u16,
    pub lap_time_ms: u32,
    pub n_samples: usize,
    pub mean: f64,
    pub std: f64,
    pub min: u16,
    pub max: u16,
}

// ─── Channel ─────────────────────────────────────────────────────────────────

/// A named data channel with its complete time-series sample array.
///
/// Channel names and IDs are read directly from the XRK file — they reflect
/// whatever the user configured in AiM Race Studio 3.
#[derive(Debug, Clone)]
pub struct Channel {
    /// Numeric channel ID (0-based, as stored in the file)
    pub id: ChannelId,
    /// Human-readable name from the file (user-defined in Race Studio)
    pub name: String,
    /// Short 4-character code from the file (e.g. "Ch01", "InlA")
    pub short_name: String,
    /// All samples, sorted by time
    pub samples: Vec<Sample>,
}

impl Channel {
    /// Minimum raw ADC value across all samples.
    pub fn min(&self) -> Option<u16> {
        self.samples.iter().map(|s| s.raw).min()
    }

    /// Maximum raw ADC value across all samples.
    pub fn max(&self) -> Option<u16> {
        self.samples.iter().map(|s| s.raw).max()
    }

    /// Mean raw ADC value.
    pub fn mean(&self) -> Option<f64> {
        if self.samples.is_empty() { return None; }
        let sum: f64 = self.samples.iter().map(|s| s.raw as f64).sum();
        Some(sum / self.samples.len() as f64)
    }

    /// Effective sample rate in Hz given the total session duration.
    pub fn sample_rate_hz(&self, duration_sec: f64) -> f64 {
        if duration_sec <= 0.0 || self.samples.is_empty() { return 0.0; }
        self.samples.len() as f64 / duration_sec
    }

    /// Samples within a session-relative time window [start_sec, end_sec].
    pub fn samples_in_range(&self, start_sec: f64, end_sec: f64) -> &[Sample] {
        let lo = self.samples.partition_point(|s| (s.time_sec as f64) < start_sec);
        let hi = self.samples.partition_point(|s| (s.time_sec as f64) <= end_sec);
        &self.samples[lo..hi]
    }

    /// Compute descriptive statistics for each lap.
    pub fn per_lap_stats(&self, laps: &[Lap]) -> Vec<LapStats> {
        laps.iter().map(|lap| {
            let slice = self.samples_in_range(lap.start_sec, lap.end_sec());
            let n = slice.len();
            if n == 0 {
                return LapStats {
                    lap_number: lap.number,
                    lap_time_ms: lap.time_ms,
                    n_samples: 0,
                    mean: 0.0, std: 0.0, min: 0, max: 0,
                };
            }
            let mean = slice.iter().map(|s| s.raw as f64).sum::<f64>() / n as f64;
            let variance = slice.iter()
                .map(|s| { let d = s.raw as f64 - mean; d * d })
                .sum::<f64>() / n as f64;
            LapStats {
                lap_number: lap.number,
                lap_time_ms: lap.time_ms,
                n_samples: n,
                mean,
                std: variance.sqrt(),
                min: slice.iter().map(|s| s.raw).min().unwrap_or(0),
                max: slice.iter().map(|s| s.raw).max().unwrap_or(0),
            }
        }).collect()
    }
}

// ─── XrkFile ──────────────────────────────────────────────────────────────────

/// A fully-parsed AiM XRK session.
///
/// All channel names, IDs, and sample data are read directly from the file.
/// No assumptions are made about what any channel represents.
#[derive(Debug)]
pub struct XrkFile {
    /// Session metadata (track, date, time, vehicle, logger)
    pub info: SessionInfo,
    /// All timed laps, sorted by lap number
    pub laps: Vec<Lap>,
    /// All channels found in the file, in the order they appear
    pub channels: Vec<Channel>,
}

impl XrkFile {
    /// Open and parse an XRK file from disk.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, XrkError> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    /// Parse an XRK file already loaded into memory.
    pub fn from_bytes(data: &[u8]) -> Result<Self, XrkError> {
        parser::parse(data)
    }

    /// Look up a channel by its name (case-sensitive, as stored in the file).
    pub fn channel(&self, name: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.name == name)
    }

    /// Look up a channel by its numeric ID.
    pub fn channel_by_id(&self, id: ChannelId) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    /// All channel names present in this file.
    pub fn channel_names(&self) -> Vec<&str> {
        self.channels.iter().map(|c| c.name.as_str()).collect()
    }

    /// The fastest lap with a duration of at least `min_ms` milliseconds.
    /// Pass `min_ms = 0` to include all laps.
    pub fn best_lap(&self, min_ms: u32) -> Option<&Lap> {
        self.laps.iter()
            .filter(|l| l.time_ms >= min_ms)
            .min_by_key(|l| l.time_ms)
    }
}
