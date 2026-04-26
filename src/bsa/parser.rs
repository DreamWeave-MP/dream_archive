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

    let archive_flags = ArchiveFlags::from_bits_retain(cursor.u32()?);
    if !archive_flags.contains(ArchiveFlags::DIRECTORY_STRINGS) {
        return Err(Error::NotImplemented(
            "TES4 hash-only archives without directory name strings",
        ));
    }
    if !archive_flags.contains(ArchiveFlags::FILE_STRINGS) {
        return Err(Error::NotImplemented(
            "TES4 hash-only archives without file name strings",
        ));
    }
    let folder_count = cursor.u32()?;
    let file_count = cursor.u32()?;
    let folder_names_len = cursor.u32()?;
    let file_names_len = cursor.u32()?;
    let archive_types = ArchiveTypes::from_bits_retain(cursor.u16()?);
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
    file_records_offset: u32,
}

fn read_entries(bytes: &[u8], info: ArchiveInfo) -> Result<Vec<Entry>> {
    let mut cursor = Cursor::new(bytes);
    cursor.seek(info.folder_record_offset.try_into()?)?;
    let folder_count = info.folder_count.try_into()?;
    let mut folders = Vec::new();
    folders.try_reserve_exact(folder_count)?;
    let mut parsed_file_count = 0u32;
    for _ in 0..info.folder_count {
        let folder = read_folder_record(&mut cursor, info.version)?;
        parsed_file_count = parsed_file_count
            .checked_add(folder.file_count)
            .ok_or(Error::OutOfBounds)?;
        folders.push(folder);
    }
    if parsed_file_count != info.file_count {
        return Err(Error::OutOfBounds);
    }

    let mut entries = Vec::new();
    entries.try_reserve_exact(info.file_count.try_into()?)?;
    for folder in folders {
        if usize::try_from(folder.file_records_offset)? < cursor.position() {
            return Err(Error::OutOfBounds);
        }
        let folder_name = normalize_folder_name(read_bzstring(&mut cursor)?);
        let mut records = Vec::new();
        records.try_reserve_exact(folder.file_count.try_into()?)?;
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
    if cursor.position() != file_names_offset {
        return Err(Error::OutOfBounds);
    }
    let file_names_len: usize = info.file_names_len.try_into()?;
    let file_names_end = file_names_offset
        .checked_add(file_names_len)
        .ok_or(Error::OutOfBounds)?;
    let mut names = Cursor::new(
        bytes
            .get(file_names_offset..file_names_end)
            .ok_or(Error::OutOfBounds)?,
    );
    let entries = entries
        .into_iter()
        .map(|(folder, record)| Ok(Entry::new(folder, read_zstring(&mut names)?, record)))
        .collect::<Result<Vec<_>>>()?;
    if names.position() != file_names_len {
        return Err(Error::OutOfBounds);
    }
    for entry in &entries {
        validate_file_extent(entry.file(), file_names_end, bytes.len())?;
    }
    Ok(entries)
}

fn read_folder_record(cursor: &mut Cursor<'_>, version: ArchiveVersion) -> Result<FolderRecord> {
    let _hash = cursor.bytes(8)?;
    let file_count = cursor.u32()?;
    let mut file_records_offset = cursor.u32()?;
    if matches!(version, ArchiveVersion::v105) {
        file_records_offset = cursor.u32()?;
        let _padding = cursor.u32()?;
    }
    Ok(FolderRecord {
        file_count,
        file_records_offset,
    })
}

fn read_file_record(cursor: &mut Cursor<'_>) -> Result<FileRecord> {
    let _hash = cursor.bytes(8)?;
    let size = cursor.u32()?;
    let offset = cursor.u32()?;
    Ok(FileRecord {
        stored_size: size & !(1 << 30 | 1 << 31),
        data_offset: offset,
        compression_toggled: size & (1 << 30) != 0,
        checked: size & (1 << 31) != 0,
    })
}

fn validate_file_extent(
    record: FileRecord,
    payload_start: usize,
    archive_size: usize,
) -> Result<()> {
    let start: usize = record.data_offset.try_into()?;
    let len: usize = record.stored_size.try_into()?;
    if start < payload_start || start.checked_add(len).ok_or(Error::OutOfBounds)? > archive_size {
        return Err(Error::OutOfBounds);
    }
    Ok(())
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
    if !bytes.ends_with(&[0]) {
        return Err(crate::read::Error::UnexpectedEof.into());
    }
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
