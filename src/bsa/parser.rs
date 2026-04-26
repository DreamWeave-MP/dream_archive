use super::{Archive, ArchiveFlags, ArchiveInfo, ArchiveTypes, ArchiveVersion, Error, Result};
use crate::{read::Cursor, storage::Storage};

const MAGIC: u32 = u32::from_le_bytes(*b"BSA\0");
const HEADER_SIZE: u32 = 0x24;

pub(super) fn parse(storage: Storage) -> Result<Archive> {
    let info = read_header(storage.as_bytes())?;
    Ok(Archive::from_parts(storage, info))
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
