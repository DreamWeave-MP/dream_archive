//! TES3/Morrowind BSA support.
//!
//! TES3 archives are the old, comparatively honest layout: uncompressed entries,
//! byte-string names, and lookup using Morrowind-style path normalization. Entry
//! paths are always present in the archive table, but they are still bytes, not
//! guaranteed Unicode. Use the shared BSA encoding helpers when user-facing text
//! has to round-trip through a legacy code page.

mod archive;
mod builder;
mod hash;
mod parser;

pub use super::{Error, Result};
pub use archive::{Archive, ArchiveInfo, Entry, EntryId, FileRecord};
pub use builder::Builder;
pub use hash::{FileHash, hash_file};
