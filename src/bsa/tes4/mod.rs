//! TES4-family PC BSA support.
//!
//! This covers the Oblivion/Fallout/Skyrim-era PC layouts, including v103/v104
//! zlib archives and v105 LZ4-frame archives. Names may come from string tables,
//! embedded payload names, both, or neither. [`Entry::path()`] is therefore
//! optional. Path-based extraction only works when names are recoverable;
//! hash-only archives need [`Archive::extract_to_with_paths`] with a
//! caller-provided byte-path dictionary. Console layouts and `XMem` are rejected
//! instead of being treated as almost-the-same. They are not.

mod archive;
mod builder;
mod hash;
mod parser;

pub use super::{Error, Result};
pub use archive::{
    Archive, ArchiveFlags, ArchiveInfo, ArchiveTypes, ArchiveVersion, Entry, EntryId, FileRecord,
};
pub use builder::{Builder, GameProfile, NameMode};
pub use hash::{HashFields, hash_directory, hash_file};
