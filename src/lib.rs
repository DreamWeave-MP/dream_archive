//! Small, pure-Rust reader for Bethesda BA2 archives.
//!
//! The implementation is intentionally read-oriented. It targets the same BA2
//! runtime operations `OpenMW` needs: detect, list, hash lookup, extract GNRL
//! files, and reconstruct DDS streams from DX10 texture archives.

pub mod ba2;

use std::io::{self, Read};

/// Makes a shallow copy of the input in APIs that support borrowed data.
pub struct Borrowed<'borrow>(pub &'borrow [u8]);

/// Makes a deep copy of the input in APIs that support owned data.
pub struct Copied<'copy>(pub &'copy [u8]);

/// Archive family detected by [`guess_format`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileFormat {
    /// Bethesda BA2 (`BTDX`).
    BA2,
}

/// Weak archive-family sniffing. This is not full validation.
///
/// # Errors
///
/// Returns an I/O error if the first four bytes can not be read.
pub fn guess_format(input: &mut impl Read) -> io::Result<Option<FileFormat>> {
    let mut magic = [0; 4];
    input.read_exact(&mut magic)?;
    Ok(match &magic {
        b"BTDX" => Some(FileFormat::BA2),
        _ => None,
    })
}

pub use bstr::{BStr, BString, ByteSlice, ByteVec};
