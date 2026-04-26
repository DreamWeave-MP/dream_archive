//! Small, pure-Rust reader for Bethesda archive formats.
//!
//! The implementation is intentionally read-oriented. It targets the runtime
//! operations `OpenMW` needs: detect, list, lookup, extract files, and
//! reconstruct DDS streams from BA2 DX10 texture archives.
//!
//! # Example
//!
//! ```ignore
//! # #[cfg(feature = "ba2")]
//! # fn main() -> dream_archive::ba2::Result<()> {
//! let archive = dream_archive::ba2::Archive::open_path("Data/SomeArchive.ba2")?;
//! for entry in archive.entries() {
//!     println!("{}", entry.name());
//! }
//! if let Some(bytes) = archive.read_file("textures/example.dds")? {
//!     println!("extracted {} bytes", bytes.len());
//! }
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "ba2")]
pub mod ba2;
#[cfg(feature = "bsa-tes4")]
pub mod bsa;
#[cfg(any(feature = "ba2", feature = "bsa-tes4"))]
mod read;
#[cfg(any(feature = "ba2", feature = "bsa-tes4"))]
mod storage;

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
    /// Bethesda BSA.
    BSA(BsaFormat),
}

/// BSA archive generation detected by [`guess_format`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BsaFormat {
    /// Morrowind-era BSA with version `0x100` and no FourCC-style magic.
    TES3,
    /// Oblivion/Fallout-era BSA with `BSA\0` magic.
    TES4,
}

/// Weak archive-family sniffing. This is not full validation, but it reads
/// enough header bytes to distinguish BA2, TES3 BSA, and TES4+ BSA containers.
///
/// # Errors
///
/// Returns an I/O error if the first four bytes can not be read.
pub fn guess_format(input: &mut impl Read) -> io::Result<Option<FileFormat>> {
    let mut magic = [0; 4];
    input.read_exact(&mut magic)?;
    Ok(match &magic {
        #[cfg(feature = "ba2")]
        b"BTDX" => Some(FileFormat::BA2),
        #[cfg(feature = "bsa-tes4")]
        b"BSA\0" => Some(FileFormat::BSA(BsaFormat::TES4)),
        #[cfg(feature = "bsa-tes3")]
        [0, 1, 0, 0] => Some(FileFormat::BSA(BsaFormat::TES3)),
        _ => None,
    })
}

pub use bstr::{BStr, BString, ByteSlice, ByteVec};
