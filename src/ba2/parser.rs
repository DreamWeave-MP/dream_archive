use super::{
    Archive, ArchiveFile, ArchiveInfo, ArchiveVersion, Ba2CompressionFormat, Chunk, Entry, Error,
    FileHeader, Hash, PayloadFormat, Result, TextureHeader,
};
use crate::read::Cursor;
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
        ArchiveInfo {
            format: header.format,
            version: header.version,
            compression_format: header.compression_format,
            strings: header.string_table_offset != 0,
        },
        entries,
    ))
}

struct RawHeader {
    format: PayloadFormat,
    version: ArchiveVersion,
    file_count: usize,
    string_table_offset: u64,
    compression_format: Ba2CompressionFormat,
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
        if matches!(version, ArchiveVersion::v2 | ArchiveVersion::v3) {
            let _ = cursor.u64()?;
        }
        let compression_format = if version == ArchiveVersion::v3 && cursor.u32()? == 3 {
            Ba2CompressionFormat::LZ4
        } else {
            Ba2CompressionFormat::Zip
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

fn read_version(raw: u32) -> Result<ArchiveVersion> {
    match raw {
        1 => Ok(ArchiveVersion::v1),
        2 => Ok(ArchiveVersion::v2),
        3 => Ok(ArchiveVersion::v3),
        7 => Ok(ArchiveVersion::v7),
        8 => Ok(ArchiveVersion::v8),
        _ => Err(Error::InvalidVersion(raw)),
    }
}

fn read_format(raw: u32) -> Result<PayloadFormat> {
    match raw {
        GNRL => Ok(PayloadFormat::GNRL),
        DX10 => Ok(PayloadFormat::DX10),
        GNMF => Ok(PayloadFormat::GNMF),
        _ => Err(Error::InvalidFormat(raw)),
    }
}

fn read_entry_record(
    cursor: &mut Cursor<'_>,
    format: PayloadFormat,
    bytes: &[u8],
) -> Result<Entry> {
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

fn validate_file_header_size(format: PayloadFormat, size: u16) -> Result<()> {
    if matches!(
        (format, size),
        (PayloadFormat::GNRL, FILE_HEADER_SIZE_GNRL)
            | (PayloadFormat::DX10, FILE_HEADER_SIZE_DX10)
            | (PayloadFormat::GNMF, FILE_HEADER_SIZE_GNMF)
    ) {
        Ok(())
    } else {
        Err(Error::InvalidChunkSize(size))
    }
}

fn read_file_header(cursor: &mut Cursor<'_>, format: PayloadFormat) -> Result<FileHeader> {
    Ok(match format {
        PayloadFormat::GNRL => FileHeader::GNRL,
        PayloadFormat::DX10 => FileHeader::DX10(TextureHeader {
            height: cursor.u16()?,
            width: cursor.u16()?,
            mip_count: cursor.u8()?,
            format: cursor.u8()?,
            flags: cursor.u8()?,
            tile_mode: cursor.u8()?,
        }),
        PayloadFormat::GNMF => {
            let mut metadata = [0u32; 8];
            for slot in &mut metadata {
                *slot = cursor.u32()?;
            }
            FileHeader::GNMF(metadata)
        }
    })
}

fn read_chunk(cursor: &mut Cursor<'_>, format: PayloadFormat, bytes: &[u8]) -> Result<Chunk> {
    let offset = cursor.u64()?;
    let packed = cursor.u32()?;
    let size = cursor.u32()?;
    let mips = match format {
        PayloadFormat::GNRL => None,
        PayloadFormat::DX10 | PayloadFormat::GNMF => Some(cursor.u16()?..=cursor.u16()?),
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
