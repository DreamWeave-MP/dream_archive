use super::{
    ArchiveVersion, Ba2CompressionFormat, Error, FileHash, PayloadFormat, Result, dds,
    dds::DdsHeader, hash_file, parser,
};
use crate::{Copied, storage::Storage};
use bstr::{BStr, BString, ByteSlice as _};
use std::collections::HashMap;
use std::path::Path;

/// Metadata read from the archive header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveInfo {
    pub format: PayloadFormat,
    pub version: ArchiveVersion,
    pub compression_format: Ba2CompressionFormat,
    pub strings: bool,
}

impl Default for ArchiveInfo {
    fn default() -> Self {
        Self {
            format: PayloadFormat::GNRL,
            version: ArchiveVersion::v1,
            compression_format: Ba2CompressionFormat::Zip,
            strings: false,
        }
    }
}

/// Texture file header stored in DX10 BA2 archives.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextureHeader {
    pub height: u16,
    pub width: u16,
    pub mip_count: u8,
    pub format: u8,
    /// Low byte of the BA2 texture flags field. Bit 0 marks cubemaps.
    pub flags: u8,
    /// High byte of the BA2 texture flags field. `OpenMW` treats this as tile mode.
    pub tile_mode: u8,
}

impl TextureHeader {
    fn dds_header(self) -> DdsHeader {
        DdsHeader {
            height: self.height,
            width: self.width,
            mip_count: self.mip_count,
            format: self.format,
            flags: self.flags,
        }
    }
}

/// Per-file metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum FileHeader {
    #[default]
    GNRL,
    DX10(TextureHeader),
    GNMF([u32; 8]),
}

/// A file entry in the archive index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    name: BString,
    hash: FileHash,
    file: ArchiveFile,
}

impl Entry {
    #[must_use]
    pub fn name(&self) -> &BStr {
        self.name.as_bstr()
    }
    #[must_use]
    pub fn hash(&self) -> FileHash {
        self.hash
    }
    #[must_use]
    pub fn file(&self) -> &ArchiveFile {
        &self.file
    }
}

/// Chunked file stored in a BA2 archive.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ArchiveFile {
    pub header: FileHeader,
    pub chunks: Vec<super::Chunk>,
}

impl ArchiveFile {
    #[must_use]
    pub fn len(&self) -> usize {
        self.chunks.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
    #[must_use]
    pub fn chunks(&self) -> &[super::Chunk] {
        &self.chunks
    }
}

/// Parsed BA2 archive. The archive bytes are immutable, so extraction can be
/// performed through shared references.
#[derive(Clone, Debug)]
pub struct Archive {
    storage: Storage,
    info: ArchiveInfo,
    entries: Vec<Entry>,
    lookup: HashMap<FileHash, usize>,
}

impl Archive {
    /// Read an archive from an owned byte buffer.
    ///
    /// # Errors
    ///
    /// Returns an error when the buffer is not a valid supported BA2 archive.
    pub fn from_vec(bytes: Vec<u8>) -> Result<Self> {
        parser::parse(Storage::from_vec(bytes))
    }

    /// Read an archive from a byte slice, copying the archive data.
    ///
    /// # Errors
    ///
    /// Returns an error when the slice is not a valid supported BA2 archive.
    pub fn read(bytes: &[u8]) -> Result<Self> {
        Self::from_vec(bytes.to_vec())
    }

    /// Read an archive from a filesystem path.
    ///
    /// # Errors
    ///
    /// Returns an I/O error if the path can not be read, or a parse error when
    /// the file is not a valid supported BA2 archive.
    pub fn open_path(path: impl AsRef<Path>) -> Result<Self> {
        parser::parse(Storage::open_path(path)?)
    }

    /// Metadata read from the archive header.
    #[must_use]
    pub fn info(&self) -> ArchiveInfo {
        self.info
    }

    /// All entries, in archive table order.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Number of files in the archive.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get an entry by BA2 hash. Names are not required for this lookup.
    #[must_use]
    pub fn get_by_hash(&self, hash: FileHash) -> Option<&Entry> {
        self.lookup.get(&hash).map(|&index| &self.entries[index])
    }

    /// Get an entry by path. The path is normalized using BA2 rules.
    #[must_use]
    pub fn get(&self, path: impl AsRef<[u8]>) -> Option<&Entry> {
        self.get_by_hash(hash_file(path.as_ref().as_bstr()).0)
    }

    /// Whether a path exists in this archive.
    #[must_use]
    pub fn contains(&self, path: impl AsRef<[u8]>) -> bool {
        self.get(path).is_some()
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
    /// Returns an error if chunk offsets are invalid, decompression fails, the
    /// declared decompressed size does not match, or the file format is not yet
    /// implemented.
    pub fn read_entry_into(&self, entry: &Entry, out: &mut Vec<u8>) -> Result<()> {
        let before = out.len();
        let result = match entry.file.header {
            FileHeader::GNRL => self.extract_chunks(&entry.file, out),
            FileHeader::DX10(texture) => dds::write_dds_header(out, texture.dds_header())
                .and_then(|()| self.extract_chunks(&entry.file, out)),
            FileHeader::GNMF(_) => Err(Error::NotImplemented),
        };
        if result.is_err() {
            out.truncate(before);
        }
        result
    }

    /// Extract an entry into a writer.
    ///
    /// # Errors
    ///
    /// Returns an error if chunk offsets are invalid, decompression fails, the
    /// declared decompressed size does not match, writing fails, or the file
    /// format is not yet implemented.
    pub fn extract_entry(&self, entry: &Entry, mut out: impl std::io::Write) -> Result<u64> {
        match entry.file.header {
            FileHeader::GNRL => self.extract_chunks_to_writer(&entry.file, &mut out),
            FileHeader::DX10(texture) => {
                let mut header = Vec::new();
                dds::write_dds_header(&mut header, texture.dds_header())?;
                out.write_all(&header)?;
                Ok(u64::try_from(header.len())?
                    + self.extract_chunks_to_writer(&entry.file, &mut out)?)
            }
            FileHeader::GNMF(_) => Err(Error::NotImplemented),
        }
    }

    /// Extract an entry into a new vector.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::read_entry_into`].
    pub fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        out.try_reserve_exact(entry.extraction_capacity_hint()?)?;
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

    fn extract_chunks(&self, file: &ArchiveFile, out: &mut Vec<u8>) -> Result<()> {
        for chunk in &file.chunks {
            chunk.extract(self.storage.as_bytes(), self.info.compression_format, out)?;
        }
        Ok(())
    }

    fn extract_chunks_to_writer(
        &self,
        file: &ArchiveFile,
        out: &mut impl std::io::Write,
    ) -> Result<u64> {
        let mut written = 0u64;
        for chunk in &file.chunks {
            written += chunk.extract_to_writer(
                self.storage.as_bytes(),
                self.info.compression_format,
                out,
            )?;
        }
        Ok(written)
    }

    pub(super) fn from_parts(storage: Storage, info: ArchiveInfo, entries: Vec<Entry>) -> Self {
        let mut lookup = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            lookup.entry(entry.hash).or_insert(index);
        }
        Self {
            storage,
            info,
            entries,
            lookup,
        }
    }
}

impl Entry {
    pub(super) fn new(hash: FileHash, file: ArchiveFile) -> Self {
        Self {
            name: BString::new(Vec::new()),
            hash,
            file,
        }
    }

    pub(super) fn set_name(&mut self, name: BString) {
        self.name = name;
    }

    fn extraction_capacity_hint(&self) -> Result<usize> {
        let chunks_size = self.file.decompressed_size()?;
        if matches!(self.file.header, FileHeader::DX10(_)) {
            chunks_size
                .checked_add(dds::MAX_HEADER_SIZE)
                .ok_or(Error::OutOfBounds)
        } else {
            Ok(chunks_size)
        }
    }
}

impl ArchiveFile {
    fn decompressed_size(&self) -> Result<usize> {
        self.chunks.iter().try_fold(0usize, |sum, chunk| {
            let size: usize = chunk.size().try_into()?;
            sum.checked_add(size).ok_or(Error::OutOfBounds)
        })
    }
}

impl TryFrom<Copied<'_>> for Archive {
    type Error = Error;
    fn try_from(value: Copied<'_>) -> Result<Self> {
        Self::read(value.0)
    }
}
