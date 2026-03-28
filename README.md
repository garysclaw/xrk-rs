# xrk-rs

A pure-Rust parser for **AiM Sports XRK** binary telemetry files.

- ✅ **Cross-platform** — Windows, macOS, Linux (no AiM SDK required)
- ✅ **Fast** — parses a 2MB session file in <10ms
- ✅ **Polars DataFrames** — native Rust DataFrames via `--features dataframe`
- ✅ **Python bindings** — via PyO3/maturin with `--features python`
- ✅ **Zero unsafe** — safe Rust throughout

---

## Quick start

```rust
use xrk::XrkFile;

let session = XrkFile::open("data/session.xrk")?;

println!("Track:    {}", session.info.track);
println!("Date:     {} {}", session.info.date, session.info.time);
println!("Duration: {:.1}s", session.info.duration_sec);

// Lap times
for lap in &session.laps {
    println!("  Lap {:2}: {}", lap.number, lap.time_str());
}

// Best lap (ignoring sub-5s laps)
if let Some(best) = session.best_lap(5_000) {
    println!("Best lap: Lap {} — {}", best.number, best.time_str());
}

// Shock potentiometer data
if let Some(lf) = session.channel_by_name("LF_Shock") {
    println!("LF_Shock: {} samples, mean {:.3}V",
        lf.samples.len(),
        lf.mean_voltage().unwrap_or(0.0));
}

// Per-lap shock statistics
if let Some(lf) = session.channel_by_id(xrk::CH_LF_SHOCK) {
    for stat in lf.per_lap_stats(&session.laps) {
        println!("  Lap {}: mean={:.0} std={:.0} n={}",
            stat.lap_number, stat.mean_raw, stat.std_raw, stat.n_samples);
    }
}
```

---

## Polars DataFrames

Enable with `--features dataframe`:

```toml
xrk = { version = "0.1", features = ["dataframe"] }
```

```rust
use xrk::{XrkFile, dataframe::SessionDataFrames};

let session = XrkFile::open("data/session.xrk")?;
let dfs = SessionDataFrames::from_session(&session);

// DataFrame of all lap times
println!("{}", dfs.laps);
// ┌────────────┬─────────┬──────────┬───────────┐
// │ lap_number ┆ time_ms ┆ time_str ┆ start_sec │
// ╞════════════╪═════════╪══════════╪═══════════╡
// │          1 ┆   32295 ┆ 0:32.295 ┆     98.82 │
// │          7 ┆   18696 ┆ 0:18.696 ┆    461.01 │
// └────────────┴─────────┴──────────┴───────────┘

// Full shock time-series: time_sec, LF_raw, RF_raw, LR_raw, RR_raw, *_voltage
println!("{}", dfs.shocks);

// Accelerometer time-series with G calibration applied
println!("{}", dfs.accel);

// Per-lap shock stats (great for setup comparison)
println!("{}", dfs.lap_shock_stats);

// Save everything to Parquet for use in Python / Jupyter
dfs.save_parquet("output/")?;
dfs.save_csv("output/")?;
```

---

## Python bindings

Build with [maturin](https://github.com/PyO3/maturin):

```bash
pip install maturin
maturin develop --features python
```

```python
import xrk

session = xrk.open("data/session.xrk")

print(f"Track: {session.track}")
print(f"Best lap: {session.best_lap_str()}")

for lap in session.laps():
    print(f"  Lap {lap.number}: {lap.time_str()}")

# Get channel data
lf = session.channel("LF_Shock")
print(f"LF_Shock: {lf.n_samples} samples, {lf.mean_voltage:.3f}V mean")

# Voltages as a Python list — hand directly to numpy or polars
voltages = lf.voltages()

# Per-lap shock stats as list of dicts — polars/pandas ready
stats = session.shock_lap_stats()
import polars as pl
df = pl.DataFrame(stats)
print(df)
```

---

## Channels decoded

| ID | Name | Type | Sample Rate |
|----|------|------|-------------|
| 19 | LF_Shock | Shock pot (0–5V ADC) | ~81 Hz |
| 20 | RF_Shock | Shock pot (0–5V ADC) | ~81 Hz |
| 21 | LR_Shock | Shock pot (0–5V ADC) | ~81 Hz |
| 22 | RR_Shock | Shock pot (0–5V ADC) | ~81 Hz |
| 23 | InlineAcc | Accelerometer | ~16 Hz |
| 24 | LateralAcc | Accelerometer | ~16 Hz |
| 25 | VerticalAcc | Accelerometer | ~16 Hz |
| 26 | RollRate | Gyroscope | ~16 Hz |
| 27 | PitchRate | Gyroscope | ~16 Hz |
| 28 | YawRate | Gyroscope | ~16 Hz |
| 10 | ExternalVoltage | Voltage monitor | varies |
| 18 | RPM | Engine speed | varies |

---

## ADC calibration

Raw sample values are **uint16** (0–65535 = 0–5 V). Convert to physical units
with the [`Calibration`] struct or the per-sample helpers:

```rust
// Manual 2-point calibration for a 50mm shock pot:
// At full droop: 0.75V → 0mm
// At full bump:  4.10V → 50mm
let gain   = 50.0 / (4.10 - 0.75);   // mm/V = 14.93
let offset = -0.75 * gain;            // = -11.2mm

let position_mm = sample.calibrate(gain, offset);

// Built-in accelerometer calibration (validated VerticalAcc = 1.000G):
let g_force = Calibration::ACCEL_2G.apply(&sample);
```

---

## XRK format notes

AiM XRK files use a chunked binary format. Key structures:

```
<hTAG[5]  ...data...          chunk header + data
)(M ts[4] ch[2] n[2] v[n×2]  measurement sample record
)(S ts[4] ch[2] flags[2]      section separator
<hLAP ... lap_num[2] time_ms[4] ... start_ts[4]
```

This library was developed by reverse-engineering the binary format.
For the official AiM SDK (`libxdrk.so` / `MatLabXRK.dll`), see
[AiM Sports](https://www.aim-sportline.com/en/software-xdrk.htm).

---

## License

MIT
