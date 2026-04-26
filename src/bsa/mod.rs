mod archive;
mod parser;

pub use archive::{Archive, ArchiveFlags, ArchiveInfo, ArchiveTypes, ArchiveVersion};

use std::{io, num::TryFromIntError};

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
    #[error("archive integer field can not fit on this platform")]
    IntegralTruncation,
    #[error(transparent)]
    Io(#[from] io::Error),
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
