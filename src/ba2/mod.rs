mod archive;
mod builder;
mod chunk;
mod dds;
mod hash;
mod parser;

pub use archive::{Archive, ArchiveFile, ArchiveInfo, Entry, FileHeader, TextureHeader};
pub use builder::Builder;
pub use chunk::{Ba2CompressionFormat, Chunk};
pub use hash::{FileHash, Hash, hash_file, hash_file_in_place};

use std::{collections::TryReserveError, fmt, io, num::TryFromIntError};

/// Result type for BA2 operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by the BA2 reader.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    DecompressionSizeMismatch { expected: usize, actual: usize },
    TrailingCompressedData,
    Dds(&'static str),
    InvalidFormat(u32),
    InvalidMagic(u32),
    InvalidVersion(u32),
    InvalidChunkSentinel(u32),
    InvalidChunkSize(u16),
    InvalidArchivePath,
    DuplicatePath,
    FileNotFound(bstr::BString),
    OutOfBounds,
    IntegralTruncation,
    Capacity,
    NotImplemented(&'static str),
    Io(io::Error),
    Zlib(String),
    Lz4(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecompressionSizeMismatch { expected, actual } => write!(
                f,
                "buffer failed to decompress to the expected size: expected {expected} bytes, got {actual} bytes"
            ),
            Self::TrailingCompressedData => f.write_str("compressed payload has trailing data"),
            Self::Dds(message) => write!(f, "invalid DDS/texture metadata: {message}"),
            Self::InvalidFormat(value) => {
                write!(f, "invalid format read from archive header: {value:#010x}")
            }
            Self::InvalidMagic(value) => {
                write!(f, "invalid magic read from archive header: {value:#010x}")
            }
            Self::InvalidVersion(value) => {
                write!(f, "invalid version read from archive header: {value}")
            }
            Self::InvalidChunkSentinel(value) => {
                write!(f, "invalid chunk sentinel: {value:#010x}")
            }
            Self::InvalidChunkSize(value) => {
                write!(f, "invalid chunk size read from file header: {value}")
            }
            Self::InvalidArchivePath => f.write_str("invalid archive path"),
            Self::DuplicatePath => f.write_str("duplicate archive path"),
            Self::FileNotFound(path) => write!(f, "archive member not found: {path}"),
            Self::OutOfBounds => f.write_str("archive offset or size is out of bounds"),
            Self::IntegralTruncation => {
                f.write_str("archive integer field can not fit on this platform")
            }
            Self::Capacity => {
                f.write_str("archive table or payload requests more memory than can be allocated")
            }
            Self::NotImplemented(feature) => {
                write!(f, "support for this feature is not implemented: {feature}")
            }
            Self::Io(error) => error.fmt(f),
            Self::Zlib(error) => write!(f, "zlib decompression failed: {error}"),
            Self::Lz4(error) => write!(f, "lz4 decompression failed: {error}"),
        }
    }
}

impl From<TryReserveError> for Error {
    fn from(_: TryReserveError) -> Self {
        Self::Capacity
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

/// BA2 archive payload format.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PayloadFormat {
    #[default]
    GNRL,
    DX10,
    GNMF,
}

/// BA2 archive version.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ArchiveVersion {
    #[default]
    v1 = 1,
    v2 = 2,
    v3 = 3,
    v7 = 7,
    v8 = 8,
}

pub(crate) const fn fourcc(bytes: [u8; 4]) -> u32 {
    u32::from_le_bytes(bytes)
}
