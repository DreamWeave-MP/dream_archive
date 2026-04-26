use super::{ArchiveVersion, Error, FileHash, Result, hash_file};
use bstr::{BString, ByteSlice as _};
use flate2::{Compression, write::ZlibEncoder};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
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
    entries: Vec<BuilderEntry>,
}

#[derive(Clone, Debug)]
struct BuilderEntry {
    name: BString,
    hash: FileHash,
    bytes: Vec<u8>,
}

struct PreparedEntry<'a> {
    entry: &'a BuilderEntry,
    stored: Vec<u8>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            version: ArchiveVersion::v1,
            compression: None,
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
    /// duplicate after BA2 path normalization, or allocation fails.
    pub fn add_bytes(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>) -> Result<()> {
        let name = normalize_stored_path(path.as_ref())?;
        let (hash, normalized) = hash_file(name.as_bstr());
        debug_assert_eq!(name, normalized);
        if self.entries.iter().any(|entry| entry.name == name) {
            return Err(Error::DuplicatePath);
        }

        let mut owned = Vec::new();
        owned.try_reserve_exact(bytes.as_ref().len())?;
        owned.extend_from_slice(bytes.as_ref());
        self.entries.push(BuilderEntry {
            name,
            hash,
            bytes: owned,
        });
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
        self.write_to(BufWriter::new(file))
    }

    /// Write the archive to a byte vector.
    ///
    /// # Errors
    ///
    /// Returns an error if archive integer fields overflow their BA2 on-disk
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
    /// their BA2 on-disk sizes.
    pub fn write_to(&self, mut out: impl Write) -> Result<()> {
        let entries = self.sorted_entries();
        let prepared = self.prepare_entries(&entries)?;
        let string_table_offset = string_table_offset(entries.len(), self.version)?;
        let payload_offset = string_table_offset
            .checked_add(string_table_len(&entries)?)
            .ok_or(Error::OutOfBounds)?;

        write_u32(&mut out, MAGIC)?;
        write_u32(&mut out, self.version as u32)?;
        write_u32(&mut out, GNRL)?;
        write_u32(&mut out, entries.len().try_into()?)?;
        write_u64(&mut out, string_table_offset.try_into()?)?;
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

        let mut next_payload_offset: u64 = payload_offset.try_into()?;
        for entry in &prepared {
            write_hash(&mut out, entry.entry.hash)?;
            out.write_all(&[0])?;
            out.write_all(&[1])?;
            write_u16(&mut out, FILE_HEADER_SIZE_GNRL)?;
            write_u64(&mut out, next_payload_offset)?;
            write_u32(
                &mut out,
                if self.compression.is_some() {
                    entry.stored.len().try_into()?
                } else {
                    0
                },
            )?;
            write_u32(&mut out, entry.entry.bytes.len().try_into()?)?;
            write_u32(&mut out, CHUNK_SENTINEL)?;
            next_payload_offset = next_payload_offset
                .checked_add(entry.stored.len().try_into()?)
                .ok_or(Error::OutOfBounds)?;
        }

        for entry in &entries {
            write_u16(&mut out, entry.name.len().try_into()?)?;
            out.write_all(&entry.name)?;
        }
        for entry in &prepared {
            out.write_all(&entry.stored)?;
        }
        Ok(())
    }

    fn prepare_entries<'a>(&self, entries: &[&'a BuilderEntry]) -> Result<Vec<PreparedEntry<'a>>> {
        if self.compression == Some(super::Ba2CompressionFormat::LZ4)
            && self.version != ArchiveVersion::v3
        {
            return Err(Error::NotImplemented);
        }
        let mut prepared = Vec::new();
        prepared.try_reserve_exact(entries.len())?;
        for entry in entries {
            let stored = match self.compression {
                None => entry.bytes.clone(),
                Some(super::Ba2CompressionFormat::Zip) => zlib_compress(&entry.bytes)?,
                Some(super::Ba2CompressionFormat::LZ4) => lz4_flex::block::compress(&entry.bytes),
            };
            prepared.push(PreparedEntry { entry, stored });
        }
        Ok(prepared)
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

    fn archive_size_hint(&self) -> Result<usize> {
        let entries = self.sorted_entries();
        let prepared = self.prepare_entries(&entries)?;
        string_table_offset(entries.len(), self.version)?
            .checked_add(string_table_len(&entries)?)
            .and_then(|offset| {
                offset.checked_add(
                    prepared
                        .iter()
                        .try_fold(0usize, |sum, entry| sum.checked_add(entry.stored.len()))?,
                )
            })
            .ok_or(Error::OutOfBounds)
    }
}

fn string_table_offset(file_count: usize, version: ArchiveVersion) -> Result<usize> {
    header_size(version)?
        .checked_add(FILE_RECORD_SIZE_GNRL * file_count)
        .ok_or(Error::OutOfBounds)
}

fn header_size(version: ArchiveVersion) -> Result<usize> {
    match version {
        ArchiveVersion::v1 | ArchiveVersion::v7 | ArchiveVersion::v8 => Ok(HEADER_SIZE_V1),
        ArchiveVersion::v2 => HEADER_SIZE_V1.checked_add(8).ok_or(Error::OutOfBounds),
        ArchiveVersion::v3 => HEADER_SIZE_V1.checked_add(12).ok_or(Error::OutOfBounds),
    }
}

fn string_table_len(entries: &[&BuilderEntry]) -> Result<usize> {
    entries.iter().try_fold(0usize, |sum, entry| {
        sum.checked_add(2 + entry.name.len())
            .ok_or(Error::OutOfBounds)
    })
}

fn normalize_stored_path(path: &[u8]) -> Result<BString> {
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

fn zlib_compress(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}

fn write_hash(out: &mut impl Write, hash: FileHash) -> Result<()> {
    write_u32(out, hash.file)?;
    write_u32(out, hash.extension)?;
    write_u32(out, hash.directory)
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
