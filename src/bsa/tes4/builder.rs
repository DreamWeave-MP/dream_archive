use super::{
    ArchiveFlags, ArchiveTypes, ArchiveVersion, Error, HashFields, Result, hash_directory,
    hash_file,
};
use crate::{
    CompressionOverride,
    bsa::{FilenameEncoding, encode_filename},
    builder_fs,
};
use bstr::BString;
use flate2::{Compression, write::ZlibEncoder};
use std::{
    borrow::Cow,
    collections::HashSet,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
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
    name_mode: NameMode,
    zlib_level: Compression,
    entries: Vec<BuilderEntry>,
    paths: HashSet<(BString, BString)>,
}

/// How the TES4 writer stores recoverable path names.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NameMode {
    /// Write folder and file string tables.
    #[default]
    Strings,
    /// Omit string tables and rely on folder/file hashes for lookup.
    HashOnly,
    /// Write full virtual paths next to file payloads, without string tables.
    Embedded,
    /// Write both string tables and embedded full virtual paths.
    StringsAndEmbedded,
}

/// PC game-oriented TES4-family writer preset.
///
/// Profiles intentionally set only the layout generation that is intrinsic to a
/// game family. Compression, archive type bits, and name storage mode remain
/// explicit policy choices because real tools produce different combinations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameProfile {
    /// The Elder Scrolls IV: Oblivion PC BSA archives.
    Oblivion,
    /// Fallout 3 PC BSA archives, excluding `XMem` output.
    Fallout3,
    /// Fallout: New Vegas PC BSA archives, excluding `XMem` output.
    FalloutNewVegas,
    /// The Elder Scrolls V: Skyrim Legendary Edition PC BSA archives.
    SkyrimLe,
    /// Skyrim Special/Anniversary Edition PC BSA archives.
    SkyrimSe,
}

impl GameProfile {
    const fn version(self) -> ArchiveVersion {
        match self {
            Self::Oblivion => ArchiveVersion::v103,
            Self::Fallout3 | Self::FalloutNewVegas | Self::SkyrimLe => ArchiveVersion::v104,
            Self::SkyrimSe => ArchiveVersion::v105,
        }
    }
}

impl NameMode {
    const fn directory_strings(self) -> bool {
        matches!(self, Self::Strings | Self::StringsAndEmbedded)
    }

    const fn file_strings(self) -> bool {
        matches!(self, Self::Strings | Self::StringsAndEmbedded)
    }

    const fn embedded_file_names(self) -> bool {
        matches!(self, Self::Embedded | Self::StringsAndEmbedded)
    }
}

#[derive(Clone, Debug)]
struct BuilderEntry {
    folder: BString,
    name: BString,
    folder_hash: HashFields,
    file_hash: HashFields,
    compression: CompressionOverride,
    bytes: Vec<u8>,
}

struct PreparedEntry<'a> {
    entry: &'a BuilderEntry,
    stored: Cow<'a, [u8]>,
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
            name_mode: NameMode::Strings,
            zlib_level: Compression::default(),
            entries: Vec::new(),
            paths: HashSet::new(),
        }
    }
}

impl Builder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder configured for a PC game archive family.
    ///
    /// The preset selects the archive version. It deliberately leaves
    /// compression disabled, archive type as [`ArchiveTypes::MISC`], and name
    /// storage as [`NameMode::Strings`]. Those are archive policy decisions, not
    /// inherent consequences of the game name.
    #[must_use]
    pub fn with_profile(profile: GameProfile) -> Self {
        let mut builder = Self::new();
        builder.set_profile(profile);
        builder
    }

    #[must_use]
    pub fn oblivion() -> Self {
        Self::with_profile(GameProfile::Oblivion)
    }

    #[must_use]
    pub fn fallout3() -> Self {
        Self::with_profile(GameProfile::Fallout3)
    }

    #[must_use]
    pub fn fallout_new_vegas() -> Self {
        Self::with_profile(GameProfile::FalloutNewVegas)
    }

    #[must_use]
    pub fn skyrim_le() -> Self {
        Self::with_profile(GameProfile::SkyrimLe)
    }

    #[must_use]
    pub fn skyrim_se() -> Self {
        Self::with_profile(GameProfile::SkyrimSe)
    }

    pub fn set_profile(&mut self, profile: GameProfile) -> &mut Self {
        self.version = profile.version();
        self
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
    pub fn name_mode(&self) -> NameMode {
        self.name_mode
    }

    pub fn set_name_mode(&mut self, name_mode: NameMode) -> &mut Self {
        self.name_mode = name_mode;
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

    /// Add an owned copy of one raw file payload.
    ///
    /// # Errors
    ///
    /// Returns an error if the path can not be represented safely, is a
    /// duplicate after TES4 archive normalization, or allocation fails.
    pub fn add_bytes(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>) -> Result<()> {
        self.add_bytes_with_compression(path, bytes, CompressionOverride::Inherit)
    }

    /// Add bytes with an explicit per-file compression policy.
    ///
    /// TES4 stores overrides as the archive compression flag plus, when needed,
    /// the per-file compression toggle bit.
    ///
    /// # Errors
    ///
    /// Returns an error if the path can not be represented safely, is a
    /// duplicate after TES4 archive normalization, or allocation fails.
    pub fn add_bytes_with_compression(
        &mut self,
        path: impl AsRef<[u8]>,
        bytes: impl AsRef<[u8]>,
        compression: CompressionOverride,
    ) -> Result<()> {
        let (folder, name) = normalize_stored_path(path.as_ref())?;
        let path_key = (folder.clone(), name.clone());
        if self.paths.contains(&path_key) {
            return Err(Error::DuplicatePath);
        }
        let mut owned = Vec::new();
        owned.try_reserve_exact(bytes.as_ref().len())?;
        owned.extend_from_slice(bytes.as_ref());
        let folder_hash = hash_directory(&folder).0;
        let file_hash = hash_file(&name).0;
        self.entries.try_reserve(1)?;
        self.paths.try_reserve(1)?;
        self.paths.insert(path_key);
        self.entries.push(BuilderEntry {
            folder,
            name,
            folder_hash,
            file_hash,
            compression,
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
        self.add_file_with_compression(archive_path, source, CompressionOverride::Inherit)
    }

    /// Read a filesystem file with a per-file compression policy.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the file fails or adding the archive entry fails.
    pub fn add_file_with_compression(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        source: impl AsRef<Path>,
        compression: CompressionOverride,
    ) -> Result<()> {
        let bytes = fs::read(source)?;
        self.add_bytes_with_compression(archive_path, bytes, compression)
    }

    /// Recursively add all files below `root` using paths relative to `root`.
    ///
    /// File symlinks are followed for payload bytes, but stored at the relative
    /// path where the symlink was found. Directory symlinks are ignored. Paths
    /// are taken from the platform filesystem bytes where available; use
    /// [`Self::add_encoded_path`] when a legacy BSA filename encoding is required.
    ///
    /// # Errors
    ///
    /// Returns an error if directory traversal, file reading, or adding an entry fails.
    pub fn add_dir(&mut self, root: impl AsRef<Path>) -> Result<()> {
        self.add_dir_with_compression(root, CompressionOverride::Inherit)
    }

    /// Recursively add a directory with a per-file compression policy.
    ///
    /// File symlinks are followed for payload bytes. Directory symlinks are
    /// ignored.
    ///
    /// # Errors
    ///
    /// Returns an error if directory traversal, file reading, or adding an entry fails.
    pub fn add_dir_with_compression(
        &mut self,
        root: impl AsRef<Path>,
        compression: CompressionOverride,
    ) -> Result<()> {
        let root = root.as_ref();
        for path in builder_fs::collect_files(root)? {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::InvalidArchivePath)?;
            let archive_path =
                builder_fs::path_to_archive_bytes(relative).ok_or(Error::InvalidArchivePath)?;
            self.add_file_with_compression(archive_path, &path, compression)?;
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
    pub fn to_vec(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
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
        if self.name_mode.embedded_file_names() && self.version == ArchiveVersion::v103 {
            return Err(Error::NotImplemented(
                "TES4 embedded file names require version 104 or 105",
            ));
        }
        let entries = self.sorted_entries();
        let prepared = self.prepare_entries(&entries)?;
        let folders = sorted_folders(&entries);
        let folder_names_len = if self.name_mode.directory_strings() {
            folder_names_len(&folders)?
        } else {
            0
        };
        let file_names_len = if self.name_mode.file_strings() {
            file_names_len(&entries)?
        } else {
            0
        };
        let data_offset = data_offset(
            entries.len(),
            folders.len(),
            folder_names_len,
            file_names_len,
            self.version,
            self.name_mode,
        )?;

        write_u32(&mut out, MAGIC)?;
        write_u32(&mut out, self.version as u32)?;
        write_u32(&mut out, HEADER_SIZE)?;
        write_u32(
            &mut out,
            if self.name_mode.directory_strings() {
                ArchiveFlags::DIRECTORY_STRINGS.bits()
            } else {
                0
            } | if self.name_mode.file_strings() {
                ArchiveFlags::FILE_STRINGS.bits()
            } else {
                0
            } | if self.name_mode.embedded_file_names() {
                ArchiveFlags::EMBEDDED_FILE_NAMES.bits()
            } else {
                0
            } | if self.compressed {
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
                .checked_add(if self.name_mode.directory_strings() {
                    1 + folder.name.len() + 1
                } else {
                    0
                })
                .and_then(|offset| {
                    offset.checked_add(FILE_RECORD_SIZE * (folder.files_end - folder.files_start))
                })
                .ok_or(Error::OutOfBounds)?;
        }

        let mut payload_offset: u32 = data_offset.try_into()?;
        for folder in &folders {
            if self.name_mode.directory_strings() {
                write_bzstring(&mut out, folder.name)?;
            }
            for entry in &prepared[folder.files_start..folder.files_end] {
                write_hash(&mut out, entry.entry.file_hash)?;
                let file_size = entry.file_size(self.compressed, self.name_mode)?;
                write_u32(&mut out, file_size)?;
                write_u32(&mut out, payload_offset)?;
                payload_offset = payload_offset
                    .checked_add(file_size & !(1 << 30 | 1 << 31))
                    .ok_or(Error::OutOfBounds)?;
            }
        }

        if self.name_mode.file_strings() {
            for entry in &entries {
                out.write_all(&entry.name)?;
                out.write_all(&[0])?;
            }
        }
        for entry in &prepared {
            if self.name_mode.embedded_file_names() {
                write_bstring(&mut out, &entry.entry.embedded_name()?)?;
            }
            out.write_all(&entry.stored)?;
        }
        Ok(())
    }

    fn prepare_entries<'a>(&self, entries: &[&'a BuilderEntry]) -> Result<Vec<PreparedEntry<'a>>> {
        let mut prepared = Vec::new();
        prepared.try_reserve_exact(entries.len())?;
        for entry in entries {
            let stored = if entry.is_compressed(self.compressed) {
                Cow::Owned(compressed_payload(
                    self.version,
                    &entry.bytes,
                    self.zlib_level,
                )?)
            } else {
                Cow::Borrowed(entry.bytes.as_slice())
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
}

impl PreparedEntry<'_> {
    fn file_size(&self, default_compressed: bool, name_mode: NameMode) -> Result<u32> {
        const RESERVED_FILE_SIZE_BITS: usize = (1 << 30) | (1 << 31);
        let embedded_len = if name_mode.embedded_file_names() {
            self.entry.embedded_name()?.len() + 1
        } else {
            0
        };
        let stored_len = self
            .stored
            .len()
            .checked_add(embedded_len)
            .ok_or(Error::OutOfBounds)?;
        if stored_len & RESERVED_FILE_SIZE_BITS != 0 {
            return Err(Error::OutOfBounds);
        }
        let mut size: u32 = stored_len.try_into()?;
        if self.entry.is_compressed(default_compressed) != default_compressed {
            size |= 1 << 30;
        }
        Ok(size)
    }
}

impl BuilderEntry {
    fn is_compressed(&self, default_compressed: bool) -> bool {
        match self.compression {
            CompressionOverride::Inherit => default_compressed,
            CompressionOverride::Store => false,
            CompressionOverride::Compress => true,
        }
    }

    fn embedded_name(&self) -> Result<BString> {
        if self.folder.is_empty() {
            return Ok(self.name.clone());
        }
        let mut out = Vec::new();
        out.try_reserve_exact(self.folder.len() + 1 + self.name.len())?;
        out.extend_from_slice(&self.folder);
        out.push(b'\\');
        out.extend_from_slice(&self.name);
        Ok(BString::from(out))
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
    name_mode: NameMode,
) -> Result<usize> {
    usize::try_from(HEADER_SIZE)?
        .checked_add(folder_record_size(version) * folder_count)
        .and_then(|offset| {
            offset.checked_add(if name_mode.directory_strings() {
                folder_count + folder_names_len
            } else {
                0
            })
        })
        .and_then(|offset| offset.checked_add(FILE_RECORD_SIZE * file_count))
        .and_then(|offset| {
            offset.checked_add(if name_mode.file_strings() {
                file_names_len
            } else {
                0
            })
        })
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

fn write_bstring(out: &mut impl Write, bytes: &[u8]) -> Result<()> {
    out.write_all(&[bytes.len().try_into()?])?;
    out.write_all(bytes)?;
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
