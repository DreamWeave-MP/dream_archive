use super::{ArchiveVersion, Error, FileHash, Result, hash_file};
use crate::{BString, ByteSlice as _};
use crate::{CompressionOverride, builder_fs};
use flate2::{Compression, write::ZlibEncoder};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{self, BufWriter, Cursor, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const MAGIC: u32 = u32::from_le_bytes(*b"BTDX");
const GNRL: u32 = u32::from_le_bytes(*b"GNRL");
const HEADER_SIZE_V1: usize = 24;
const FILE_RECORD_SIZE_GNRL: usize = 36;
const FILE_HEADER_SIZE_GNRL: u16 = 0x10;
const CHUNK_SENTINEL: u32 = 0xBAAD_F00D;

/// Builder for string-backed BA2 GNRL archives.
///
/// This deliberately writes one boring GNRL chunk per file. DX10 has texture
/// metadata semantics. That is a separate feature, not a checkbox on a
/// constructor pretending to be done.
#[derive(Clone, Debug)]
pub struct Builder {
    version: ArchiveVersion,
    compression: Option<super::Ba2CompressionFormat>,
    zlib_level: Compression,
    entries: Vec<BuilderEntry>,
    names: HashSet<BString>,
}

#[derive(Clone, Debug)]
struct BuilderEntry {
    name: BString,
    hash: FileHash,
    compression: CompressionOverride,
    source: EntrySource,
}

#[derive(Clone, Debug)]
enum EntrySource {
    Bytes(Vec<u8>),
    File {
        path: PathBuf,
        len: u64,
    },
    ArchiveEntry {
        archive: Arc<super::Archive>,
        id: super::EntryId,
        len: u64,
    },
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            version: ArchiveVersion::v1,
            compression: None,
            zlib_level: Compression::default(),
            entries: Vec::new(),
            names: HashSet::new(),
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
    pub fn compression(&self) -> Option<super::Ba2CompressionFormat> {
        self.compression
    }

    pub fn set_compression(
        &mut self,
        compression: Option<super::Ba2CompressionFormat>,
    ) -> &mut Self {
        self.compression = compression;
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
    /// duplicate after BA2 path normalization, or allocation fails.
    pub fn add_bytes(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>) -> Result<()> {
        self.add_bytes_with_compression(path, bytes, CompressionOverride::Inherit)
    }

    /// Add bytes with an explicit per-file compression policy.
    ///
    /// # Errors
    ///
    /// Returns an error if the path can not be represented safely, is a
    /// duplicate after BA2 path normalization, or allocation fails.
    pub fn add_bytes_with_compression(
        &mut self,
        path: impl AsRef<[u8]>,
        bytes: impl AsRef<[u8]>,
        compression: CompressionOverride,
    ) -> Result<()> {
        let name = normalize_stored_path(path.as_ref())?;
        let (hash, normalized) = hash_file(name.as_bstr());
        debug_assert_eq!(name, normalized);
        if self.names.contains(name.as_bstr()) {
            return Err(Error::DuplicatePath);
        }

        let mut owned = Vec::new();
        owned.try_reserve_exact(bytes.as_ref().len())?;
        owned.extend_from_slice(bytes.as_ref());
        self.add_source(name, hash, compression, EntrySource::Bytes(owned))
    }

    /// Record a filesystem file and store it at `archive_path` when written.
    ///
    /// Symlinked files are followed for their payload bytes; the archive path is
    /// still the path supplied by the caller. Which is the point, otherwise this
    /// API would be a very small symlink-resolution surprise generator.
    ///
    /// # Errors
    ///
    /// Returns an error if source metadata lookup fails or adding the archive entry fails. Payload read errors are reported when writing.
    pub fn add_file(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        source: impl AsRef<Path>,
    ) -> Result<()> {
        self.add_file_with_compression(archive_path, source, CompressionOverride::Inherit)
    }

    /// Record a filesystem file with a per-file compression policy.
    ///
    /// # Errors
    ///
    /// Returns an error if source metadata lookup fails or adding the archive entry fails. Payload read errors are reported when writing.
    pub fn add_file_with_compression(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        source: impl AsRef<Path>,
        compression: CompressionOverride,
    ) -> Result<()> {
        let source = source.as_ref();
        let len = fs::metadata(source)?.len();
        let name = normalize_stored_path(archive_path.as_ref())?;
        let (hash, normalized) = hash_file(name.as_bstr());
        debug_assert_eq!(name, normalized);
        self.add_source(
            name,
            hash,
            compression,
            EntrySource::File {
                path: source.to_path_buf(),
                len,
            },
        )
    }

    /// Preserve an entry from another BA2 archive without materializing it in
    /// the builder. The entry is decoded and then stored according to this
    /// builder's compression policy when written.
    ///
    /// # Errors
    ///
    /// Returns an error if the archive path is invalid, duplicated, or the source id is invalid.
    pub fn add_archive_entry(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        archive: Arc<super::Archive>,
        id: super::EntryId,
    ) -> Result<()> {
        self.add_archive_entry_with_compression(
            archive_path,
            archive,
            id,
            CompressionOverride::Inherit,
        )
    }

    /// Preserve an entry from another BA2 archive with an explicit compression
    /// policy for the new archive.
    ///
    /// # Errors
    ///
    /// Returns an error if the archive path is invalid, duplicated, or the source id is invalid.
    pub fn add_archive_entry_with_compression(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        archive: Arc<super::Archive>,
        id: super::EntryId,
        compression: CompressionOverride,
    ) -> Result<()> {
        let len = archive.extracted_len_by_id(id)?;
        let name = normalize_stored_path(archive_path.as_ref())?;
        let (hash, normalized) = hash_file(name.as_bstr());
        debug_assert_eq!(name, normalized);
        self.add_source(
            name,
            hash,
            compression,
            EntrySource::ArchiveEntry { archive, id, len },
        )
    }

    /// Recursively add all files below `root` using paths relative to `root`.
    ///
    /// File symlinks are followed for payload bytes, but stored at the relative
    /// path where the symlink was found. Directory symlinks are ignored.
    ///
    /// # Errors
    ///
    /// Returns an error if directory traversal, source metadata lookup, or adding an entry fails. Payload read errors are reported when writing.
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
    /// Returns an error if directory traversal, source metadata lookup, or adding an entry fails. Payload read errors are reported when writing.
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

    /// Write the archive to a filesystem path.
    ///
    /// # Errors
    ///
    /// Returns an error if creating/writing the file fails or archive integer
    /// fields overflow their BA2 on-disk sizes.
    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = File::create(path)?;
        self.write_seek(BufWriter::new(file))
    }

    /// Write the archive to a byte vector.
    ///
    /// This necessarily buffers the final archive because the return value is a
    /// `Vec<u8>`. Use [`Self::write_seek`] for filesystem output.
    ///
    /// # Errors
    ///
    /// Returns an error if archive metadata overflows, output allocation fails, or a deferred source can not be read.
    pub fn to_vec(&self) -> Result<Vec<u8>> {
        let mut out = Cursor::new(Vec::new());
        self.write_seek(&mut out)?;
        Ok(out.into_inner())
    }

    /// Write the archive to a seekable output, streaming deferred payloads.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails, archive metadata overflows, or a deferred source can not be read.
    pub fn write_seek<W: Write + Seek>(&self, mut out: W) -> Result<()> {
        if self.compression == Some(super::Ba2CompressionFormat::LZ4)
            && self.version != ArchiveVersion::v3
            && self
                .entries
                .iter()
                .any(|entry| entry.is_compressed(self.compression))
        {
            return Err(Error::NotImplemented("BA2 LZ4 writer requires version 3"));
        }

        let entries = self.sorted_entries();
        let payload_start = payload_offset(entries.len(), self.version)?;

        write_u32(&mut out, MAGIC)?;
        write_u32(&mut out, self.version as u32)?;
        write_u32(&mut out, GNRL)?;
        write_u32(&mut out, entries.len().try_into()?)?;
        write_u64(&mut out, 0)?;
        if matches!(self.version, ArchiveVersion::v2 | ArchiveVersion::v3) {
            write_u64(&mut out, 1)?;
        }
        if self.version == ArchiveVersion::v3 {
            write_u32(
                &mut out,
                if self.compression == Some(super::Ba2CompressionFormat::LZ4) {
                    3
                } else {
                    0
                },
            )?;
        }

        for entry in &entries {
            write_hash(&mut out, entry.hash)?;
            out.write_all(&[0])?;
            out.write_all(&[1])?;
            write_u16(&mut out, FILE_HEADER_SIZE_GNRL)?;
            write_u64(&mut out, 0)?;
            write_u32(&mut out, 0)?;
            write_u32(&mut out, entry.source.len().try_into()?)?;
            write_u32(&mut out, CHUNK_SENTINEL)?;
        }

        out.seek(SeekFrom::Start(payload_start.try_into()?))?;
        for (index, entry) in entries.iter().enumerate() {
            let data_offset = out.stream_position()?;
            let stored_len = entry.write_payload(&mut out, self.compression, self.zlib_level)?;
            let resume = out.stream_position()?;
            let record_offset = u64::try_from(header_size(self.version)?)?
                + u64::try_from(
                    index
                        .checked_mul(FILE_RECORD_SIZE_GNRL)
                        .ok_or(Error::OutOfBounds)?,
                )?;
            out.seek(SeekFrom::Start(record_offset + 16))?;
            write_u64(&mut out, data_offset)?;
            write_u32(
                &mut out,
                if entry.is_compressed(self.compression) {
                    stored_len.try_into()?
                } else {
                    0
                },
            )?;
            out.seek(SeekFrom::Start(resume))?;
        }

        let string_table_offset = out.stream_position()?;
        out.seek(SeekFrom::Start(16))?;
        write_u64(&mut out, string_table_offset)?;
        out.seek(SeekFrom::Start(string_table_offset))?;
        for entry in &entries {
            write_u16(&mut out, entry.name.len().try_into()?)?;
            out.write_all(&entry.name)?;
        }
        Ok(())
    }

    fn add_source(
        &mut self,
        name: BString,
        hash: FileHash,
        compression: CompressionOverride,
        source: EntrySource,
    ) -> Result<()> {
        if self.names.contains(name.as_bstr()) {
            return Err(Error::DuplicatePath);
        }
        self.entries.try_reserve(1)?;
        self.names.try_reserve(1)?;
        self.names.insert(name.clone());
        self.entries.push(BuilderEntry {
            name,
            hash,
            compression,
            source,
        });
        Ok(())
    }

    fn sorted_entries(&self) -> Vec<&BuilderEntry> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by(|left, right| {
            left.hash
                .cmp(&right.hash)
                .then_with(|| left.name.cmp(&right.name))
        });
        entries
    }
}

impl BuilderEntry {
    fn is_compressed(&self, default: Option<super::Ba2CompressionFormat>) -> bool {
        match self.compression {
            CompressionOverride::Inherit => default.is_some(),
            CompressionOverride::Store => false,
            CompressionOverride::Compress => true,
        }
    }

    fn write_payload(
        &self,
        out: &mut impl Write,
        default: Option<super::Ba2CompressionFormat>,
        zlib_level: Compression,
    ) -> Result<u64> {
        if self.is_compressed(default) {
            match default.unwrap_or(super::Ba2CompressionFormat::Zip) {
                super::Ba2CompressionFormat::Zip => {
                    let mut counter = CountingWriter::new(out);
                    {
                        let mut encoder = ZlibEncoder::new(&mut counter, zlib_level);
                        self.source.copy_to(&mut encoder)?;
                        encoder.finish()?;
                    }
                    Ok(counter.written())
                }
                super::Ba2CompressionFormat::LZ4 => {
                    let bytes = self.source.to_vec()?;
                    let compressed = lz4_flex::block::compress(&bytes);
                    out.write_all(&compressed)?;
                    Ok(compressed.len().try_into()?)
                }
            }
        } else {
            self.source.copy_to(out)
        }
    }
}

impl EntrySource {
    fn len(&self) -> u64 {
        match self {
            Self::Bytes(bytes) => bytes.len() as u64,
            Self::File { len, .. } | Self::ArchiveEntry { len, .. } => *len,
        }
    }

    fn copy_to(&self, out: &mut impl Write) -> Result<u64> {
        match self {
            Self::Bytes(bytes) => {
                out.write_all(bytes)?;
                Ok(bytes.len().try_into()?)
            }
            Self::File { path, len } => {
                let mut file = File::open(path)?;
                let copied = io::copy(&mut file, out)?;
                if copied == *len {
                    Ok(copied)
                } else {
                    Err(Error::Io(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "deferred source file size changed before archive write",
                    )))
                }
            }
            Self::ArchiveEntry { archive, id, len } => {
                let copied = archive.extract_entry_by_id(*id, out)?;
                if copied == *len {
                    Ok(copied)
                } else {
                    Err(Error::OutOfBounds)
                }
            }
        }
    }

    fn to_vec(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.try_reserve_exact(self.len().try_into()?)?;
        self.copy_to(&mut out)?;
        Ok(out)
    }
}

struct CountingWriter<'a, W> {
    inner: &'a mut W,
    written: u64,
}

impl<'a, W> CountingWriter<'a, W> {
    const fn new(inner: &'a mut W) -> Self {
        Self { inner, written: 0 }
    }

    const fn written(&self) -> u64 {
        self.written
    }
}

impl<W: Write> Write for CountingWriter<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.written = self
            .written
            .checked_add(u64::try_from(written).map_err(io::Error::other)?)
            .ok_or_else(|| io::Error::other("byte counter overflow"))?;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn payload_offset(file_count: usize, version: ArchiveVersion) -> Result<usize> {
    header_size(version)?
        .checked_add(FILE_RECORD_SIZE_GNRL * file_count)
        .ok_or(Error::OutOfBounds)
}

pub(super) fn header_size(version: ArchiveVersion) -> Result<usize> {
    match version {
        ArchiveVersion::v1 | ArchiveVersion::v7 | ArchiveVersion::v8 => Ok(HEADER_SIZE_V1),
        ArchiveVersion::v2 => HEADER_SIZE_V1.checked_add(8).ok_or(Error::OutOfBounds),
        ArchiveVersion::v3 => HEADER_SIZE_V1.checked_add(12).ok_or(Error::OutOfBounds),
    }
}

pub(super) fn normalize_stored_path(path: &[u8]) -> Result<BString> {
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
    if components.is_empty() {
        return Err(Error::InvalidArchivePath);
    }

    let mut out = Vec::new();
    out.try_reserve_exact(path.len())?;
    for (index, component) in components.iter().enumerate() {
        if index != 0 {
            out.push(b'\\');
        }
        out.extend(component.iter().copied().map(|byte| match byte {
            b'A'..=b'Z' => byte + 32,
            _ => byte,
        }));
    }
    if out.len() >= 260 {
        return Err(Error::InvalidArchivePath);
    }
    Ok(BString::from(out))
}

pub(super) fn zlib_compress(bytes: &[u8], level: Compression) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), level);
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}

pub(super) fn write_hash(out: &mut impl Write, hash: FileHash) -> Result<()> {
    write_u32(out, hash.file)?;
    write_u32(out, hash.extension)?;
    write_u32(out, hash.directory)
}

pub(super) fn write_u16(out: &mut impl Write, value: u16) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}

pub(super) fn write_u32(out: &mut impl Write, value: u32) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}

pub(super) fn write_u64(out: &mut impl Write, value: u64) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}
