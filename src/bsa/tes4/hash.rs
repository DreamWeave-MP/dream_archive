use crate::bsa::hash::{crc32, normalize_hash_path};

/// TES4-family BSA directory or file hash payload.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct HashFields {
    pub last: u8,
    pub last2: u8,
    pub length: u8,
    pub first: u8,
    pub crc: u32,
}

impl HashFields {
    #[must_use]
    pub const fn numeric(self) -> u64 {
        (self.last as u64)
            | ((self.last2 as u64) << 8)
            | ((self.length as u64) << 16)
            | ((self.first as u64) << 24)
            | ((self.crc as u64) << 32)
    }
}

/// Hash a TES4 BSA directory path and return the normalized path that was hashed.
#[must_use]
pub fn hash_directory(path: &[u8]) -> (HashFields, Vec<u8>) {
    let normalized = normalize_hash_path(path);
    (hash_normalized_directory(&normalized), normalized.into())
}

/// Hash a TES4 BSA file path and return the normalized file name that was hashed.
///
/// Parent directories are intentionally discarded before hashing the file name.
/// That is archive format behavior, not a judgement call. Sadly.
#[must_use]
pub fn hash_file(path: &[u8]) -> (HashFields, Vec<u8>) {
    let mut normalized = normalize_hash_path(path);
    if let Some(pos) = normalized.iter().rposition(|byte| *byte == b'\\') {
        normalized.drain(..=pos);
    }
    (hash_normalized_file(&normalized), normalized.into())
}

fn hash_normalized_directory(path: &[u8]) -> HashFields {
    let mut hash = HashFields::default();
    let len = path.len();
    if len >= 3 {
        hash.last2 = path[len - 2];
    }
    if len >= 1 {
        hash.last = path[len - 1];
        hash.first = path[0];
    }
    hash.length = len.to_le_bytes()[0];
    if hash.length > 3 {
        hash.crc = crc32(&path[1..len - 2]);
    }
    hash
}

fn hash_normalized_file(path: &[u8]) -> HashFields {
    const LUT: [u32; 6] = [
        make_four(b""),
        make_four(b".nif"),
        make_four(b".kf"),
        make_four(b".dds"),
        make_four(b".wav"),
        make_four(b".adp"),
    ];

    let (stem, extension) = if let Some(split_at) = path.iter().rposition(|byte| *byte == b'.') {
        (&path[..split_at], &path[split_at..])
    } else {
        (path, &[][..])
    };

    if stem.is_empty() || stem.len() >= 260 || extension.len() >= 16 {
        return HashFields::default();
    }

    let mut hash = hash_normalized_directory(stem);
    hash.crc = hash.crc.wrapping_add(crc32(extension));
    let fourcc = make_four(extension);
    if let Some(index) = LUT.iter().position(|value| *value == fourcc) {
        let index: u8 = [0, 1, 2, 3, 4, 5][index];
        hash.first = hash.first.wrapping_add(32u8.wrapping_mul(index & 0xfc));
        hash.last = hash.last.wrapping_add((index & 0xfe).wrapping_shl(6));
        hash.last2 = hash.last2.wrapping_add(index.wrapping_shl(7));
    }
    hash
}

const fn make_four(bytes: &[u8]) -> u32 {
    let mut value = 0u32;
    let mut index = 0;
    while index < 4 {
        let byte = if index < bytes.len() { bytes[index] } else { 0 };
        value |= (byte as u32) << (index * 8);
        index += 1;
    }
    value
}
