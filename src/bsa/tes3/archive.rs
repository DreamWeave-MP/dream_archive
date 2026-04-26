use super::{Error, Result, parser};
use crate::{Copied, storage::Storage};
use bstr::{BStr, BString};
use std::collections::HashMap;
use std::path::Path;

/// Metadata read from a TES3 BSA archive header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveInfo {
    pub hash_offset: u32,
    pub file_count: u32,
}

/// One file entry in a TES3 BSA archive index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    path: BString,
    lookup_path: BString,
    record: FileRecord,
    hash: u64,
}

impl Entry {
    #[must_use]
    pub fn path(&self) -> &BStr {
        self.path.as_ref()
    }

    #[must_use]
    pub fn file(&self) -> FileRecord {
        self.record
    }

    #[must_use]
    pub fn hash(&self) -> u64 {
        self.hash
    }
}

/// Raw file location metadata from a TES3 BSA file record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileRecord {
    pub size: u32,
    pub offset: u32,
}

/// Parsed TES3 BSA archive.
#[derive(Clone, Debug)]
pub struct Archive {
    storage: Storage,
    info: ArchiveInfo,
    entries: Vec<Entry>,
    data_offset: usize,
    lookup: HashMap<BString, usize>,
}

impl Archive {
    /// Read an archive from an owned byte buffer.
    ///
    /// # Errors
    ///
    /// Returns an error when the buffer is not a supported TES3 BSA archive.
    pub fn from_vec(bytes: Vec<u8>) -> Result<Self> {
        parser::parse(Storage::from_vec(bytes))
    }

    /// Read an archive from a byte slice, copying the archive data.
    ///
    /// # Errors
    ///
    /// Returns an error when the slice is not a supported TES3 BSA archive.
    pub fn read(bytes: &[u8]) -> Result<Self> {
        Self::from_vec(bytes.to_vec())
    }

    /// Read an archive from a filesystem path using mmap-backed storage.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the path can not be mapped, or a parse error when
    /// the file is not a supported TES3 BSA archive.
    pub fn open_path(path: impl AsRef<Path>) -> Result<Self> {
        parser::parse(Storage::open_path(path)?)
    }

    #[must_use]
    pub fn info(&self) -> ArchiveInfo {
        self.info
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    #[must_use]
    pub fn get(&self, path: impl AsRef<[u8]>) -> Option<&Entry> {
        let normalized = normalize_path(path.as_ref());
        self.lookup
            .get(normalized.as_slice())
            .map(|&index| &self.entries[index])
    }

    #[must_use]
    pub fn contains(&self, path: impl AsRef<[u8]>) -> bool {
        self.get(path).is_some()
    }

    #[must_use]
    pub fn archive_size(&self) -> usize {
        self.storage.as_bytes().len()
    }

    /// Extract an entry into `out`.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry points outside the archive or output
    /// allocation fails.
    pub fn read_entry_into(&self, entry: &Entry, out: &mut Vec<u8>) -> Result<()> {
        let payload = self.entry_payload(entry)?;
        out.try_reserve_exact(payload.len())?;
        out.extend_from_slice(payload);
        Ok(())
    }

    /// Extract an entry into a writer without allocating a payload buffer.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry points outside the archive or writing fails.
    pub fn extract_entry(&self, entry: &Entry, mut out: impl std::io::Write) -> Result<u64> {
        let payload = self.entry_payload(entry)?;
        out.write_all(payload)?;
        Ok(payload.len().try_into()?)
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

    /// Extract a path into a writer without allocating a payload buffer.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::extract_entry`] if the path exists.
    pub fn extract_file(
        &self,
        path: impl AsRef<[u8]>,
        out: impl std::io::Write,
    ) -> Result<Option<u64>> {
        self.get(path)
            .map(|entry| self.extract_entry(entry, out))
            .transpose()
    }

    pub(super) fn from_parts(
        storage: Storage,
        info: ArchiveInfo,
        entries: Vec<Entry>,
        data_offset: usize,
    ) -> Self {
        let mut lookup = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            lookup.entry(entry.lookup_path.clone()).or_insert(index);
        }
        Self {
            storage,
            info,
            entries,
            data_offset,
            lookup,
        }
    }

    fn entry_payload<'a>(&'a self, entry: &Entry) -> Result<&'a [u8]> {
        let relative: usize = entry.record.offset.try_into()?;
        let start = self
            .data_offset
            .checked_add(relative)
            .ok_or(Error::OutOfBounds)?;
        let len: usize = entry.record.size.try_into()?;
        self.storage
            .as_bytes()
            .get(start..start.checked_add(len).ok_or(Error::OutOfBounds)?)
            .ok_or(Error::OutOfBounds)
    }
}

impl Entry {
    pub(super) fn new(path: BString, record: FileRecord, hash: u64) -> Self {
        let lookup_path = BString::from(normalize_path(&path));
        Self {
            path,
            lookup_path,
            record,
            hash,
        }
    }
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
    type Error = Error;
    fn try_from(value: Copied<'_>) -> Result<Self> {
        Self::read(value.0)
    }
}
