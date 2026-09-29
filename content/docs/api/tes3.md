+++
title = "bsa::tes3"
description = "Morrowind's BSA: Archive, Entry, EntryId, FileRecord, ArchiveInfo, FileHash and hash_file, and Builder."
weight = 50

[extra]
kind = "api"
+++

The `bsa::tes3` module, behind the `bsa-tes3` feature. It re-exports `bsa::Error` and
`bsa::Result` ([Errors](@/docs/api/errors.md#bsa-error)); every `Result` here is that one. TES3
members are never compressed, so every read is a copy or a borrow of the archive's bytes.

## Archive

{{ api_signature(value="struct Archive") }}

An opened TES3 BSA. `Clone`, `Debug`, `TryFrom<Copied<'_>>`.

### Opening

{{ api_signature(value="fn open_path(path: impl AsRef<Path>) -> Result<Archive>") }}

{{ api_signature(value="fn from_vec(bytes: Vec<u8>) -> Result<Archive>") }}

{{ api_signature(value="fn from_slice(bytes: &[u8]) -> Result<Archive>") }}

Parse an archive from a memory-mapped file, an owned buffer, or a copy of a slice. The header,
the file records, the names and the hashes are read, and every record is checked to lie inside
the file. A first word other than `0x100` is `InvalidVersion`.

### Metadata

{{ api_signature(value="fn info(&self) -> ArchiveInfo") }}

{{ api_signature(value="fn len(&self) -> usize") }}

{{ api_signature(value="fn is_empty(&self) -> bool") }}

{{ api_signature(value="fn archive_size(&self) -> usize") }}

The header's fields, the number of entries, whether there are none, and the archive's size in
bytes.

### Entries

{{ api_signature(value="fn entries(&self) -> &[Entry]") }}

{{ api_signature(value="fn entries_with_ids(&self) -> impl Iterator<Item = (EntryId, &Entry)>") }}

{{ api_signature(value="fn entry_by_id(&self, id: EntryId) -> Option<&Entry>") }}

{{ api_signature(value="fn entry_by_id_required(&self, id: EntryId) -> Result<&Entry>") }}

The entries in archive order, alone or with their ids, and the entry an id names:
`entry_by_id_required` returns `OutOfBounds` for an id past the end.

### Lookup

{{ api_signature(value="fn get(&self, path: impl AsRef<[u8]>) -> Option<&Entry>") }}

{{ api_signature(value="fn get_id(&self, path: impl AsRef<[u8]>) -> Option<EntryId>") }}

{{ api_signature(value="fn get_required(&self, path: impl AsRef<[u8]>) -> Result<&Entry>") }}

{{ api_signature(value="fn contains(&self, path: impl AsRef<[u8]>) -> bool") }}

The entry at a path, its id, the entry or `FileNotFound`, and whether it is there. The path is
normalized with dream-path's rules and compared with the stored paths normalized the same way
([Archive paths](@/docs/paths.md#lookups)).

{{ api_signature(value="fn get_normalized(&self, path: &NormalizedPath) -> Option<&Entry>") }}

{{ api_signature(value="fn contains_normalized(&self, path: &NormalizedPath) -> bool") }}

The same with a key normalized once.

{{ api_signature(value="fn get_by_hash(&self, hash: u64) -> Option<&Entry>") }}

{{ api_signature(value="fn get_id_by_hash(&self, hash: u64) -> Option<EntryId>") }}

{{ api_signature(value="fn contains_hash(&self, hash: u64) -> bool") }}

The same by stored hash, the value `Entry::hash` and `hash_file(path).0.numeric()` give, without
consulting names. The first entry in archive order wins a collision.

### Reading

{{ api_signature(value="fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>>") }}

{{ api_signature(value="fn read_entry_into(&self, entry: &Entry, out: &mut Vec<u8>) -> Result<()>") }}

{{ api_signature(value="fn read_file(&self, path: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>>") }}

{{ api_signature(value="fn read_file_required(&self, path: impl AsRef<[u8]>) -> Result<Vec<u8>>") }}

A member copied into memory; `read_entry_into` appends to `out`.

{{ api_signature(value="fn extract_entry(&self, entry: &Entry, out: impl Write) -> Result<u64>") }}

{{ api_signature(value="fn extract_entry_by_id(&self, id: EntryId, out: impl Write) -> Result<u64>") }}

{{ api_signature(value="fn extract_file(&self, path: impl AsRef<[u8]>, out: impl Write) -> Result<Option<u64>>") }}

{{ api_signature(value="fn extract_file_required(&self, path: impl AsRef<[u8]>, out: impl Write) -> Result<u64>") }}

A member written to `out` straight from the archive's bytes; returns the bytes written.

{{ api_signature(value="fn extract_entry_to_path(&self, entry: &Entry, path: impl AsRef<Path>) -> Result<u64>") }}

A member written to a file atomically, with parent directories created.

{{ api_signature(value="fn open_entry<'a>(&'a self, entry: &'a Entry) -> Result<Box<dyn Read + 'a>>") }}

{{ api_signature(value="fn open_file(&self, path: impl AsRef<[u8]>) -> Result<Option<Box<dyn Read + '_>>>") }}

{{ api_signature(value="fn open_file_required(&self, path: impl AsRef<[u8]>) -> Result<Box<dyn Read + '_>>") }}

A member as a reader over the archive's bytes.

{{ api_signature(value="fn extract_to(&self, target_dir: impl AsRef<Path>) -> Result<u64>") }}

{{ api_signature(value="fn extract_to_with_encoding(&self, target_dir: impl AsRef<Path>, encoding: FilenameEncoding) -> Result<u64>") }}

Every entry below `target_dir`, at its stored path or at its path decoded through `encoding`;
see [Extracting](@/docs/extracting.md).

## ArchiveInfo

{{ api_signature(value="struct ArchiveInfo { pub hash_offset: u32, pub file_count: u32 }") }}

The header after its version: where the hash table starts, counted from the end of the 12-byte
header, and how many files there are. `Clone`, `Copy`, `Debug`, `Eq`, `PartialEq`.

## Entry

{{ api_signature(value="struct Entry") }}

One file. `Clone`, `Debug`, `Eq`, `PartialEq`.

{{ api_signature(value="fn path(&self) -> &BStr") }}

The stored path, as bytes. A TES3 entry always has one.

{{ api_signature(value="fn file(&self) -> FileRecord") }}

{{ api_signature(value="fn hash(&self) -> u64") }}

The file record, and the stored hash as `FileHash::numeric()` spells it.

{{ api_signature(value="struct FileRecord { pub size: u32, pub offset: u32 }") }}

The member's size, and where its data starts, counted from the start of the data section. `Clone`,
`Copy`, `Debug`, `Eq`, `PartialEq`.

## EntryId

{{ api_signature(value="struct EntryId(usize)") }}

{{ api_signature(value="const fn from_index(index: usize) -> EntryId") }}

{{ api_signature(value="const fn index(self) -> usize") }}

An entry's 0-based position in one archive's table. `Clone`, `Copy`, `Debug`, `Eq`, `Hash`,
`PartialEq`.

## Hashes

{{ api_signature(value="struct FileHash { pub lo: u32, pub hi: u32 }") }}

The two halves of a TES3 hash, as the archive stores them. `Clone`, `Copy`, `Debug`, `Default`,
`Eq`, `Hash`, `Ord`, `PartialEq`, `PartialOrd`.

{{ api_signature(value="const fn numeric(self) -> u64") }}

`hi | lo << 32`: the value entries report and hash lookups take.

{{ api_signature(value="fn hash_file(path: &[u8]) -> (FileHash, Vec<u8>)") }}

The hash of a path, and the normalized path that was hashed. [Hashes](@/docs/hashes.md#tes3)
describes the normalization.

## Builder

{{ api_signature(value="struct Builder") }}

Writes a TES3 BSA. It has no settings: TES3 has one version and no compression. `Clone`,
`Debug`, `Default`. Also `Tes3BsaBuilder` at the crate root.

{{ api_signature(value="fn new() -> Builder") }}

{{ api_signature(value="fn len(&self) -> usize") }}

{{ api_signature(value="fn is_empty(&self) -> bool") }}

An empty builder, how many members it has, and whether it has none.

Each call below normalizes its archive path and fails with `InvalidArchivePath` or
`DuplicatePath` as [What builders store](@/docs/paths.md#what-builders-store) describes.

{{ api_signature(value="fn add_bytes(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>) -> Result<()>") }}

A copy of `bytes`, taken now.

{{ api_signature(value="fn add_encoded_path(&mut self, path: &str, encoding: FilenameEncoding, bytes: impl AsRef<[u8]>) -> Result<()>") }}

`add_bytes` with `path` encoded by `encode_filename` first; an unrepresentable character is
`FilenameEncoding`.

{{ api_signature(value="fn add_file(&mut self, archive_path: impl AsRef<[u8]>, source: impl AsRef<Path>) -> Result<()>") }}

The file at `source`, following symlinks: its size now, its bytes when the archive is written.

{{ api_signature(value="fn add_dir(&mut self, root: impl AsRef<Path>) -> Result<()>") }}

Every file below `root` as `add_file`, at its path relative to `root`.

{{ api_signature(value="fn add_archive_entry(&mut self, archive_path: impl AsRef<[u8]>, archive: Arc<Archive>, id: EntryId) -> Result<()>") }}

Entry `id` of `archive`, copied when the archive is written. An invalid id is `OutOfBounds` now.

{{ api_signature(value="fn write_path(&self, path: impl AsRef<Path>) -> Result<()>") }}

{{ api_signature(value="fn write_seek<W: Write + Seek>(&self, out: W) -> Result<()>") }}

{{ api_signature(value="fn to_vec(&self) -> Result<Vec<u8>>") }}

Writes the archive to a new or truncated file, to a writer, or to memory, in one pass: records,
names, hashes, then data. Members are sorted by hash, then by path.
