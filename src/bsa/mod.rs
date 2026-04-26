//! BSA support shared by TES3/Morrowind and TES4-family archives.
//!
//! # Filename bytes and localization
//!
//! BSA paths are stored as bytes. The archive does not tell us whether those
//! bytes are UTF-8, Windows-1251, CP437, or whatever a modding tool inherited
//! from the user's locale in 2006. Consequently, lookup APIs take byte-like
//! paths (`impl AsRef<[u8]>`) and apply only archive/VFS byte normalization:
//! separator folding and ASCII case handling. They do not perform Unicode
//! case-folding, Unicode normalization, or code page detection.
//!
//! For display, decode explicitly:
//!
//! ```
//! # use dream_archive::bsa::{FilenameEncoding, decode_filename_lossy};
//! let shown = decode_filename_lossy(b"Mar\xeda.txt", FilenameEncoding::Windows1252);
//! assert_eq!(shown, "María.txt");
//! ```
//!
//! For lookup from UI text, encode explicitly and then pass the resulting bytes
//! to the archive API:
//!
//! ```
//! # use dream_archive::bsa::{FilenameEncoding, encode_filename};
//! # fn example() -> Result<(), dream_archive::bsa::FilenameEncodeError> {
//! let path = encode_filename("María.txt", FilenameEncoding::Windows1252)?;
//! // archive.read_file(path.as_ref())?;
//! # Ok(())
//! # }
//! ```
//!
//! Encoding is lossless. If a character can not be represented in the selected
//! legacy encoding, [`encode_filename`] returns [`FilenameEncodeError`] instead
//! of replacing it with `?`. Silent filename corruption is still corruption,
//! even if it smiles at you.
//!
//! For extraction, `extract_to` preserves raw archive bytes where the platform
//! can represent them. On platforms whose filesystem paths are Unicode-first,
//! or when you want localized byte paths decoded to human-readable names, use
//! `extract_to_with_encoding` on the TES3/TES4 archive types and choose the code
//! page yourself. There is intentionally no auto-detection.

#[cfg(feature = "bsa-tes3")]
pub mod tes3;
#[cfg(feature = "bsa-tes4")]
pub mod tes4;

mod encoding;
mod hash;

pub use encoding::{FilenameEncodeError, FilenameEncoding, decode_filename_lossy, encode_filename};

use std::{collections::TryReserveError, fmt, io, num::TryFromIntError};

pub(crate) fn normalize_lookup_path(path: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(path.len());
    for byte in path.iter().copied() {
        let byte = match byte {
            b'\\' => b'/',
            b'A'..=b'Z' => byte + 32,
            _ => byte,
        };
        if byte == b'/' && (out.is_empty() || out.last() == Some(&b'/')) {
            continue;
        }
        out.push(byte);
    }
    out
}

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
    InvalidArchivePath,
    ArchivePathsUnavailable,
    DuplicatePath,
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
            Self::InvalidArchivePath => f.write_str("archive path can not be stored safely"),
            Self::ArchivePathsUnavailable => {
                f.write_str("archive does not contain filenames required for path-based extraction")
            }
            Self::DuplicatePath => f.write_str("archive contains duplicate normalized paths"),
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
