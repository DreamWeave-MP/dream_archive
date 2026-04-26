use super::{
    Archive, ArchiveFile, ArchiveOptions, Chunk, CompressionFormat, Entry, Error, FileHeader,
    Format, Hash, Result, TextureHeader, Version,
};
use bstr::BString;
use std::sync::Arc;

const MAGIC: u32 = u32::from_le_bytes(*b"BTDX");
const GNRL: u32 = u32::from_le_bytes(*b"GNRL");
const DX10: u32 = u32::from_le_bytes(*b"DX10");
const GNMF: u32 = u32::from_le_bytes(*b"GNMF");

const FILE_HEADER_SIZE_GNRL: u16 = 0x10;
const FILE_HEADER_SIZE_DX10: u16 = 0x18;
const FILE_HEADER_SIZE_GNMF: u16 = 0x30;
const CHUNK_SENTINEL: u32 = 0xBAAD_F00D;

pub(super) fn parse(bytes: Arc<[u8]>) -> Result<Archive> {
    let mut cursor = Cursor::new(&bytes);
    let header = RawHeader::read(&mut cursor)?;
    let mut entries = Vec::with_capacity(header.file_count);
    for _ in 0..header.file_count {
        entries.push(read_entry_record(&mut cursor, header.format, &bytes)?);
    }

    read_string_table(&bytes, header.string_table_offset, &mut entries)?;

    Ok(Archive::from_parts(
        bytes,
        ArchiveOptions {
            format: header.format,
            version: header.version,
            compression_format: header.compression_format,
            strings: header.string_table_offset != 0,
        },
        entries,
    ))
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
    let hash = Hash {
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
    Ok(Entry::new(hash, ArchiveFile { header, chunks }))
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

fn read_chunk(cursor: &mut Cursor<'_>, format: Format, bytes: &[u8]) -> Result<Chunk> {
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
    let chunk = Chunk::new(offset, packed, size, mips);
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
        entry.set_name(BString::new(names.bytes(len)?.to_vec()));
    }
    Ok(())
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
