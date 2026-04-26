use super::{Archive, ArchiveInfo, Entry, Error, FileRecord, Result};
use crate::{read::Cursor, storage::Storage};
use bstr::BString;

const VERSION: u32 = 0x0000_0100;
const HEADER_SIZE: usize = 12;
const FILE_RECORD_SIZE: usize = 8;
const NAME_OFFSET_SIZE: usize = 4;
const HASH_SIZE: usize = 8;

pub(super) fn parse(storage: Storage) -> Result<Archive> {
    let bytes = storage.as_bytes();
    let header = read_header(bytes)?;
    let layout = Layout::new(bytes.len(), header)?;
    let records = read_file_records(bytes, header.file_count)?;
    let names = read_names(bytes, header.file_count, layout.names, layout.hashes)?;
    let hashes = read_hashes(bytes, header.file_count, layout.hashes)?;

    let file_count: usize = header.file_count.try_into()?;
    let mut entries = Vec::new();
    entries.try_reserve_exact(file_count)?;
    for index in 0..file_count {
        let record = records[index];
        validate_file_extent(record, layout.data, bytes.len())?;
        entries.push(Entry::new(names[index].clone(), record, hashes[index]));
    }

    Ok(Archive::from_parts(
        storage,
        ArchiveInfo {
            hash_offset: header.hash_offset,
            file_count: header.file_count,
        },
        entries,
        layout.data,
    ))
}

#[derive(Clone, Copy)]
struct RawHeader {
    hash_offset: u32,
    file_count: u32,
}

fn read_header(bytes: &[u8]) -> Result<RawHeader> {
    let mut cursor = Cursor::new(bytes);
    let version = cursor.u32()?;
    if version != VERSION {
        return Err(Error::InvalidVersion(version));
    }
    Ok(RawHeader {
        hash_offset: cursor.u32()?,
        file_count: cursor.u32()?,
    })
}

struct Layout {
    names: usize,
    hashes: usize,
    data: usize,
}

impl Layout {
    fn new(archive_size: usize, header: RawHeader) -> Result<Self> {
        let file_count: usize = header.file_count.try_into()?;
        let file_records_size = FILE_RECORD_SIZE
            .checked_mul(file_count)
            .ok_or(Error::OutOfBounds)?;
        let name_offsets = HEADER_SIZE
            .checked_add(file_records_size)
            .ok_or(Error::OutOfBounds)?;
        let name_offsets_size = NAME_OFFSET_SIZE
            .checked_mul(file_count)
            .ok_or(Error::OutOfBounds)?;
        let names = name_offsets
            .checked_add(name_offsets_size)
            .ok_or(Error::OutOfBounds)?;
        let hashes = HEADER_SIZE
            .checked_add(header.hash_offset.try_into()?)
            .ok_or(Error::OutOfBounds)?;
        let hashes_size = HASH_SIZE
            .checked_mul(file_count)
            .ok_or(Error::OutOfBounds)?;
        let data = hashes.checked_add(hashes_size).ok_or(Error::OutOfBounds)?;
        if names > hashes || data > archive_size {
            return Err(Error::OutOfBounds);
        }
        Ok(Self {
            names,
            hashes,
            data,
        })
    }
}

fn read_file_records(bytes: &[u8], file_count: u32) -> Result<Vec<FileRecord>> {
    let count: usize = file_count.try_into()?;
    let mut records = Vec::new();
    records.try_reserve_exact(count)?;
    let mut cursor = Cursor::new(bytes);
    cursor.seek(HEADER_SIZE)?;
    for _ in 0..file_count {
        records.push(FileRecord {
            size: cursor.u32()?,
            offset: cursor.u32()?,
        });
    }
    Ok(records)
}

fn read_names(
    bytes: &[u8],
    file_count: u32,
    names_offset: usize,
    hashes_offset: usize,
) -> Result<Vec<BString>> {
    let count: usize = file_count.try_into()?;
    let mut names = Vec::new();
    names.try_reserve_exact(count)?;
    let mut offsets = Cursor::new(bytes);
    offsets.seek(HEADER_SIZE + FILE_RECORD_SIZE * count)?;
    let names_blob = bytes
        .get(names_offset..hashes_offset)
        .ok_or(Error::OutOfBounds)?;
    for _ in 0..file_count {
        let relative: usize = offsets.u32()?.try_into()?;
        names.push(read_zstring(names_blob, relative)?);
    }
    Ok(names)
}

fn read_hashes(bytes: &[u8], file_count: u32, hashes_offset: usize) -> Result<Vec<u64>> {
    let count: usize = file_count.try_into()?;
    let mut hashes = Vec::new();
    hashes.try_reserve_exact(count)?;
    let mut cursor = Cursor::new(bytes);
    cursor.seek(hashes_offset)?;
    for _ in 0..file_count {
        hashes.push(cursor.u64()?);
    }
    Ok(hashes)
}

fn read_zstring(bytes: &[u8], offset: usize) -> Result<BString> {
    let mut cursor = Cursor::new(bytes);
    cursor.seek(offset)?;
    let mut out = Vec::new();
    loop {
        let byte = cursor.u8()?;
        if byte == 0 {
            return Ok(BString::from(out));
        }
        out.push(byte);
    }
}

fn validate_file_extent(record: FileRecord, data_offset: usize, archive_size: usize) -> Result<()> {
    let relative: usize = record.offset.try_into()?;
    let start = data_offset
        .checked_add(relative)
        .ok_or(Error::OutOfBounds)?;
    let len: usize = record.size.try_into()?;
    if start.checked_add(len).ok_or(Error::OutOfBounds)? > archive_size {
        return Err(Error::OutOfBounds);
    }
    Ok(())
}
