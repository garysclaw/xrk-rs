//! Python bindings via PyO3.
//!
//! Exposes the XRK parser to Python as a native extension module.
//! Build with: `maturin develop --features python`
//!
//! # Python usage
//!
//! ```python
//! import xrk
//!
//! session = xrk.open("session.xrk")
//! print(f"Track: {session.track}")
//! print(f"Best lap: {session.best_lap_str()}")
//!
//! for lap in session.laps():
//!     print(f"  Lap {lap.number}: {lap.time_str()}")
//!
//! ch = session.channel("LF_Shock")
//! if ch:
//!     print(f"LF_Shock: {ch.n_samples} samples, mean {ch.mean_voltage:.3f} V")
//!
//! # Per-lap stats as a list of dicts (pandas/polars ready)
//! import polars as pl
//! df = pl.DataFrame(session.all_channel_lap_stats())
//! print(df)
//! ```

use crate::types::{Channel, Lap, LapStats, XrkFile};
use pyo3::prelude::*;

// ─── Python wrapper types ─────────────────────────────────────────────────────

#[pyclass(name = "Lap")]
pub struct PyLap {
    inner: Lap,
}

#[pymethods]
impl PyLap {
    #[getter]
    fn number(&self) -> u16 {
        self.inner.number
    }
    #[getter]
    fn time_ms(&self) -> u32 {
        self.inner.time_ms
    }
    #[getter]
    fn start_sec(&self) -> f64 {
        self.inner.start_sec
    }
    #[getter]
    fn end_sec(&self) -> f64 {
        self.inner.end_sec()
    }
    fn time_str(&self) -> String {
        self.inner.time_str()
    }
    fn __repr__(&self) -> String {
        format!("<Lap {} {}>", self.inner.number, self.inner.time_str())
    }
}

#[pyclass(name = "ChannelData")]
pub struct PyChannel {
    inner: Channel,
}

#[pymethods]
impl PyChannel {
    #[getter]
    fn id(&self) -> u16 {
        self.inner.id
    }
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }
    #[getter]
    fn n_samples(&self) -> usize {
        self.inner.samples.len()
    }

    /// Sample timestamps as a list of floats (seconds).
    fn times(&self) -> Vec<f32> {
        self.inner.samples.iter().map(|s| s.time_sec).collect()
    }

    /// Raw ADC values as a list of ints (0–65535).
    fn raw_values(&self) -> Vec<u16> {
        self.inner.samples.iter().map(|s| s.raw).collect()
    }

    /// Voltage values as a list of floats (0.0–5.0 V).
    fn voltages(&self) -> Vec<f32> {
        self.inner
            .samples
            .iter()
            .map(|s| s.raw as f32 / 65535.0 * 5.0)
            .collect()
    }

    /// Samples calibrated to physical units: `physical = gain * voltage + offset`
    fn calibrated(&self, gain: f32, offset: f32) -> Vec<f32> {
        self.inner
            .samples
            .iter()
            .map(|s| {
                let v = s.raw as f32 / 65535.0 * 5.0;
                gain * v + offset
            })
            .collect()
    }

    /// Mean voltage across all samples (0.0–5.0 V).
    #[getter]
    fn mean_voltage(&self) -> f32 {
        match self.inner.mean() {
            Some(m) => (m / 65535.0 * 5.0) as f32,
            None => 0.0,
        }
    }

    /// Minimum raw ADC value.
    #[getter]
    fn min_raw(&self) -> u16 {
        self.inner.min().unwrap_or(0)
    }

    /// Maximum raw ADC value.
    #[getter]
    fn max_raw(&self) -> u16 {
        self.inner.max().unwrap_or(0)
    }

    fn __repr__(&self) -> String {
        format!(
            "<ChannelData '{}' id={} n={} mean={:.3}V>",
            self.inner.name,
            self.inner.id,
            self.inner.samples.len(),
            self.mean_voltage(),
        )
    }
}

#[pyclass(name = "LapStats")]
pub struct PyLapStats {
    inner: LapStats,
}

#[pymethods]
impl PyLapStats {
    #[getter]
    fn lap_number(&self) -> u16 {
        self.inner.lap_number
    }
    #[getter]
    fn lap_time_ms(&self) -> u32 {
        self.inner.lap_time_ms
    }
    #[getter]
    fn n_samples(&self) -> usize {
        self.inner.n_samples
    }
    /// Mean raw ADC count (0–65535).
    #[getter]
    fn mean_raw(&self) -> f64 {
        self.inner.mean
    }
    /// Std-dev of raw ADC counts.
    #[getter]
    fn std_raw(&self) -> f64 {
        self.inner.std
    }
    /// Min raw ADC value.
    #[getter]
    fn min_raw(&self) -> u16 {
        self.inner.min
    }
    /// Max raw ADC value.
    #[getter]
    fn max_raw(&self) -> u16 {
        self.inner.max
    }
    /// Mean voltage (0.0–5.0 V).
    #[getter]
    fn mean_voltage(&self) -> f32 {
        (self.inner.mean / 65535.0 * 5.0) as f32
    }

    fn to_dict(&self, py: Python<'_>) -> PyObject {
        use pyo3::types::PyDict;
        let d = PyDict::new(py);
        d.set_item("lap", self.inner.lap_number).unwrap();
        d.set_item("time_ms", self.inner.lap_time_ms).unwrap();
        d.set_item("n_samples", self.inner.n_samples).unwrap();
        d.set_item("mean_raw", self.inner.mean).unwrap();
        d.set_item("std_raw", self.inner.std).unwrap();
        d.set_item("min_raw", self.inner.min).unwrap();
        d.set_item("max_raw", self.inner.max).unwrap();
        d.set_item("mean_v", (self.inner.mean / 65535.0 * 5.0) as f32)
            .unwrap();
        d.into()
    }
}

// ─── Main session object ──────────────────────────────────────────────────────

#[pyclass(name = "Session")]
pub struct PySession {
    inner: XrkFile,
}

#[pymethods]
impl PySession {
    // --- Metadata ---
    #[getter]
    fn track(&self) -> &str {
        &self.inner.info.track
    }
    #[getter]
    fn date(&self) -> &str {
        &self.inner.info.date
    }
    #[getter]
    fn time(&self) -> &str {
        &self.inner.info.time
    }
    #[getter]
    fn vehicle(&self) -> &str {
        &self.inner.info.vehicle
    }
    #[getter]
    fn duration_sec(&self) -> f64 {
        self.inner.info.duration_sec
    }
    #[getter]
    fn file_size(&self) -> usize {
        self.inner.info.file_size
    }

    // --- Laps ---
    fn laps(&self) -> Vec<PyLap> {
        self.inner
            .laps
            .iter()
            .map(|l| PyLap { inner: l.clone() })
            .collect()
    }

    fn lap(&self, number: u16) -> Option<PyLap> {
        self.inner
            .laps
            .iter()
            .find(|l| l.number == number)
            .map(|l| PyLap { inner: l.clone() })
    }

    fn best_lap_str(&self) -> String {
        self.inner
            .best_lap(5_000)
            .map(|l| l.time_str())
            .unwrap_or_else(|| "N/A".to_string())
    }

    // --- Channels ---
    fn channel(&self, name: &str) -> Option<PyChannel> {
        self.inner
            .channel(name)
            .map(|c| PyChannel { inner: c.clone() })
    }

    fn channel_names(&self) -> Vec<String> {
        self.inner.channels.iter().map(|c| c.name.clone()).collect()
    }

    // --- Per-lap statistics for all channels (returns list of dicts) ---
    /// Returns per-lap statistics for every channel as a flat list of dicts.
    /// Each dict has keys: channel, lap, time_ms, n_samples,
    ///                     mean_raw, std_raw, min_raw, max_raw, mean_v
    fn all_channel_lap_stats(&self, py: Python<'_>) -> Vec<PyObject> {
        use pyo3::types::PyDict;
        let mut rows = Vec::new();
        for ch in &self.inner.channels {
            let stats = ch.per_lap_stats(&self.inner.laps);
            for s in &stats {
                let d = PyDict::new(py);
                d.set_item("channel", &ch.name).unwrap();
                d.set_item("lap", s.lap_number).unwrap();
                d.set_item("time_ms", s.lap_time_ms).unwrap();
                d.set_item("n_samples", s.n_samples).unwrap();
                d.set_item("mean_raw", s.mean).unwrap();
                d.set_item("std_raw", s.std).unwrap();
                d.set_item("min_raw", s.min).unwrap();
                d.set_item("max_raw", s.max).unwrap();
                d.set_item("mean_v", (s.mean / 65535.0 * 5.0) as f32)
                    .unwrap();
                rows.push(d.into());
            }
        }
        rows
    }

    fn __repr__(&self) -> String {
        format!(
            "<Session track='{}' date='{}' laps={} channels={}>",
            self.inner.info.track,
            self.inner.info.date,
            self.inner.laps.len(),
            self.inner.channels.len(),
        )
    }
}

// ─── Module entry point ───────────────────────────────────────────────────────

/// Open and parse an AiM XRK telemetry file from disk.
#[pyfunction]
fn open(path: &str) -> PyResult<PySession> {
    let inner =
        XrkFile::open(path).map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
    Ok(PySession { inner })
}

/// Parse an XRK file from bytes already loaded in Python.
#[pyfunction]
fn from_bytes(data: &[u8]) -> PyResult<PySession> {
    let inner = XrkFile::from_bytes(data)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))?;
    Ok(PySession { inner })
}

#[pymodule]
pub fn xrk(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_function(wrap_pyfunction!(from_bytes, m)?)?;
    m.add_class::<PySession>()?;
    m.add_class::<PyLap>()?;
    m.add_class::<PyChannel>()?;
    m.add_class::<PyLapStats>()?;
    Ok(())
}
