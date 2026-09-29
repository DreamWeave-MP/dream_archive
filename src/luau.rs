//! Luau bindings for `dream_archive` as an [l3i](https://github.com/DreamWeave-MP/l3i)
//! extension: `dream.archive`, module `@dream/archive`, type `dream.archive.Archive`.
//!
//! Enable the `luau` feature. The crate never creates a VM: the host composes
//! [`ArchiveExtension`] into a `RuntimePlan` and instantiates runtimes from it;
//! a compatibility global (`dreamArchive`) is the host's decision
//! (`RuntimePolicy::compat_global("@dream/archive", "dreamArchive")`).
//!
//! ```no_run
//! use l3i::Runtime;
//! use l3i::extension::{RuntimePlan, RuntimePolicy};
//!
//! # fn main() -> l3i::Result<()> {
//! let plan = RuntimePlan::builder()
//!     .policy(RuntimePolicy::new().compat_global("@dream/archive", "dreamArchive"))
//!     .extension(dream_archive::luau::ArchiveExtension)
//!     .finalize()?;
//! let runtime = Runtime::from_plan(&plan)?;
//! runtime.exec(
//!     r#"
//!     local dreamArchive = require("@dream/archive")
//!     local b = dreamArchive.ba2.Builder.new()
//!     b:addBytes("meshes/example.nif", "payload")
//!     local archive = dreamArchive.openBytes(b:toBytes())
//!     assert(archive:readFileRequired("meshes/example.nif") == "payload")
//! "#,
//! )?;
//! # Ok(())
//! # }
//! ```
//!
//! # Module shape
//!
//! `@dream/archive` has `openPath`, `openBytes`, `detectPath`, `guessFormat`, `normalizePath`,
//! and the nested modules `ba2` (`@dream/archive/ba2`: `openPath`, `openBytes`, `hashFile`,
//! `Builder.new`, `Dx10Builder.new`, `compression`, `version`) and `bsa` (`@dream/archive/bsa`:
//! `encodeFilename`, `decodeFilenameLossy`, `normalizePath`, `encoding`, and `tes3` /
//! `tes4` with their `openPath`, `openBytes`, hash helpers, `Builder.new`, and constants).
//! Every `open*` returns the one [`Archive`] type; format-specific members (`info` for BA2,
//! `extractToWithEncoding` for BSA, `extractToWithPaths` for TES4) raise on the wrong format.
//!
//! # Entries and hashes
//!
//! `archive:entries()` is a sequence view (`#`, `[i]`, `for`, `:toTable()`) of [`Entry`]
//! handles, and `archive:entry(i)` / `archive:get(path)` / `archive:getByHash(...)` return one.
//! A handle reads its metadata straight from the archive index: `index`, `id`, `format`,
//! `path`, `name`, `folder`, `size`, `offset`, `storedSize`, `compressed`, `chunks`, and the
//! hashes. TES3 and TES4 hashes are Luau integers carrying all 64 bits (`hash`, TES4
//! `folderHash`; `hashHex` / `folderHashHex` keep the old hex spelling); BA2 hashes are the
//! three exact `u32` fields `directoryHash`, `fileHash`, `extensionHash`.
//! `containsHash` / `getByHash` take the same shapes: `(hash)` for TES3,
//! `(folderHash, fileHash)` for TES4, `(directory, file, extension)` for BA2.
//!
//! # Bytes
//!
//! Archive paths are byte strings; host filesystem paths are UTF-8 strings. Payload inputs
//! (`openBytes`, builder `addBytes` and friends) accept a Luau `buffer` or a string.
//! `readInto(entry | path, buffer, offset?)` decodes straight into the script's buffer and
//! returns the byte count; it is the fast path. `readFile*` / `readEntry` / `extractEntry`
//! keep returning strings for compatibility and copy twice (the slow path).

use std::cell::{OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use l3i::bind::Call;
use l3i::convert::{Bits64, BufferView, BytesView, Exact};
use l3i::direct::field::{DirectField, FieldValue};
use l3i::extension::{Extension, ExtensionDescriptor, InstallContext, TagPolicy};
use l3i::options::Options;
use l3i::sequence::{Sequence, SequenceSource};
use l3i::stack::{Scope, ValueView};
use l3i::userdata::{Owned, Userdata};
use l3i::value::Table;
use l3i::{Error, Result, Runtime};

use crate::ByteSlice as _;
use crate::bsa::tes4::HashFields;
use crate::{BsaFormat, FileFormat};

/// The extension id.
pub const EXTENSION_ID: &str = "dream.archive";
/// The canonical module path.
pub const MODULE: &str = "@dream/archive";
/// The BA2 module path (also `ba2` on the main module).
pub const BA2_MODULE: &str = "@dream/archive/ba2";
/// The BSA module path (also `bsa` on the main module).
pub const BSA_MODULE: &str = "@dream/archive/bsa";
/// The TES3 module path (also `bsa.tes3`).
pub const TES3_MODULE: &str = "@dream/archive/bsa/tes3";
/// The TES4 module path (also `bsa.tes4`).
pub const TES4_MODULE: &str = "@dream/archive/bsa/tes4";
/// The stable key of the archive userdata type, for augmentation.
pub const ARCHIVE_KEY: &str = "dream.archive.Archive";
/// The stable key of the entry handle type.
pub const ENTRY_KEY: &str = "dream.archive.Entry";
/// The stable key of the entries sequence view.
pub const ENTRIES_KEY: &str = "dream.archive.Entries";

/// The `dream.archive` extension.
#[derive(Clone, Copy, Debug, Default)]
pub struct ArchiveExtension;

// ---------------------------------------------------------------------------------------------
// Archive handles
// ---------------------------------------------------------------------------------------------

/// An opened archive as scripts see it (`dream.archive.Archive`). Cheap to clone: entry
/// handles and the entries view share the parsed index.
#[derive(Clone)]
pub struct Archive(Rc<Opened>);

struct Opened {
    archive: crate::Archive,
    path: Option<PathBuf>,
    /// The format archive behind an `Arc`, cloned once for builders that preserve entries.
    shared: OnceCell<Shared>,
}

enum Shared {
    Ba2(Arc<crate::ba2::Archive>),
    Tes3(Arc<crate::bsa::tes3::Archive>),
    Tes4(Arc<crate::bsa::tes4::Archive>),
}

// SAFETY: plain Rust data (`Rc` to the parsed index and an optional path); dropping it never
// touches the Lua API.
unsafe impl Userdata for Archive {
    const NAME: &'static str = "dream.archive.Archive";
}

impl Archive {
    /// Wraps an opened archive; `path` is the host path it came from, if any.
    #[must_use]
    pub fn new(archive: crate::Archive, path: Option<PathBuf>) -> Self {
        Self(Rc::new(Opened {
            archive,
            path,
            shared: OnceCell::new(),
        }))
    }

    /// The opened archive.
    #[must_use]
    pub fn archive(&self) -> &crate::Archive {
        &self.0.archive
    }

    /// The host path used to open the archive; `None` for byte-backed archives.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.0.path.as_deref()
    }

    /// The entry handle for a 0-based index, if it exists.
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<Entry> {
        (index < self.0.archive.len()).then(|| Entry {
            archive: Rc::clone(&self.0),
            index,
        })
    }

    fn label(&self) -> String {
        self.0
            .path
            .as_ref()
            .map_or_else(|| "<memory>".to_owned(), |path| path.display().to_string())
    }

    fn format_name(&self) -> &'static str {
        format_name(self.0.archive.format())
    }

    fn wrong_format(&self, method: &str, expected: &str) -> Error {
        Error::runtime(format!(
            "{method} needs a {expected} archive; this archive is {}",
            self.format_name()
        ))
    }

    fn shared_ba2(&self) -> Result<Arc<crate::ba2::Archive>> {
        let shared = self.0.shared.get_or_init(|| match &self.0.archive {
            crate::Archive::BA2(archive) => Shared::Ba2(Arc::new(archive.clone())),
            crate::Archive::Tes3Bsa(archive) => Shared::Tes3(Arc::new(archive.clone())),
            crate::Archive::Tes4Bsa(archive) => Shared::Tes4(Arc::new(archive.clone())),
        });
        match shared {
            Shared::Ba2(archive) => Ok(Arc::clone(archive)),
            _ => Err(self.wrong_format("addArchiveEntry", "BA2")),
        }
    }

    fn shared_tes3(&self) -> Result<Arc<crate::bsa::tes3::Archive>> {
        let shared = self.0.shared.get_or_init(|| match &self.0.archive {
            crate::Archive::BA2(archive) => Shared::Ba2(Arc::new(archive.clone())),
            crate::Archive::Tes3Bsa(archive) => Shared::Tes3(Arc::new(archive.clone())),
            crate::Archive::Tes4Bsa(archive) => Shared::Tes4(Arc::new(archive.clone())),
        });
        match shared {
            Shared::Tes3(archive) => Ok(Arc::clone(archive)),
            _ => Err(self.wrong_format("addArchiveEntry", "TES3 BSA")),
        }
    }

    fn shared_tes4(&self) -> Result<Arc<crate::bsa::tes4::Archive>> {
        let shared = self.0.shared.get_or_init(|| match &self.0.archive {
            crate::Archive::BA2(archive) => Shared::Ba2(Arc::new(archive.clone())),
            crate::Archive::Tes3Bsa(archive) => Shared::Tes3(Arc::new(archive.clone())),
            crate::Archive::Tes4Bsa(archive) => Shared::Tes4(Arc::new(archive.clone())),
        });
        match shared {
            Shared::Tes4(archive) => Ok(Arc::clone(archive)),
            _ => Err(self.wrong_format("addArchiveEntry", "TES4 BSA")),
        }
    }

    /// A handle from a 1-based script index; an error names the bound.
    fn handle(&self, index: Exact<i64>) -> Result<Entry> {
        usize::try_from(index.0.wrapping_sub(1))
            .ok()
            .and_then(|index| self.entry(index))
            .ok_or_else(|| Error::runtime("entry index out of bounds"))
    }

    fn get(&self, path: &[u8]) -> Option<Entry> {
        let index = match &self.0.archive {
            crate::Archive::BA2(archive) => archive.get_id(path)?.index(),
            crate::Archive::Tes3Bsa(archive) => archive.get_id(path)?.index(),
            crate::Archive::Tes4Bsa(archive) => archive.get_id(path)?.index(),
        };
        self.entry(index)
    }

    fn contains(&self, path: &[u8]) -> bool {
        match &self.0.archive {
            crate::Archive::BA2(archive) => archive.contains(path),
            crate::Archive::Tes3Bsa(archive) => archive.contains(path),
            crate::Archive::Tes4Bsa(archive) => archive.contains(path),
        }
    }

    /// The hash arguments of `containsHash` / `getByHash` in this archive's shape.
    fn hash_query(
        &self,
        first: ValueView<'_>,
        second: Option<ValueView<'_>>,
        third: Option<ValueView<'_>>,
    ) -> Result<HashQuery> {
        let extra = |present: bool, message: &str| {
            if present {
                Err(Error::runtime(message.to_owned()))
            } else {
                Ok(())
            }
        };
        match self.0.archive.format() {
            FileFormat::BSA(BsaFormat::TES3) => {
                extra(
                    second.is_some() || third.is_some(),
                    "a TES3 hash lookup takes one integer: the entry hash",
                )?;
                Ok(HashQuery::Tes3(first.read::<Bits64>()?.0))
            }
            FileFormat::BSA(BsaFormat::TES4) => {
                extra(
                    third.is_some(),
                    "a TES4 hash lookup takes two integers: the folder hash and the file hash",
                )?;
                let file = second.ok_or_else(|| {
                    Error::runtime(
                        "a TES4 hash lookup takes two integers: the folder hash and the file hash",
                    )
                })?;
                Ok(HashQuery::Tes4(
                    HashFields::from_numeric(first.read::<Bits64>()?.0),
                    HashFields::from_numeric(file.read::<Bits64>()?.0),
                ))
            }
            FileFormat::BA2 => {
                let missing = || {
                    Error::runtime(
                        "a BA2 hash lookup takes three numbers: the directory, file, and extension hashes",
                    )
                };
                let file = second.ok_or_else(missing)?;
                let extension = third.ok_or_else(missing)?;
                Ok(HashQuery::Ba2(crate::ba2::FileHash(crate::ba2::Hash {
                    directory: first.read::<Exact<u32>>()?.0,
                    file: file.read::<Exact<u32>>()?.0,
                    extension: extension.read::<Exact<u32>>()?.0,
                })))
            }
        }
    }

    fn get_by_hash(&self, query: HashQuery) -> Option<Entry> {
        let index = match (&self.0.archive, query) {
            (crate::Archive::BA2(archive), HashQuery::Ba2(hash)) => {
                archive.get_id_by_hash(hash)?.index()
            }
            (crate::Archive::Tes3Bsa(archive), HashQuery::Tes3(hash)) => {
                archive.get_id_by_hash(hash)?.index()
            }
            (crate::Archive::Tes4Bsa(archive), HashQuery::Tes4(folder, file)) => {
                archive.get_id_by_hash(folder, file)?.index()
            }
            _ => return None,
        };
        self.entry(index)
    }

    /// The entry `target` names: a handle of this archive or a path.
    fn target(&self, target: ValueView<'_>) -> Result<Entry> {
        if let Some(entry) = l3i::userdata::receiver::<Entry>(target) {
            if !Rc::ptr_eq(&entry.archive, &self.0) {
                return Err(Error::runtime(
                    "the entry handle belongs to a different archive",
                ));
            }
            return Ok(entry.clone());
        }
        if target.is_string() {
            let path = target.read::<&[u8]>()?;
            return self
                .get(path)
                .ok_or_else(|| archive_error(crate::Error::FileNotFound(path.into())));
        }
        Err(Error::runtime(format!(
            "expected an entry handle or an archive path, got {}",
            target.type_of().name()
        )))
    }

    /// Decodes `entry` into `out`, which must hold its whole payload.
    fn extract_into(entry: &Entry, out: &mut [u8]) -> Result<usize> {
        let size = entry.size()?;
        let available = u64::try_from(out.len()).unwrap_or(u64::MAX);
        if size > available {
            return Err(Error::runtime(format!(
                "readInto: the {size}-byte entry does not fit in the {available} bytes left in the buffer"
            )));
        }
        let mut sink = out;
        let written = match entry.row() {
            Row::Ba2(archive, entry) => archive
                .extract_entry(entry, &mut sink)
                .map_err(archive_error)?,
            Row::Tes3(archive, entry) => archive
                .extract_entry(entry, &mut sink)
                .map_err(archive_error)?,
            Row::Tes4(archive, entry) => archive
                .extract_entry(entry, &mut sink)
                .map_err(archive_error)?,
        };
        usize::try_from(written).map_err(|_| Error::runtime("readInto: payload size overflow"))
    }
}

enum HashQuery {
    Tes3(u64),
    Tes4(HashFields, HashFields),
    Ba2(crate::ba2::FileHash),
}

/// One entry of an opened archive (`dream.archive.Entry`): an index into the shared parsed
/// index, so cloning is two words.
#[derive(Clone)]
pub struct Entry {
    archive: Rc<Opened>,
    index: usize,
}

// SAFETY: as `Archive`.
unsafe impl Userdata for Entry {
    const NAME: &'static str = "dream.archive.Entry";
}

enum Row<'a> {
    Ba2(&'a crate::ba2::Archive, &'a crate::ba2::Entry),
    Tes3(&'a crate::bsa::tes3::Archive, &'a crate::bsa::tes3::Entry),
    Tes4(&'a crate::bsa::tes4::Archive, &'a crate::bsa::tes4::Entry),
}

impl Entry {
    /// The 0-based index in archive table order.
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /// The archive this entry belongs to.
    #[must_use]
    pub fn archive(&self) -> Archive {
        Archive(Rc::clone(&self.archive))
    }

    /// The entry as the top-level facade sees it.
    #[must_use]
    pub fn facade(&self) -> crate::Entry<'_> {
        match self.row() {
            Row::Ba2(_, entry) => crate::Entry::BA2(entry),
            Row::Tes3(_, entry) => crate::Entry::Tes3Bsa(entry),
            Row::Tes4(_, entry) => crate::Entry::Tes4Bsa(entry),
        }
    }

    fn row(&self) -> Row<'_> {
        // The handle was made from a bounds-checked index into an immutable table.
        match &self.archive.archive {
            crate::Archive::BA2(archive) => Row::Ba2(archive, &archive.entries()[self.index]),
            crate::Archive::Tes3Bsa(archive) => Row::Tes3(archive, &archive.entries()[self.index]),
            crate::Archive::Tes4Bsa(archive) => Row::Tes4(archive, &archive.entries()[self.index]),
        }
    }

    fn path(&self) -> Option<&[u8]> {
        self.facade().path().map(|path| path.as_bytes())
    }

    fn name(&self) -> Option<&[u8]> {
        match self.row() {
            Row::Ba2(_, entry) => (!entry.name().is_empty()).then(|| entry.name().as_bytes()),
            Row::Tes3(_, entry) => {
                let path = entry.path().as_bytes();
                let start = path
                    .iter()
                    .rposition(|byte| matches!(byte, b'/' | b'\\'))
                    .map_or(0, |pos| pos + 1);
                Some(&path[start..])
            }
            Row::Tes4(_, entry) => entry.name().map(|name| name.as_bytes()),
        }
    }

    fn folder(&self) -> Option<&[u8]> {
        match self.row() {
            Row::Ba2(_, entry) => {
                let name = entry.name().as_bytes();
                let end = name.iter().rposition(|byte| matches!(byte, b'/' | b'\\'))?;
                Some(&name[..end])
            }
            Row::Tes3(_, entry) => {
                let path = entry.path().as_bytes();
                let end = path.iter().rposition(|byte| matches!(byte, b'/' | b'\\'))?;
                Some(&path[..end])
            }
            Row::Tes4(_, entry) => entry.folder().map(|folder| folder.as_bytes()),
        }
    }

    /// The decoded payload size.
    fn size(&self) -> Result<u64> {
        match self.row() {
            Row::Ba2(archive, entry) => archive.extracted_len(entry).map_err(archive_error),
            Row::Tes3(_, entry) => Ok(u64::from(entry.file().size)),
            Row::Tes4(archive, entry) => archive.extracted_len(entry).map_err(archive_error),
        }
    }

    fn stored_size(&self) -> u64 {
        match self.row() {
            Row::Ba2(_, entry) => entry
                .file()
                .chunks()
                .iter()
                .map(|chunk| u64::from(chunk.stored_size()))
                .sum(),
            Row::Tes3(_, entry) => u64::from(entry.file().size),
            Row::Tes4(_, entry) => u64::from(entry.file().stored_size),
        }
    }

    fn offset(&self) -> Option<u64> {
        match self.row() {
            Row::Ba2(_, entry) => entry.file().chunks().first().map(crate::ba2::Chunk::offset),
            Row::Tes3(_, entry) => Some(u64::from(entry.file().offset)),
            Row::Tes4(_, entry) => Some(u64::from(entry.file().data_offset)),
        }
    }

    fn compressed(&self) -> bool {
        match self.row() {
            Row::Ba2(_, entry) => entry
                .file()
                .chunks()
                .iter()
                .any(crate::ba2::Chunk::is_compressed),
            Row::Tes3(..) => false,
            Row::Tes4(archive, entry) => entry.file().is_compressed(archive.info().archive_flags),
        }
    }

    fn hash(&self) -> Option<Bits64> {
        match self.row() {
            Row::Ba2(..) => None,
            Row::Tes3(_, entry) => Some(Bits64(entry.hash())),
            Row::Tes4(_, entry) => Some(Bits64(entry.file_hash().numeric())),
        }
    }

    fn folder_hash(&self) -> Option<Bits64> {
        match self.row() {
            Row::Tes4(_, entry) => Some(Bits64(entry.folder_hash().numeric())),
            _ => None,
        }
    }

    fn ba2_hash(&self) -> Option<crate::ba2::FileHash> {
        match self.row() {
            Row::Ba2(_, entry) => Some(entry.hash()),
            _ => None,
        }
    }
}

struct IndexField;

impl DirectField<Entry> for IndexField {
    fn get(entry: &Entry) -> FieldValue {
        // Indices are far below 2^53.
        #[allow(clippy::cast_precision_loss)]
        FieldValue::Number(entry.index as f64 + 1.0)
    }
}

/// The entries of an archive as a sequence view (`dream.archive.Entries`).
pub struct Entries(Rc<Opened>);

impl SequenceSource for Entries {
    const NAME: &'static str = "dream.archive.Entries";
    type Item = Owned<Entry>;

    fn len(&self) -> usize {
        self.0.archive.len()
    }

    fn get(&self, index: usize) -> Option<Owned<Entry>> {
        (index < self.0.archive.len()).then(|| {
            Owned(Entry {
                archive: Rc::clone(&self.0),
                index,
            })
        })
    }
}

// ---------------------------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct Ba2Builder(RefCell<crate::ba2::Builder>);

#[derive(Default)]
struct Ba2Dx10Builder(RefCell<crate::ba2::Dx10Builder>);

#[derive(Default)]
struct Tes3Builder(RefCell<crate::bsa::tes3::Builder>);

#[derive(Default)]
struct Tes4Builder(RefCell<crate::bsa::tes4::Builder>);

// SAFETY: builders are plain Rust data whose destructors never touch the Lua API.
unsafe impl Userdata for Ba2Builder {
    const NAME: &'static str = "dream.archive.Ba2Builder";
}
// SAFETY: as `Ba2Builder`.
unsafe impl Userdata for Ba2Dx10Builder {
    const NAME: &'static str = "dream.archive.Ba2Dx10Builder";
}
// SAFETY: as `Ba2Builder`.
unsafe impl Userdata for Tes3Builder {
    const NAME: &'static str = "dream.archive.Tes3Builder";
}
// SAFETY: as `Ba2Builder`.
unsafe impl Userdata for Tes4Builder {
    const NAME: &'static str = "dream.archive.Tes4Builder";
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

fn archive_error(error: impl std::fmt::Display) -> Error {
    Error::runtime(error.to_string())
}

fn format_name(format: FileFormat) -> &'static str {
    match format {
        FileFormat::BA2 => "ba2",
        FileFormat::BSA(BsaFormat::TES3) => "bsaTes3",
        FileFormat::BSA(BsaFormat::TES4) => "bsaTes4",
    }
}

fn u64_hex(value: u64) -> String {
    format!("{value:016x}")
}

/// A `u32` or `u64` as an exact Luau number (all fit in 53 bits, or are checked by the
/// converter when they do not).
fn number(value: impl Into<u64>) -> i64 {
    i64::try_from(value.into()).unwrap_or(i64::MAX)
}

fn compression_override(value: Option<&str>) -> Result<crate::CompressionOverride> {
    match value.unwrap_or("inherit") {
        "inherit" => Ok(crate::CompressionOverride::Inherit),
        "store" => Ok(crate::CompressionOverride::Store),
        "compress" => Ok(crate::CompressionOverride::Compress),
        other => Err(Error::runtime(format!(
            "unknown compression override: {other}"
        ))),
    }
}

fn zlib_level(level: Exact<u32>) -> Result<flate2::Compression> {
    if level.0 <= 9 {
        Ok(flate2::Compression::new(level.0))
    } else {
        Err(Error::runtime(format!(
            "zlib compression level must be 0..=9, got {}",
            level.0
        )))
    }
}

fn ba2_version(version: Exact<u32>) -> Result<crate::ba2::ArchiveVersion> {
    match version.0 {
        1 => Ok(crate::ba2::ArchiveVersion::v1),
        2 => Ok(crate::ba2::ArchiveVersion::v2),
        3 => Ok(crate::ba2::ArchiveVersion::v3),
        7 => Ok(crate::ba2::ArchiveVersion::v7),
        8 => Ok(crate::ba2::ArchiveVersion::v8),
        other => Err(Error::runtime(format!("unsupported BA2 version: {other}"))),
    }
}

fn ba2_compression(compression: Option<&str>) -> Result<Option<crate::ba2::Ba2CompressionFormat>> {
    match compression {
        None | Some("none" | "store") => Ok(None),
        Some("zip") => Ok(Some(crate::ba2::Ba2CompressionFormat::Zip)),
        Some("lz4") => Ok(Some(crate::ba2::Ba2CompressionFormat::LZ4)),
        Some(other) => Err(Error::runtime(format!("unknown BA2 compression: {other}"))),
    }
}

fn filename_encoding(encoding: &str) -> Result<crate::bsa::FilenameEncoding> {
    match encoding {
        "utf8" => Ok(crate::bsa::FilenameEncoding::Utf8),
        "windows1250" | "cp1250" => Ok(crate::bsa::FilenameEncoding::Windows1250),
        "windows1251" | "cp1251" => Ok(crate::bsa::FilenameEncoding::Windows1251),
        "windows1252" | "cp1252" => Ok(crate::bsa::FilenameEncoding::Windows1252),
        "cp437" => Ok(crate::bsa::FilenameEncoding::Cp437),
        other => Err(Error::runtime(format!(
            "unknown filename encoding: {other}"
        ))),
    }
}

fn tes4_version(version: Exact<u32>) -> Result<crate::bsa::tes4::ArchiveVersion> {
    match version.0 {
        103 => Ok(crate::bsa::tes4::ArchiveVersion::v103),
        104 => Ok(crate::bsa::tes4::ArchiveVersion::v104),
        105 => Ok(crate::bsa::tes4::ArchiveVersion::v105),
        other => Err(Error::runtime(format!(
            "unsupported TES4 BSA version: {other}"
        ))),
    }
}

fn name_mode(mode: &str) -> Result<crate::bsa::tes4::NameMode> {
    match mode {
        "strings" => Ok(crate::bsa::tes4::NameMode::Strings),
        "hashOnly" => Ok(crate::bsa::tes4::NameMode::HashOnly),
        "embedded" => Ok(crate::bsa::tes4::NameMode::Embedded),
        "stringsAndEmbedded" => Ok(crate::bsa::tes4::NameMode::StringsAndEmbedded),
        other => Err(Error::runtime(format!("unknown TES4 name mode: {other}"))),
    }
}

fn game_profile(profile: &str) -> Result<crate::bsa::tes4::GameProfile> {
    match profile {
        "oblivion" => Ok(crate::bsa::tes4::GameProfile::Oblivion),
        "fallout3" => Ok(crate::bsa::tes4::GameProfile::Fallout3),
        "falloutNewVegas" => Ok(crate::bsa::tes4::GameProfile::FalloutNewVegas),
        "skyrimLe" => Ok(crate::bsa::tes4::GameProfile::SkyrimLe),
        "skyrimSe" => Ok(crate::bsa::tes4::GameProfile::SkyrimSe),
        other => Err(Error::runtime(format!("unknown TES4 profile: {other}"))),
    }
}

fn entry_id<Id>(index: Exact<i64>, from_index: fn(usize) -> Id) -> Result<Id> {
    usize::try_from(index.0.wrapping_sub(1))
        .map(from_index)
        .map_err(|_| Error::runtime("entry index out of bounds"))
}

/// The bytes of a payload argument, handed to a builder that copies them before returning.
fn payload_bytes<R>(bytes: BytesView<'_>, body: impl FnOnce(&[u8]) -> R) -> R {
    // SAFETY: `body` is a builder call that copies the bytes into its own storage and returns
    // without calling into Lua; no other view of the buffer exists in this call.
    body(unsafe { bytes.bytes_unchecked() })
}

fn tes4_hash_table(scope: &impl Scope, hash: HashFields) -> Result<Table> {
    let table = Table::new(scope, 0, 7)?;
    table.set(scope, "last", &i64::from(hash.last))?;
    table.set(scope, "last2", &i64::from(hash.last2))?;
    table.set(scope, "length", &i64::from(hash.length))?;
    table.set(scope, "first", &i64::from(hash.first))?;
    table.set(scope, "crc", &i64::from(hash.crc))?;
    table.set(scope, "hex", &u64_hex(hash.numeric()))?;
    table.set(scope, "hash", &Bits64(hash.numeric()))?;
    Ok(table)
}

/// The string elements of a Luau array, in order, ignoring anything else in the table.
fn string_array(call: &Call<'_>, table: &Table) -> Result<Vec<Vec<u8>>> {
    call.with_frame(|frame| {
        let view = table.push_to(frame)?;
        let mut owned = Vec::with_capacity(view.raw_len());
        for index in 1..=i64::try_from(view.raw_len()).unwrap_or(i64::MAX) {
            frame.with_frame(|step| {
                let value = view.raw_get_index(step, index)?;
                if value.is_string() {
                    owned.push(value.read::<&[u8]>()?.to_vec());
                }
                Ok(())
            })?;
        }
        Ok(owned)
    })
}

/// The chunk of a Luau path an archive's `extractTo` writes.
fn extract_to(archive: &Archive, target: &str) -> Result<i64> {
    archive
        .archive()
        .extract_to(target)
        .map(number)
        .map_err(archive_error)
}

// ---------------------------------------------------------------------------------------------
// The extension
// ---------------------------------------------------------------------------------------------

impl Extension for ArchiveExtension {
    fn id(&self) -> &'static str {
        EXTENSION_ID
    }

    fn describe(&self, d: &mut ExtensionDescriptor) -> Result<()> {
        describe_archive(d);
        describe_entry(d);
        d.sequence::<Entries>(ENTRIES_KEY)
            .item_type("dream_archive_Entry")
            .tag(TagPolicy::Never)
            .doc("The entries of an archive: `#`, `[i]`, `for`, `:toTable()`.");
        describe_ba2_builders(d);
        describe_tes3_builder(d);
        describe_tes4_builder(d);
        describe_modules(d);
        Ok(())
    }

    /// The nested module tables (`ba2`, `bsa`, `bsa.tes3`, `bsa.tes4`), the `Builder.new`
    /// tables, and the enum tables exist only in a live VM.
    fn install(&self, cx: &mut InstallContext<'_>) -> Result<()> {
        let runtime = cx.runtime();
        let ba2_builder = constructor_table(runtime, "dream.archive.ba2.Builder.new", || {
            Owned(Ba2Builder::default())
        })?;
        let dx10_builder = constructor_table(runtime, "dream.archive.ba2.Dx10Builder.new", || {
            Owned(Ba2Dx10Builder::default())
        })?;
        let compression = enum_table(runtime, &["none", "store", "zip", "lz4"])?;
        let version = number_table(
            runtime,
            &[("V1", 1u8), ("V2", 2), ("V3", 3), ("V7", 7), ("V8", 8)],
        )?;
        let ba2 = cx.module(BA2_MODULE)?;
        ba2.set("Builder", &ba2_builder)?
            .set("Dx10Builder", &dx10_builder)?
            .set("compression", &compression)?
            .set("version", &version)?;
        let ba2 = ba2.table().clone();

        let tes3_builder =
            constructor_table(runtime, "dream.archive.bsa.tes3.Builder.new", || {
                Owned(Tes3Builder::default())
            })?;
        let tes3 = cx.module(TES3_MODULE)?;
        tes3.set("Builder", &tes3_builder)?;
        let tes3 = tes3.table().clone();

        let tes4_builder =
            constructor_table(runtime, "dream.archive.bsa.tes4.Builder.new", || {
                Owned(Tes4Builder::default())
            })?;
        let name_mode = enum_table(
            runtime,
            &["strings", "hashOnly", "embedded", "stringsAndEmbedded"],
        )?;
        let profile = enum_table(
            runtime,
            &[
                "oblivion",
                "fallout3",
                "falloutNewVegas",
                "skyrimLe",
                "skyrimSe",
            ],
        )?;
        let archive_types = number_table(
            runtime,
            &[
                ("MESHES", crate::bsa::tes4::ArchiveTypes::MESHES.bits()),
                ("TEXTURES", crate::bsa::tes4::ArchiveTypes::TEXTURES.bits()),
                ("MENUS", crate::bsa::tes4::ArchiveTypes::MENUS.bits()),
                ("SOUNDS", crate::bsa::tes4::ArchiveTypes::SOUNDS.bits()),
                ("VOICES", crate::bsa::tes4::ArchiveTypes::VOICES.bits()),
                ("SHADERS", crate::bsa::tes4::ArchiveTypes::SHADERS.bits()),
                ("TREES", crate::bsa::tes4::ArchiveTypes::TREES.bits()),
                ("FONTS", crate::bsa::tes4::ArchiveTypes::FONTS.bits()),
                ("MISC", crate::bsa::tes4::ArchiveTypes::MISC.bits()),
            ],
        )?;
        let tes4 = cx.module(TES4_MODULE)?;
        tes4.set("Builder", &tes4_builder)?
            .set("nameMode", &name_mode)?
            .set("profile", &profile)?
            .set("archiveTypes", &archive_types)?;
        let tes4 = tes4.table().clone();

        let encoding = enum_table(
            runtime,
            &["utf8", "windows1250", "windows1251", "windows1252", "cp437"],
        )?;
        let bsa = cx.module(BSA_MODULE)?;
        bsa.set("encoding", &encoding)?
            .set("tes3", &tes3)?
            .set("tes4", &tes4)?;
        let bsa = bsa.table().clone();

        cx.module(MODULE)?.set("ba2", &ba2)?.set("bsa", &bsa)?;
        Ok(())
    }
}

/// A frozen `{ new = <constructor> }` table.
fn constructor_table<F: l3i::bind::Binding<M>, M>(
    runtime: &Runtime,
    debug_name: &str,
    new: F,
) -> Result<Table> {
    let function = runtime.bind_function(debug_name, new)?;
    let table = Table::new(&runtime.stack(), 0, 1)?;
    table.set(&runtime.stack(), "new", &function)?;
    l3i::readonly::make_read_only(runtime, &table)?;
    Ok(table)
}

/// A frozen table mapping each name to itself.
fn enum_table(runtime: &Runtime, values: &[&str]) -> Result<Table> {
    let table = Table::new(&runtime.stack(), 0, values.len())?;
    for value in values {
        table.set(&runtime.stack(), value, *value)?;
    }
    l3i::readonly::make_read_only(runtime, &table)?;
    Ok(table)
}

/// A frozen table of named numbers.
fn number_table(runtime: &Runtime, values: &[(&str, impl Into<u64> + Copy)]) -> Result<Table> {
    let table = Table::new(&runtime.stack(), 0, values.len())?;
    for (name, value) in values {
        table.set(&runtime.stack(), name, &number(*value))?;
    }
    l3i::readonly::make_read_only(runtime, &table)?;
    Ok(table)
}

// One declaration per member reads best as one list, however long.
#[allow(clippy::too_many_lines)]
fn describe_archive(d: &mut ExtensionDescriptor) {
    let mut archive = d.userdata::<Archive>(ARCHIVE_KEY);
    archive
        .tag(TagPolicy::Preferred)
        .doc("An opened BA2, TES3 BSA, or TES4 BSA archive.");
    archive
        .method("format", |a: &Archive| a.format_name())
        .signature("(self): string")
        .doc("\"ba2\", \"bsaTes3\", or \"bsaTes4\".");
    archive
        .method("len", |a: &Archive| number(a.archive().len() as u64))
        .signature("(self): number");
    archive
        .method("isEmpty", |a: &Archive| a.archive().is_empty())
        .signature("(self): boolean");
    archive
        .method("archiveSize", |a: &Archive| {
            number(match a.archive() {
                crate::Archive::BA2(archive) => archive.archive_size(),
                crate::Archive::Tes3Bsa(archive) => archive.archive_size(),
                crate::Archive::Tes4Bsa(archive) => archive.archive_size(),
            } as u64)
        })
        .signature("(self): number");
    archive
        .method("entries", |a: &Archive| {
            Owned(Sequence(Entries(Rc::clone(&a.0))))
        })
        .signature("(self): dream_archive_Entries")
        .doc("A view of the entries; nothing is copied until a handle is read.");
    archive
        .method("entry", |a: &Archive, index: Exact<i64>| {
            usize::try_from(index.0.wrapping_sub(1))
                .ok()
                .and_then(|index| a.entry(index))
                .map(Owned)
        })
        .signature("(self, index: number): dream_archive_Entry?");
    archive
        .method("contains", |a: &Archive, path: &[u8]| a.contains(path))
        .signature("(self, path: string): boolean");
    archive
        .method("get", |a: &Archive, path: &[u8]| a.get(path).map(Owned))
        .signature("(self, path: string): dream_archive_Entry?");
    archive
        .method(
            "containsHash",
            |a: &Archive, first: ValueView, second: Option<ValueView>, third: Option<ValueView>| {
                let query = a.hash_query(first, second, third)?;
                Ok::<bool, Error>(a.get_by_hash(query).is_some())
            },
        )
        .signature(
            "(self, hash: integer | number, second: (integer | number)?, third: number?): boolean",
        )
        .doc("TES3: (hash); TES4: (folderHash, fileHash); BA2: (directory, file, extension).");
    archive
        .method(
            "getByHash",
            |a: &Archive, first: ValueView, second: Option<ValueView>, third: Option<ValueView>| {
                let query = a.hash_query(first, second, third)?;
                Ok::<Option<Owned<Entry>>, Error>(a.get_by_hash(query).map(Owned))
            },
        )
        .signature("(self, hash: integer | number, second: (integer | number)?, third: number?): dream_archive_Entry?");
    archive
        .method("readFile", |a: &Archive, path: &[u8]| {
            a.archive().read_file(path).map_err(archive_error)
        })
        .signature("(self, path: string): string?")
        .doc("The slow path: two copies. Prefer readInto.");
    archive
        .method("readFileRequired", |a: &Archive, path: &[u8]| {
            a.archive().read_file_required(path).map_err(archive_error)
        })
        .signature("(self, path: string): string");
    archive
        .method("extractFile", |a: &Archive, path: &[u8]| {
            let mut out = Vec::new();
            match a.archive().extract_file(path, &mut out) {
                Ok(Some(_)) => Ok(Some(out)),
                Ok(None) => Ok(None),
                Err(error) => Err(archive_error(error)),
            }
        })
        .signature("(self, path: string): string?");
    archive
        .method("extractFileRequired", |a: &Archive, path: &[u8]| {
            let mut out = Vec::new();
            a.archive()
                .extract_file_required(path, &mut out)
                .map_err(archive_error)?;
            Ok::<Vec<u8>, Error>(out)
        })
        .signature("(self, path: string): string");
    archive
        .method("readEntry", |a: &Archive, index: Exact<i64>| {
            let entry = a.handle(index)?;
            match entry.row() {
                Row::Ba2(archive, entry) => archive.read_entry(entry).map_err(archive_error),
                Row::Tes3(archive, entry) => archive.read_entry(entry).map_err(archive_error),
                Row::Tes4(archive, entry) => archive.read_entry(entry).map_err(archive_error),
            }
        })
        .signature("(self, index: number): string");
    archive
        .method("extractEntry", |a: &Archive, index: Exact<i64>| {
            let entry = a.handle(index)?;
            let mut out = Vec::new();
            match entry.row() {
                Row::Ba2(archive, entry) => archive
                    .extract_entry(entry, &mut out)
                    .map_err(archive_error),
                Row::Tes3(archive, entry) => archive
                    .extract_entry(entry, &mut out)
                    .map_err(archive_error),
                Row::Tes4(archive, entry) => archive
                    .extract_entry(entry, &mut out)
                    .map_err(archive_error),
            }?;
            Ok::<Vec<u8>, Error>(out)
        })
        .signature("(self, index: number): string");
    archive
        .method(
            "extractEntryToPath",
            |a: &Archive, index: Exact<i64>, path: &str| {
                let entry = a.handle(index)?;
                match entry.row() {
                    Row::Ba2(archive, entry) => archive
                        .extract_entry_to_path(entry, path)
                        .map_err(archive_error),
                    Row::Tes3(archive, entry) => archive
                        .extract_entry_to_path(entry, path)
                        .map_err(archive_error),
                    Row::Tes4(archive, entry) => archive
                        .extract_entry_to_path(entry, path)
                        .map_err(archive_error),
                }
                .map(number)
            },
        )
        .signature("(self, index: number, path: string): number");
    archive
        .method(
            "readInto",
            |a: &Archive, target: ValueView, mut buffer: BufferView, offset: Option<Exact<usize>>| {
                let entry = a.target(target)?;
                let offset = offset.map_or(0, |offset| offset.0);
                if offset > buffer.len() {
                    return Err(Error::runtime("buffer access out of bounds"));
                }
                // SAFETY: the decoder writes the slice and returns; it holds no Lua handle,
                // and no other view of this buffer exists in this call, so nothing reads or
                // writes the buffer through another path while the slice lives.
                let out = unsafe { buffer.bytes_mut_unchecked() };
                let written = Archive::extract_into(&entry, &mut out[offset..])?;
                Ok::<i64, Error>(number(written as u64))
            },
        )
        .signature("(self, entry: dream_archive_Entry | string, buffer: buffer, offset: number?): number")
        .doc("Decodes one entry straight into the buffer at offset (default 0); returns the byte count. Errors leave the buffer partially written.");
    archive
        .method("extractTo", |a: &Archive, target: &str| {
            extract_to(a, target)
        })
        .signature("(self, target: string): number");
    archive
        .method(
            "extractToWithEncoding",
            |a: &Archive, target: &str, encoding: &str| {
                let encoding = filename_encoding(encoding)?;
                match a.archive() {
                    crate::Archive::Tes3Bsa(archive) => archive
                        .extract_to_with_encoding(target, encoding)
                        .map(number)
                        .map_err(archive_error),
                    crate::Archive::Tes4Bsa(archive) => archive
                        .extract_to_with_encoding(target, encoding)
                        .map(number)
                        .map_err(archive_error),
                    crate::Archive::BA2(_) => Err(a.wrong_format("extractToWithEncoding", "BSA")),
                }
            },
        )
        .signature("(self, target: string, encoding: string): number")
        .doc("BSA only.");
    archive
        .method(
            "extractToWithPaths",
            |a: &Archive, call: &Call, target: &str, paths: Table| {
                let crate::Archive::Tes4Bsa(archive) = a.archive() else {
                    return Err(a.wrong_format("extractToWithPaths", "TES4 BSA"));
                };
                let paths = string_array(call, &paths)?;
                archive
                    .extract_to_with_paths(target, paths.iter().map(Vec::as_slice))
                    .map(number)
                    .map_err(archive_error)
            },
        )
        .signature("(self, target: string, paths: { string }): number")
        .doc("TES4 only: names hash-only entries from candidate paths.");
    archive
        .method("info", |a: &Archive, call: &Call| {
            let crate::Archive::BA2(archive) = a.archive() else {
                return Err(a.wrong_format("info", "BA2"));
            };
            let info = archive.info();
            let table = Table::new(call, 0, 4)?;
            table.set(
                call,
                "format",
                match info.format {
                    crate::ba2::PayloadFormat::GNRL => "gnrl",
                    crate::ba2::PayloadFormat::DX10 => "dx10",
                    crate::ba2::PayloadFormat::GNMF => "gnmf",
                },
            )?;
            table.set(call, "version", &(info.version as i64))?;
            table.set(
                call,
                "compression",
                match info.compression_format {
                    crate::ba2::Ba2CompressionFormat::Zip => "zip",
                    crate::ba2::Ba2CompressionFormat::LZ4 => "lz4",
                },
            )?;
            table.set(call, "strings", &info.strings)?;
            Ok(table)
        })
        .signature(
            "(self): { format: string, version: number, compression: string, strings: boolean }",
        )
        .doc("BA2 only.");
    archive.metamethod("__tostring", |a: &Archive| {
        format!(
            "dream.archive.Archive({}, {} entries, {})",
            a.format_name(),
            a.archive().len(),
            a.label()
        )
    });
}

fn describe_entry(d: &mut ExtensionDescriptor) {
    let mut entry = d.userdata::<Entry>(ENTRY_KEY);
    entry
        .tag(TagPolicy::Preferred)
        .doc("One entry of an archive; reads its metadata from the shared index.");
    entry
        .field::<IndexField>("index")
        .signature("number")
        .doc("1-based position in archive table order.");
    entry
        .field::<IndexField>("id")
        .signature("number")
        .doc("The stable id builders' addArchiveEntry takes; equals index.");
    entry
        .getter("format", |e: &Entry| format_name(e.facade().format()))
        .signature("string");
    entry
        .getter("path", |e: &Entry| e.path().map(<[u8]>::to_vec))
        .signature("string?")
        .doc("Raw archive path bytes; nil for hash-only entries.");
    entry
        .getter("name", |e: &Entry| e.name().map(<[u8]>::to_vec))
        .signature("string?");
    entry
        .getter("folder", |e: &Entry| e.folder().map(<[u8]>::to_vec))
        .signature("string?");
    entry
        .getter("size", |e: &Entry| e.size().ok().map(number))
        .signature("number?")
        .doc("Decoded payload size; nil when the format cannot tell (BA2 GNMF).");
    entry
        .getter("storedSize", |e: &Entry| number(e.stored_size()))
        .signature("number");
    entry
        .getter("offset", |e: &Entry| e.offset().map(number))
        .signature("number?");
    entry
        .getter("compressed", |e: &Entry| e.compressed())
        .signature("boolean");
    entry
        .getter("chunks", |e: &Entry| match e.row() {
            Row::Ba2(_, entry) => Some(number(entry.file().len() as u64)),
            _ => None,
        })
        .signature("number?");
    entry
        .getter("hash", |e: &Entry| e.hash())
        .signature("integer?")
        .doc("TES3: the entry hash; TES4: the file hash; nil for BA2.");
    entry
        .getter("hashHex", |e: &Entry| e.hash().map(|hash| u64_hex(hash.0)))
        .signature("string?");
    entry
        .getter("folderHash", |e: &Entry| e.folder_hash())
        .signature("integer?")
        .doc("TES4 only.");
    entry
        .getter("folderHashHex", |e: &Entry| {
            e.folder_hash().map(|hash| u64_hex(hash.0))
        })
        .signature("string?");
    entry
        .getter("directoryHash", |e: &Entry| {
            e.ba2_hash().map(|hash| i64::from(hash.directory))
        })
        .signature("number?")
        .doc("BA2 only.");
    entry
        .getter("fileHash", |e: &Entry| {
            e.ba2_hash().map(|hash| i64::from(hash.file))
        })
        .signature("number?")
        .doc("BA2 only.");
    entry
        .getter("extensionHash", |e: &Entry| {
            e.ba2_hash().map(|hash| i64::from(hash.extension))
        })
        .signature("number?")
        .doc("BA2 only.");
    entry.metamethod("__tostring", |e: &Entry| {
        format!(
            "dream.archive.Entry({}, {})",
            e.index + 1,
            e.path().map_or_else(
                || "<hash only>".to_owned(),
                |path| String::from_utf8_lossy(path).into_owned()
            )
        )
    });
    entry.metamethod("__eq", |a: &Entry, b: &Entry| {
        Rc::ptr_eq(&a.archive, &b.archive) && a.index == b.index
    });
}

#[allow(clippy::too_many_lines)]
fn describe_ba2_builders(d: &mut ExtensionDescriptor) {
    let mut builder = d.userdata::<Ba2Builder>("dream.archive.Ba2Builder");
    builder
        .tag(TagPolicy::Never)
        .doc("Builds a general (GNRL) BA2 archive.");
    builder
        .method("len", |b: &Ba2Builder| number(b.0.borrow().len() as u64))
        .signature("(self): number");
    builder
        .method("isEmpty", |b: &Ba2Builder| b.0.borrow().is_empty())
        .signature("(self): boolean");
    builder
        .method("setVersion", |b: &Ba2Builder, version: Exact<u32>| {
            b.0.borrow_mut().set_version(ba2_version(version)?);
            Ok::<(), Error>(())
        })
        .signature("(self, version: number)");
    builder
        .method(
            "setCompression",
            |b: &Ba2Builder, compression: Option<&str>| {
                b.0.borrow_mut()
                    .set_compression(ba2_compression(compression)?);
                Ok::<(), Error>(())
            },
        )
        .signature("(self, compression: string?)");
    builder
        .method("setZlibLevel", |b: &Ba2Builder, level: Exact<u32>| {
            b.0.borrow_mut().set_zlib_level(zlib_level(level)?);
            Ok::<(), Error>(())
        })
        .signature("(self, level: number)");
    builder
        .method(
            "addBytes",
            |b: &Ba2Builder, path: &[u8], bytes: BytesView| {
                payload_bytes(bytes, |bytes| b.0.borrow_mut().add_bytes(path, bytes))
                    .map_err(archive_error)
            },
        )
        .signature("(self, path: string, bytes: buffer | string)");
    builder
        .method(
            "addBytesWithCompression",
            |b: &Ba2Builder, path: &[u8], bytes: BytesView, compression: Option<&str>| {
                let compression = compression_override(compression)?;
                payload_bytes(bytes, |bytes| {
                    b.0.borrow_mut()
                        .add_bytes_with_compression(path, bytes, compression)
                })
                .map_err(archive_error)
            },
        )
        .signature("(self, path: string, bytes: buffer | string, compression: string?)");
    builder
        .method("addFile", |b: &Ba2Builder, path: &[u8], source: &str| {
            b.0.borrow_mut()
                .add_file(path, source)
                .map_err(archive_error)
        })
        .signature("(self, archivePath: string, source: string)");
    builder
        .method(
            "addArchiveEntry",
            |b: &Ba2Builder, path: &[u8], archive: &Archive, index: Exact<i64>| {
                let shared = archive.shared_ba2()?;
                let id = entry_id(index, crate::ba2::EntryId::from_index)?;
                b.0.borrow_mut()
                    .add_archive_entry(path, shared, id)
                    .map_err(archive_error)
            },
        )
        .signature("(self, archivePath: string, archive: dream_archive_Archive, id: number)");
    builder
        .method(
            "addArchiveEntryWithCompression",
            |b: &Ba2Builder, path: &[u8], archive: &Archive, index: Exact<i64>, compression: Option<&str>| {
                let shared = archive.shared_ba2()?;
                let id = entry_id(index, crate::ba2::EntryId::from_index)?;
                let compression = compression_override(compression)?;
                b.0.borrow_mut()
                    .add_archive_entry_with_compression(path, shared, id, compression)
                    .map_err(archive_error)
            },
        )
        .signature("(self, archivePath: string, archive: dream_archive_Archive, id: number, compression: string?)");
    builder
        .method("addDir", |b: &Ba2Builder, root: &str| {
            b.0.borrow_mut().add_dir(root).map_err(archive_error)
        })
        .signature("(self, root: string)");
    builder
        .method("writePath", |b: &Ba2Builder, path: &str| {
            b.0.borrow().write_path(path).map_err(archive_error)
        })
        .signature("(self, path: string)");
    builder
        .method("toBytes", |b: &Ba2Builder| {
            b.0.borrow().to_vec().map_err(archive_error)
        })
        .signature("(self): string");
    builder
        .method("toString", |b: &Ba2Builder| {
            b.0.borrow().to_vec().map_err(archive_error)
        })
        .signature("(self): string");
    builder
        .method("toBuffer", |b: &Ba2Builder, call: &Call| {
            let bytes = b.0.borrow().to_vec().map_err(archive_error)?;
            let buffer = l3i::convert::new_buffer(call, bytes.len())?;
            buffer.write(0, &bytes)?;
            l3i::value::Value::store(call.top_value())
        })
        .signature("(self): buffer");

    let mut dx10 = d.userdata::<Ba2Dx10Builder>("dream.archive.Ba2Dx10Builder");
    dx10.tag(TagPolicy::Never)
        .doc("Builds a texture (DX10) BA2 archive.");
    dx10.method(
        "len",
        |b: &Ba2Dx10Builder| number(b.0.borrow().len() as u64),
    )
    .signature("(self): number");
    dx10.method("isEmpty", |b: &Ba2Dx10Builder| b.0.borrow().is_empty())
        .signature("(self): boolean");
    dx10.method("setVersion", |b: &Ba2Dx10Builder, version: Exact<u32>| {
        b.0.borrow_mut().set_version(ba2_version(version)?);
        Ok::<(), Error>(())
    })
    .signature("(self, version: number)");
    dx10.method(
        "setCompression",
        |b: &Ba2Dx10Builder, compression: Option<&str>| {
            b.0.borrow_mut()
                .set_compression(ba2_compression(compression)?);
            Ok::<(), Error>(())
        },
    )
    .signature("(self, compression: string?)");
    dx10.method("setZlibLevel", |b: &Ba2Dx10Builder, level: Exact<u32>| {
        b.0.borrow_mut().set_zlib_level(zlib_level(level)?);
        Ok::<(), Error>(())
    })
    .signature("(self, level: number)");
    dx10.method(
        "addDdsBytes",
        |b: &Ba2Dx10Builder, path: &[u8], dds: BytesView| {
            payload_bytes(dds, |dds| b.0.borrow_mut().add_dds_bytes(path, dds))
                .map_err(archive_error)
        },
    )
    .signature("(self, path: string, dds: buffer | string)");
    dx10.method(
        "addTextureBytes",
        |b: &Ba2Dx10Builder, call: &Call, path: &[u8], header: ValueView, bytes: BytesView| {
            let header = Options::read(call, header, "Dx10Builder:addTextureBytes", |o| {
                Ok(crate::ba2::TextureHeader {
                    height: o.required::<Exact<u16>>("height")?.0,
                    width: o.required::<Exact<u16>>("width")?.0,
                    mip_count: o.required::<Exact<u8>>("mipCount")?.0,
                    format: o.required::<Exact<u8>>("format")?.0,
                    flags: o.required::<Exact<u8>>("flags")?.0,
                    tile_mode: o.required::<Exact<u8>>("tileMode")?.0,
                })
            })?;
            payload_bytes(bytes, |bytes| {
                b.0.borrow_mut().add_texture_bytes(path, header, bytes)
            })
            .map_err(archive_error)
        },
    )
    .signature("(self, path: string, header: { height: number, width: number, mipCount: number, format: number, flags: number, tileMode: number }, bytes: buffer | string)");
    dx10.method(
        "addDdsFile",
        |b: &Ba2Dx10Builder, path: &[u8], source: &str| {
            b.0.borrow_mut()
                .add_dds_file(path, source)
                .map_err(archive_error)
        },
    )
    .signature("(self, archivePath: string, source: string)");
    dx10.method(
        "addArchiveEntry",
        |b: &Ba2Dx10Builder, path: &[u8], archive: &Archive, index: Exact<i64>| {
            let shared = archive.shared_ba2()?;
            let id = entry_id(index, crate::ba2::EntryId::from_index)?;
            b.0.borrow_mut()
                .add_archive_entry(path, shared, id)
                .map_err(archive_error)
        },
    )
    .signature("(self, archivePath: string, archive: dream_archive_Archive, id: number)");
    dx10.method("writePath", |b: &Ba2Dx10Builder, path: &str| {
        b.0.borrow().write_path(path).map_err(archive_error)
    })
    .signature("(self, path: string)");
    dx10.method("toBytes", |b: &Ba2Dx10Builder| {
        b.0.borrow().to_vec().map_err(archive_error)
    })
    .signature("(self): string");
    dx10.method("toString", |b: &Ba2Dx10Builder| {
        b.0.borrow().to_vec().map_err(archive_error)
    })
    .signature("(self): string");
}

fn describe_tes3_builder(d: &mut ExtensionDescriptor) {
    let mut builder = d.userdata::<Tes3Builder>("dream.archive.Tes3Builder");
    builder
        .tag(TagPolicy::Never)
        .doc("Builds a TES3 (Morrowind) BSA archive.");
    builder
        .method("len", |b: &Tes3Builder| number(b.0.borrow().len() as u64))
        .signature("(self): number");
    builder
        .method("isEmpty", |b: &Tes3Builder| b.0.borrow().is_empty())
        .signature("(self): boolean");
    builder
        .method(
            "addBytes",
            |b: &Tes3Builder, path: &[u8], bytes: BytesView| {
                payload_bytes(bytes, |bytes| b.0.borrow_mut().add_bytes(path, bytes))
                    .map_err(archive_error)
            },
        )
        .signature("(self, path: string, bytes: buffer | string)");
    builder
        .method(
            "addEncodedPath",
            |b: &Tes3Builder, path: &str, encoding: &str, bytes: BytesView| {
                let encoding = filename_encoding(encoding)?;
                payload_bytes(bytes, |bytes| {
                    b.0.borrow_mut().add_encoded_path(path, encoding, bytes)
                })
                .map_err(archive_error)
            },
        )
        .signature("(self, path: string, encoding: string, bytes: buffer | string)")
        .doc("Encodes the UTF-8 path to the archive's legacy filename bytes.");
    builder
        .method("addFile", |b: &Tes3Builder, path: &[u8], source: &str| {
            b.0.borrow_mut()
                .add_file(path, source)
                .map_err(archive_error)
        })
        .signature("(self, archivePath: string, source: string)");
    builder
        .method(
            "addArchiveEntry",
            |b: &Tes3Builder, path: &[u8], archive: &Archive, index: Exact<i64>| {
                let shared = archive.shared_tes3()?;
                let id = entry_id(index, crate::bsa::tes3::EntryId::from_index)?;
                b.0.borrow_mut()
                    .add_archive_entry(path, shared, id)
                    .map_err(archive_error)
            },
        )
        .signature("(self, archivePath: string, archive: dream_archive_Archive, id: number)");
    builder
        .method("addDir", |b: &Tes3Builder, root: &str| {
            b.0.borrow_mut().add_dir(root).map_err(archive_error)
        })
        .signature("(self, root: string)");
    builder
        .method("writePath", |b: &Tes3Builder, path: &str| {
            b.0.borrow().write_path(path).map_err(archive_error)
        })
        .signature("(self, path: string)");
    builder
        .method("toBytes", |b: &Tes3Builder| {
            b.0.borrow().to_vec().map_err(archive_error)
        })
        .signature("(self): string");
    builder
        .method("toString", |b: &Tes3Builder| {
            b.0.borrow().to_vec().map_err(archive_error)
        })
        .signature("(self): string");
}

#[allow(clippy::too_many_lines)]
fn describe_tes4_builder(d: &mut ExtensionDescriptor) {
    let mut builder = d.userdata::<Tes4Builder>("dream.archive.Tes4Builder");
    builder
        .tag(TagPolicy::Never)
        .doc("Builds a TES4-family (Oblivion to Skyrim SE) BSA archive.");
    builder
        .method("len", |b: &Tes4Builder| number(b.0.borrow().len() as u64))
        .signature("(self): number");
    builder
        .method("isEmpty", |b: &Tes4Builder| b.0.borrow().is_empty())
        .signature("(self): boolean");
    builder
        .method("setVersion", |b: &Tes4Builder, version: Exact<u32>| {
            b.0.borrow_mut().set_version(tes4_version(version)?);
            Ok::<(), Error>(())
        })
        .signature("(self, version: number)")
        .doc("103, 104, or 105.");
    builder
        .method("setProfile", |b: &Tes4Builder, profile: &str| {
            b.0.borrow_mut().set_profile(game_profile(profile)?);
            Ok::<(), Error>(())
        })
        .signature("(self, profile: string)");
    builder
        .method("setArchiveTypes", |b: &Tes4Builder, bits: Exact<u16>| {
            b.0.borrow_mut()
                .set_archive_types(crate::bsa::tes4::ArchiveTypes::from_bits_retain(bits.0));
        })
        .signature("(self, bits: number)");
    builder
        .method("setCompressed", |b: &Tes4Builder, compressed: bool| {
            b.0.borrow_mut().set_compressed(compressed);
        })
        .signature("(self, compressed: boolean)");
    builder
        .method("setNameMode", |b: &Tes4Builder, mode: &str| {
            b.0.borrow_mut().set_name_mode(name_mode(mode)?);
            Ok::<(), Error>(())
        })
        .signature("(self, mode: string)");
    builder
        .method("setZlibLevel", |b: &Tes4Builder, level: Exact<u32>| {
            b.0.borrow_mut().set_zlib_level(zlib_level(level)?);
            Ok::<(), Error>(())
        })
        .signature("(self, level: number)");
    builder
        .method(
            "addBytes",
            |b: &Tes4Builder, path: &[u8], bytes: BytesView| {
                payload_bytes(bytes, |bytes| b.0.borrow_mut().add_bytes(path, bytes))
                    .map_err(archive_error)
            },
        )
        .signature("(self, path: string, bytes: buffer | string)");
    builder
        .method(
            "addBytesWithCompression",
            |b: &Tes4Builder, path: &[u8], bytes: BytesView, compression: Option<&str>| {
                let compression = compression_override(compression)?;
                payload_bytes(bytes, |bytes| {
                    b.0.borrow_mut()
                        .add_bytes_with_compression(path, bytes, compression)
                })
                .map_err(archive_error)
            },
        )
        .signature("(self, path: string, bytes: buffer | string, compression: string?)");
    builder
        .method(
            "addEncodedPath",
            |b: &Tes4Builder, path: &str, encoding: &str, bytes: BytesView| {
                let encoding = filename_encoding(encoding)?;
                payload_bytes(bytes, |bytes| {
                    b.0.borrow_mut().add_encoded_path(path, encoding, bytes)
                })
                .map_err(archive_error)
            },
        )
        .signature("(self, path: string, encoding: string, bytes: buffer | string)");
    builder
        .method("addFile", |b: &Tes4Builder, path: &[u8], source: &str| {
            b.0.borrow_mut()
                .add_file(path, source)
                .map_err(archive_error)
        })
        .signature("(self, archivePath: string, source: string)");
    builder
        .method(
            "addArchiveEntry",
            |b: &Tes4Builder, path: &[u8], archive: &Archive, index: Exact<i64>| {
                let shared = archive.shared_tes4()?;
                let id = entry_id(index, crate::bsa::tes4::EntryId::from_index)?;
                b.0.borrow_mut()
                    .add_archive_entry(path, shared, id)
                    .map_err(archive_error)
            },
        )
        .signature("(self, archivePath: string, archive: dream_archive_Archive, id: number)");
    builder
        .method(
            "addArchiveEntryWithCompression",
            |b: &Tes4Builder, path: &[u8], archive: &Archive, index: Exact<i64>, compression: Option<&str>| {
                let shared = archive.shared_tes4()?;
                let id = entry_id(index, crate::bsa::tes4::EntryId::from_index)?;
                let compression = compression_override(compression)?;
                b.0.borrow_mut()
                    .add_archive_entry_with_compression(path, shared, id, compression)
                    .map_err(archive_error)
            },
        )
        .signature("(self, archivePath: string, archive: dream_archive_Archive, id: number, compression: string?)");
    builder
        .method("addDir", |b: &Tes4Builder, root: &str| {
            b.0.borrow_mut().add_dir(root).map_err(archive_error)
        })
        .signature("(self, root: string)");
    builder
        .method("writePath", |b: &Tes4Builder, path: &str| {
            b.0.borrow().write_path(path).map_err(archive_error)
        })
        .signature("(self, path: string)");
    builder
        .method("toBytes", |b: &Tes4Builder| {
            b.0.borrow().to_vec().map_err(archive_error)
        })
        .signature("(self): string");
    builder
        .method("toString", |b: &Tes4Builder| {
            b.0.borrow().to_vec().map_err(archive_error)
        })
        .signature("(self): string");
}

fn open_path(path: &str) -> Result<Owned<Archive>> {
    let archive = crate::Archive::open_path(path).map_err(archive_error)?;
    Ok(Owned(Archive::new(archive, Some(PathBuf::from(path)))))
}

fn open_bytes(bytes: BytesView<'_>) -> Result<Owned<Archive>> {
    // The parser copies into its own storage: one copy either way.
    let archive = crate::Archive::from_vec(bytes.to_vec()).map_err(archive_error)?;
    Ok(Owned(Archive::new(archive, None)))
}

fn open_as(
    bytes: Option<&[u8]>,
    path: Option<&str>,
    expected: FileFormat,
) -> Result<Owned<Archive>> {
    let archive = match (expected, bytes, path) {
        (FileFormat::BA2, Some(bytes), _) => {
            crate::Archive::BA2(crate::ba2::Archive::from_slice(bytes).map_err(archive_error)?)
        }
        (FileFormat::BA2, _, Some(path)) => {
            crate::Archive::BA2(crate::ba2::Archive::open_path(path).map_err(archive_error)?)
        }
        (FileFormat::BSA(BsaFormat::TES3), Some(bytes), _) => crate::Archive::Tes3Bsa(
            crate::bsa::tes3::Archive::from_slice(bytes).map_err(archive_error)?,
        ),
        (FileFormat::BSA(BsaFormat::TES3), _, Some(path)) => crate::Archive::Tes3Bsa(
            crate::bsa::tes3::Archive::open_path(path).map_err(archive_error)?,
        ),
        (FileFormat::BSA(BsaFormat::TES4), Some(bytes), _) => crate::Archive::Tes4Bsa(
            crate::bsa::tes4::Archive::from_slice(bytes).map_err(archive_error)?,
        ),
        (FileFormat::BSA(BsaFormat::TES4), _, Some(path)) => crate::Archive::Tes4Bsa(
            crate::bsa::tes4::Archive::open_path(path).map_err(archive_error)?,
        ),
        _ => return Err(Error::logic("open_as needs bytes or a path")),
    };
    Ok(Owned(Archive::new(archive, path.map(PathBuf::from))))
}

fn open_format_bytes(bytes: BytesView<'_>, expected: FileFormat) -> Result<Owned<Archive>> {
    // SAFETY: the parser copies the slice into its own storage before returning and holds no
    // Lua handle; no other view of the buffer exists in this call.
    open_as(Some(unsafe { bytes.bytes_unchecked() }), None, expected)
}

fn detect_path(path: &str) -> Result<Option<&'static str>> {
    match crate::detect_path(path) {
        Ok(format) => Ok(format.map(format_name)),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
        Err(error) => Err(archive_error(error)),
    }
}

fn guess_format(bytes: BytesView<'_>) -> Option<&'static str> {
    let mut magic = [0u8; 4];
    bytes.read(0, &mut magic).ok()?;
    crate::guess_format(&mut std::io::Cursor::new(magic))
        .ok()
        .flatten()
        .map(format_name)
}

// The Luau types of the module members, spelled once: each nested module's table type is the
// signature of the member that embeds it, so `dreamArchive.bsa.tes3.openBytes` is typed all the
// way down (the planner's generated `Module_*` aliases cannot be named from a signature).
const OPEN_PATH: &str = "(path: string) -> dream_archive_Archive";
const OPEN_BYTES: &str = "(bytes: buffer | string) -> dream_archive_Archive";
const NORMALIZE_PATH: &str = "(path: string) -> string";
const BA2_HASH_FILE: &str =
    "(path: string) -> { directory: number, file: number, extension: number, normalized: string }";
const BA2_BUILDER: &str = "{ new: () -> dream_archive_Ba2Builder }";
const BA2_DX10_BUILDER: &str = "{ new: () -> dream_archive_Ba2Dx10Builder }";
const BA2_COMPRESSION: &str = "{ none: \"none\", store: \"store\", zip: \"zip\", lz4: \"lz4\" }";
const BA2_VERSION: &str = "{ V1: number, V2: number, V3: number, V7: number, V8: number }";
const TES3_HASH_FILE: &str =
    "(path: string) -> { lo: number, hi: number, hex: string, hash: integer, normalized: string }";
const TES3_BUILDER: &str = "{ new: () -> dream_archive_Tes3Builder }";
const TES4_HASH: &str = "(path: string) -> { last: number, last2: number, length: number, first: number, crc: number, hex: string, hash: integer }";
const TES4_BUILDER: &str = "{ new: () -> dream_archive_Tes4Builder }";
const TES4_NAME_MODE: &str = "{ strings: \"strings\", hashOnly: \"hashOnly\", embedded: \"embedded\", stringsAndEmbedded: \"stringsAndEmbedded\" }";
const TES4_PROFILE: &str = "{ oblivion: \"oblivion\", fallout3: \"fallout3\", falloutNewVegas: \"falloutNewVegas\", skyrimLe: \"skyrimLe\", skyrimSe: \"skyrimSe\" }";
const TES4_ARCHIVE_TYPES: &str = "{ MESHES: number, TEXTURES: number, MENUS: number, SOUNDS: number, VOICES: number, SHADERS: number, TREES: number, FONTS: number, MISC: number }";
const ENCODE_FILENAME: &str = "(text: string, encoding: string) -> string";
const DECODE_FILENAME: &str = "(bytes: string, encoding: string) -> string";
const BSA_ENCODING: &str = "{ utf8: \"utf8\", windows1250: \"windows1250\", windows1251: \"windows1251\", windows1252: \"windows1252\", cp437: \"cp437\" }";

/// The generated module types the definitions declare (`Module_` plus the module path with
/// every other character as `_`); a member may reference a module declared later in the plan.
const BA2_MODULE_TYPE: &str = "Module__dream_archive_ba2";
const BSA_MODULE_TYPE: &str = "Module__dream_archive_bsa";
const TES3_MODULE_TYPE: &str = "Module__dream_archive_bsa_tes3";
const TES4_MODULE_TYPE: &str = "Module__dream_archive_bsa_tes4";

#[allow(clippy::too_many_lines)]
fn describe_modules(d: &mut ExtensionDescriptor) {
    d.module(MODULE)
        .doc("Bethesda BA2 and BSA archives: open, list, look up, read, extract, and build.")
        .function("openPath", open_path)
        .signature(OPEN_PATH)
        .function("openBytes", open_bytes)
        .signature(OPEN_BYTES)
        .function("detectPath", detect_path)
        .signature("(path: string) -> string?")
        .doc("nil for unknown or too-short headers.")
        .function("guessFormat", guess_format)
        .signature("(bytes: buffer | string) -> string?")
        .function("normalizePath", |path: &[u8]| {
            dream_path::normalize_path(path)
        })
        .signature(NORMALIZE_PATH)
        .installed("ba2")
        .signature(BA2_MODULE_TYPE)
        .doc("The @dream/archive/ba2 module.")
        .installed("bsa")
        .signature(BSA_MODULE_TYPE)
        .doc("The @dream/archive/bsa module.");

    d.module(BA2_MODULE)
        .doc("BA2 archives: format-specific opening, hashing, and builders.")
        .function("openPath", |path: &str| {
            open_as(None, Some(path), FileFormat::BA2)
        })
        .signature(OPEN_PATH)
        .function("openBytes", |bytes: BytesView| {
            open_format_bytes(bytes, FileFormat::BA2)
        })
        .signature(OPEN_BYTES)
        .function("hashFile", |call: &Call, path: &[u8]| {
            let (hash, normalized) = crate::ba2::hash_file(path.as_bstr());
            let table = Table::new(call, 0, 4)?;
            table.set(call, "directory", &i64::from(hash.directory))?;
            table.set(call, "file", &i64::from(hash.file))?;
            table.set(call, "extension", &i64::from(hash.extension))?;
            table.set(call, "normalized", normalized.as_slice())?;
            Ok::<Table, Error>(table)
        })
        .signature(BA2_HASH_FILE)
        .installed("Builder")
        .signature(BA2_BUILDER)
        .installed("Dx10Builder")
        .signature(BA2_DX10_BUILDER)
        .installed("compression")
        .signature(BA2_COMPRESSION)
        .installed("version")
        .signature(BA2_VERSION);

    d.module(BSA_MODULE)
        .doc("BSA archives: filename encodings and the TES3 / TES4 modules.")
        .function("encodeFilename", |path: &str, encoding: &str| {
            crate::bsa::encode_filename(path, filename_encoding(encoding)?)
                .map(|encoded| encoded.as_ref().to_vec())
                .map_err(archive_error)
        })
        .signature(ENCODE_FILENAME)
        .function("decodeFilenameLossy", |bytes: &[u8], encoding: &str| {
            Ok::<String, Error>(
                crate::bsa::decode_filename_lossy(bytes, filename_encoding(encoding)?).into_owned(),
            )
        })
        .signature(DECODE_FILENAME)
        .function("normalizePath", |path: &[u8]| {
            dream_path::normalize_path(path)
        })
        .signature(NORMALIZE_PATH)
        .installed("encoding")
        .signature(BSA_ENCODING)
        .installed("tes3")
        .signature(TES3_MODULE_TYPE)
        .doc("The @dream/archive/bsa/tes3 module.")
        .installed("tes4")
        .signature(TES4_MODULE_TYPE)
        .doc("The @dream/archive/bsa/tes4 module.");

    d.module(TES3_MODULE)
        .doc("TES3 (Morrowind) BSA archives.")
        .function("openPath", |path: &str| {
            open_as(None, Some(path), FileFormat::BSA(BsaFormat::TES3))
        })
        .signature(OPEN_PATH)
        .function("openBytes", |bytes: BytesView| {
            open_format_bytes(bytes, FileFormat::BSA(BsaFormat::TES3))
        })
        .signature(OPEN_BYTES)
        .function("hashFile", |call: &Call, path: &[u8]| {
            let (hash, normalized) = crate::bsa::tes3::hash_file(path);
            let table = Table::new(call, 0, 5)?;
            table.set(call, "lo", &i64::from(hash.lo))?;
            table.set(call, "hi", &i64::from(hash.hi))?;
            table.set(call, "hex", &u64_hex(hash.numeric()))?;
            table.set(call, "hash", &Bits64(hash.numeric()))?;
            table.set(call, "normalized", normalized.as_slice())?;
            Ok::<Table, Error>(table)
        })
        .signature(TES3_HASH_FILE)
        .installed("Builder")
        .signature(TES3_BUILDER);

    d.module(TES4_MODULE)
        .doc("TES4-family (Oblivion to Skyrim SE) BSA archives.")
        .function("openPath", |path: &str| {
            open_as(None, Some(path), FileFormat::BSA(BsaFormat::TES4))
        })
        .signature(OPEN_PATH)
        .function("openBytes", |bytes: BytesView| {
            open_format_bytes(bytes, FileFormat::BSA(BsaFormat::TES4))
        })
        .signature(OPEN_BYTES)
        .function("hashDirectory", |call: &Call, path: &[u8]| {
            tes4_hash_table(call, crate::bsa::tes4::hash_directory(path).0)
        })
        .signature(TES4_HASH)
        .function("hashFile", |call: &Call, path: &[u8]| {
            tes4_hash_table(call, crate::bsa::tes4::hash_file(path).0)
        })
        .signature(TES4_HASH)
        .installed("Builder")
        .signature(TES4_BUILDER)
        .installed("nameMode")
        .signature(TES4_NAME_MODE)
        .installed("profile")
        .signature(TES4_PROFILE)
        .installed("archiveTypes")
        .signature(TES4_ARCHIVE_TYPES);
}
