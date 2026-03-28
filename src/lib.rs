//! # xrk — AiM XRK telemetry file parser
//!
//! A pure-Rust, cross-platform parser for AiM Sports XRK binary telemetry
//! files. No AiM SDK required.
//!
//! ## Quick start
//!
//! ```no_run
//! use xrk::XrkFile;
//!
//! let session = XrkFile::open("data/38_Mobile_In_a_0023.xrk").unwrap();
//!
//! println!("Track: {}", session.info.track);
//! println!("Laps:  {}", session.laps.len());
//!
//! for lap in &session.laps {
//!     println!("  Lap {:2}: {}", lap.number, lap.time_str());
//! }
//!
//! if let Some(ch) = session.channel_by_name("LF_Shock") {
//!     println!("LF_Shock samples: {}", ch.samples.len());
//! }
//! ```

pub mod error;
pub mod parser;
pub mod types;

#[cfg(feature = "dataframe")]
pub mod dataframe;

#[cfg(feature = "python")]
pub mod python;

pub use error::XrkError;
pub use types::{
    Calibration, Channel, ChannelId, Lap, LapStats, Sample, SessionInfo, XrkFile,
    CH_INLINE_ACC, CH_LATERAL_ACC, CH_VERTICAL_ACC,
    CH_LF_SHOCK, CH_RF_SHOCK, CH_LR_SHOCK, CH_RR_SHOCK,
    CH_ROLL_RATE, CH_PITCH_RATE, CH_YAW_RATE,
    CH_GPS_LAT_ACC, CH_GPS_INL_ACC, CH_GPS_YAW_RATE,
    CH_LUMINOSITY, CH_RPM, CH_VBAT, CH_LOGGER_TEMP, CH_ODOMETER,
};
