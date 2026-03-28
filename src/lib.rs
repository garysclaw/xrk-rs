//! # xrk — AiM XRK telemetry file parser
//!
//! A pure-Rust, cross-platform parser for AiM Sports XRK binary telemetry
//! files. No AiM SDK required.
//!
//! The library has two layers:
//!
//! 1. **Parser** — reads the binary file, returns raw ADC samples with
//!    channel names exactly as stored in the file. Zero assumptions.
//!
//! 2. **Config** — an optional, serializable [`LoggerConfig`] that maps
//!    channel names → physical calibrations. Save once per car, apply to
//!    every session. Pairs naturally with a Tauri/web app that lets users
//!    manage their configs.
//!
//! ## Quick start (raw)
//!
//! ```no_run
//! use xrk::XrkFile;
//!
//! let session = XrkFile::open("session.xrk").unwrap();
//!
//! println!("Track: {}", session.info.track);
//! for ch in &session.channels {
//!     println!("  {} — {} samples", ch.name, ch.samples.len());
//! }
//! for lap in &session.laps {
//!     println!("  Lap {:2}: {}", lap.number, lap.time_str());
//! }
//! ```
//!
//! ## With calibration config
//!
//! ```no_run
//! use xrk::{XrkFile, config::{LoggerConfig, Calibration}};
//!
//! // Load a saved config (or build one with LoggerConfig::new())
//! let cfg = LoggerConfig::load("my_car.json").unwrap();
//! let session = XrkFile::open("session.xrk").unwrap();
//!
//! if let Some(ch) = session.channel("LF_Shock") {
//!     for sample in &ch.samples {
//!         // cfg.apply() returns Option<f64> — None if channel not in config
//!         let mm = cfg.apply("LF_Shock", sample.raw).unwrap_or(0.0);
//!         println!("{:.3}s  {:.1} mm", sample.time_sec, mm);
//!     }
//! }
//! ```

pub mod config;
pub mod error;
pub mod parser;
pub mod types;

#[cfg(feature = "dataframe")]
pub mod dataframe;

#[cfg(feature = "python")]
pub mod python;

pub use config::{Calibration, LoggerConfig};
pub use error::XrkError;
pub use types::{Channel, ChannelId, Lap, LapStats, Sample, SessionInfo, XrkFile};
