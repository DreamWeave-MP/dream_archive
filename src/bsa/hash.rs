use crate::BString;

pub(crate) fn normalize_hash_path(path: &[u8]) -> BString {
    let mut normalized = BString::from(path);
    for byte in normalized.iter_mut() {
        *byte = match *byte {
            b'/' => b'\\',
            b'A'..=b'Z' => *byte + 32,
            _ => *byte,
        };
    }
    while normalized.last() == Some(&b'\\') {
        normalized.pop();
    }
    while normalized.first() == Some(&b'\\') {
        normalized.remove(0);
    }
    if normalized.is_empty() || normalized.len() >= 260 {
        normalized.clear();
        normalized.push(b'.');
    }
    normalized
}

#[cfg(feature = "bsa-tes4")]
pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0u32;
    for byte in bytes {
        crc = u32::from(*byte).wrapping_add(crc.wrapping_mul(0x1003f));
    }
    crc
}
