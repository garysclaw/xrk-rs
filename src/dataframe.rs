//! Polars DataFrame output for XRK session data.
//!
//! Enabled with `--features dataframe`.
//!
//! # Example
//!
//! ```no_run
//! use xrk::XrkFile;
//! use xrk::dataframe::{SessionDataFrames, channel_df};
//!
//! let session = XrkFile::open("session.xrk").unwrap();
//! let dfs = SessionDataFrames::from_session(&session);
//!
//! // Lap times as a Polars DataFrame
//! println!("{}", dfs.laps);
//!
//! // Per-channel time-series DataFrame (time_sec, raw, voltage)
//! if let Some(df) = dfs.channel("LF_Shock") {
//!     println!("{}", df);
//! }
//!
//! // Per-lap statistics across all channels
//! let stats = dfs.lap_stats(&session);
//! println!("{}", stats);
//!
//! // Save to Parquet (load in Python with polars or pandas)
//! dfs.save_parquet("output/").unwrap();
//! ```

use polars::prelude::*;
use crate::types::{Channel, XrkFile};
use std::collections::HashMap;
use std::path::Path;

/// Collection of DataFrames extracted from an XRK session.
pub struct SessionDataFrames {
    /// Lap timing data: lap_number (u32), time_ms (u32), time_str (str), start_sec (f64)
    pub laps: DataFrame,
    /// Per-channel time-series DataFrames, keyed by channel name.
    /// Each DataFrame has columns: `time_sec` (f32), `raw` (u32), `voltage` (f32)
    pub channels: HashMap<String, DataFrame>,
}

impl SessionDataFrames {
    /// Build all DataFrames from a parsed XrkFile.
    pub fn from_session(session: &XrkFile) -> Self {
        let laps = build_laps_df(session);
        let channels = session.channels.iter()
            .map(|ch| (ch.name.clone(), channel_df(ch)))
            .collect();
        SessionDataFrames { laps, channels }
    }

    /// Look up the time-series DataFrame for a specific channel name.
    pub fn channel(&self, name: &str) -> Option<&DataFrame> {
        self.channels.get(name)
    }

    /// Build a per-lap statistics DataFrame across all channels.
    ///
    /// Columns: `lap_number`, `time_ms`, then for each channel:
    /// `{name}_n`, `{name}_mean`, `{name}_std`, `{name}_min`, `{name}_max`
    pub fn lap_stats(&self, session: &XrkFile) -> DataFrame {
        build_lap_stats_df(session)
    }

    /// Save all DataFrames to Parquet files in `output_dir`.
    ///
    /// Creates one file per channel plus `laps.parquet`.
    pub fn save_parquet(&self, output_dir: impl AsRef<Path>) -> PolarsResult<()> {
        let dir = output_dir.as_ref();
        std::fs::create_dir_all(dir).ok();
        write_parquet(&self.laps, dir.join("laps.parquet"))?;
        for (name, df) in &self.channels {
            let safe = name.replace(['/', '\\', ' '], "_");
            write_parquet(df, dir.join(format!("channel_{safe}.parquet")))?;
        }
        Ok(())
    }

    /// Save all DataFrames to CSV files in `output_dir`.
    pub fn save_csv(&self, output_dir: impl AsRef<Path>) -> PolarsResult<()> {
        let dir = output_dir.as_ref();
        std::fs::create_dir_all(dir).ok();
        write_csv(&mut self.laps.clone(), dir.join("laps.csv"))?;
        for (name, df) in &self.channels {
            let safe = name.replace(['/', '\\', ' '], "_");
            write_csv(&mut df.clone(), dir.join(format!("channel_{safe}.csv")))?;
        }
        Ok(())
    }
}

// ─── DataFrame builders ───────────────────────────────────────────────────────

fn build_laps_df(session: &XrkFile) -> DataFrame {
    let numbers:   Vec<u32>    = session.laps.iter().map(|l| l.number as u32).collect();
    let times_ms:  Vec<u32>    = session.laps.iter().map(|l| l.time_ms).collect();
    let time_strs: Vec<String> = session.laps.iter().map(|l| l.time_str()).collect();
    let starts:    Vec<f64>    = session.laps.iter().map(|l| l.start_sec).collect();

    df! {
        "lap_number" => numbers,
        "time_ms"    => times_ms,
        "time_str"   => time_strs,
        "start_sec"  => starts,
    }
    .expect("laps DataFrame build failed")
}

/// Build a time-series DataFrame for a single channel.
///
/// Columns: `time_sec` (f32), `raw` (u32 — ADC counts 0–65535),
/// `voltage` (f32 — converted to 0.0–5.0 V)
pub fn channel_df(channel: &Channel) -> DataFrame {
    let times:    Vec<f32> = channel.samples.iter().map(|s| s.time_sec).collect();
    let raws:     Vec<u32> = channel.samples.iter().map(|s| s.raw as u32).collect();
    let voltages: Vec<f32> = channel.samples.iter()
        .map(|s| s.raw as f32 / 65535.0 * 5.0)
        .collect();

    df! {
        "time_sec" => times,
        "raw"      => raws,
        "voltage"  => voltages,
    }
    .expect("channel DataFrame build failed")
}

fn build_lap_stats_df(session: &XrkFile) -> DataFrame {
    let lap_nums: Vec<u32> = session.laps.iter().map(|l| l.number as u32).collect();
    let times_ms: Vec<u32> = session.laps.iter().map(|l| l.time_ms).collect();

    let mut cols: Vec<Column> = vec![
        Series::new("lap_number".into(), lap_nums).into(),
        Series::new("time_ms".into(), times_ms).into(),
    ];

    for ch in &session.channels {
        let stats = ch.per_lap_stats(&session.laps);
        let ns:    Vec<u32> = stats.iter().map(|s| s.n_samples as u32).collect();
        let means: Vec<f64> = stats.iter().map(|s| s.mean).collect();
        let stds:  Vec<f64> = stats.iter().map(|s| s.std).collect();
        let mins:  Vec<u32> = stats.iter().map(|s| s.min as u32).collect();
        let maxs:  Vec<u32> = stats.iter().map(|s| s.max as u32).collect();

        let n = &ch.name;
        cols.push(Series::new(format!("{n}_n").into(),    ns).into());
        cols.push(Series::new(format!("{n}_mean").into(), means).into());
        cols.push(Series::new(format!("{n}_std").into(),  stds).into());
        cols.push(Series::new(format!("{n}_min").into(),  mins).into());
        cols.push(Series::new(format!("{n}_max").into(),  maxs).into());
    }

    DataFrame::new(cols).expect("lap_stats DataFrame build failed")
}

// ─── File I/O helpers ─────────────────────────────────────────────────────────

fn write_parquet(df: &DataFrame, path: impl AsRef<Path>) -> PolarsResult<()> {
    let mut file = std::fs::File::create(path)?;
    ParquetWriter::new(&mut file).finish(&mut df.clone())?;
    Ok(())
}

fn write_csv(df: &mut DataFrame, path: impl AsRef<Path>) -> PolarsResult<()> {
    let mut file = std::fs::File::create(path)?;
    CsvWriter::new(&mut file).finish(df)?;
    Ok(())
}
