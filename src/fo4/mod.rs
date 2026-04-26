mod archive;
mod chunk;
mod dds;
mod hash;

pub use archive::{Archive, ArchiveOptions, Entry, File, FileHeader, TextureHeader};
pub use chunk::{Chunk, CompressionFormat};
pub use hash::{FileHash, Hash, hash_file, hash_file_in_place};

use std::{io, num::TryFromIntError};

/// Result type for BA2 operations.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by the BA2 reader.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(
        "buffer failed to decompress to the expected size: expected {expected} bytes, got {actual} bytes"
    )]
    DecompressionSizeMismatch { expected: usize, actual: usize },
    #[error("invalid DDS/texture metadata: {0}")]
    Dds(&'static str),
    #[error("invalid format read from archive header: {0:#010x}")]
    InvalidFormat(u32),
    #[error("invalid magic read from archive header: {0:#010x}")]
    InvalidMagic(u32),
    #[error("invalid version read from archive header: {0}")]
    InvalidVersion(u32),
    #[error("invalid chunk sentinel: {0:#010x}")]
    InvalidChunkSentinel(u32),
    #[error("invalid chunk size read from file header: {0}")]
    InvalidChunkSize(u16),
    #[error("archive offset or size is out of bounds")]
    OutOfBounds,
    #[error("archive integer field can not fit on this platform")]
    IntegralTruncation,
    #[error("support for this feature is not implemented")]
    NotImplemented,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("zlib decompression failed: {0}")]
    Zlib(String),
    #[error("lz4 decompression failed: {0}")]
    Lz4(String),
}

impl From<TryFromIntError> for Error {
    fn from(_: TryFromIntError) -> Self {
        Self::IntegralTruncation
    }
}

/// Archive payload format.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Format {
    #[default]
    GNRL,
    DX10,
    GNMF,
}

/// BA2 archive version.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Version {
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
