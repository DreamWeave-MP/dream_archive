use super::{
    ArchiveFlags, ArchiveTypes, ArchiveVersion, Error, HashFields, Result, hash_directory,
    hash_file,
};
use crate::bsa::{FilenameEncoding, encode_filename};
use bstr::BString;
use flate2::{Compression, write::ZlibEncoder};
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

const MAGIC: u32 = u32::from_le_bytes(*b"BSA\0");
const HEADER_SIZE: u32 = 0x24;
const FOLDER_RECORD_SIZE_V104: usize = 16;
const FOLDER_RECORD_SIZE_V105: usize = 24;
const FILE_RECORD_SIZE: usize = 16;

/// Builder for string-backed TES4-family BSA archives.
///
/// Directory strings and file strings are present, and output order is
/// deterministic by TES4 hashes. Embedded names and hash-only output remain
/// separate features, not flags to accidentally trip over.
#[derive(Clone, Debug)]
pub struct Builder {
    version: ArchiveVersion,
    archive_types: ArchiveTypes,
    compressed: bool,
    zlib_level: Compression,
    entries: Vec<BuilderEntry>,
}

#[derive(Clone, Debug)]
struct BuilderEntry {
    folder: BString,
    name: BString,
    folder_hash: HashFields,
    file_hash: HashFields,
    compressed: Option<bool>,
    bytes: Vec<u8>,
}

struct PreparedEntry<'a> {
    entry: &'a BuilderEntry,
    stored: Vec<u8>,
}

#[derive(Clone, Copy)]
struct SortedFolder<'a> {
    name: &'a [u8],
    hash: HashFields,
    files_start: usize,
    files_end: usize,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            version: ArchiveVersion::v104,
            archive_types: ArchiveTypes::MISC,
            compressed: false,
            zlib_level: Compression::default(),
            entries: Vec::new(),
        }
    }
}

impl Builder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn version(&self) -> ArchiveVersion {
        self.version
    }

    pub fn set_version(&mut self, version: ArchiveVersion) -> &mut Self {
        self.version = version;
        self
    }

    #[must_use]
    pub fn archive_types(&self) -> ArchiveTypes {
        self.archive_types
    }

    pub fn set_archive_types(&mut self, archive_types: ArchiveTypes) -> &mut Self {
        self.archive_types = archive_types;
        self
    }

    #[must_use]
    pub fn compressed(&self) -> bool {
        self.compressed
    }

    pub fn set_compressed(&mut self, compressed: bool) -> &mut Self {
        self.compressed = compressed;
        self
    }

    #[must_use]
    pub fn zlib_level(&self) -> Compression {
        self.zlib_level
    }

    pub fn set_zlib_level(&mut self, level: Compression) -> &mut Self {
        self.zlib_level = level;
        self
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add an owned copy of one uncompressed file payload.
    ///
    /// # Errors
    ///
    /// Returns an error if the path can not be represented safely, is a
    /// duplicate after TES4 archive normalization, or allocation fails.
    pub fn add_bytes(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>) -> Result<()> {
        self.add_bytes_with_compression(path, bytes, None)
    }

    /// Add bytes with an explicit per-file compression override.
    ///
    /// `None` uses the builder default. TES4 stores this as the archive
    /// compression flag plus, when needed, the per-file compression toggle bit.
    ///
    /// # Errors
    ///
    /// Returns an error if the path can not be represented safely, is a
    /// duplicate after TES4 archive normalization, or allocation fails.
    pub fn add_bytes_with_compression(
        &mut self,
        path: impl AsRef<[u8]>,
        bytes: impl AsRef<[u8]>,
        compressed: Option<bool>,
    ) -> Result<()> {
        let (folder, name) = normalize_stored_path(path.as_ref())?;
        if self
            .entries
            .iter()
            .any(|entry| entry.folder == folder && entry.name == name)
        {
            return Err(Error::DuplicatePath);
        }
        let mut owned = Vec::new();
        owned.try_reserve_exact(bytes.as_ref().len())?;
        owned.extend_from_slice(bytes.as_ref());
        let folder_hash = hash_directory(&folder).0;
        let file_hash = hash_file(&name).0;
        self.entries.push(BuilderEntry {
            folder,
            name,
            folder_hash,
            file_hash,
            compressed,
            bytes: owned,
        });
        Ok(())
    }

    /// Read a filesystem file and store it at `archive_path`.
    ///
    /// Symlinked files are followed for payload bytes; the archive path is the
    /// path supplied by the caller.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the file fails or adding the archive entry fails.
    pub fn add_file(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        source: impl AsRef<Path>,
    ) -> Result<()> {
        self.add_file_with_compression(archive_path, source, None)
    }

    /// Read a filesystem file with a per-file compression override.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the file fails or adding the archive entry fails.
    pub fn add_file_with_compression(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        source: impl AsRef<Path>,
        compressed: Option<bool>,
    ) -> Result<()> {
        let bytes = fs::read(source)?;
        self.add_bytes_with_compression(archive_path, bytes, compressed)
    }

    /// Recursively add all files below `root` using paths relative to `root`.
    ///
    /// File symlinks are followed for payload bytes, but stored at the relative
    /// path where the symlink was found.
    ///
    /// # Errors
    ///
    /// Returns an error if directory traversal, file reading, or adding an entry fails.
    pub fn add_dir(&mut self, root: impl AsRef<Path>) -> Result<()> {
        self.add_dir_with_compression(root, None)
    }

    /// Recursively add a directory with a per-file compression override.
    ///
    /// # Errors
    ///
    /// Returns an error if directory traversal, file reading, or adding an entry fails.
    pub fn add_dir_with_compression(
        &mut self,
        root: impl AsRef<Path>,
        compressed: Option<bool>,
    ) -> Result<()> {
        let root = root.as_ref();
        for path in collect_files(root)? {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::InvalidArchivePath)?;
            let archive_path = path_to_archive_bytes(relative)?;
            self.add_file_with_compression(archive_path, &path, compressed)?;
        }
        Ok(())
    }

    /// Encode a Unicode archive path with an explicit legacy filename encoding,
    /// then add the payload.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` can not be encoded losslessly, the encoded
    /// path is invalid for a TES4 archive, is a duplicate, or allocation fails.
    pub fn add_encoded_path(
        &mut self,
        path: &str,
        encoding: FilenameEncoding,
        bytes: impl AsRef<[u8]>,
    ) -> Result<()> {
        let encoded = encode_filename(path, encoding)?;
        self.add_bytes(encoded.as_ref(), bytes)
    }

    /// Write the archive to a filesystem path.
    ///
    /// # Errors
    ///
    /// Returns an error if creating/writing the file fails or archive integer
    /// fields overflow their TES4 on-disk sizes.
    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = File::create(path)?;
        self.write_to(BufWriter::new(file))
    }

    /// Write the archive to a byte vector.
    ///
    /// # Errors
    ///
    /// Returns an error if archive integer fields overflow their TES4 on-disk
    /// sizes or output allocation fails.
    pub fn into_vec(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.try_reserve_exact(self.archive_size_hint()?)?;
        self.write_to(&mut out)?;
        Ok(out)
    }

    /// Write the archive to `out`.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails or archive integer fields overflow
    /// their TES4 on-disk sizes.
    pub fn write_to(&self, mut out: impl Write) -> Result<()> {
        let entries = self.sorted_entries();
        let prepared = self.prepare_entries(&entries)?;
        let folders = sorted_folders(&entries);
        let folder_names_len = folder_names_len(&folders)?;
        let file_names_len = file_names_len(&entries)?;
        let data_offset = data_offset(
            entries.len(),
            folders.len(),
            folder_names_len,
            file_names_len,
            self.version,
        )?;

        write_u32(&mut out, MAGIC)?;
        write_u32(&mut out, self.version as u32)?;
        write_u32(&mut out, HEADER_SIZE)?;
        write_u32(
            &mut out,
            ArchiveFlags::DIRECTORY_STRINGS.bits()
                | ArchiveFlags::FILE_STRINGS.bits()
                | if self.compressed {
                    ArchiveFlags::COMPRESSED.bits()
                } else {
                    0
                },
        )?;
        write_u32(&mut out, folders.len().try_into()?)?;
        write_u32(&mut out, entries.len().try_into()?)?;
        write_u32(&mut out, folder_names_len.try_into()?)?;
        write_u32(&mut out, file_names_len.try_into()?)?;
        write_u16(&mut out, self.archive_types.bits())?;
        write_u16(&mut out, 0)?;

        let mut folder_block_offset = usize::try_from(HEADER_SIZE)?
            .checked_add(folder_record_size(self.version) * folders.len())
            .ok_or(Error::OutOfBounds)?;
        for folder in &folders {
            write_folder_record(&mut out, self.version, folder, folder_block_offset)?;
            folder_block_offset = folder_block_offset
                .checked_add(1 + folder.name.len() + 1)
                .and_then(|offset| {
                    offset.checked_add(FILE_RECORD_SIZE * (folder.files_end - folder.files_start))
                })
                .ok_or(Error::OutOfBounds)?;
        }

        let mut payload_offset: u32 = data_offset.try_into()?;
        for folder in &folders {
            write_bzstring(&mut out, folder.name)?;
            for entry in &prepared[folder.files_start..folder.files_end] {
                write_hash(&mut out, entry.entry.file_hash)?;
                write_u32(&mut out, entry.file_size(self.compressed)?)?;
                write_u32(&mut out, payload_offset)?;
                payload_offset = payload_offset
                    .checked_add(entry.stored.len().try_into()?)
                    .ok_or(Error::OutOfBounds)?;
            }
        }

        for entry in &entries {
            out.write_all(&entry.name)?;
            out.write_all(&[0])?;
        }
        for entry in &prepared {
            out.write_all(&entry.stored)?;
        }
        Ok(())
    }

    fn prepare_entries<'a>(&self, entries: &[&'a BuilderEntry]) -> Result<Vec<PreparedEntry<'a>>> {
        let mut prepared = Vec::new();
        prepared.try_reserve_exact(entries.len())?;
        for entry in entries {
            let stored = if entry.is_compressed(self.compressed) {
                compressed_payload(self.version, &entry.bytes, self.zlib_level)?
            } else {
                entry.bytes.clone()
            };
            prepared.push(PreparedEntry { entry, stored });
        }
        Ok(prepared)
    }

    fn sorted_entries(&self) -> Vec<&BuilderEntry> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by(|left, right| {
            left.folder_hash
                .numeric()
                .cmp(&right.folder_hash.numeric())
                .then_with(|| left.folder.cmp(&right.folder))
                .then_with(|| left.file_hash.numeric().cmp(&right.file_hash.numeric()))
                .then_with(|| left.name.cmp(&right.name))
        });
        entries
    }

    fn archive_size_hint(&self) -> Result<usize> {
        let entries = self.sorted_entries();
        let prepared = self.prepare_entries(&entries)?;
        let folders = sorted_folders(&entries);
        let folder_names_len = folder_names_len(&folders)?;
        let file_names_len = file_names_len(&entries)?;
        data_offset(
            entries.len(),
            folders.len(),
            folder_names_len,
            file_names_len,
            self.version,
        )?
        .checked_add(
            prepared
                .iter()
                .try_fold(0usize, |sum, entry| sum.checked_add(entry.stored.len()))
                .ok_or(Error::OutOfBounds)?,
        )
        .ok_or(Error::OutOfBounds)
    }
}

impl PreparedEntry<'_> {
    fn file_size(&self, default_compressed: bool) -> Result<u32> {
        let mut size: u32 = self.stored.len().try_into()?;
        if self.entry.is_compressed(default_compressed) != default_compressed {
            size |= 1 << 30;
        }
        Ok(size)
    }
}

impl BuilderEntry {
    fn is_compressed(&self, default_compressed: bool) -> bool {
        self.compressed.unwrap_or(default_compressed)
    }
}

fn sorted_folders<'a>(entries: &[&'a BuilderEntry]) -> Vec<SortedFolder<'a>> {
    let mut folders = Vec::new();
    let mut start = 0;
    while start < entries.len() {
        let folder = entries[start].folder.as_slice();
        let hash = entries[start].folder_hash;
        let mut end = start + 1;
        while end < entries.len() && entries[end].folder.as_slice() == folder {
            end += 1;
        }
        folders.push(SortedFolder {
            name: folder,
            hash,
            files_start: start,
            files_end: end,
        });
        start = end;
    }
    folders
}

fn folder_names_len(folders: &[SortedFolder<'_>]) -> Result<usize> {
    folders.iter().try_fold(0usize, |sum, folder| {
        sum.checked_add(folder.name.len() + 1)
            .ok_or(Error::OutOfBounds)
    })
}

fn file_names_len(entries: &[&BuilderEntry]) -> Result<usize> {
    entries.iter().try_fold(0usize, |sum, entry| {
        sum.checked_add(entry.name.len() + 1)
            .ok_or(Error::OutOfBounds)
    })
}

fn data_offset(
    file_count: usize,
    folder_count: usize,
    folder_names_len: usize,
    file_names_len: usize,
    version: ArchiveVersion,
) -> Result<usize> {
    usize::try_from(HEADER_SIZE)?
        .checked_add(folder_record_size(version) * folder_count)
        .and_then(|offset| offset.checked_add(folder_count + folder_names_len))
        .and_then(|offset| offset.checked_add(FILE_RECORD_SIZE * file_count))
        .and_then(|offset| offset.checked_add(file_names_len))
        .ok_or(Error::OutOfBounds)
}

fn folder_record_size(version: ArchiveVersion) -> usize {
    match version {
        ArchiveVersion::v103 | ArchiveVersion::v104 => FOLDER_RECORD_SIZE_V104,
        ArchiveVersion::v105 => FOLDER_RECORD_SIZE_V105,
    }
}

fn write_folder_record(
    out: &mut impl Write,
    version: ArchiveVersion,
    folder: &SortedFolder<'_>,
    file_records_offset: usize,
) -> Result<()> {
    write_hash(out, folder.hash)?;
    write_u32(out, (folder.files_end - folder.files_start).try_into()?)?;
    match version {
        ArchiveVersion::v103 | ArchiveVersion::v104 => {
            write_u32(out, file_records_offset.try_into()?)?;
        }
        ArchiveVersion::v105 => {
            write_u32(out, 0)?;
            write_u64(out, file_records_offset.try_into()?)?;
        }
    }
    Ok(())
}

fn compressed_payload(
    version: ArchiveVersion,
    bytes: &[u8],
    zlib_level: Compression,
) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    out.try_reserve_exact(bytes.len().saturating_add(4))?;
    out.extend_from_slice(&u32::try_from(bytes.len())?.to_le_bytes());
    match version {
        ArchiveVersion::v103 | ArchiveVersion::v104 => {
            let mut encoder = ZlibEncoder::new(Vec::new(), zlib_level);
            encoder.write_all(bytes)?;
            out.extend_from_slice(&encoder.finish()?);
        }
        ArchiveVersion::v105 => {
            let mut encoder = lz4_flex::frame::FrameEncoder::new(Vec::new());
            encoder.write_all(bytes)?;
            out.extend_from_slice(
                &encoder
                    .finish()
                    .map_err(|error| Error::Lz4Frame(error.to_string()))?,
            );
        }
    }
    Ok(out)
}

fn normalize_stored_path(path: &[u8]) -> Result<(BString, BString)> {
    if path.is_empty() {
        return Err(Error::InvalidArchivePath);
    }
    let mut components = Vec::new();
    for component in path.split(|byte| matches!(*byte, b'/' | b'\\')) {
        if component.is_empty() || component == b"." {
            continue;
        }
        if component == b".." || component.contains(&0) || component.contains(&b':') {
            return Err(Error::InvalidArchivePath);
        }
        components.push(component);
    }
    let Some(name) = components.pop() else {
        return Err(Error::InvalidArchivePath);
    };
    if name.is_empty() {
        return Err(Error::InvalidArchivePath);
    }
    let mut folder = Vec::new();
    folder.try_reserve_exact(path.len())?;
    for (index, component) in components.iter().enumerate() {
        if index != 0 {
            folder.push(b'\\');
        }
        extend_lowercase(&mut folder, component);
    }
    let mut name_out = Vec::new();
    name_out.try_reserve_exact(name.len())?;
    extend_lowercase(&mut name_out, name);
    Ok((BString::from(folder), BString::from(name_out)))
}

fn extend_lowercase(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend(bytes.iter().copied().map(|byte| match byte {
        b'A'..=b'Z' => byte + 32,
        _ => byte,
    }));
}

fn write_bzstring(out: &mut impl Write, bytes: &[u8]) -> Result<()> {
    let len = bytes.len().checked_add(1).ok_or(Error::OutOfBounds)?;
    out.write_all(&[len.try_into()?])?;
    out.write_all(bytes)?;
    out.write_all(&[0])?;
    Ok(())
}

fn write_hash(out: &mut impl Write, hash: HashFields) -> Result<()> {
    out.write_all(&hash.numeric().to_le_bytes())?;
    Ok(())
}

fn write_u16(out: &mut impl Write, value: u16) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_u32(out: &mut impl Write, value: u32) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_u64(out: &mut impl Write, value: u64) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&dir)? {
            entries.push(entry?.path());
        }
        entries.sort();
        for path in entries {
            let symlink_metadata = fs::symlink_metadata(&path)?;
            if symlink_metadata.file_type().is_symlink() {
                if fs::metadata(&path)?.is_file() {
                    files.push(path);
                }
            } else if symlink_metadata.is_dir() {
                pending.push(path);
            } else if symlink_metadata.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn path_to_archive_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for component in path.components() {
        if !out.is_empty() {
            out.push(b'/');
        }
        let std::path::Component::Normal(part) = component else {
            return Err(Error::InvalidArchivePath);
        };
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            out.extend_from_slice(part.as_bytes());
        }
        #[cfg(not(unix))]
        {
            out.extend_from_slice(part.to_str().ok_or(Error::InvalidArchivePath)?.as_bytes());
        }
    }
    Ok(out)
}
