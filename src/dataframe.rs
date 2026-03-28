//! Polars DataFrame output for XRK session data.
//!
//! Enabled with `--features dataframe`.
//!
//! # Example
//!
//! ```no_run
//! use xrk::XrkFile;
//! use xrk::dataframe::SessionDataFrames;
//!
//! let session = XrkFile::open("data/38_Mobile_In_a_0023.xrk").unwrap();
//! let dfs = SessionDataFrames::from_session(&session);
//!
//! // Lap times as a Polars DataFrame
//! println!("{}", dfs.laps);
//!
//! // All shock data as a single time-series DataFrame
//! println!("{}", dfs.shocks);
//!
//! // Save to Parquet for later analysis
//! dfs.save_parquet("output/").unwrap();
//! ```

use polars::prelude::*;
use crate::types::{XrkFile, CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK,
                   CH_INLINE_ACC, CH_LATERAL_ACC, CH_VERTICAL_ACC,
                   CH_ROLL_RATE, CH_PITCH_RATE, CH_YAW_RATE};
use std::path::Path;

/// Collection of DataFrames extracted from an XRK session.
pub struct SessionDataFrames {
    /// Lap timing data: lap_number, time_ms, time_str, start_sec
    pub laps: DataFrame,

    /// Shock potentiometer time-series: time_sec, LF_raw, RF_raw, LR_raw, RR_raw,
    ///                                   LF_voltage, RF_voltage, LR_voltage, RR_voltage
    pub shocks: DataFrame,

    /// Accelerometer time-series: time_sec, inline_raw, lateral_raw, vertical_raw,
    ///                             inline_g, lateral_g, vertical_g
    pub accel: DataFrame,

    /// Gyroscope time-series: time_sec, roll_raw, pitch_raw, yaw_raw
    pub gyro: DataFrame,

    /// Per-lap statistics for all shock channels:
    /// lap, time_ms, {corner}_mean_raw, {corner}_std_raw, {corner}_mean_v
    pub lap_shock_stats: DataFrame,
}

impl SessionDataFrames {
    /// Build all DataFrames from a parsed XrkFile.
    pub fn from_session(session: &XrkFile) -> Self {
        SessionDataFrames {
            laps:            build_laps_df(session),
            shocks:          build_shocks_df(session),
            accel:           build_accel_df(session),
            gyro:            build_gyro_df(session),
            lap_shock_stats: build_lap_shock_stats_df(session),
        }
    }

    /// Save all DataFrames to Parquet files in `output_dir`.
    pub fn save_parquet(&self, output_dir: impl AsRef<Path>) -> PolarsResult<()> {
        let dir = output_dir.as_ref();
        std::fs::create_dir_all(dir).ok();

        write_parquet(&self.laps,           dir.join("laps.parquet"))?;
        write_parquet(&self.shocks,         dir.join("shocks.parquet"))?;
        write_parquet(&self.accel,          dir.join("accel.parquet"))?;
        write_parquet(&self.gyro,           dir.join("gyro.parquet"))?;
        write_parquet(&self.lap_shock_stats, dir.join("lap_shock_stats.parquet"))?;
        Ok(())
    }

    /// Save all DataFrames to CSV files in `output_dir`.
    pub fn save_csv(&self, output_dir: impl AsRef<Path>) -> PolarsResult<()> {
        let dir = output_dir.as_ref();
        std::fs::create_dir_all(dir).ok();

        write_csv(&mut self.laps.clone(),           dir.join("laps.csv"))?;
        write_csv(&mut self.shocks.clone(),         dir.join("shocks.csv"))?;
        write_csv(&mut self.accel.clone(),          dir.join("accel.csv"))?;
        write_csv(&mut self.gyro.clone(),           dir.join("gyro.csv"))?;
        write_csv(&mut self.lap_shock_stats.clone(), dir.join("lap_shock_stats.csv"))?;
        Ok(())
    }
}

// ─── DataFrame builders ───────────────────────────────────────────────────────

fn build_laps_df(session: &XrkFile) -> DataFrame {
    let numbers: Vec<u16>  = session.laps.iter().map(|l| l.number).collect();
    let times_ms: Vec<u32> = session.laps.iter().map(|l| l.time_ms).collect();
    let time_strs: Vec<&str> = session.laps.iter().map(|l| l.time_str().as_str())
        .collect::<Vec<_>>(); // will fix borrowing below
    let starts: Vec<f64>   = session.laps.iter().map(|l| l.start_sec).collect();
    let time_strs: Vec<String> = session.laps.iter().map(|l| l.time_str()).collect();

    df! {
        "lap_number" => numbers,
        "time_ms"    => times_ms,
        "time_str"   => time_strs,
        "start_sec"  => starts,
    }
    .expect("failed to build laps DataFrame")
}

fn build_shocks_df(session: &XrkFile) -> DataFrame {
    // Use LF_Shock timestamps as the time axis (all shock channels are sampled together)
    let Some(lf) = session.channel_by_id(CH_LF_SHOCK) else {
        return DataFrame::default();
    };

    let times: Vec<f32> = lf.samples.iter().map(|s| s.time_sec).collect();
    let n = times.len();

    // Helper: get raw values for a channel, padding/truncating to length n
    let get_raws = |id: u16| -> Vec<u16> {
        match session.channel_by_id(id) {
            Some(ch) => ch.samples.iter().take(n).map(|s| s.raw).collect(),
            None     => vec![0u16; n],
        }
    };

    let lf_raw = get_raws(CH_LF_SHOCK);
    let rf_raw = get_raws(CH_RF_SHOCK);
    let lr_raw = get_raws(CH_LR_SHOCK);
    let rr_raw = get_raws(CH_RR_SHOCK);

    // Voltage conversion
    let to_v = |raw: &[u16]| -> Vec<f32> {
        raw.iter().map(|&r| r as f32 / 65535.0 * 5.0).collect()
    };

    df! {
        "time_sec"   => times,
        "LF_raw"     => lf_raw.clone(),
        "RF_raw"     => rf_raw.clone(),
        "LR_raw"     => lr_raw.clone(),
        "RR_raw"     => rr_raw.clone(),
        "LF_voltage" => to_v(&lf_raw),
        "RF_voltage" => to_v(&rf_raw),
        "LR_voltage" => to_v(&lr_raw),
        "RR_voltage" => to_v(&rr_raw),
    }
    .expect("failed to build shocks DataFrame")
}

fn build_accel_df(session: &XrkFile) -> DataFrame {
    let Some(inline) = session.channel_by_id(CH_INLINE_ACC) else {
        return DataFrame::default();
    };

    let times: Vec<f32> = inline.samples.iter().map(|s| s.time_sec).collect();
    let n = times.len();

    let get_raws = |id: u16| -> Vec<u16> {
        match session.channel_by_id(id) {
            Some(ch) => ch.samples.iter().take(n).map(|s| s.raw).collect(),
            None     => vec![0u16; n],
        }
    };

    // Calibration: 0G = 2.5V, scale = 1.185 V/G (validated via VerticalAcc = 1.000G)
    let to_g = |raw: &[u16]| -> Vec<f32> {
        raw.iter().map(|&r| {
            let v = r as f32 / 65535.0 * 5.0;
            (v - 2.5) / 1.185
        }).collect()
    };

    let il_raw = get_raws(CH_INLINE_ACC);
    let la_raw = get_raws(CH_LATERAL_ACC);
    let va_raw = get_raws(CH_VERTICAL_ACC);

    df! {
        "time_sec"    => times,
        "inline_raw"  => il_raw.clone(),
        "lateral_raw" => la_raw.clone(),
        "vertical_raw"=> va_raw.clone(),
        "inline_g"    => to_g(&il_raw),
        "lateral_g"   => to_g(&la_raw),
        "vertical_g"  => to_g(&va_raw),
    }
    .expect("failed to build accel DataFrame")
}

fn build_gyro_df(session: &XrkFile) -> DataFrame {
    let Some(roll) = session.channel_by_id(CH_ROLL_RATE) else {
        return DataFrame::default();
    };

    let times: Vec<f32> = roll.samples.iter().map(|s| s.time_sec).collect();
    let n = times.len();

    let get_raws = |id: u16| -> Vec<u16> {
        match session.channel_by_id(id) {
            Some(ch) => ch.samples.iter().take(n).map(|s| s.raw).collect(),
            None     => vec![0u16; n],
        }
    };

    df! {
        "time_sec"  => times,
        "roll_raw"  => get_raws(CH_ROLL_RATE),
        "pitch_raw" => get_raws(CH_PITCH_RATE),
        "yaw_raw"   => get_raws(CH_YAW_RATE),
    }
    .expect("failed to build gyro DataFrame")
}

fn build_lap_shock_stats_df(session: &XrkFile) -> DataFrame {
    let shock_ids   = [CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK];
    let shock_names = ["LF", "RF", "LR", "RR"];

    let mut lap_nums:  Vec<u16>   = Vec::new();
    let mut time_ms:   Vec<u32>   = Vec::new();
    let mut time_strs: Vec<String> = Vec::new();

    // Collect per-corner stats
    let mut corner_stats: Vec<(Vec<f64>, Vec<f64>, Vec<f32>)> =
        vec![(vec![], vec![], vec![]); 4];

    for lap in &session.laps {
        lap_nums.push(lap.number);
        time_ms.push(lap.time_ms);
        time_strs.push(lap.time_str());

        for (i, &id) in shock_ids.iter().enumerate() {
            if let Some(ch) = session.channel_by_id(id) {
                let stats = ch.per_lap_stats(&session.laps);
                if let Some(s) = stats.iter().find(|s| s.lap_number == lap.number) {
                    corner_stats[i].0.push(s.mean_raw);
                    corner_stats[i].1.push(s.std_raw);
                    corner_stats[i].2.push(s.mean_voltage());
                } else {
                    corner_stats[i].0.push(0.0);
                    corner_stats[i].1.push(0.0);
                    corner_stats[i].2.push(0.0);
                }
            }
        }
    }

    let mut cols: Vec<Column> = vec![
        Series::new("lap".into(), lap_nums).into(),
        Series::new("time_ms".into(), time_ms).into(),
        Series::new("time_str".into(), time_strs).into(),
    ];

    for (i, name) in shock_names.iter().enumerate() {
        cols.push(Series::new(format!("{name}_mean_raw").into(), corner_stats[i].0.clone()).into());
        cols.push(Series::new(format!("{name}_std_raw").into(),  corner_stats[i].1.clone()).into());
        cols.push(Series::new(format!("{name}_mean_v").into(),   corner_stats[i].2.clone()).into());
    }

    DataFrame::new(cols).expect("failed to build lap_shock_stats DataFrame")
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
