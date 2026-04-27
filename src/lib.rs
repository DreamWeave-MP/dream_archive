//! Pure-Rust library for Bethesda archive formats.
//!
//! The top-level [`Archive`] facade is the ordinary entry point: detect an
//! archive, list entries, look files up, and extract them without first caring
//! whether the container is BA2, TES3 BSA, or TES4 BSA. Format-specific modules
//! remain available for archive metadata, hash-only BSA workflows, builder
//! policy, and other places where the formats insist on being different
//! (because of course they do).
//!
//! Archive paths are byte strings. Bethesda archive formats do not reliably
//! declare filename encodings, and older tools commonly wrote paths using the
//! system code page of the machine that produced the archive. Keep archive
//! lookup byte-first when possible. If accepting user-facing Unicode text, use
//! the BSA filename helpers to encode that text explicitly before lookup rather
//! than guessing a code page.
//!
//! # Feature flags
//!
//! The default feature set enables BA2 and BSA support. Individual format
//! families can be selected with `ba2`, `bsa-tes3`, and `bsa-tes4`; `bsa` enables
//! both BSA generations. `parallel` enables Rayon-backed bulk extraction paths.
//!
//! The `lua` feature exposes an embedded [`mlua`] API through the
//! `dream_archive::lua` module. It enables BA2 and BSA support, uses vendored
//! `LuaJIT` with Lua 5.2 compatibility, and is meant for applications that embed a
//! Lua VM. It does not install a standalone C module named `dream_archive`; the
//! host application creates and registers the module table.
//!
//! # Capability summary
//!
//! - BA2 GNRL: read, extract, and write.
//! - BA2 DX10: read, extract by reconstructing DDS headers from archive
//!   texture metadata, and write from supported DDS files or explicit texture
//!   metadata plus raw texture payload bytes.
//! - BA2 GNMF: Sony GNM (`.gnf`) texture metadata parsing only. Payload
//!   extraction/writing is intentionally out of scope for now because correct
//!   support requires console texture swizzle/unswizzle semantics, not a raw
//!   chunk dump.
//! - TES3 BSA: read, extract, and write.
//! - TES4 BSA: read, extract, and write for PC v103/v104/v105 archives,
//!   including compressed, hash-only, and embedded-name layouts.
//! - Console/Xbox archive layouts and `XMem` compression are explicitly
//!   unsupported.
//!
//! # Example
//!
//! ```no_run
//! # #[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
//! # fn main() -> dream_archive::Result<()> {
//! let archive = dream_archive::Archive::open_path("Data/SomeArchive.bsa")?;
//! for entry in archive.entries() {
//!     if let Some(path) = entry.path() {
//!         println!("{path}");
//!     }
//! }
//!
//! let bytes = archive.read_file_required("textures/example.dds")?;
//! println!("extracted {} bytes", bytes.len());
//! # Ok(())
//! # }
//! # #[cfg(not(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4")))]
//! # fn main() {}
//! ```
//!
//! # Building archives
//!
//! ```no_run
//! # #[cfg(feature = "bsa-tes3")]
//! # fn main() -> dream_archive::bsa::Result<()> {
//! let mut builder = dream_archive::Tes3BsaBuilder::new();
//! builder.add_dir("Data")?;
//! builder.write_path("MyMod.bsa")?;
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "bsa-tes3"))]
//! # fn main() {}
//! ```
//!
//! ```no_run
//! # #[cfg(feature = "bsa-tes4")]
//! # fn main() -> dream_archive::bsa::Result<()> {
//! let mut builder = dream_archive::Tes4BsaBuilder::skyrim_le();
//! builder.add_dir("Data")?;
//! builder.write_path("MyMod.bsa")?;
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "bsa-tes4"))]
//! # fn main() {}
//! ```
//!
//! ```no_run
//! # #[cfg(feature = "ba2")]
//! # fn main() -> dream_archive::ba2::Result<()> {
//! let mut builder = dream_archive::Ba2Builder::new();
//! builder.set_compression(Some(dream_archive::ba2::Ba2CompressionFormat::Zip));
//! builder.add_dir("Data")?;
//! builder.write_path("MyMod.ba2")?;
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "ba2"))]
//! # fn main() {}
//! ```
//!
//! ```no_run
//! # #[cfg(feature = "ba2")]
//! # fn main() -> dream_archive::ba2::Result<()> {
//! let mut builder = dream_archive::Ba2Dx10Builder::new();
//! builder.add_dds_file("textures/example.dds", "example.dds")?;
//! builder.write_path("Textures.ba2")?;
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "ba2"))]
//! # fn main() {}
//! ```
//!
//! Builder `add_file` / `add_dir` calls are deferred: they record source paths
//! and sizes, then read payload bytes during `write_path`, `write_seek`, or
//! `to_vec`. Use `add_bytes` when the payload is already in memory. Rewriters can
//! preserve unchanged members with format-specific `add_archive_entry` methods
//! and stable entry ids from `entries_with_ids`, avoiding the old “extract every
//! preserved file into a Vec first” routine. That routine had all the elegance of
//! swallowing a filing cabinet.
//!
//! # Legacy BSA filename encodings
//!
//! ```
//! # #[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
//! # fn main() -> dream_archive::bsa::Result<()> {
//! let encoded = dream_archive::bsa::encode_filename(
//!     "textures/zażółć.dds",
//!     dream_archive::bsa::FilenameEncoding::Windows1250,
//! )?;
//! assert_eq!(encoded.as_ref(), b"textures/za\xbf\xf3\xb3\xe6.dds");
//! # Ok(())
//! # }
//! # #[cfg(not(any(feature = "bsa-tes3", feature = "bsa-tes4")))]
//! # fn main() {}
//! ```
//!
//! # Lua bindings
//!
//! Enable the `lua` feature to build an [`mlua`]-based module table:
//!
//! ```rust,no_run
//! # #[cfg(feature = "lua")]
//! # fn main() -> mlua::Result<()> {
//! let lua = mlua::Lua::new();
//! let module = dream_archive::lua::create_module(&lua)?;
//! lua.globals().set("dream_archive", module)?;
//!
//! lua.load(r#"
//!     local builder = dream_archive.ba2.Builder.new()
//!     builder:add_bytes("meshes/example.nif", "payload")
//!     local archive = dream_archive.open_bytes(builder:to_bytes())
//!     assert(archive:read_file_required("meshes/example.nif") == "payload")
//! "#).exec()?;
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "lua"))]
//! # fn main() {}
//! ```
//!
//! Lua strings are used as byte buffers for archive paths and payloads. Host
//! filesystem paths are the explicit exception and must be valid UTF-8 when
//! passed through Lua. Optional reads/extractions return `nil` for missing
//! archive members; `*_required` methods raise Lua errors. See the `lua` module
//! documentation for the full Lua-facing contract.

#[cfg(feature = "ba2")]
pub mod ba2;
#[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
pub mod bsa;
#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
mod builder_fs;
#[cfg(feature = "ba2")]
mod dds;
#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
mod extract;
#[cfg(feature = "lua")]
pub mod lua;
#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
mod read;
#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
mod storage;

use std::io::{self, Read};

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

#[cfg(feature = "ba2")]
pub type Ba2Builder = ba2::Builder;
#[cfg(feature = "ba2")]
pub type Ba2Dx10Builder = ba2::Dx10Builder;
#[cfg(feature = "bsa-tes3")]
pub type Tes3BsaBuilder = bsa::tes3::Builder;
#[cfg(feature = "bsa-tes4")]
pub type Tes4BsaBuilder = bsa::tes4::Builder;

/// Per-file compression policy for archive builders.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CompressionOverride {
    /// Use the builder's default compression policy.
    #[default]
    Inherit,
    /// Store this file uncompressed.
    Store,
    /// Compress this file using the builder's configured compression method.
    Compress,
}

/// Result type for top-level archive auto-detection operations.
#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by top-level archive auto-detection operations.
#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The input does not match an enabled archive family.
    UnknownFormat,
    /// A required archive member was absent.
    FileNotFound(BString),
    /// Filesystem or stream I/O failed.
    Io(io::Error),
    /// BA2-specific parsing or extraction failed.
    #[cfg(feature = "ba2")]
    Ba2(ba2::Error),
    /// BSA-specific parsing or extraction failed.
    #[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
    Bsa(bsa::Error),
}

#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownFormat => f.write_str("unknown or disabled archive format"),
            Self::FileNotFound(path) => write!(f, "archive member not found: {path}"),
            Self::Io(error) => error.fmt(f),
            #[cfg(feature = "ba2")]
            Self::Ba2(error) => error.fmt(f),
            #[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
            Self::Bsa(error) => error.fmt(f),
        }
    }
}

#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            #[cfg(feature = "ba2")]
            Self::Ba2(error) => Some(error),
            #[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
            Self::Bsa(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(feature = "ba2")]
impl From<ba2::Error> for Error {
    fn from(error: ba2::Error) -> Self {
        Self::Ba2(error)
    }
}

#[cfg(any(feature = "bsa-tes3", feature = "bsa-tes4"))]
impl From<bsa::Error> for Error {
    fn from(error: bsa::Error) -> Self {
        Self::Bsa(error)
    }
}

/// Parsed archive opened through the top-level auto-detection facade.
#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
#[derive(Clone, Debug)]
pub enum Archive {
    #[cfg(feature = "ba2")]
    BA2(ba2::Archive),
    #[cfg(feature = "bsa-tes3")]
    Tes3Bsa(bsa::tes3::Archive),
    #[cfg(feature = "bsa-tes4")]
    Tes4Bsa(bsa::tes4::Archive),
}

/// Borrowed entry view returned by the top-level archive facade.
#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
#[derive(Clone, Copy, Debug)]
pub enum Entry<'a> {
    #[cfg(feature = "ba2")]
    BA2(&'a ba2::Entry),
    #[cfg(feature = "bsa-tes3")]
    Tes3Bsa(&'a bsa::tes3::Entry),
    #[cfg(feature = "bsa-tes4")]
    Tes4Bsa(&'a bsa::tes4::Entry),
}

/// Non-allocating iterator over entries in a top-level [`Archive`].
#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
#[derive(Clone, Debug)]
pub enum Entries<'a> {
    #[cfg(feature = "ba2")]
    BA2(std::slice::Iter<'a, ba2::Entry>),
    #[cfg(feature = "bsa-tes3")]
    Tes3Bsa(std::slice::Iter<'a, bsa::tes3::Entry>),
    #[cfg(feature = "bsa-tes4")]
    Tes4Bsa(std::slice::Iter<'a, bsa::tes4::Entry>),
}

#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
impl<'a> Iterator for Entries<'a> {
    type Item = Entry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            #[cfg(feature = "ba2")]
            Self::BA2(entries) => entries.next().map(Entry::BA2),
            #[cfg(feature = "bsa-tes3")]
            Self::Tes3Bsa(entries) => entries.next().map(Entry::Tes3Bsa),
            #[cfg(feature = "bsa-tes4")]
            Self::Tes4Bsa(entries) => entries.next().map(Entry::Tes4Bsa),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            #[cfg(feature = "ba2")]
            Self::BA2(entries) => entries.size_hint(),
            #[cfg(feature = "bsa-tes3")]
            Self::Tes3Bsa(entries) => entries.size_hint(),
            #[cfg(feature = "bsa-tes4")]
            Self::Tes4Bsa(entries) => entries.size_hint(),
        }
    }
}

#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
impl ExactSizeIterator for Entries<'_> {}

#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
impl std::iter::FusedIterator for Entries<'_> {}

#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
impl<'a> Entry<'a> {
    /// Stored archive path, when the archive format provides one.
    ///
    /// Hash-only/stringless archives have entries but no recoverable path text.
    /// Use the format-specific hash lookup APIs for those. An empty BA2 entry
    /// name is treated as missing path text rather than as a real empty path.
    #[must_use]
    pub fn path(self) -> Option<&'a BStr> {
        match self {
            #[cfg(feature = "ba2")]
            Self::BA2(entry) => (!entry.name().is_empty()).then(|| entry.name()),
            #[cfg(feature = "bsa-tes3")]
            Self::Tes3Bsa(entry) => Some(entry.path()),
            #[cfg(feature = "bsa-tes4")]
            Self::Tes4Bsa(entry) => entry.path(),
        }
    }

    #[must_use]
    pub fn format(self) -> FileFormat {
        match self {
            #[cfg(feature = "ba2")]
            Self::BA2(_) => FileFormat::BA2,
            #[cfg(feature = "bsa-tes3")]
            Self::Tes3Bsa(_) => FileFormat::BSA(BsaFormat::TES3),
            #[cfg(feature = "bsa-tes4")]
            Self::Tes4Bsa(_) => FileFormat::BSA(BsaFormat::TES4),
        }
    }
}

#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
impl Archive {
    /// Detect and open an archive from a filesystem path.
    ///
    /// # Errors
    ///
    /// Returns an I/O error, [`Error::UnknownFormat`], or a format-specific parse error.
    pub fn open_path(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let path = path.as_ref();
        match detect_path(path)? {
            #[cfg(feature = "ba2")]
            Some(FileFormat::BA2) => Ok(Self::BA2(ba2::Archive::open_path(path)?)),
            #[cfg(feature = "bsa-tes3")]
            Some(FileFormat::BSA(BsaFormat::TES3)) => {
                Ok(Self::Tes3Bsa(bsa::tes3::Archive::open_path(path)?))
            }
            #[cfg(feature = "bsa-tes4")]
            Some(FileFormat::BSA(BsaFormat::TES4)) => {
                Ok(Self::Tes4Bsa(bsa::tes4::Archive::open_path(path)?))
            }
            _ => Err(Error::UnknownFormat),
        }
    }

    /// Detect and parse an archive from a byte slice, copying it into owned storage.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownFormat`] or a format-specific parse error.
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let mut cursor = std::io::Cursor::new(bytes);
        match guess_format(&mut cursor)? {
            #[cfg(feature = "ba2")]
            Some(FileFormat::BA2) => Ok(Self::BA2(ba2::Archive::from_slice(bytes)?)),
            #[cfg(feature = "bsa-tes3")]
            Some(FileFormat::BSA(BsaFormat::TES3)) => {
                Ok(Self::Tes3Bsa(bsa::tes3::Archive::from_slice(bytes)?))
            }
            #[cfg(feature = "bsa-tes4")]
            Some(FileFormat::BSA(BsaFormat::TES4)) => {
                Ok(Self::Tes4Bsa(bsa::tes4::Archive::from_slice(bytes)?))
            }
            _ => Err(Error::UnknownFormat),
        }
    }

    /// Detect and parse an archive from an owned byte vector.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownFormat`] or a format-specific parse error.
    pub fn from_vec(bytes: Vec<u8>) -> Result<Self> {
        let mut cursor = std::io::Cursor::new(bytes.as_slice());
        match guess_format(&mut cursor)? {
            #[cfg(feature = "ba2")]
            Some(FileFormat::BA2) => Ok(Self::BA2(ba2::Archive::from_vec(bytes)?)),
            #[cfg(feature = "bsa-tes3")]
            Some(FileFormat::BSA(BsaFormat::TES3)) => {
                Ok(Self::Tes3Bsa(bsa::tes3::Archive::from_vec(bytes)?))
            }
            #[cfg(feature = "bsa-tes4")]
            Some(FileFormat::BSA(BsaFormat::TES4)) => {
                Ok(Self::Tes4Bsa(bsa::tes4::Archive::from_vec(bytes)?))
            }
            _ => Err(Error::UnknownFormat),
        }
    }

    #[must_use]
    pub fn format(&self) -> FileFormat {
        match self {
            #[cfg(feature = "ba2")]
            Self::BA2(_) => FileFormat::BA2,
            #[cfg(feature = "bsa-tes3")]
            Self::Tes3Bsa(_) => FileFormat::BSA(BsaFormat::TES3),
            #[cfg(feature = "bsa-tes4")]
            Self::Tes4Bsa(_) => FileFormat::BSA(BsaFormat::TES4),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            #[cfg(feature = "ba2")]
            Self::BA2(archive) => archive.len(),
            #[cfg(feature = "bsa-tes3")]
            Self::Tes3Bsa(archive) => archive.len(),
            #[cfg(feature = "bsa-tes4")]
            Self::Tes4Bsa(archive) => archive.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn entries(&self) -> Entries<'_> {
        match self {
            #[cfg(feature = "ba2")]
            Self::BA2(archive) => Entries::BA2(archive.entries().iter()),
            #[cfg(feature = "bsa-tes3")]
            Self::Tes3Bsa(archive) => Entries::Tes3Bsa(archive.entries().iter()),
            #[cfg(feature = "bsa-tes4")]
            Self::Tes4Bsa(archive) => Entries::Tes4Bsa(archive.entries().iter()),
        }
    }

    /// Read an optional archive member into memory.
    ///
    /// # Errors
    ///
    /// Returns a format-specific extraction error when the member exists but can
    /// not be decoded or read.
    pub fn read_file(&self, path: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>> {
        match self {
            #[cfg(feature = "ba2")]
            Self::BA2(archive) => Ok(archive.read_file(path)?),
            #[cfg(feature = "bsa-tes3")]
            Self::Tes3Bsa(archive) => Ok(archive.read_file(path)?),
            #[cfg(feature = "bsa-tes4")]
            Self::Tes4Bsa(archive) => Ok(archive.read_file(path)?),
        }
    }

    /// Read a required archive member into memory.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FileNotFound`] if the member is absent, or a
    /// format-specific extraction error when it exists but can not be decoded or
    /// read.
    pub fn read_file_required(&self, path: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        let path = path.as_ref();
        self.read_file(path)?
            .ok_or_else(|| Error::FileNotFound(BString::from(path)))
    }

    /// Extract an optional archive member into a writer.
    ///
    /// # Errors
    ///
    /// Returns a format-specific extraction error when the member exists but can
    /// not be decoded or written.
    pub fn extract_file(
        &self,
        path: impl AsRef<[u8]>,
        out: impl std::io::Write,
    ) -> Result<Option<u64>> {
        match self {
            #[cfg(feature = "ba2")]
            Self::BA2(archive) => Ok(archive.extract_file(path, out)?),
            #[cfg(feature = "bsa-tes3")]
            Self::Tes3Bsa(archive) => Ok(archive.extract_file(path, out)?),
            #[cfg(feature = "bsa-tes4")]
            Self::Tes4Bsa(archive) => Ok(archive.extract_file(path, out)?),
        }
    }

    /// Extract a required archive member into a writer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FileNotFound`] if the member is absent, or a
    /// format-specific extraction error when it exists but can not be decoded or
    /// written.
    pub fn extract_file_required(
        &self,
        path: impl AsRef<[u8]>,
        out: impl std::io::Write,
    ) -> Result<u64> {
        let path = path.as_ref();
        self.extract_file(path, out)?
            .ok_or_else(|| Error::FileNotFound(BString::from(path)))
    }

    /// Extract every named and extractable archive member into a directory.
    ///
    /// Extraction is atomic per output file, not transactional for the whole
    /// archive: files successfully written before a later error remain in place.
    /// Hash-only TES4 archives, BA2 archives without string tables, and
    /// metadata-only payload families such as BA2 GNMF require format-specific
    /// handling instead.
    ///
    /// # Errors
    ///
    /// Returns a format-specific extraction error, path-safety error, or I/O
    /// error.
    pub fn extract_to(&self, target_dir: impl AsRef<std::path::Path>) -> Result<u64> {
        match self {
            #[cfg(feature = "ba2")]
            Self::BA2(archive) => Ok(archive.extract_to(target_dir)?),
            #[cfg(feature = "bsa-tes3")]
            Self::Tes3Bsa(archive) => Ok(archive.extract_to(target_dir)?),
            #[cfg(feature = "bsa-tes4")]
            Self::Tes4Bsa(archive) => Ok(archive.extract_to(target_dir)?),
        }
    }
}

/// Detect the archive family of a filesystem path.
///
/// # Errors
///
/// Returns an I/O error if the file can not be opened or read.
#[cfg(any(feature = "ba2", feature = "bsa-tes3", feature = "bsa-tes4"))]
pub fn detect_path(path: impl AsRef<std::path::Path>) -> io::Result<Option<FileFormat>> {
    let mut file = std::fs::File::open(path)?;
    guess_format(&mut file)
}
