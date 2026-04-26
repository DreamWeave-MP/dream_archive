use super::{
    Archive, ArchiveFlags, ArchiveInfo, ArchiveTypes, ArchiveVersion, Entry, Error, FileRecord,
    Result,
};
use crate::{read::Cursor, storage::Storage};
use bstr::BString;

const MAGIC: u32 = u32::from_le_bytes(*b"BSA\0");
const HEADER_SIZE: u32 = 0x24;

pub(super) fn parse(storage: Storage) -> Result<Archive> {
    let bytes = storage.as_bytes();
    let info = read_header(bytes)?;
    let entries = read_entries(bytes, info)?;
    Ok(Archive::from_parts(storage, info, entries))
}

fn read_header(bytes: &[u8]) -> Result<ArchiveInfo> {
    let mut cursor = Cursor::new(bytes);
    let magic = cursor.u32()?;
    if magic != MAGIC {
        return Err(Error::InvalidMagic(magic));
    }

    let version = match cursor.u32()? {
        103 => ArchiveVersion::v103,
        104 => ArchiveVersion::v104,
        105 => ArchiveVersion::v105,
        raw => return Err(Error::InvalidVersion(raw)),
    };

    let folder_record_offset = cursor.u32()?;
    if folder_record_offset != HEADER_SIZE {
        return Err(Error::InvalidHeaderSize(folder_record_offset));
    }

    let archive_flags = ArchiveFlags::from_bits_truncate(cursor.u32()?);
    let folder_count = cursor.u32()?;
    let file_count = cursor.u32()?;
    let folder_names_len = cursor.u32()?;
    let file_names_len = cursor.u32()?;
    let archive_types = ArchiveTypes::from_bits_truncate(cursor.u16()?);
    let _padding = cursor.u16()?;

    Ok(ArchiveInfo {
        version,
        folder_record_offset,
        archive_flags,
        folder_count,
        file_count,
        folder_names_len,
        file_names_len,
        archive_types,
    })
}

#[derive(Clone, Copy)]
struct FolderRecord {
    file_count: u32,
}

fn read_entries(bytes: &[u8], info: ArchiveInfo) -> Result<Vec<Entry>> {
    let mut cursor = Cursor::new(bytes);
    cursor.seek(info.folder_record_offset.try_into()?)?;
    let mut folders = Vec::with_capacity(info.folder_count.try_into()?);
    for _ in 0..info.folder_count {
        folders.push(read_folder_record(&mut cursor, info.version)?);
    }

    let mut entries = Vec::with_capacity(info.file_count.try_into()?);
    for folder in folders {
        let folder_name = if info.archive_flags.contains(ArchiveFlags::DIRECTORY_STRINGS) {
            normalize_folder_name(read_bzstring(&mut cursor)?)
        } else {
            BString::new(Vec::new())
        };
        let mut records = Vec::with_capacity(folder.file_count.try_into()?);
        for _ in 0..folder.file_count {
            records.push(read_file_record(&mut cursor)?);
        }
        entries.extend(
            records
                .into_iter()
                .map(|record| (folder_name.clone(), record)),
        );
    }

    let file_names_offset = compute_file_names_offset(bytes, info)?;
    let mut names = Cursor::new(bytes);
    names.seek(file_names_offset)?;
    entries
        .into_iter()
        .map(|(folder, record)| Ok(Entry::new(folder, read_zstring(&mut names)?, record)))
        .collect::<Result<Vec<_>>>()
}

fn read_folder_record(cursor: &mut Cursor<'_>, version: ArchiveVersion) -> Result<FolderRecord> {
    let _hash = cursor.bytes(8)?;
    let file_count = cursor.u32()?;
    let _offset_or_padding = cursor.u32()?;
    if matches!(version, ArchiveVersion::v105) {
        let _ = cursor.u64()?;
    }
    Ok(FolderRecord { file_count })
}

fn read_file_record(cursor: &mut Cursor<'_>) -> Result<FileRecord> {
    let _hash = cursor.bytes(8)?;
    let size = cursor.u32()?;
    let offset = cursor.u32()?;
    Ok(FileRecord {
        stored_size: size & !(1 << 30 | 1 << 31),
        data_offset: offset & !(1 << 31),
        compression_toggled: size & (1 << 30) != 0,
    })
}

fn compute_file_names_offset(bytes: &[u8], info: ArchiveInfo) -> Result<usize> {
    let folder_entry_size = match info.version {
        ArchiveVersion::v103 | ArchiveVersion::v104 => 16usize,
        ArchiveVersion::v105 => 24usize,
    };
    let folder_table_size = folder_entry_size
        .checked_mul(info.folder_count.try_into()?)
        .ok_or(Error::OutOfBounds)?;
    let mut offset: usize = info.folder_record_offset.try_into()?;
    offset = offset
        .checked_add(folder_table_size)
        .ok_or(Error::OutOfBounds)?;
    if info.archive_flags.contains(ArchiveFlags::DIRECTORY_STRINGS) {
        offset = offset
            .checked_add(info.folder_count.try_into()?)
            .and_then(|value| value.checked_add(info.folder_names_len.try_into().ok()?))
            .ok_or(Error::OutOfBounds)?;
    }
    offset = offset
        .checked_add(
            16usize
                .checked_mul(info.file_count.try_into()?)
                .ok_or(Error::OutOfBounds)?,
        )
        .ok_or(Error::OutOfBounds)?;
    let file_names_len: usize = info.file_names_len.try_into()?;
    let end = offset
        .checked_add(file_names_len)
        .ok_or(Error::OutOfBounds)?;
    if end > bytes.len() {
        return Err(Error::OutOfBounds);
    }
    Ok(offset)
}

fn read_bzstring(cursor: &mut Cursor<'_>) -> Result<BString> {
    let len = cursor.u8()? as usize;
    let bytes = cursor.bytes(len)?;
    Ok(BString::from(strip_nul(bytes)))
}

fn read_zstring(cursor: &mut Cursor<'_>) -> Result<BString> {
    let mut out = Vec::new();
    loop {
        let byte = cursor.u8()?;
        if byte == 0 {
            return Ok(BString::from(out));
        }
        out.push(byte);
    }
}

fn strip_nul(bytes: &[u8]) -> Vec<u8> {
    bytes.strip_suffix(&[0]).unwrap_or(bytes).to_vec()
}

fn normalize_folder_name(name: BString) -> BString {
    if name.as_slice() == b"." {
        BString::new(Vec::new())
    } else {
        name
    }
}
