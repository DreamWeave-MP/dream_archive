use super::{Result, parser};
use crate::{Copied, storage::Storage};
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
        self.info.file_count as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.info.file_count == 0
    }

    /// Size in bytes of the mapped or owned archive data.
    #[must_use]
    pub fn archive_size(&self) -> usize {
        self.storage.as_bytes().len()
    }

    pub(super) fn from_parts(storage: Storage, info: ArchiveInfo) -> Self {
        Self { storage, info }
    }
}

impl TryFrom<Copied<'_>> for Archive {
    type Error = super::Error;
    fn try_from(value: Copied<'_>) -> Result<Self> {
        Self::read(value.0)
    }
}
