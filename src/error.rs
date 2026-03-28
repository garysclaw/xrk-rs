use thiserror::Error;

#[derive(Debug, Error)]
pub enum XrkError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("File too small to be a valid XRK file ({0} bytes)")]
    FileTooSmall(usize),

    #[error("No data markers ()(M) found — file may be empty or corrupt")]
    NoDataMarkers,

    #[error("Invalid UTF-8 in metadata field '{field}': {source}")]
    InvalidUtf8 {
        field: &'static str,
        source: std::str::Utf8Error,
    },

    #[error("Unexpected end of data at offset {offset} (need {need} bytes, have {have})")]
    UnexpectedEof {
        offset: usize,
        need: usize,
        have: usize,
    },
}
