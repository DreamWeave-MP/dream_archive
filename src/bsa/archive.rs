use super::{Error, Result, parser};
use crate::{Copied, storage::Storage};
use bstr::{BStr, BString};
use flate2::read::ZlibDecoder;
use std::io::Read as _;
use std::path::Path;

/// Metadata read from a TES4-family BSA archive header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveInfo {
    pub version: ArchiveVersion,
    pub folder_record_offset: u32,
    pub archive_flags: ArchiveFlags,
    pub folder_count: u32,
    pub file_count: u32,
    pub folder_names_len: u32,
    pub file_names_len: u32,
    pub archive_types: ArchiveTypes,
}

/// One file entry in a TES4-family BSA archive index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    path: BString,
    folder: BString,
    name: BString,
    record: FileRecord,
}

impl Entry {
    #[must_use]
    pub fn path(&self) -> &BStr {
        self.path.as_ref()
    }

    #[must_use]
    pub fn folder(&self) -> &BStr {
        self.folder.as_ref()
    }

    #[must_use]
    pub fn name(&self) -> &BStr {
        self.name.as_ref()
    }

    #[must_use]
    pub fn file(&self) -> FileRecord {
        self.record
    }
}

/// Raw file location metadata from a TES4-family BSA file record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileRecord {
    pub stored_size: u32,
    pub data_offset: u32,
    pub compression_toggled: bool,
}

impl FileRecord {
    #[must_use]
    pub fn is_compressed(self, archive_flags: ArchiveFlags) -> bool {
        archive_flags.contains(ArchiveFlags::COMPRESSED) ^ self.compression_toggled
    }
}

/// TES4-family BSA archive version.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ArchiveVersion {
    #[default]
    v103 = 103,
    v104 = 104,
    v105 = 105,
}

/// TES4-family BSA archive flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArchiveFlags(u32);

impl ArchiveFlags {
    pub const DIRECTORY_STRINGS: Self = Self(1 << 0);
    pub const FILE_STRINGS: Self = Self(1 << 1);
    pub const COMPRESSED: Self = Self(1 << 2);
    pub const XBOX_ARCHIVE: Self = Self(1 << 6);
    pub const EMBEDDED_FILE_NAMES: Self = Self(1 << 8);
    pub const XBOX_COMPRESSED: Self = Self(1 << 9);

    #[must_use]
    pub const fn from_bits_truncate(bits: u32) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// TES4-family BSA archive content-type flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArchiveTypes(u16);

impl ArchiveTypes {
    pub const MESHES: Self = Self(1 << 0);
    pub const TEXTURES: Self = Self(1 << 1);
    pub const MENUS: Self = Self(1 << 2);
    pub const SOUNDS: Self = Self(1 << 3);
    pub const VOICES: Self = Self(1 << 4);
    pub const SHADERS: Self = Self(1 << 5);
    pub const TREES: Self = Self(1 << 6);
    pub const FONTS: Self = Self(1 << 7);
    pub const MISC: Self = Self(1 << 8);

    #[must_use]
    pub const fn from_bits_truncate(bits: u16) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Parsed TES4-family BSA archive header.
#[derive(Clone, Debug)]
pub struct Archive {
    storage: Storage,
    info: ArchiveInfo,
    entries: Vec<Entry>,
}

impl Archive {
    /// Read an archive from an owned byte buffer.
    ///
    /// # Errors
    ///
    /// Returns an error when the buffer is not a supported TES4-family BSA archive.
    pub fn from_vec(bytes: Vec<u8>) -> Result<Self> {
        parser::parse(Storage::from_vec(bytes))
    }

    /// Read an archive from a byte slice, copying the archive data.
    ///
    /// # Errors
    ///
    /// Returns an error when the slice is not a supported TES4-family BSA archive.
    pub fn read(bytes: &[u8]) -> Result<Self> {
        Self::from_vec(bytes.to_vec())
    }

    /// Read an archive from a filesystem path using mmap-backed storage.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the path can not be mapped, or a parse error when
    /// the file is not a supported TES4-family BSA archive.
    pub fn open_path(path: impl AsRef<Path>) -> Result<Self> {
        parser::parse(Storage::open_path(path)?)
    }

    /// Metadata read from the archive header.
    #[must_use]
    pub fn info(&self) -> ArchiveInfo {
        self.info
    }

    /// Number of files declared by the archive header.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// All entries, in archive table order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Get an entry by case-insensitive path with slash normalization.
    #[must_use]
    pub fn get(&self, path: impl AsRef<[u8]>) -> Option<&Entry> {
        let normalized = normalize_path(path.as_ref());
        self.entries
            .iter()
            .find(|entry| entry.path.as_slice() == normalized.as_slice())
    }

    /// Size in bytes of the mapped or owned archive data.
    #[must_use]
    pub fn archive_size(&self) -> usize {
        self.storage.as_bytes().len()
    }

    /// Extract an entry into `out`.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry points outside the archive, uses a
    /// compressed storage mode that is not implemented yet, or has malformed
    /// embedded-name metadata.
    pub fn read_entry_into(&self, entry: &Entry, out: &mut Vec<u8>) -> Result<()> {
        let payload = self.entry_payload(entry)?;
        if entry.record.is_compressed(self.info.archive_flags) {
            self.decompress_entry(payload, out)
        } else {
            out.extend_from_slice(payload);
            Ok(())
        }
    }

    /// Extract an entry into a new vector.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::read_entry_into`].
    pub fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        self.read_entry_into(entry, &mut out)?;
        Ok(out)
    }

    /// Extract a path into a new vector.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::read_entry_into`] if the path exists.
    pub fn read_file(&self, path: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>> {
        self.get(path)
            .map(|entry| self.read_entry(entry))
            .transpose()
    }

    pub(super) fn from_parts(storage: Storage, info: ArchiveInfo, entries: Vec<Entry>) -> Self {
        Self {
            storage,
            info,
            entries,
        }
    }

    fn entry_payload<'a>(&'a self, entry: &Entry) -> Result<&'a [u8]> {
        let mut start: usize = entry.record.data_offset.try_into()?;
        let mut len: usize = entry.record.stored_size.try_into()?;
        let stored = self.slice(start, len)?;
        if matches!(
            self.info.version,
            ArchiveVersion::v104 | ArchiveVersion::v105
        ) && self
            .info
            .archive_flags
            .contains(ArchiveFlags::EMBEDDED_FILE_NAMES)
        {
            let embedded_name_len = stored.first().copied().ok_or(Error::OutOfBounds)? as usize + 1;
            if embedded_name_len > len {
                return Err(Error::OutOfBounds);
            }
            start = start
                .checked_add(embedded_name_len)
                .ok_or(Error::OutOfBounds)?;
            len -= embedded_name_len;
        }
        self.slice(start, len)
    }

    fn slice(&self, start: usize, len: usize) -> Result<&[u8]> {
        self.storage
            .as_bytes()
            .get(start..start.checked_add(len).ok_or(Error::OutOfBounds)?)
            .ok_or(Error::OutOfBounds)
    }

    fn decompress_entry(&self, payload: &[u8], out: &mut Vec<u8>) -> Result<()> {
        if self
            .info
            .archive_flags
            .contains(ArchiveFlags::XBOX_COMPRESSED)
        {
            return Err(Error::NotImplemented("TES4 XMem compression"));
        }
        if self.info.version == ArchiveVersion::v105 {
            return Err(Error::NotImplemented("TES4 LZ4 compressed files"));
        }
        let Some((expected_bytes, compressed)) = payload.split_first_chunk::<4>() else {
            return Err(Error::OutOfBounds);
        };
        let expected = u32::from_le_bytes(*expected_bytes).try_into()?;
        let before = out.len();
        let mut decoder = ZlibDecoder::new(compressed);
        decoder
            .read_to_end(out)
            .map_err(|error| Error::Zlib(error.to_string()))?;
        let actual = out.len() - before;
        if actual == expected {
            Ok(())
        } else {
            Err(Error::DecompressionSizeMismatch { expected, actual })
        }
    }
}

impl Entry {
    pub(super) fn new(folder: BString, name: BString, record: FileRecord) -> Self {
        let path = join_path(&folder, &name);
        Self {
            path,
            folder,
            name,
            record,
        }
    }
}

fn join_path(folder: &[u8], name: &[u8]) -> BString {
    let mut path = Vec::with_capacity(folder.len() + usize::from(!folder.is_empty()) + name.len());
    path.extend_from_slice(folder);
    if !folder.is_empty() && !name.is_empty() {
        path.push(b'\\');
    }
    path.extend_from_slice(name);
    BString::from(normalize_path(&path))
}

fn normalize_path(path: &[u8]) -> Vec<u8> {
    path.iter()
        .copied()
        .map(|byte| match byte {
            b'/' => b'\\',
            b'A'..=b'Z' => byte + 32,
            _ => byte,
        })
        .collect()
}

impl TryFrom<Copied<'_>> for Archive {
    type Error = super::Error;
    fn try_from(value: Copied<'_>) -> Result<Self> {
        Self::read(value.0)
    }
}
