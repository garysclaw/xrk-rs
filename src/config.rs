//! User-defined channel configuration and calibration.
//!
//! A [`LoggerConfig`] is a portable, serializable description of how to
//! interpret the raw ADC values in an XRK file. It is **separate** from the
//! parser — the parser just gives you raw data, the config gives it meaning.
//!
//! ## Typical workflow
//!
//! 1. Parse a session file once to see what channels it contains.
//! 2. Create a `LoggerConfig` for your car/logger (do this once, save to JSON).
//! 3. Load the config and call `session.apply_config(&config)` to get
//!    calibrated values for every channel.
//!
//! ## Example
//!
//! ```no_run
//! use xrk::{XrkFile, config::{LoggerConfig, ChannelConfig, Calibration}};
//!
//! // Build a config for your specific car
//! let mut cfg = LoggerConfig::new("My Race Car");
//!
//! // Shock pot: 2-point calibration (full droop = 0mm at 0.75V, full bump = 50mm at 4.10V)
//! cfg.add("LF_Shock", Calibration::two_point(0.75, 0.0, 4.10, 50.0), "mm");
//! cfg.add("RF_Shock", Calibration::two_point(0.72, 0.0, 4.08, 50.0), "mm");
//!
//! // Accelerometer: your sensor's spec sheet values
//! cfg.add("InlineAcc",   Calibration::linear(0.0 / 1.0, -2.5 / 1.185), "G");
//! cfg.add("LateralAcc",  Calibration::linear(1.0 / 1.185, -2.5 / 1.185), "G");
//! cfg.add("VerticalAcc", Calibration::linear(1.0 / 1.185, -2.5 / 1.185), "G");
//!
//! // Save to disk (JSON)
//! cfg.save("my_car.json").unwrap();
//!
//! // --- Later, when analysing a session ---
//! let cfg = LoggerConfig::load("my_car.json").unwrap();
//! let session = XrkFile::open("session.xrk").unwrap();
//!
//! if let Some(ch) = session.channel("LF_Shock") {
//!     for sample in &ch.samples {
//!         let mm = cfg.apply("LF_Shock", sample.raw);
//!         println!("{:.3}s  {:.1} mm", sample.time_sec, mm);
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::path::Path;

// ─── Calibration ─────────────────────────────────────────────────────────────

/// A linear calibration from raw ADC counts to a physical value.
///
/// The conversion is:
/// ```text
/// voltage       = raw / 65535.0 * 5.0
/// physical_value = gain * voltage + offset
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Calibration {
    /// Gain: physical units per volt
    pub gain: f64,
    /// Offset: physical value when voltage is 0
    pub offset: f64,
}

impl Calibration {
    /// Build a calibration from a slope and intercept directly.
    ///
    /// `physical = gain * voltage + offset`
    pub fn linear(gain: f64, offset: f64) -> Self {
        Self { gain, offset }
    }

    /// Build a calibration from two known (voltage, physical_value) pairs.
    ///
    /// ```
    /// # use xrk::config::Calibration;
    /// // Shock pot: 0.75V = 0mm (full droop), 4.10V = 50mm (full bump)
    /// let cal = Calibration::two_point(0.75, 0.0, 4.10, 50.0);
    /// ```
    pub fn two_point(v_low: f64, phys_low: f64, v_high: f64, phys_high: f64) -> Self {
        let gain   = (phys_high - phys_low) / (v_high - v_low);
        let offset = phys_low - gain * v_low;
        Self { gain, offset }
    }

    /// Apply the calibration to a raw ADC value (0–65535).
    pub fn apply(&self, raw: u16) -> f64 {
        let voltage = raw as f64 / 65535.0 * 5.0;
        self.gain * voltage + self.offset
    }
}

// ─── ChannelConfig ────────────────────────────────────────────────────────────

/// Configuration for a single channel.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChannelConfig {
    /// Channel name as it appears in the XRK file (user-defined in Race Studio)
    pub name: String,
    /// What this channel physically represents (free text, e.g. "Left front shock travel")
    pub description: String,
    /// Physical unit of the calibrated value (e.g. "mm", "G", "°C", "rpm")
    pub unit: String,
    /// Calibration to apply to raw samples
    pub calibration: Calibration,
}

impl ChannelConfig {
    /// Apply this channel's calibration to a raw ADC value.
    pub fn apply(&self, raw: u16) -> f64 {
        self.calibration.apply(raw)
    }
}

// ─── LoggerConfig ─────────────────────────────────────────────────────────────

/// A complete configuration for one logger / car setup.
///
/// Contains calibrations for every channel you care about.
/// Channels not listed in the config are still accessible — just as raw
/// ADC values without unit conversion.
///
/// Serialize to / from JSON with `--features serde`.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoggerConfig {
    /// Friendly name for this config (e.g. "Car #38 — AiM Quattro")
    pub name: String,
    /// Optional notes (logger serial, setup date, etc.)
    pub notes: String,
    /// Per-channel configurations, keyed by channel name
    pub channels: HashMap<String, ChannelConfig>,
}

impl LoggerConfig {
    /// Create a new empty config with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Default::default() }
    }

    /// Add a channel calibration.
    pub fn add(
        &mut self,
        channel_name: impl Into<String>,
        calibration: Calibration,
        unit: impl Into<String>,
    ) -> &mut Self {
        let name = channel_name.into();
        self.channels.insert(name.clone(), ChannelConfig {
            name: name.clone(),
            description: String::new(),
            unit: unit.into(),
            calibration,
        });
        self
    }

    /// Add a channel calibration with a description.
    pub fn add_with_description(
        &mut self,
        channel_name: impl Into<String>,
        calibration: Calibration,
        unit: impl Into<String>,
        description: impl Into<String>,
    ) -> &mut Self {
        let name = channel_name.into();
        self.channels.insert(name.clone(), ChannelConfig {
            name: name.clone(),
            description: description.into(),
            unit: unit.into(),
            calibration,
        });
        self
    }

    /// Look up the config for a specific channel name.
    pub fn channel(&self, name: &str) -> Option<&ChannelConfig> {
        self.channels.get(name)
    }

    /// Apply calibration to a raw value for the named channel.
    /// Returns `None` if no calibration is defined for this channel.
    pub fn apply(&self, channel_name: &str, raw: u16) -> Option<f64> {
        self.channels.get(channel_name).map(|c| c.apply(raw))
    }

    /// Apply calibration or fall back to the raw voltage (0–5V).
    pub fn apply_or_voltage(&self, channel_name: &str, raw: u16) -> f64 {
        self.apply(channel_name, raw)
            .unwrap_or_else(|| raw as f64 / 65535.0 * 5.0)
    }

    /// Load config from a JSON file (requires `--features serde`).
    #[cfg(feature = "serde")]
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let text = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&text)?)
    }

    /// Save config to a JSON file (requires `--features serde`).
    #[cfg(feature = "serde")]
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text)?;
        Ok(())
    }
}

// ─── Example config builder ───────────────────────────────────────────────────

/// Build a starter config by inspecting a session — creates entries for every
/// channel found, with no-op calibrations (raw voltage passthrough).
/// Use this as a starting point to fill in real calibration values.
#[cfg(feature = "serde")]
pub fn starter_config_from_session(session: &crate::XrkFile) -> LoggerConfig {
    let mut cfg = LoggerConfig::new(format!(
        "Config for {} — {}",
        session.info.vehicle, session.info.logger
    ));
    cfg.notes = format!(
        "Auto-generated from session: {} {} at {}. Replace calibrations with real values.",
        session.info.date, session.info.time, session.info.track
    );

    for ch in &session.channels {
        cfg.add_with_description(
            &ch.name,
            Calibration::linear(1.0, 0.0), // passthrough: returns voltage
            "V",
            format!("Channel {} ({})", ch.id, ch.short_name),
        );
    }
    cfg
}
