#[cfg(feature = "bsa-tes4")]
pub mod tes4;

use std::{collections::TryReserveError, fmt, io, num::TryFromIntError};

/// Result type for BSA operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by the BSA reader.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    InvalidMagic(u32),
    InvalidVersion(u32),
    InvalidHeaderSize(u32),
    OutOfBounds,
    InvalidFileRecordFlags(u32),
    IntegralTruncation,
    Capacity,
    DecompressionSizeMismatch { expected: usize, actual: usize },
    TrailingCompressedData,
    NotImplemented(&'static str),
    Io(io::Error),
    Zlib(String),
    InvalidLz4Frame,
    Lz4Frame(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic(value) => {
                write!(f, "invalid magic read from archive header: {value:#010x}")
            }
            Self::InvalidVersion(value) => {
                write!(f, "invalid version read from archive header: {value}")
            }
            Self::InvalidHeaderSize(value) => {
                write!(f, "invalid size read from archive header: {value}")
            }
            Self::OutOfBounds => f.write_str("archive offset or size is out of bounds"),
            Self::InvalidFileRecordFlags(value) => {
                write!(f, "invalid or unsupported file record flags: {value:#010x}")
            }
            Self::IntegralTruncation => {
                f.write_str("archive integer field can not fit on this platform")
            }
            Self::Capacity => {
                f.write_str("archive table or payload requests more memory than can be allocated")
            }
            Self::DecompressionSizeMismatch { expected, actual } => write!(
                f,
                "buffer failed to decompress to the expected size: expected {expected} bytes, got {actual} bytes"
            ),
            Self::TrailingCompressedData => f.write_str("compressed payload has trailing data"),
            Self::NotImplemented(feature) => {
                write!(f, "support for this feature is not implemented: {feature}")
            }
            Self::Io(error) => error.fmt(f),
            Self::Zlib(error) => write!(f, "zlib decompression failed: {error}"),
            Self::InvalidLz4Frame => f.write_str("expected TES4 v105 LZ4 frame data"),
            Self::Lz4Frame(error) => write!(f, "LZ4 frame decompression failed: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<TryReserveError> for Error {
    fn from(_: TryReserveError) -> Self {
        Self::Capacity
    }
}

impl From<TryFromIntError> for Error {
    fn from(_: TryFromIntError) -> Self {
        Self::IntegralTruncation
    }
}

impl From<crate::read::Error> for Error {
    fn from(value: crate::read::Error) -> Self {
        match value {
            crate::read::Error::OutOfBounds => Self::OutOfBounds,
            crate::read::Error::UnexpectedEof => Self::Io(value.into()),
        }
    }
}
