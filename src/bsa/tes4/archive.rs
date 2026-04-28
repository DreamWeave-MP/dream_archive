use super::{Error, HashFields, Result, hash_directory, hash_file, parser};
use crate::bsa::{
    FilenameEncoding, NormalizedPath, decode_filename_lossy, normalize_lookup_path,
    normalize_lookup_path_into,
};
use crate::{BStr, BString};
use crate::{
    Copied,
    extract::{
        ensure_parent_dir, output_path_decoded_into, output_path_into, write_file_atomically,
    },
    storage::Storage,
    stream::{self, BoxReader},
};
use flate2::read::ZlibDecoder;
use lz4_flex::frame::FrameDecoder;
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::io::Read as _;
use std::path::{Path, PathBuf};

const LZ4_FRAME_MAGIC: [u8; 4] = [0x04, 0x22, 0x4d, 0x18];

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

/// Stable identifier for an entry within one parsed TES4-family BSA archive.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EntryId(usize);

impl EntryId {
    #[must_use]
    pub const fn from_index(index: usize) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// One file entry in a TES4-family BSA archive index.
///
/// Paths preserve the archive's spelling. Lookup through [`Archive::get`] and
/// [`Archive::read_file`] is case-insensitive for ASCII and treats `/` as `\`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    path: Option<BString>,
    folder: Option<BString>,
    name: Option<BString>,
    lookup_path: Option<BString>,
    folder_hash: HashFields,
    file_hash: HashFields,
    record: FileRecord,
}

impl Entry {
    #[must_use]
    pub fn path(&self) -> Option<&BStr> {
        self.path.as_ref().map(AsRef::as_ref)
    }

    #[must_use]
    pub fn folder(&self) -> Option<&BStr> {
        self.folder.as_ref().map(AsRef::as_ref)
    }

    #[must_use]
    pub fn name(&self) -> Option<&BStr> {
        self.name.as_ref().map(AsRef::as_ref)
    }

    #[must_use]
    pub fn folder_hash(&self) -> HashFields {
        self.folder_hash
    }

    #[must_use]
    pub fn file_hash(&self) -> HashFields {
        self.file_hash
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
    pub checked: bool,
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
    /// Preserve all bits, including flags this crate does not currently name.
    pub const fn from_bits_retain(bits: u32) -> Self {
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
    /// Preserve all bits, including content-type flags this crate does not currently name.
    pub const fn from_bits_retain(bits: u16) -> Self {
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
    lookup: HashMap<BString, usize>,
    hash_lookup: HashMap<(u64, u64), usize>,
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
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
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

    pub fn entries_with_ids(&self) -> impl Iterator<Item = (EntryId, &Entry)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (EntryId(index), entry))
    }

    #[must_use]
    pub fn entry_by_id(&self, id: EntryId) -> Option<&Entry> {
        self.entries.get(id.0)
    }

    /// Get an entry by stable id, returning an error when it is absent.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfBounds`] when `id` does not identify an entry in this archive.
    pub fn entry_by_id_required(&self, id: EntryId) -> Result<&Entry> {
        self.entry_by_id(id).ok_or(Error::OutOfBounds)
    }

    /// Return the decoded size that extraction would write for an entry.
    ///
    /// # Errors
    ///
    /// Returns an error if payload metadata is out of bounds or malformed.
    pub fn extracted_len(&self, entry: &Entry) -> Result<u64> {
        if entry.record.is_compressed(self.info.archive_flags) {
            let payload = self.entry_payload(entry)?;
            if payload.len() < 4 {
                return Err(Error::OutOfBounds);
            }
            Ok(u64::from(u32::from_le_bytes([
                payload[0], payload[1], payload[2], payload[3],
            ])))
        } else {
            Ok(u64::try_from(self.entry_payload(entry)?.len())?)
        }
    }

    /// Return the decoded size that extraction would write for an entry id.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfBounds`] for an invalid id, or payload metadata errors.
    pub fn extracted_len_by_id(&self, id: EntryId) -> Result<u64> {
        self.extracted_len(self.entry_by_id_required(id)?)
    }

    /// Extract an entry selected by stable id into a writer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfBounds`] for an invalid id, or the same errors as entry extraction.
    pub fn extract_entry_by_id(&self, id: EntryId, out: impl std::io::Write) -> Result<u64> {
        self.extract_entry(self.entry_by_id_required(id)?, out)
    }

    /// Get an entry by case-insensitive path with slash normalization.
    #[must_use]
    pub fn get(&self, path: impl AsRef<[u8]>) -> Option<&Entry> {
        self.index_for_path(path.as_ref())
            .map(|&index| &self.entries[index])
    }

    /// Get an entry by a path that was normalized once for repeated string lookup.
    ///
    /// Unlike [`Self::get`], this does not perform TES4 hash fallback for
    /// hash-only archives. Use [`Self::get_by_hash`] for explicit hash lookup.
    #[must_use]
    pub fn get_normalized(&self, path: &NormalizedPath) -> Option<&Entry> {
        self.lookup
            .get(path.as_bytes())
            .map(|&index| &self.entries[index])
    }

    /// Get an entry by path, returning an error when it is absent.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FileNotFound`] when the normalized path/hash is not present.
    pub fn get_required(&self, path: impl AsRef<[u8]>) -> Result<&Entry> {
        let path = path.as_ref();
        self.get(path)
            .ok_or_else(|| Error::FileNotFound(BString::from(path)))
    }

    /// Get an entry by TES4 folder and file hashes.
    #[must_use]
    pub fn get_by_hash(&self, folder_hash: HashFields, file_hash: HashFields) -> Option<&Entry> {
        self.hash_lookup
            .get(&(folder_hash.numeric(), file_hash.numeric()))
            .map(|&index| &self.entries[index])
    }

    /// Whether a path exists in this archive.
    #[must_use]
    pub fn contains(&self, path: impl AsRef<[u8]>) -> bool {
        self.get(path).is_some()
    }

    #[must_use]
    pub fn contains_normalized(&self, path: &NormalizedPath) -> bool {
        self.get_normalized(path).is_some()
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
            out.try_reserve_exact(payload.len())?;
            out.extend_from_slice(payload);
            Ok(())
        }
    }

    /// Extract an entry into a writer.
    ///
    /// Compressed entries are decompressed into a temporary buffer before they
    /// are written so that malformed compressed data does not leave partial
    /// bytes in arbitrary caller-owned writers. Use
    /// [`Self::extract_entry_to_path`] for streaming filesystem extraction with
    /// temporary-file rollback.
    ///
    /// # Errors
    ///
    /// Returns an error if the entry points outside the archive, uses an
    /// unsupported compression mode, decompression fails, or writing fails.
    pub fn extract_entry(&self, entry: &Entry, mut out: impl std::io::Write) -> Result<u64> {
        let payload = self.entry_payload(entry)?;
        if entry.record.is_compressed(self.info.archive_flags) {
            self.decompress_entry_to_writer(payload, &mut out)
        } else {
            out.write_all(payload)?;
            Ok(payload.len().try_into()?)
        }
    }

    /// Extract an entry to a filesystem path atomically.
    ///
    /// The payload is written to a temporary file in the destination directory
    /// and renamed into place only after extraction succeeds. For compressed
    /// entries this avoids buffering the full decompressed payload while still
    /// preserving the existing destination on failure.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::extract_entry`], plus filesystem
    /// errors for creating, writing, or renaming the output file.
    pub fn extract_entry_to_path(&self, entry: &Entry, path: impl AsRef<Path>) -> Result<u64> {
        write_file_atomically(path.as_ref(), |file| {
            self.extract_entry_streaming(entry, file)
        })
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

    /// Extract a required path into a new vector.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FileNotFound`] if the path does not exist, or the same
    /// errors as [`Self::read_entry`] when it does.
    pub fn read_file_required(&self, path: impl AsRef<[u8]>) -> Result<Vec<u8>> {
        self.read_entry(self.get_required(path)?)
    }

    /// Extract a path into a writer.
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

    /// Extract a required path into a writer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FileNotFound`] if the path does not exist, or the same
    /// errors as [`Self::extract_entry`] when it does.
    pub fn extract_file_required(
        &self,
        path: impl AsRef<[u8]>,
        out: impl std::io::Write,
    ) -> Result<u64> {
        self.extract_entry(self.get_required(path)?, out)
    }

    /// Open an entry as a reader.
    ///
    /// Uncompressed entries are read directly from archive storage. Compressed
    /// entries are validated and buffered before the reader is returned, keeping
    /// construction errors in the format-specific [`Error`] type instead of
    /// smuggling them through later [`std::io::Error`] reads.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::read_entry`].
    pub fn open_entry<'a>(&'a self, entry: &'a Entry) -> Result<BoxReader<'a>> {
        let payload = self.entry_payload(entry)?;
        if entry.record.is_compressed(self.info.archive_flags) {
            let mut out = Vec::new();
            self.decompress_entry(payload, &mut out)?;
            Ok(stream::owned_reader(out))
        } else {
            Ok(stream::borrowed_reader(payload))
        }
    }

    /// Open an optional path as a reader.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::open_entry`] if the path exists.
    pub fn open_file(&self, path: impl AsRef<[u8]>) -> Result<Option<BoxReader<'_>>> {
        self.get(path)
            .map(|entry| self.open_entry(entry))
            .transpose()
    }

    /// Open a required path as a reader.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FileNotFound`] if the path does not exist, or the same
    /// errors as [`Self::open_entry`] when it does.
    pub fn open_file_required(&self, path: impl AsRef<[u8]>) -> Result<BoxReader<'_>> {
        self.open_entry(self.get_required(path)?)
    }

    /// Extract entries whose names are supplied by an external path dictionary.
    ///
    /// This is intended for hash-only TES4 archives. The archive cannot provide
    /// paths it did not store, so the caller supplies candidate paths from a
    /// manifest, plugin records, loose-file tree, or similar oracle. Paths not
    /// present in the archive are ignored.
    ///
    /// Returns the number of payload bytes written after decompression.
    ///
    /// # Errors
    ///
    /// Returns an error if a matched dictionary path is not a safe output path,
    /// directory or file creation fails, or entry extraction fails.
    pub fn extract_to_with_paths<P>(
        &self,
        target_dir: impl AsRef<Path>,
        paths: impl IntoIterator<Item = P>,
    ) -> Result<u64>
    where
        P: AsRef<[u8]>,
    {
        let target_dir = target_dir.as_ref();
        let mut written = 0u64;
        let mut output_path = PathBuf::new();
        let mut last_parent = PathBuf::new();
        let mut extracted = HashSet::new();
        let mut normalized = Vec::new();
        for path in paths {
            let path = path.as_ref();
            let Some(&index) = self.index_for_path_with_scratch(path, &mut normalized) else {
                continue;
            };
            if !extracted.insert(index) {
                continue;
            }
            output_path_into(&mut output_path, target_dir, path)?;
            ensure_parent_dir(&output_path, &mut last_parent)?;
            written += self.extract_entry_to_path(&self.entries[index], &output_path)?;
        }
        Ok(written)
    }

    /// Extract every entry to `target_dir`, preserving archive paths.
    ///
    /// Returns the number of payload bytes written after decompression.
    ///
    /// # Errors
    ///
    /// Returns an error if an archive entry has no safe output path, directory
    /// or file creation fails, or entry extraction fails.
    pub fn extract_to(&self, target_dir: impl AsRef<Path>) -> Result<u64> {
        let target_dir = target_dir.as_ref();
        #[cfg(feature = "parallel")]
        {
            self.extract_to_parallel(target_dir)
        }
        #[cfg(not(feature = "parallel"))]
        {
            self.extract_to_sequential(target_dir)
        }
    }

    /// Extract every named entry to `target_dir`, decoding archive path bytes
    /// with an explicit filename encoding before creating filesystem paths.
    ///
    /// Hash-only entries without recovered names still can not be extracted by
    /// this API; use [`Self::extract_to_with_paths`] with a byte-path dictionary
    /// for those.
    ///
    /// # Errors
    ///
    /// Returns an error if an archive entry has no path, has no safe output
    /// path, directory or file creation fails, or entry extraction fails.
    pub fn extract_to_with_encoding(
        &self,
        target_dir: impl AsRef<Path>,
        encoding: FilenameEncoding,
    ) -> Result<u64> {
        let target_dir = target_dir.as_ref();
        let mut written = 0u64;
        let mut path = PathBuf::new();
        let mut last_parent = PathBuf::new();
        for entry in &self.entries {
            let path_bytes = entry.path().ok_or(Error::ArchivePathsUnavailable)?;
            output_path_decoded_into(&mut path, target_dir, path_bytes, |component| {
                decode_filename_lossy(component, encoding)
            })?;
            ensure_parent_dir(&path, &mut last_parent)?;
            written += self.extract_entry_to_path(entry, &path)?;
        }
        Ok(written)
    }

    fn extract_to_sequential(&self, target_dir: &Path) -> Result<u64> {
        let mut written = 0u64;
        let mut path = PathBuf::new();
        let mut last_parent = PathBuf::new();
        for entry in &self.entries {
            let path_bytes = entry.path().ok_or(Error::ArchivePathsUnavailable)?;
            output_path_into(&mut path, target_dir, path_bytes)?;
            ensure_parent_dir(&path, &mut last_parent)?;
            written += self.extract_entry_to_path(entry, &path)?;
        }
        Ok(written)
    }

    #[cfg(feature = "parallel")]
    fn extract_to_parallel(&self, target_dir: &Path) -> Result<u64> {
        let paths = self.extract_output_paths(target_dir)?;
        if crate::extract::has_duplicate_paths(&paths) {
            return self.extract_to_sequential(target_dir);
        }
        crate::extract::ensure_parent_dirs(&paths)?;
        self.entries
            .par_iter()
            .zip(paths.par_iter())
            .map(|(entry, path)| self.extract_entry_to_path(entry, path))
            .try_reduce(|| 0, |left, right| Ok(left + right))
    }

    #[cfg(feature = "parallel")]
    fn extract_output_paths(&self, target_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        paths.try_reserve_exact(self.entries.len())?;
        for entry in &self.entries {
            let mut path = PathBuf::new();
            let path_bytes = entry.path().ok_or(Error::ArchivePathsUnavailable)?;
            output_path_into(&mut path, target_dir, path_bytes)?;
            paths.push(path);
        }
        Ok(paths)
    }

    pub(super) fn from_parts(storage: Storage, info: ArchiveInfo, entries: Vec<Entry>) -> Self {
        let mut lookup = HashMap::with_capacity(entries.len());
        let mut hash_lookup = HashMap::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            if let Some(lookup_path) = &entry.lookup_path {
                lookup.entry(lookup_path.clone()).or_insert(index);
            }
            hash_lookup
                .entry((entry.folder_hash.numeric(), entry.file_hash.numeric()))
                .or_insert(index);
        }
        Self {
            storage,
            info,
            entries,
            lookup,
            hash_lookup,
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

    fn index_for_path(&self, path: &[u8]) -> Option<&usize> {
        let normalized = normalize_lookup_path(path);
        if self.lookup.is_empty() {
            self.hash_lookup.get(&path_hash(path))
        } else {
            self.lookup.get(normalized.as_slice())
        }
    }

    fn index_for_path_with_scratch<'a>(
        &'a self,
        path: &[u8],
        normalized: &mut Vec<u8>,
    ) -> Option<&'a usize> {
        normalize_lookup_path_into(normalized, path);
        if self.lookup.is_empty() {
            self.hash_lookup.get(&path_hash(path))
        } else {
            self.lookup.get(normalized.as_slice())
        }
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
            return decompress_lz4_frame_payload(payload, out);
        }
        let Some((expected_bytes, compressed)) = payload.split_first_chunk::<4>() else {
            return Err(Error::OutOfBounds);
        };
        let expected = u32::from_le_bytes(*expected_bytes).try_into()?;
        let before = out.len();
        let mut decoder = ZlibDecoder::new(compressed);
        read_decompressed(&mut decoder, expected, out, |error| {
            Error::Zlib(error.to_string())
        })?;
        if decoder.total_in() == u64::try_from(compressed.len())? {
            Ok(())
        } else {
            out.truncate(before);
            Err(Error::TrailingCompressedData)
        }
    }

    fn decompress_entry_to_writer(
        &self,
        payload: &[u8],
        out: &mut impl std::io::Write,
    ) -> Result<u64> {
        if self
            .info
            .archive_flags
            .contains(ArchiveFlags::XBOX_COMPRESSED)
        {
            return Err(Error::NotImplemented("TES4 XMem compression"));
        }
        if self.info.version == ArchiveVersion::v105 {
            return decompress_lz4_frame_payload_to_writer(payload, out);
        }
        let Some((expected_bytes, compressed)) = payload.split_first_chunk::<4>() else {
            return Err(Error::OutOfBounds);
        };
        let expected = u32::from_le_bytes(*expected_bytes).try_into()?;
        let mut decoder = ZlibDecoder::new(compressed);
        let mut buffer = Vec::new();
        read_decompressed(&mut decoder, expected, &mut buffer, |error| {
            Error::Zlib(error.to_string())
        })?;
        if decoder.total_in() == u64::try_from(compressed.len())? {
            out.write_all(&buffer)?;
            Ok(buffer.len().try_into()?)
        } else {
            Err(Error::TrailingCompressedData)
        }
    }

    fn extract_entry_streaming(&self, entry: &Entry, mut out: impl std::io::Write) -> Result<u64> {
        let payload = self.entry_payload(entry)?;
        if entry.record.is_compressed(self.info.archive_flags) {
            self.decompress_entry_to_writer_streaming(payload, &mut out)
        } else {
            out.write_all(payload)?;
            Ok(payload.len().try_into()?)
        }
    }

    fn decompress_entry_to_writer_streaming(
        &self,
        payload: &[u8],
        out: &mut impl std::io::Write,
    ) -> Result<u64> {
        if self
            .info
            .archive_flags
            .contains(ArchiveFlags::XBOX_COMPRESSED)
        {
            return Err(Error::NotImplemented("TES4 XMem compression"));
        }
        if self.info.version == ArchiveVersion::v105 {
            return decompress_lz4_frame_payload_to_writer_streaming(payload, out);
        }
        let Some((expected_bytes, compressed)) = payload.split_first_chunk::<4>() else {
            return Err(Error::OutOfBounds);
        };
        let expected = u32::from_le_bytes(*expected_bytes).try_into()?;
        let mut decoder = ZlibDecoder::new(compressed);
        let actual = copy_decompressed(&mut decoder, expected, out, |error| {
            Error::Zlib(error.to_string())
        })?;
        if decoder.total_in() == u64::try_from(compressed.len())? {
            Ok(actual.try_into()?)
        } else {
            Err(Error::TrailingCompressedData)
        }
    }
}

fn decompress_lz4_frame_payload(payload: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let Some((expected_bytes, compressed)) = payload.split_first_chunk::<4>() else {
        return Err(Error::OutOfBounds);
    };
    if !compressed.starts_with(&LZ4_FRAME_MAGIC) {
        return Err(Error::InvalidLz4Frame);
    }
    let expected = u32::from_le_bytes(*expected_bytes).try_into()?;
    let before = out.len();
    let mut decoder = FrameDecoder::new(compressed);
    read_decompressed(&mut decoder, expected, out, |error| {
        Error::Lz4Frame(error.to_string())
    })?;
    if decoder.get_ref().is_empty() {
        Ok(())
    } else {
        out.truncate(before);
        Err(Error::TrailingCompressedData)
    }
}

fn decompress_lz4_frame_payload_to_writer(
    payload: &[u8],
    out: &mut impl std::io::Write,
) -> Result<u64> {
    let Some((expected_bytes, compressed)) = payload.split_first_chunk::<4>() else {
        return Err(Error::OutOfBounds);
    };
    if !compressed.starts_with(&LZ4_FRAME_MAGIC) {
        return Err(Error::InvalidLz4Frame);
    }
    let expected = u32::from_le_bytes(*expected_bytes).try_into()?;
    let mut decoder = FrameDecoder::new(compressed);
    let mut buffer = Vec::new();
    read_decompressed(&mut decoder, expected, &mut buffer, |error| {
        Error::Lz4Frame(error.to_string())
    })?;
    if decoder.get_ref().is_empty() {
        out.write_all(&buffer)?;
        Ok(buffer.len().try_into()?)
    } else {
        Err(Error::TrailingCompressedData)
    }
}

fn decompress_lz4_frame_payload_to_writer_streaming(
    payload: &[u8],
    out: &mut impl std::io::Write,
) -> Result<u64> {
    let Some((expected_bytes, compressed)) = payload.split_first_chunk::<4>() else {
        return Err(Error::OutOfBounds);
    };
    if !compressed.starts_with(&LZ4_FRAME_MAGIC) {
        return Err(Error::InvalidLz4Frame);
    }
    let expected = u32::from_le_bytes(*expected_bytes).try_into()?;
    let mut decoder = FrameDecoder::new(compressed);
    let actual = copy_decompressed(&mut decoder, expected, out, |error| {
        Error::Lz4Frame(error.to_string())
    })?;
    if decoder.get_ref().is_empty() {
        Ok(actual.try_into()?)
    } else {
        Err(Error::TrailingCompressedData)
    }
}

fn read_decompressed(
    decoder: &mut impl std::io::Read,
    expected: usize,
    out: &mut Vec<u8>,
    map_error: impl FnOnce(std::io::Error) -> Error,
) -> Result<()> {
    let before = out.len();
    out.try_reserve_exact(expected)?;
    let mut limited = decoder.take(expected as u64 + 1);
    if let Err(error) = limited.read_to_end(out) {
        out.truncate(before);
        return Err(map_error(error));
    }
    let actual = out.len() - before;
    if actual == expected {
        Ok(())
    } else {
        out.truncate(before);
        Err(Error::DecompressionSizeMismatch { expected, actual })
    }
}

fn copy_decompressed(
    decoder: &mut impl std::io::Read,
    expected: usize,
    out: &mut impl std::io::Write,
    map_error: impl FnOnce(std::io::Error) -> Error,
) -> Result<usize> {
    let mut limited = decoder.take(expected as u64 + 1);
    let actual = std::io::copy(&mut limited, out)
        .map_err(map_error)
        .and_then(|actual| usize::try_from(actual).map_err(Error::from))?;
    if actual == expected {
        Ok(actual)
    } else {
        Err(Error::DecompressionSizeMismatch { expected, actual })
    }
}

impl Entry {
    pub(super) fn new(
        folder: Option<BString>,
        name: Option<BString>,
        folder_hash: HashFields,
        file_hash: HashFields,
        record: FileRecord,
    ) -> Self {
        let path = folder
            .as_ref()
            .zip(name.as_ref())
            .map(|(folder, name)| join_path(folder, name));
        let lookup_path = path
            .as_ref()
            .map(|path| BString::from(normalize_lookup_path(path)));
        Self {
            path,
            folder,
            name,
            lookup_path,
            folder_hash,
            file_hash,
            record,
        }
    }
}

fn path_hash(path: &[u8]) -> (u64, u64) {
    let separator = path.iter().rposition(|byte| matches!(*byte, b'/' | b'\\'));
    let folder = separator.map_or(&[][..], |end| &path[..end]);
    let name = separator.map_or(path, |end| &path[end + 1..]);
    (
        hash_directory(folder).0.numeric(),
        hash_file(name).0.numeric(),
    )
}

fn join_path(folder: &[u8], name: &[u8]) -> BString {
    let mut path = Vec::with_capacity(folder.len() + usize::from(!folder.is_empty()) + name.len());
    path.extend_from_slice(folder);
    if !folder.is_empty() && !name.is_empty() {
        path.push(b'\\');
    }
    path.extend_from_slice(name);
    BString::from(path)
}

impl TryFrom<Copied<'_>> for Archive {
    type Error = Error;
    fn try_from(value: Copied<'_>) -> Result<Self> {
        Self::from_slice(value.0)
    }
}
