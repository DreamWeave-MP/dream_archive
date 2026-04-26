mod archive;
mod parser;

pub use archive::{
    Archive, ArchiveFlags, ArchiveInfo, ArchiveTypes, ArchiveVersion, Entry, FileRecord,
};

use std::{collections::TryReserveError, io, num::TryFromIntError};

/// Result type for BSA operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by the BSA reader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid magic read from archive header: {0:#010x}")]
    InvalidMagic(u32),
    #[error("invalid version read from archive header: {0}")]
    InvalidVersion(u32),
    #[error("invalid size read from archive header: {0}")]
    InvalidHeaderSize(u32),
    #[error("archive offset or size is out of bounds")]
    OutOfBounds,
    #[error("invalid or unsupported file record flags: {0:#010x}")]
    InvalidFileRecordFlags(u32),
    #[error("archive integer field can not fit on this platform")]
    IntegralTruncation,
    #[error("archive table or payload requests more memory than can be allocated")]
    Capacity,
    #[error(
        "buffer failed to decompress to the expected size: expected {expected} bytes, got {actual} bytes"
    )]
    DecompressionSizeMismatch { expected: usize, actual: usize },
    #[error("compressed payload has trailing data")]
    TrailingCompressedData,
    #[error("support for this feature is not implemented: {0}")]
    NotImplemented(&'static str),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("zlib decompression failed: {0}")]
    Zlib(String),
    #[error("expected TES4 v105 LZ4 frame data")]
    InvalidLz4Frame,
    #[error("LZ4 frame decompression failed: {0}")]
    Lz4Frame(String),
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
