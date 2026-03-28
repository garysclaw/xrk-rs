use std::path::Path;
use crate::{parser, XrkError};

/// Channel identifier (0-based index matching AiM CHS chunk order).
pub type ChannelId = u16;

// ─── Known channel IDs (from CHS chunk order in AiM Quattro/Solo2 loggers) ────

pub const CH_MASTER_CLK:   ChannelId = 0;
pub const CH_LAP_TIME:     ChannelId = 1;
pub const CH_PREDICTIVE:   ChannelId = 2;
pub const CH_LOGGER_TEMP:  ChannelId = 9;
pub const CH_VBAT:         ChannelId = 10;
pub const CH_ODOMETER:     ChannelId = 12;
pub const CH_RPM:          ChannelId = 18;
pub const CH_LF_SHOCK:     ChannelId = 19;
pub const CH_RF_SHOCK:     ChannelId = 20;
pub const CH_LR_SHOCK:     ChannelId = 21;
pub const CH_RR_SHOCK:     ChannelId = 22;
pub const CH_INLINE_ACC:   ChannelId = 23;
pub const CH_LATERAL_ACC:  ChannelId = 24;
pub const CH_VERTICAL_ACC: ChannelId = 25;
pub const CH_ROLL_RATE:    ChannelId = 26;
pub const CH_PITCH_RATE:   ChannelId = 27;
pub const CH_YAW_RATE:     ChannelId = 28;
pub const CH_GPS_LAT_ACC:  ChannelId = 29;
pub const CH_GPS_INL_ACC:  ChannelId = 30;
pub const CH_GPS_YAW_RATE: ChannelId = 31;
pub const CH_LUMINOSITY:   ChannelId = 35;

/// Return the human-readable name for a known channel ID.
pub fn channel_name(id: ChannelId) -> &'static str {
    match id {
        CH_MASTER_CLK   => "MasterClock",
        CH_LAP_TIME     => "LapTime",
        CH_PREDICTIVE   => "PredictiveTime",
        CH_LOGGER_TEMP  => "LoggerTemp",
        CH_VBAT         => "ExternalVoltage",
        CH_ODOMETER     => "TotalOdometer",
        CH_RPM          => "RPM",
        CH_LF_SHOCK     => "LF_Shock",
        CH_RF_SHOCK     => "RF_Shock",
        CH_LR_SHOCK     => "LR_Shock",
        CH_RR_SHOCK     => "RR_Shock",
        CH_INLINE_ACC   => "InlineAcc",
        CH_LATERAL_ACC  => "LateralAcc",
        CH_VERTICAL_ACC => "VerticalAcc",
        CH_ROLL_RATE    => "RollRate",
        CH_PITCH_RATE   => "PitchRate",
        CH_YAW_RATE     => "YawRate",
        CH_GPS_LAT_ACC  => "GPS_LateralAcc",
        CH_GPS_INL_ACC  => "GPS_InlineAcc",
        CH_GPS_YAW_RATE => "GPS_YawRate",
        CH_LUMINOSITY   => "Luminosity",
        _               => "Unknown",
    }
}

// ─── Session metadata ─────────────────────────────────────────────────────────

/// Top-level session metadata extracted from XRK header chunks.
#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    /// Recording date string as stored in the file (e.g. "01/19/2026")
    pub date: String,
    /// Recording time string (e.g. "13:56:02")
    pub time: String,
    /// Track / venue name
    pub track: String,
    /// Vehicle / car identifier
    pub vehicle: String,
    /// Logger serial / ID string
    pub logger_id: String,
    /// Approximate session duration in seconds (derived from timestamp range)
    pub duration_sec: f64,
    /// Total number of channel definitions found
    pub channel_count: usize,
    /// Total file size in bytes
    pub file_size: usize,
}

// ─── Lap record ───────────────────────────────────────────────────────────────

/// A single timed lap.
#[derive(Debug, Clone)]
pub struct Lap {
    /// 1-based lap number
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

    /// Lap duration in seconds (floating point).
    pub fn time_sec(&self) -> f64 {
        self.time_ms as f64 / 1000.0
    }

    /// Session-relative end time in seconds.
    pub fn end_sec(&self) -> f64 {
        self.start_sec + self.time_sec()
    }
}

// ─── Channel sample ───────────────────────────────────────────────────────────

/// A single time-stamped measurement sample.
#[derive(Debug, Clone, Copy)]
pub struct Sample {
    /// Session-relative time in seconds
    pub time_sec: f32,
    /// Raw ADC value (0–65535 represents 0–5 V)
    pub raw: u16,
}

impl Sample {
    /// Convert raw ADC count to voltage (0–65535 → 0.0–5.0 V).
    #[inline]
    pub fn voltage(&self) -> f32 {
        self.raw as f32 / 65535.0 * 5.0
    }

    /// Apply a linear calibration: `physical = gain * voltage + offset`.
    ///
    /// Use [`Calibration::apply`] for convenience.
    #[inline]
    pub fn calibrate(&self, gain: f32, offset: f32) -> f32 {
        self.voltage() * gain + offset
    }
}

// ─── Calibration ─────────────────────────────────────────────────────────────

/// Linear voltage-to-physical-unit calibration for an analog channel.
///
/// `physical_value = gain * voltage + offset`
///
/// To derive `gain` and `offset` from a 2-point calibration:
/// ```text
/// gain   = (phys_high - phys_low) / (v_high - v_low)
/// offset = phys_low - gain * v_low
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Calibration {
    /// Scale factor (physical unit per volt)
    pub gain: f32,
    /// Zero offset in physical units
    pub offset: f32,
    /// Physical unit label (e.g. "mm", "G", "deg/s")
    pub unit: &'static str,
}

impl Calibration {
    /// Apply calibration to a raw sample value, returning the physical value.
    pub fn apply(&self, sample: &Sample) -> f32 {
        sample.calibrate(self.gain, self.offset)
    }

    /// Standard ±2G accelerometer calibration (0G = 2.5V, 1G = 1.185V).
    ///
    /// Validated against VerticalAcc channel (mean = 1.000G when stationary).
    pub const ACCEL_2G: Calibration = Calibration {
        gain: 1.0 / 1.185,
        offset: -2.5 / 1.185,
        unit: "G",
    };
}

// ─── Channel ─────────────────────────────────────────────────────────────────

/// A named data channel with its full time-series sample array.
#[derive(Debug, Clone)]
pub struct Channel {
    /// Channel identifier (0-based index)
    pub id: ChannelId,
    /// Human-readable channel name
    pub name: String,
    /// All samples recorded for this channel, in time order
    pub samples: Vec<Sample>,
}

impl Channel {
    /// Minimum raw ADC value across all samples.
    pub fn min_raw(&self) -> Option<u16> {
        self.samples.iter().map(|s| s.raw).min()
    }

    /// Maximum raw ADC value across all samples.
    pub fn max_raw(&self) -> Option<u16> {
        self.samples.iter().map(|s| s.raw).max()
    }

    /// Mean raw ADC value.
    pub fn mean_raw(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let sum: f64 = self.samples.iter().map(|s| s.raw as f64).sum();
        Some(sum / self.samples.len() as f64)
    }

    /// Mean voltage (0–5V).
    pub fn mean_voltage(&self) -> Option<f32> {
        self.mean_raw().map(|v| (v / 65535.0 * 5.0) as f32)
    }

    /// Samples falling within a time window [start_sec, end_sec].
    pub fn samples_in_range(&self, start_sec: f64, end_sec: f64) -> &[Sample] {
        let start = self
            .samples
            .partition_point(|s| (s.time_sec as f64) < start_sec);
        let end = self
            .samples
            .partition_point(|s| (s.time_sec as f64) <= end_sec);
        &self.samples[start..end]
    }

    /// Per-lap statistics: (lap_number, mean_raw, std_raw, min_raw, max_raw, n_samples)
    pub fn per_lap_stats(&self, laps: &[Lap]) -> Vec<LapStats> {
        laps.iter()
            .map(|lap| {
                let slice = self.samples_in_range(lap.start_sec, lap.end_sec());
                let n = slice.len();
                if n == 0 {
                    return LapStats {
                        lap_number: lap.number,
                        lap_time_ms: lap.time_ms,
                        n_samples: 0,
                        mean_raw: 0.0,
                        std_raw: 0.0,
                        min_raw: 0,
                        max_raw: 0,
                    };
                }
                let mean = slice.iter().map(|s| s.raw as f64).sum::<f64>() / n as f64;
                let variance = slice
                    .iter()
                    .map(|s| {
                        let d = s.raw as f64 - mean;
                        d * d
                    })
                    .sum::<f64>()
                    / n as f64;
                LapStats {
                    lap_number: lap.number,
                    lap_time_ms: lap.time_ms,
                    n_samples: n,
                    mean_raw: mean,
                    std_raw: variance.sqrt(),
                    min_raw: slice.iter().map(|s| s.raw).min().unwrap_or(0),
                    max_raw: slice.iter().map(|s| s.raw).max().unwrap_or(0),
                }
            })
            .collect()
    }
}

/// Per-lap statistics for a single channel.
#[derive(Debug, Clone)]
pub struct LapStats {
    pub lap_number: u16,
    pub lap_time_ms: u32,
    pub n_samples: usize,
    pub mean_raw: f64,
    pub std_raw: f64,
    pub min_raw: u16,
    pub max_raw: u16,
}

impl LapStats {
    pub fn mean_voltage(&self) -> f32 {
        (self.mean_raw / 65535.0 * 5.0) as f32
    }
}

// ─── XrkFile — top-level entry point ─────────────────────────────────────────

/// A fully-parsed AiM XRK session file.
///
/// Created via [`XrkFile::open`] or [`XrkFile::from_bytes`].
#[derive(Debug)]
pub struct XrkFile {
    /// Session metadata (date, time, track, vehicle)
    pub info: SessionInfo,
    /// All timed lap records, sorted by lap number
    pub laps: Vec<Lap>,
    /// All decoded channels, keyed by channel ID
    pub channels: Vec<Channel>,
}

impl XrkFile {
    /// Open and parse an XRK file from disk.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, XrkError> {
        let bytes = std::fs::read(path)?;
        Self::from_bytes(&bytes)
    }

    /// Parse an XRK file from a byte slice (useful for in-memory or mmap).
    pub fn from_bytes(data: &[u8]) -> Result<Self, XrkError> {
        parser::parse(data)
    }

    /// Look up a channel by its exact name (case-sensitive).
    pub fn channel_by_name(&self, name: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.name == name)
    }

    /// Look up a channel by its numeric ID.
    pub fn channel_by_id(&self, id: ChannelId) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    /// All four shock channels, in order: LF, RF, LR, RR.
    /// Returns `None` for any corner not present in the file.
    pub fn shock_channels(&self) -> [Option<&Channel>; 4] {
        [
            self.channel_by_id(CH_LF_SHOCK),
            self.channel_by_id(CH_RF_SHOCK),
            self.channel_by_id(CH_LR_SHOCK),
            self.channel_by_id(CH_RR_SHOCK),
        ]
    }

    /// Accelerometer channels: Inline, Lateral, Vertical.
    pub fn accel_channels(&self) -> [Option<&Channel>; 3] {
        [
            self.channel_by_id(CH_INLINE_ACC),
            self.channel_by_id(CH_LATERAL_ACC),
            self.channel_by_id(CH_VERTICAL_ACC),
        ]
    }

    /// Best (fastest) lap, ignoring laps shorter than `min_ms` milliseconds.
    pub fn best_lap(&self, min_ms: u32) -> Option<&Lap> {
        self.laps
            .iter()
            .filter(|l| l.time_ms >= min_ms)
            .min_by_key(|l| l.time_ms)
    }

    /// Effective sample rate for a channel in Hz (samples / session duration).
    pub fn sample_rate_hz(&self, channel_id: ChannelId) -> Option<f64> {
        let ch = self.channel_by_id(channel_id)?;
        if ch.samples.is_empty() || self.info.duration_sec == 0.0 {
            return None;
        }
        Some(ch.samples.len() as f64 / self.info.duration_sec)
    }
}
