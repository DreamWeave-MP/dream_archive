use crate::bsa::hash::normalize_hash_path;

/// TES3/Morrowind BSA file hash.
///
/// The numeric value is the canonical ordering value used by TES3 hash tables.
/// `lo` and `hi` are the two little-endian `u32` fields as stored on disk.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct FileHash {
    pub lo: u32,
    pub hi: u32,
}

impl FileHash {
    #[must_use]
    pub const fn numeric(self) -> u64 {
        (self.hi as u64) | ((self.lo as u64) << 32)
    }
}

/// Hash a TES3 BSA file path and return the normalized path that was hashed.
///
/// Normalization is the archive hash normalization, not the `OpenMW` VFS lookup
/// normalization: `/` becomes `\\`, ASCII is lowercased, leading/trailing
/// separators are removed, and empty/too-long paths hash as `.`.
#[must_use]
pub fn hash_file(path: &[u8]) -> (FileHash, Vec<u8>) {
    let normalized = normalize_hash_path(path);
    (hash_normalized_file(&normalized), normalized.into())
}

#[must_use]
pub(crate) fn hash_normalized_file(path: &[u8]) -> FileHash {
    let midpoint = path.len() / 2;
    let mut lo = 0u32;
    let mut hi = 0u32;
    for (index, byte) in path.iter().take(midpoint).enumerate() {
        lo ^= u32::from(*byte) << ((index % 4) * 8);
    }
    for (index, byte) in path.iter().skip(midpoint).enumerate() {
        let rot = u32::from(*byte) << ((index % 4) * 8);
        hi = u32::rotate_right(hi ^ rot, rot);
    }
    FileHash { lo, hi }
}
