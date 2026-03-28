# xrk-rs

A pure-Rust, cross-platform parser for **AiM Sports XRK** binary telemetry files.

- ✅ **No AiM SDK required** — pure binary parsing, works on Windows / macOS / Linux
- ✅ **Generic** — returns whatever channels are in the file, no hardcoded assumptions
- ✅ **Configurable** — bring your own `LoggerConfig` for calibration (save as JSON, reuse across sessions)
- ✅ **Polars DataFrames** — native Rust DataFrames via `--features dataframe`
- ✅ **Python bindings** — via PyO3/maturin with `--features python`

---

## Two-layer design

```
┌─────────────────────────────────────────────────────┐
│  Your app (Tauri, CLI, Python script, …)            │
│                                                     │
│  LoggerConfig  ← save once per car, load per session│
│  (channel name → gain/offset/unit)                  │
├─────────────────────────────────────────────────────┤
│  xrk-rs                                             │
│  XrkFile::open()  →  raw ADC samples + channel names│
│  (no assumptions about what any channel means)      │
└─────────────────────────────────────────────────────┘
```

The parser gives you **exactly what is in the file** — channel names as the
user set them in Race Studio 3, raw uint16 ADC values, lap times, session
metadata. The `LoggerConfig` layer is optional and lives outside the parser.

---

## Quick start

```rust
use xrk::XrkFile;

let session = XrkFile::open("session.xrk")?;

// Session metadata
println!("Track:    {}", session.info.track);
println!("Date:     {} {}", session.info.date, session.info.time);
println!("Duration: {:.1}s", session.info.duration_sec);

// Every channel the file contains (names come from the file)
for ch in &session.channels {
    println!("  [{}] {:20} — {:6} samples  {:.0} Hz",
        ch.id, ch.name, ch.samples.len(),
        ch.sample_rate_hz(session.info.duration_sec));
}

// Lap times
for lap in &session.laps {
    println!("  Lap {:2}: {}", lap.number, lap.time_str());
}

// Best lap
if let Some(best) = session.best_lap(0) {
    println!("Best: Lap {} — {}", best.number, best.time_str());
}
```

---

## Calibration config

Define calibrations once per car, save to JSON, reuse forever.

```rust
use xrk::{XrkFile, config::{LoggerConfig, Calibration}};

// Build your config (do this once, then save to disk)
let mut cfg = LoggerConfig::new("Car #38 — AiM Quattro");

// 2-point calibration: measure voltage at full droop and full bump
cfg.add("LF_Shock", Calibration::two_point(0.75, 0.0, 4.10, 50.0), "mm");
cfg.add("RF_Shock", Calibration::two_point(0.72, 0.0, 4.08, 50.0), "mm");
cfg.add("LR_Shock", Calibration::two_point(0.80, 0.0, 4.12, 50.0), "mm");
cfg.add("RR_Shock", Calibration::two_point(0.78, 0.0, 4.15, 50.0), "mm");

// Linear calibration from your sensor's spec sheet
cfg.add("InlineAcc",   Calibration::linear(1.0 / 1.185, -2.5 / 1.185), "G");
cfg.add("VerticalAcc", Calibration::linear(1.0 / 1.185, -2.5 / 1.185), "G");
cfg.add("RPM",         Calibration::linear(8000.0, 0.0), "rpm");

// Save to disk (requires --features serde)
cfg.save("car38.json")?;

// --- Later, when analysing a session ---
let cfg     = LoggerConfig::load("car38.json")?;
let session = XrkFile::open("session.xrk")?;

if let Some(ch) = session.channel("LF_Shock") {
    for sample in &ch.samples {
        let mm = cfg.apply("LF_Shock", sample.raw).unwrap_or(0.0);
        println!("{:.3}s  {:.1} mm", sample.time_sec, mm);
    }
}
```

The JSON config file looks like this — easy to edit by hand or from an app:

```json
{
  "name": "Car #38 — AiM Quattro",
  "notes": "2-point cal done 2026-01-15 on jackstands",
  "channels": {
    "LF_Shock": {
      "name": "LF_Shock",
      "description": "Left front shock travel",
      "unit": "mm",
      "calibration": { "gain": 14.925, "offset": -11.194 }
    }
  }
}
```

---

## Polars DataFrames

```toml
xrk = { version = "0.1", features = ["dataframe"] }
```

```rust
use xrk::{XrkFile, dataframe::SessionDataFrames};

let session = XrkFile::open("session.xrk")?;
let dfs = SessionDataFrames::from_session(&session);

// Lap times table
println!("{}", dfs.laps);

// All channels as a single wide time-series DataFrame
println!("{}", dfs.channels);

// Save to Parquet (load in Python with polars or pandas)
dfs.save_parquet("output/")?;
```

---

## Python bindings

```bash
pip install maturin
maturin develop --features python
```

```python
import xrk

session = xrk.open("session.xrk")
print(session)  # <Session track='Mobile In' date='01/19/2026' laps=9 channels=27>

for lap in session.laps():
    print(f"  Lap {lap.number}: {lap.time_str()}")

ch = session.channel("LF_Shock")
print(f"{ch.name}: {ch.n_samples} samples")
print(ch.raw_values()[:10])  # raw ADC counts
```

---

## ADC values and calibration

Raw sample values are **uint16** (0–65535). For AiM loggers the full range
represents 0–5 V, but always verify against your specific logger hardware.

```rust
// Manual conversion
let voltage = sample.raw as f64 / 65535.0 * 5.0;

// With Calibration struct
let cal = Calibration::two_point(v_low, phys_low, v_high, phys_high);
let mm  = cal.apply(sample.raw);
```

---

## XRK format notes

AiM XRK files use a chunked binary format. Key structures:

```text
<hTAG[5]  ...                           tagged chunk
)(M ts[4] ch_id[2] n[2] vals[n*2]       measurement samples
)(S ts[4] ch_id[2] flags[2]             section separator
<hCHS ...                               channel definition (name, short name)
<hLAP ... lap_num[2] time_ms[4] ...     lap record
```

This library was developed by reverse-engineering the binary format.
For the official AiM SDK see [AiM Sports](https://www.aim-sportline.com).

---

## License

MIT
