use super::{
    Error, FileHash, Format, Result, Version, chunk::CompressionFormat, dds, dds::DdsHeader,
    hash_file,
};
use crate::{Borrowed, Copied};
use bstr::{BStr, BString, ByteSlice as _};
use std::{fs, path::Path, sync::Arc};

const MAGIC: u32 = u32::from_le_bytes(*b"BTDX");
const GNRL: u32 = u32::from_le_bytes(*b"GNRL");
const DX10: u32 = u32::from_le_bytes(*b"DX10");
const GNMF: u32 = u32::from_le_bytes(*b"GNMF");

const FILE_HEADER_SIZE_GNRL: u16 = 0x10;
const FILE_HEADER_SIZE_DX10: u16 = 0x18;
const FILE_HEADER_SIZE_GNMF: u16 = 0x30;
const CHUNK_SENTINEL: u32 = 0xBAAD_F00D;

/// Metadata read from the archive header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveOptions {
    pub format: Format,
    pub version: Version,
    pub compression_format: CompressionFormat,
    pub strings: bool,
}

impl Default for ArchiveOptions {
    fn default() -> Self {
        Self {
            format: Format::GNRL,
            version: Version::v1,
            compression_format: CompressionFormat::Zip,
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
    file: File,
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
    pub fn file(&self) -> &File {
        &self.file
    }
}

/// Chunked file stored in a BA2 archive.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct File {
    pub header: FileHeader,
    pub chunks: Vec<super::Chunk>,
}

impl File {
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
    bytes: Arc<[u8]>,
    options: ArchiveOptions,
    entries: Vec<Entry>,
}

impl Archive {
    /// Read an archive from an owned byte buffer.
    ///
    /// # Errors
    ///
    /// Returns an error when the buffer is not a valid supported BA2 archive.
    pub fn from_vec(bytes: Vec<u8>) -> Result<Self> {
        Self::parse(Arc::from(bytes.into_boxed_slice()))
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
        Self::from_vec(fs::read(path)?)
    }

    /// Metadata read from the archive header.
    #[must_use]
    pub fn options(&self) -> ArchiveOptions {
        self.options
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
        self.entries.iter().find(|entry| entry.hash == hash)
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

    /// Extract an entry into `out`.
    ///
    /// # Errors
    ///
    /// Returns an error if chunk offsets are invalid, decompression fails, the
    /// declared decompressed size does not match, or the file format is not yet
    /// implemented.
    pub fn read_entry_into(&self, entry: &Entry, out: &mut Vec<u8>) -> Result<()> {
        match entry.file.header {
            FileHeader::GNRL => self.extract_chunks(&entry.file, out),
            FileHeader::DX10(texture) => {
                dds::write_dds_header(out, texture.dds_header())?;
                self.extract_chunks(&entry.file, out)
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

    fn extract_chunks(&self, file: &File, out: &mut Vec<u8>) -> Result<()> {
        for chunk in &file.chunks {
            chunk.extract(&self.bytes, self.options.compression_format, out)?;
        }
        Ok(())
    }

    fn parse(bytes: Arc<[u8]>) -> Result<Self> {
        let mut cursor = Cursor::new(&bytes);
        let header = RawHeader::read(&mut cursor)?;
        let mut entries = Vec::with_capacity(header.file_count);
        for _ in 0..header.file_count {
            entries.push(read_entry_record(&mut cursor, header.format, &bytes)?);
        }

        read_string_table(&bytes, header.string_table_offset, &mut entries)?;

        Ok(Self {
            bytes,
            options: ArchiveOptions {
                format: header.format,
                version: header.version,
                compression_format: header.compression_format,
                strings: header.string_table_offset != 0,
            },
            entries,
        })
    }
}

struct RawHeader {
    format: Format,
    version: Version,
    file_count: usize,
    string_table_offset: u64,
    compression_format: CompressionFormat,
}

impl RawHeader {
    fn read(cursor: &mut Cursor<'_>) -> Result<Self> {
        let magic = cursor.u32()?;
        if magic != MAGIC {
            return Err(Error::InvalidMagic(magic));
        }
        let version = read_version(cursor.u32()?)?;
        let format = read_format(cursor.u32()?)?;
        let file_count = cursor.u32()? as usize;
        let string_table_offset = cursor.u64()?;
        if matches!(version, Version::v2 | Version::v3) {
            let _ = cursor.u64()?;
        }
        let compression_format = if version == Version::v3 && cursor.u32()? == 3 {
            CompressionFormat::LZ4
        } else {
            CompressionFormat::Zip
        };
        Ok(Self {
            format,
            version,
            file_count,
            string_table_offset,
            compression_format,
        })
    }
}

fn read_version(raw: u32) -> Result<Version> {
    match raw {
        1 => Ok(Version::v1),
        2 => Ok(Version::v2),
        3 => Ok(Version::v3),
        7 => Ok(Version::v7),
        8 => Ok(Version::v8),
        _ => Err(Error::InvalidVersion(raw)),
    }
}

fn read_format(raw: u32) -> Result<Format> {
    match raw {
        GNRL => Ok(Format::GNRL),
        DX10 => Ok(Format::DX10),
        GNMF => Ok(Format::GNMF),
        _ => Err(Error::InvalidFormat(raw)),
    }
}

fn read_entry_record(cursor: &mut Cursor<'_>, format: Format, bytes: &[u8]) -> Result<Entry> {
    let hash = super::Hash {
        file: cursor.u32()?,
        extension: cursor.u32()?,
        directory: cursor.u32()?,
    }
    .into();
    let _unknown = cursor.u8()?;
    let chunk_count = cursor.u8()? as usize;
    let file_header_size = cursor.u16()?;
    validate_file_header_size(format, file_header_size)?;
    let header = read_file_header(cursor, format)?;
    let mut chunks = Vec::with_capacity(chunk_count);
    for _ in 0..chunk_count {
        chunks.push(read_chunk(cursor, format, bytes)?);
    }
    Ok(Entry {
        name: BString::new(Vec::new()),
        hash,
        file: File { header, chunks },
    })
}

fn validate_file_header_size(format: Format, size: u16) -> Result<()> {
    if matches!(
        (format, size),
        (Format::GNRL, FILE_HEADER_SIZE_GNRL)
            | (Format::DX10, FILE_HEADER_SIZE_DX10)
            | (Format::GNMF, FILE_HEADER_SIZE_GNMF)
    ) {
        Ok(())
    } else {
        Err(Error::InvalidChunkSize(size))
    }
}

fn read_file_header(cursor: &mut Cursor<'_>, format: Format) -> Result<FileHeader> {
    Ok(match format {
        Format::GNRL => FileHeader::GNRL,
        Format::DX10 => FileHeader::DX10(TextureHeader {
            height: cursor.u16()?,
            width: cursor.u16()?,
            mip_count: cursor.u8()?,
            format: cursor.u8()?,
            flags: cursor.u8()?,
            tile_mode: cursor.u8()?,
        }),
        Format::GNMF => {
            let mut metadata = [0u32; 8];
            for slot in &mut metadata {
                *slot = cursor.u32()?;
            }
            FileHeader::GNMF(metadata)
        }
    })
}

fn read_chunk(cursor: &mut Cursor<'_>, format: Format, bytes: &[u8]) -> Result<super::Chunk> {
    let offset = cursor.u64()?;
    let packed = cursor.u32()?;
    let size = cursor.u32()?;
    let mips = match format {
        Format::GNRL => None,
        Format::DX10 | Format::GNMF => Some(cursor.u16()?..=cursor.u16()?),
    };
    let sentinel = cursor.u32()?;
    if sentinel != CHUNK_SENTINEL {
        return Err(Error::InvalidChunkSentinel(sentinel));
    }
    let chunk = super::Chunk::new(offset, packed, size, mips);
    let _ = chunk.stored_bytes(bytes)?;
    Ok(chunk)
}

fn read_string_table(bytes: &[u8], offset: u64, entries: &mut [Entry]) -> Result<()> {
    if offset == 0 {
        return Ok(());
    }
    let mut names = Cursor::new(bytes);
    names.seek(offset.try_into()?)?;
    for entry in entries {
        let len = names.u16()? as usize;
        entry.name = BString::new(names.bytes(len)?.to_vec());
    }
    Ok(())
}

impl TryFrom<Borrowed<'_>> for Archive {
    type Error = Error;
    fn try_from(value: Borrowed<'_>) -> Result<Self> {
        Self::read(value.0)
    }
}

impl TryFrom<Copied<'_>> for Archive {
    type Error = Error;
    fn try_from(value: Copied<'_>) -> Result<Self> {
        Self::read(value.0)
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.bytes.len() {
            return Err(Error::OutOfBounds);
        }
        self.pos = pos;
        Ok(())
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(len).ok_or(Error::OutOfBounds)?;
        let bytes = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))?;
        self.pos = end;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read as _, path::PathBuf};
    use walkdir::WalkDir;

    fn fixture(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
    }

    #[test]
    fn invalid_headers_match_expected_errors() {
        let root = fixture("bsa-rs/data/fo4_invalid_test");
        assert!(matches!(
            Archive::open_path(root.join("invalid_magic.ba2")),
            Err(Error::InvalidMagic(_))
        ));
        assert!(matches!(
            Archive::open_path(root.join("invalid_format.ba2")),
            Err(Error::InvalidFormat(_))
        ));
        assert!(matches!(
            Archive::open_path(root.join("invalid_version.ba2")),
            Err(Error::InvalidVersion(0x101))
        ));
        assert!(matches!(
            Archive::open_path(root.join("invalid_sentinel.ba2")),
            Err(Error::InvalidChunkSentinel(0xDEAD_BEEF))
        ));
        assert!(matches!(
            Archive::open_path(root.join("invalid_size.ba2")),
            Err(Error::InvalidChunkSize(0xCCCC))
        ));
    }

    #[test]
    fn missing_string_tables_are_accepted() {
        let root = fixture("bsa-rs/data/fo4_missing_string_table_test");
        let archive = Archive::open_path(root.join("in.ba2")).unwrap();
        assert_eq!(archive.options().format, Format::GNRL);
        let entry = archive.get("misc/example.txt").unwrap();
        assert!(entry.name().is_empty());
        let data = archive.read_entry(entry).unwrap();
        let expected = fs::read(root.join("data/misc/example.txt")).unwrap();
        assert_eq!(data, expected);
    }

    #[test]
    fn hashes_match_known_ba2_values() {
        let hash = super::super::hash_file(
            b"Textures\\CreationClub\\BGSFO4001\\AnimObjects\\PipBoy\\PipBoy02(Black)_d.DDS"
                .as_bstr(),
        )
        .0;
        assert_eq!(hash.file, 0x69E1_E82C);
        assert_eq!(hash.extension, 0x0073_6464);
        assert_eq!(hash.directory, 0x2315_7A84);

        let hash = super::super::hash_file(
            b"Materials/CreationClub/BGSFO4003/AnimObjects/PipBoy/PipBoyLabels01(Camo01).BGSM"
                .as_bstr(),
        )
        .0;
        assert_eq!(hash.file, 0x0785_843B);
        assert_eq!(hash.extension, 0x6D73_6762);
        assert_eq!(hash.directory, 0x8183_74CC);
    }

    #[test]
    fn lists_names_for_vfs_indexing() {
        let root = fixture("bsa-rs/data/fo4_next_gen_test");
        let archive = Archive::open_path(root.join("gnrl_v8.ba2")).unwrap();
        let names: Vec<_> = archive
            .entries()
            .iter()
            .map(|entry| entry.name().to_string())
            .collect();
        assert_eq!(names, ["License.txt", "SampleA.png"]);
        assert!(archive.contains("license.txt"));
        assert!(archive.contains("SampleA.png"));
    }

    #[test]
    fn reads_compressed_general_archives() {
        let root = fixture("bsa-rs/data/fo4_compression_test");
        for archive_name in ["normal.ba2", "xbox.ba2"] {
            let archive = Archive::open_path(root.join(archive_name)).unwrap();
            assert_eq!(archive.options().format, Format::GNRL);
            for item in WalkDir::new(root.join("data"))
                .into_iter()
                .filter_map(std::result::Result::ok)
            {
                if !item.file_type().is_file() {
                    continue;
                }
                let rel = item.path().strip_prefix(root.join("data")).unwrap();
                let rel = rel.to_string_lossy().replace('/', "\\");
                let data = archive.read_file(rel.as_bytes()).unwrap().unwrap();
                assert_eq!(data, fs::read(item.path()).unwrap());
            }
        }
    }

    #[test]
    fn reconstructs_dx10_dds() {
        let root = fixture("bsa-rs/data/fo4_dds_test");
        let archive = Archive::open_path(root.join("in.ba2")).unwrap();
        assert_eq!(archive.options().format, Format::DX10);
        let data = archive
            .read_file("Fence006_1K_Roughness.dds")
            .unwrap()
            .unwrap();
        let expected = fs::read(root.join("Fence006_1K_Roughness.dds")).unwrap();
        assert_eq!(data.len(), expected.len());
        assert_eq!(&data[148..], &expected[148..]);
        assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(data[128..132].try_into().unwrap()), 98);
        assert_eq!(u32::from_le_bytes(data[132..136].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(data[140..144].try_into().unwrap()), 1);
        // The source DDS fixture contains vendor/private reserved fields that
        // BA2 does not store. bsa-rs/DirectXTex regenerates these as zero; that
        // is the oracle we follow here, not the source file's NVT3 breadcrumb.
        assert_eq!(u32::from_le_bytes(data[68..72].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(data[144..148].try_into().unwrap()), 0);
    }

    #[test]
    fn reconstructs_cubemap_dds() {
        let root = fixture("bsa-rs/data/fo4_cubemap_test");
        let archive = Archive::open_path(root.join("in.ba2")).unwrap();
        let data = archive.read_file("blacksky_e.dds").unwrap().unwrap();
        assert_eq!(data, fs::read(root.join("blacksky_e.dds")).unwrap());
    }

    #[test]
    fn next_gen_versions_are_accepted() {
        let root = fixture("bsa-rs/data/fo4_next_gen_test");
        for (path, format, version) in [
            ("gnrl_v7.ba2", Format::GNRL, Version::v7),
            ("gnrl_v8.ba2", Format::GNRL, Version::v8),
            ("dx10_v7.ba2", Format::DX10, Version::v7),
            ("dx10_v8.ba2", Format::DX10, Version::v8),
        ] {
            let archive = Archive::open_path(root.join(path)).unwrap();
            assert_eq!(archive.options().format, format);
            assert_eq!(archive.options().version, version);
        }
    }

    #[test]
    fn next_gen_dx10_extracts_bsa_rs_compatible_dds() {
        let root = fixture("bsa-rs/data/fo4_next_gen_test");
        let expected = fs::read(root.join("dx10/Fence006_1K_Roughness.dds")).unwrap();
        for archive_name in ["dx10_v7.ba2", "dx10_v8.ba2"] {
            let archive = Archive::open_path(root.join(archive_name)).unwrap();
            let data = archive
                .read_file("Fence006_1K_Roughness.dds")
                .unwrap()
                .unwrap();
            assert_eq!(data.len(), expected.len());
            assert_eq!(&data[148..], &expected[148..]);
            assert_eq!(u32::from_le_bytes(data[24..28].try_into().unwrap()), 1);
            assert_eq!(u32::from_le_bytes(data[68..72].try_into().unwrap()), 0);
            assert_eq!(u32::from_le_bytes(data[144..148].try_into().unwrap()), 0);
        }
    }

    #[test]
    fn guessed_format_is_btdx() {
        let mut file = fs::File::open(fixture("bsa-rs/data/common_guess_test/fo4.ba2")).unwrap();
        assert_eq!(
            crate::guess_format(&mut file).unwrap(),
            Some(crate::FileFormat::FO4)
        );
        let mut rest = Vec::new();
        file.read_to_end(&mut rest).unwrap();
        assert!(!rest.is_empty());
    }
}
