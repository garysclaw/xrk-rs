"""
xrk — Python bindings for the AiM XRK telemetry file parser.

The compiled Rust extension exposes these types directly.
"""

from typing import Optional

class Lap:
    """A single timed lap."""
    number: int
    time_ms: int
    start_sec: float
    end_sec: float
    def time_str(self) -> str: ...

class ChannelData:
    """A named data channel with its complete time-series sample array."""
    id: int
    name: str
    n_samples: int
    mean_voltage: float
    min_raw: int
    max_raw: int
    def times(self) -> list[float]: ...
    def raw_values(self) -> list[int]: ...
    def voltages(self) -> list[float]: ...
    def calibrated(self, gain: float, offset: float) -> list[float]: ...

class LapStats:
    """Per-lap statistics for a channel."""
    lap_number: int
    lap_time_ms: int
    n_samples: int
    mean_raw: float
    std_raw: float
    min_raw: int
    max_raw: int
    mean_voltage: float
    def to_dict(self) -> dict: ...

class Session:
    """A fully-parsed AiM XRK telemetry session."""
    track: str
    date: str
    time: str
    vehicle: str
    duration_sec: float
    file_size: int
    def laps(self) -> list[Lap]: ...
    def lap(self, number: int) -> Optional[Lap]: ...
    def best_lap_str(self) -> str: ...
    def channel(self, name: str) -> Optional[ChannelData]: ...
    def channel_names(self) -> list[str]: ...
    def all_channel_lap_stats(self) -> list[dict]: ...

def open(path: str) -> Session:
    """Open and parse an AiM XRK telemetry file from disk."""
    ...

def from_bytes(data: bytes) -> Session:
    """Parse an XRK file from bytes already loaded in Python."""
    ...
