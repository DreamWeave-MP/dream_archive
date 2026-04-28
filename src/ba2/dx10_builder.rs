use super::{
    ArchiveVersion, Error, FileHash, FileHeader, PayloadFormat, Result, TextureHeader, builder,
    hash_file,
};
use crate::{BString, ByteSlice as _};
use crate::{CompressionOverride, builder_fs, dds};
use flate2::Compression;
use std::{
    borrow::Cow,
    collections::HashSet,
    fs::{self, File},
    io::{BufWriter, Cursor, Seek, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const MAGIC: u32 = u32::from_le_bytes(*b"BTDX");
const DX10: u32 = u32::from_le_bytes(*b"DX10");
const FILE_HEADER_SIZE_DX10: u16 = 0x18;
const FILE_RECORD_BASE_SIZE_DX10: usize = 24;
const CHUNK_RECORD_SIZE_DX10: usize = 24;
const CHUNK_SENTINEL: u32 = 0xBAAD_F00D;

/// Builder for BA2 DX10 texture archives.
///
/// This can either take explicit BA2 texture metadata plus raw texture payload
/// bytes, or parse a supported DDS header and strip it before writing the BA2
/// texture payload. It does not transcode texture formats or generate mips.
#[derive(Clone, Debug)]
pub struct Dx10Builder {
    version: ArchiveVersion,
    compression: Option<super::Ba2CompressionFormat>,
    zlib_level: Compression,
    entries: Vec<TextureEntry>,
    names: HashSet<BString>,
}

#[derive(Clone, Debug)]
struct TextureEntry {
    name: BString,
    hash: FileHash,
    header: TextureHeader,
    compression: CompressionOverride,
    source: TextureSource,
}

struct PreparedTexture<'a> {
    entry: &'a TextureEntry,
    chunks: Vec<PreparedChunk<'a>>,
}

struct PreparedChunk<'a> {
    stored: PreparedStored<'a>,
    packed_size: u32,
    size: u32,
    first_mip: u16,
    last_mip: u16,
}

enum PreparedStored<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
    ArchiveChunk {
        archive: &'a super::Archive,
        chunk: &'a super::Chunk,
    },
}

#[derive(Clone, Debug)]
enum TextureSource {
    Bytes(Vec<u8>),
    File {
        path: PathBuf,
        len: u64,
    },
    ArchiveEntry {
        archive: Arc<super::Archive>,
        id: super::EntryId,
    },
}

impl Default for Dx10Builder {
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

impl Dx10Builder {
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

    /// Add one texture payload with explicit BA2 DX10 metadata.
    ///
    /// `bytes` must be the texture data stored after the DDS header, not a full
    /// DDS file. The archive reader reconstructs the DDS header from `header`
    /// during extraction.
    ///
    /// # Errors
    ///
    /// Returns an error if the path is invalid, the texture metadata can not be
    /// represented as a supported DDS header, the normalized path is duplicated,
    /// or allocation fails.
    pub fn add_texture_bytes(
        &mut self,
        path: impl AsRef<[u8]>,
        header: TextureHeader,
        bytes: impl AsRef<[u8]>,
    ) -> Result<()> {
        self.add_texture_bytes_with_compression(path, header, bytes, CompressionOverride::Inherit)
    }

    /// Add one texture payload with an explicit per-file compression policy.
    ///
    /// # Errors
    ///
    /// Returns an error if the path or texture metadata is invalid, the
    /// normalized path is duplicated, or allocation fails.
    pub fn add_texture_bytes_with_compression(
        &mut self,
        path: impl AsRef<[u8]>,
        header: TextureHeader,
        bytes: impl AsRef<[u8]>,
        compression: CompressionOverride,
    ) -> Result<()> {
        validate_texture_header(header)?;
        dds::validate_texture_payload_size(header, bytes.as_ref().len())?;
        let name = builder::normalize_stored_path(path.as_ref())?;
        let (hash, normalized) = hash_file(name.as_bstr());
        debug_assert_eq!(name, normalized);
        if self.names.contains(name.as_bstr()) {
            return Err(Error::DuplicatePath);
        }

        let mut owned = Vec::new();
        owned.try_reserve_exact(bytes.as_ref().len())?;
        owned.extend_from_slice(bytes.as_ref());
        self.add_source(name, hash, header, compression, TextureSource::Bytes(owned))
    }

    /// Add one texture from a DDS file held in memory.
    ///
    /// The DDS header is parsed and stripped; only the texture payload is stored
    /// in the BA2. Supported formats are the same formats this crate can
    /// reconstruct when extracting DX10 BA2 archives.
    ///
    /// # Errors
    ///
    /// Returns an error if the DDS header or payload layout is unsupported, the
    /// path is invalid, the normalized path is duplicated, or allocation fails.
    pub fn add_dds_bytes(&mut self, path: impl AsRef<[u8]>, dds: impl AsRef<[u8]>) -> Result<()> {
        self.add_dds_bytes_with_compression(path, dds, CompressionOverride::Inherit)
    }

    /// Add one DDS texture with an explicit per-file compression policy.
    ///
    /// # Errors
    ///
    /// Returns an error if the DDS header or payload layout is unsupported, the
    /// path is invalid, the normalized path is duplicated, or allocation fails.
    pub fn add_dds_bytes_with_compression(
        &mut self,
        path: impl AsRef<[u8]>,
        dds: impl AsRef<[u8]>,
        compression: CompressionOverride,
    ) -> Result<()> {
        let texture = dds::parse_dds_for_dx10(dds.as_ref())?;
        self.add_texture_bytes_with_compression(path, texture.header, texture.payload, compression)
    }

    /// Record a filesystem texture payload and store it at `archive_path` when
    /// written.
    ///
    /// The file is treated as raw payload bytes. This method does not parse DDS.
    ///
    /// # Errors
    ///
    /// Returns an error if source metadata lookup fails or adding the texture
    /// entry fails. Payload read errors are reported when writing.
    pub fn add_texture_file(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        header: TextureHeader,
        source: impl AsRef<Path>,
    ) -> Result<()> {
        self.add_texture_file_with_compression(
            archive_path,
            header,
            source,
            CompressionOverride::Inherit,
        )
    }

    /// Record a filesystem texture payload with a per-file compression policy.
    ///
    /// # Errors
    ///
    /// Returns an error if source metadata lookup fails or adding the texture
    /// entry fails. Payload read errors are reported when writing.
    pub fn add_texture_file_with_compression(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        header: TextureHeader,
        source: impl AsRef<Path>,
        compression: CompressionOverride,
    ) -> Result<()> {
        validate_texture_header(header)?;
        let len = fs::metadata(source.as_ref())?.len();
        dds::validate_texture_payload_size(header, len.try_into()?)?;
        let name = builder::normalize_stored_path(archive_path.as_ref())?;
        let (hash, normalized) = hash_file(name.as_bstr());
        debug_assert_eq!(name, normalized);
        self.add_source(
            name,
            hash,
            header,
            compression,
            TextureSource::File {
                path: source.as_ref().to_path_buf(),
                len,
            },
        )
    }

    /// Preserve one DX10 texture entry from another BA2 archive without
    /// materializing its DDS representation in the builder.
    ///
    /// The BA2-native texture header, chunk records, and stored chunk bytes are
    /// copied unchanged when this archive is written. The output archive path is
    /// supplied by the caller; this is a repack operation, not a path alias.
    ///
    /// # Errors
    ///
    /// Returns an error if the source archive is not DX10, the entry id is
    /// invalid, the source entry is not a DX10 texture, or the output path is
    /// invalid or duplicated.
    pub fn add_archive_entry(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        archive: Arc<super::Archive>,
        id: super::EntryId,
    ) -> Result<()> {
        if archive.info().format != PayloadFormat::DX10 {
            return Err(Error::NotImplemented(
                "BA2 DX10 builder can only preserve DX10 archives",
            ));
        }
        let entry = archive.entry_by_id_required(id)?;
        let FileHeader::DX10(header) = entry.file().header else {
            return Err(Error::NotImplemented(
                "BA2 DX10 builder can only preserve DX10 entries",
            ));
        };
        let name = builder::normalize_stored_path(archive_path.as_ref())?;
        let (hash, normalized) = hash_file(name.as_bstr());
        debug_assert_eq!(name, normalized);
        self.add_source(
            name,
            hash,
            header,
            CompressionOverride::Inherit,
            TextureSource::ArchiveEntry { archive, id },
        )
    }

    /// Read a DDS file, parse and strip its header, and store its texture payload.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the file fails, the DDS is unsupported, or
    /// adding the archive entry fails.
    pub fn add_dds_file(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        source: impl AsRef<Path>,
    ) -> Result<()> {
        self.add_dds_file_with_compression(archive_path, source, CompressionOverride::Inherit)
    }

    /// Read a DDS file with a per-file compression policy.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the file fails, the DDS is unsupported, or
    /// adding the archive entry fails.
    pub fn add_dds_file_with_compression(
        &mut self,
        archive_path: impl AsRef<[u8]>,
        source: impl AsRef<Path>,
        compression: CompressionOverride,
    ) -> Result<()> {
        let bytes = fs::read(source)?;
        self.add_dds_bytes_with_compression(archive_path, bytes, compression)
    }

    /// Recursively add all files below `root` with the same texture metadata.
    ///
    /// This is mostly useful for synthetic archives and tests. Real texture
    /// directories generally need per-file dimensions and formats; pretending
    /// otherwise would be, technically, garbage.
    ///
    /// # Errors
    ///
    /// Returns an error if traversal, file reading, or adding an entry fails.
    pub fn add_dir_with_texture_header(
        &mut self,
        root: impl AsRef<Path>,
        header: TextureHeader,
    ) -> Result<()> {
        let root = root.as_ref();
        for path in builder_fs::collect_files(root)? {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| Error::InvalidArchivePath)?;
            let archive_path =
                builder_fs::path_to_archive_bytes(relative).ok_or(Error::InvalidArchivePath)?;
            self.add_texture_file(archive_path, header, &path)?;
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
    /// # Errors
    ///
    /// Returns an error if archive integer fields overflow their BA2 on-disk
    /// sizes or output allocation fails.
    pub fn to_vec(&self) -> Result<Vec<u8>> {
        let mut out = Cursor::new(Vec::new());
        self.write_seek(&mut out)?;
        Ok(out.into_inner())
    }

    /// Write the archive to `out`.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails, archive integer fields overflow their
    /// BA2 on-disk sizes, a deferred source can not be read, or a preserved
    /// archive entry can not be copied honestly under this archive's version and
    /// compression header.
    pub fn write_seek<W: Write + Seek>(&self, mut out: W) -> Result<()> {
        let entries = self.sorted_entries();
        let prepared = self.prepare_entries(&entries)?;
        let payload_offset = payload_offset(&entries, self.version)?;
        let string_table_offset = prepared.iter().try_fold(payload_offset, |offset, entry| {
            entry.chunks.iter().try_fold(offset, |offset, chunk| {
                offset
                    .checked_add(chunk.stored.len()?)
                    .ok_or(Error::OutOfBounds)
            })
        })?;

        builder::write_u32(&mut out, MAGIC)?;
        builder::write_u32(&mut out, self.version as u32)?;
        builder::write_u32(&mut out, DX10)?;
        builder::write_u32(&mut out, entries.len().try_into()?)?;
        builder::write_u64(&mut out, string_table_offset.try_into()?)?;
        if matches!(self.version, ArchiveVersion::v2 | ArchiveVersion::v3) {
            builder::write_u64(&mut out, 1)?;
        }
        if self.version == ArchiveVersion::v3 {
            builder::write_u32(
                &mut out,
                if self.compression == Some(super::Ba2CompressionFormat::LZ4) {
                    3
                } else {
                    0
                },
            )?;
        }

        let mut next_payload_offset: u64 = payload_offset.try_into()?;
        for entry in &prepared {
            write_texture_record(&mut out, entry, next_payload_offset)?;
            next_payload_offset =
                entry
                    .chunks
                    .iter()
                    .try_fold(next_payload_offset, |offset, chunk| {
                        offset
                            .checked_add(u64::try_from(chunk.stored.len()?)?)
                            .ok_or(Error::OutOfBounds)
                    })?;
        }

        for entry in &prepared {
            for chunk in &entry.chunks {
                chunk.stored.write_to(&mut out)?;
            }
        }
        for entry in &entries {
            builder::write_u16(&mut out, entry.name.len().try_into()?)?;
            out.write_all(&entry.name)?;
        }
        Ok(())
    }

    fn prepare_entries<'a>(
        &self,
        entries: &[&'a TextureEntry],
    ) -> Result<Vec<PreparedTexture<'a>>> {
        if self.compression == Some(super::Ba2CompressionFormat::LZ4)
            && self.version != ArchiveVersion::v3
            && entries
                .iter()
                .any(|entry| entry.is_compressed(self.compression))
        {
            return Err(Error::NotImplemented("BA2 LZ4 writer requires version 3"));
        }
        let mut prepared = Vec::new();
        prepared.try_reserve_exact(entries.len())?;
        for entry in entries {
            prepared.push(PreparedTexture {
                entry,
                chunks: entry.prepare_chunks(self.compression, self.zlib_level, self.version)?,
            });
        }
        Ok(prepared)
    }

    fn sorted_entries(&self) -> Vec<&TextureEntry> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by(|left, right| {
            left.hash
                .cmp(&right.hash)
                .then_with(|| left.name.cmp(&right.name))
        });
        entries
    }

    fn add_source(
        &mut self,
        name: BString,
        hash: FileHash,
        header: TextureHeader,
        compression: CompressionOverride,
        source: TextureSource,
    ) -> Result<()> {
        if self.names.contains(name.as_bstr()) {
            return Err(Error::DuplicatePath);
        }
        self.entries.try_reserve(1)?;
        self.names.try_reserve(1)?;
        self.names.insert(name.clone());
        self.entries.push(TextureEntry {
            name,
            hash,
            header,
            compression,
            source,
        });
        Ok(())
    }
}

impl TextureEntry {
    fn file_record_size(&self) -> Result<usize> {
        let chunk_count = match &self.source {
            TextureSource::Bytes(_) | TextureSource::File { .. } => 1,
            TextureSource::ArchiveEntry { archive, id } => {
                archive.entry_by_id_required(*id)?.file().chunks().len()
            }
        };
        file_record_size(chunk_count)
    }

    fn is_compressed(&self, default: Option<super::Ba2CompressionFormat>) -> bool {
        match self.compression {
            CompressionOverride::Inherit => default.is_some(),
            CompressionOverride::Store => false,
            CompressionOverride::Compress => true,
        }
    }

    fn prepare_chunks(
        &self,
        default: Option<super::Ba2CompressionFormat>,
        zlib_level: Compression,
        target_version: ArchiveVersion,
    ) -> Result<Vec<PreparedChunk<'_>>> {
        match &self.source {
            TextureSource::Bytes(bytes) => {
                self.prepare_payload_chunk(Cow::Borrowed(bytes.as_slice()), default, zlib_level)
            }
            TextureSource::File { path, len } => {
                let bytes = fs::read(path)?;
                if u64::try_from(bytes.len())? != *len {
                    return Err(Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "deferred source file size changed before archive write",
                    )));
                }
                self.prepare_payload_chunk(Cow::Owned(bytes), default, zlib_level)
            }
            TextureSource::ArchiveEntry { archive, id } => {
                validate_preserved_archive_entry(archive, *id, target_version, default)?;
                let entry = archive.entry_by_id_required(*id)?;
                let mut chunks = Vec::new();
                chunks.try_reserve_exact(entry.file().chunks().len())?;
                for chunk in entry.file().chunks() {
                    let mips = chunk
                        .mips
                        .clone()
                        .ok_or(Error::Dds("missing BA2 texture mip range"))?;
                    chunks.push(PreparedChunk {
                        stored: PreparedStored::ArchiveChunk { archive, chunk },
                        packed_size: chunk.packed_size(),
                        size: chunk.size(),
                        first_mip: *mips.start(),
                        last_mip: *mips.end(),
                    });
                }
                Ok(chunks)
            }
        }
    }

    fn prepare_payload_chunk<'a>(
        &self,
        bytes: Cow<'a, [u8]>,
        default: Option<super::Ba2CompressionFormat>,
        zlib_level: Compression,
    ) -> Result<Vec<PreparedChunk<'a>>> {
        let (stored, packed_size) = if self.is_compressed(default) {
            match default.unwrap_or(super::Ba2CompressionFormat::Zip) {
                super::Ba2CompressionFormat::Zip => {
                    let compressed = builder::zlib_compress(&bytes, zlib_level)?;
                    let packed_size = compressed.len().try_into()?;
                    (PreparedStored::Owned(compressed), packed_size)
                }
                super::Ba2CompressionFormat::LZ4 => {
                    let compressed = lz4_flex::block::compress(&bytes);
                    let packed_size = compressed.len().try_into()?;
                    (PreparedStored::Owned(compressed), packed_size)
                }
            }
        } else {
            match bytes {
                Cow::Borrowed(bytes) => (PreparedStored::Borrowed(bytes), 0),
                Cow::Owned(bytes) => (PreparedStored::Owned(bytes), 0),
            }
        };
        Ok(vec![PreparedChunk {
            stored,
            packed_size,
            size: self.source.payload_len()?.try_into()?,
            first_mip: 0,
            last_mip: u16::from(self.header.mip_count) - 1,
        }])
    }
}

fn validate_preserved_archive_entry(
    archive: &super::Archive,
    id: super::EntryId,
    target_version: ArchiveVersion,
    target_compression: Option<super::Ba2CompressionFormat>,
) -> Result<()> {
    let info = archive.info();
    if info.format != PayloadFormat::DX10 {
        return Err(Error::NotImplemented(
            "BA2 DX10 builder can only preserve DX10 archives",
        ));
    }
    if info.version != target_version {
        return Err(Error::NotImplemented(
            "BA2 DX10 archive entry preservation requires matching archive versions",
        ));
    }
    let target_compression = target_compression.unwrap_or(super::Ba2CompressionFormat::Zip);
    if info.compression_format != target_compression {
        return Err(Error::NotImplemented(
            "BA2 DX10 archive entry preservation requires matching compression formats",
        ));
    }
    let entry = archive.entry_by_id_required(id)?;
    if !matches!(entry.file().header, FileHeader::DX10(_)) {
        return Err(Error::NotImplemented(
            "BA2 DX10 builder can only preserve DX10 entries",
        ));
    }
    Ok(())
}

impl TextureSource {
    fn payload_len(&self) -> Result<u64> {
        match self {
            Self::Bytes(bytes) => Ok(bytes.len().try_into()?),
            Self::File { len, .. } => Ok(*len),
            Self::ArchiveEntry { archive, id } => {
                let entry = archive.entry_by_id_required(*id)?;
                entry.file().chunks().iter().try_fold(0u64, |sum, chunk| {
                    sum.checked_add(u64::from(chunk.size()))
                        .ok_or(Error::OutOfBounds)
                })
            }
        }
    }
}

impl PreparedStored<'_> {
    fn len(&self) -> Result<usize> {
        Ok(match self {
            Self::Borrowed(bytes) => bytes.len(),
            Self::Owned(bytes) => bytes.len(),
            Self::ArchiveChunk { chunk, .. } => chunk.stored_size().try_into()?,
        })
    }

    fn write_to(&self, out: &mut impl Write) -> Result<u64> {
        match self {
            Self::Borrowed(bytes) => {
                out.write_all(bytes)?;
                Ok(bytes.len().try_into()?)
            }
            Self::Owned(bytes) => {
                out.write_all(bytes)?;
                Ok(bytes.len().try_into()?)
            }
            Self::ArchiveChunk { archive, chunk } => archive.write_stored_chunk(chunk, out),
        }
    }
}

fn validate_texture_header(header: TextureHeader) -> Result<()> {
    if header.mip_count == 0 {
        return Err(Error::Dds("zero mip count"));
    }
    dds::validate_texture_header(header)
}

fn payload_offset(entries: &[&TextureEntry], version: ArchiveVersion) -> Result<usize> {
    entries
        .iter()
        .try_fold(builder::header_size(version)?, |sum, entry| {
            sum.checked_add(entry.file_record_size()?)
                .ok_or(Error::OutOfBounds)
        })
}

fn file_record_size(chunk_count: usize) -> Result<usize> {
    FILE_RECORD_BASE_SIZE_DX10
        .checked_add(
            chunk_count
                .checked_mul(CHUNK_RECORD_SIZE_DX10)
                .ok_or(Error::OutOfBounds)?,
        )
        .ok_or(Error::OutOfBounds)
}

fn write_texture_record(
    out: &mut impl Write,
    prepared: &PreparedTexture<'_>,
    payload_offset: u64,
) -> Result<()> {
    let entry = prepared.entry;
    builder::write_hash(out, entry.hash)?;
    out.write_all(&[0])?;
    out.write_all(&[prepared.chunks.len().try_into()?])?;
    builder::write_u16(out, FILE_HEADER_SIZE_DX10)?;
    builder::write_u16(out, entry.header.height)?;
    builder::write_u16(out, entry.header.width)?;
    out.write_all(&[entry.header.mip_count])?;
    out.write_all(&[entry.header.format])?;
    out.write_all(&[entry.header.flags])?;
    out.write_all(&[entry.header.tile_mode])?;
    let mut chunk_offset = payload_offset;
    for chunk in &prepared.chunks {
        builder::write_u64(out, chunk_offset)?;
        builder::write_u32(out, chunk.packed_size)?;
        builder::write_u32(out, chunk.size)?;
        builder::write_u16(out, chunk.first_mip)?;
        builder::write_u16(out, chunk.last_mip)?;
        builder::write_u32(out, CHUNK_SENTINEL)?;
        chunk_offset = chunk_offset
            .checked_add(u64::try_from(chunk.stored.len()?)?)
            .ok_or(Error::OutOfBounds)?;
    }
    Ok(())
}
