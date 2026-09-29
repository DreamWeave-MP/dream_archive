+++
title = "bsa::tes4"
description = "The BSA of Oblivion to Skyrim SE: Archive, Entry, EntryId, FileRecord, ArchiveInfo with ArchiveFlags, ArchiveTypes and ArchiveVersion, and the TES4 hashes."
weight = 60

[extra]
kind = "api"
+++

The `bsa::tes4` module, behind the `bsa-tes4` feature. It re-exports `bsa::Error` and
`bsa::Result` ([Errors](@/docs/api/errors.md#bsa-error)); every `Result` here is that one. The
builder is on [TES4 builder](@/docs/api/tes4-builder.md).

## Archive

{{ api_signature(value="struct Archive") }}

An opened TES4 BSA, version 103, 104 or 105. `Clone`, `Debug`, `TryFrom<Copied<'_>>`.

### Opening

{{ api_signature(value="fn open_path(path: impl AsRef<Path>) -> Result<Archive>") }}

{{ api_signature(value="fn from_vec(bytes: Vec<u8>) -> Result<Archive>") }}

{{ api_signature(value="fn from_slice(bytes: &[u8]) -> Result<Archive>") }}

Parse an archive from a memory-mapped file, an owned buffer, or a copy of a slice. The header,
the folder and file records, the names and any embedded names are read, and every record is
checked to lie inside the file. `InvalidMagic`, `InvalidVersion` or `InvalidHeaderSize` for a
header that is not TES4, `NotImplemented` for [Xbox layouts and XMem](@/docs/formats.md#what-is-refused),
and `OutOfBounds` for tables that do not fit or file counts that do not add up.

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

The entries in archive order, folder by folder, alone or with their ids, and the entry an id
names: `entry_by_id_required` returns `OutOfBounds` for an id past the end.

### Lookup

{{ api_signature(value="fn get(&self, path: impl AsRef<[u8]>) -> Option<&Entry>") }}

{{ api_signature(value="fn get_id(&self, path: impl AsRef<[u8]>) -> Option<EntryId>") }}

{{ api_signature(value="fn get_required(&self, path: impl AsRef<[u8]>) -> Result<&Entry>") }}

{{ api_signature(value="fn contains(&self, path: impl AsRef<[u8]>) -> bool") }}

The entry at a path, its id, the entry or `FileNotFound`, and whether it is there. The path is
normalized with dream-path's rules and compared with the stored paths normalized the same way. In
an archive with no names at all, the normalized path is split at its last separator and the two
halves hashed instead.

{{ api_signature(value="fn get_normalized(&self, path: &NormalizedPath) -> Option<&Entry>") }}

{{ api_signature(value="fn contains_normalized(&self, path: &NormalizedPath) -> bool") }}

The same with a key normalized once. These compare names only, with no fallback to hashes.

{{ api_signature(value="fn get_by_hash(&self, folder_hash: HashFields, file_hash: HashFields) -> Option<&Entry>") }}

{{ api_signature(value="fn get_id_by_hash(&self, folder_hash: HashFields, file_hash: HashFields) -> Option<EntryId>") }}

{{ api_signature(value="fn contains_hash(&self, folder_hash: HashFields, file_hash: HashFields) -> bool") }}

The same by the folder's and the file's hashes, without consulting names. The first entry in
archive order wins a collision.

### Reading

{{ api_signature(value="fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>>") }}

{{ api_signature(value="fn read_entry_into(&self, entry: &Entry, out: &mut Vec<u8>) -> Result<()>") }}

{{ api_signature(value="fn read_file(&self, path: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>>") }}

{{ api_signature(value="fn read_file_required(&self, path: impl AsRef<[u8]>) -> Result<Vec<u8>>") }}

A member, decompressed into memory, without its embedded name. `read_entry_into` appends to `out`
and truncates it back on error.

{{ api_signature(value="fn extract_entry(&self, entry: &Entry, out: impl Write) -> Result<u64>") }}

{{ api_signature(value="fn extract_entry_by_id(&self, id: EntryId, out: impl Write) -> Result<u64>") }}

{{ api_signature(value="fn extract_file(&self, path: impl AsRef<[u8]>, out: impl Write) -> Result<Option<u64>>") }}

{{ api_signature(value="fn extract_file_required(&self, path: impl AsRef<[u8]>, out: impl Write) -> Result<u64>") }}

A member written to `out`, decoded into a buffer first if it is compressed; returns the bytes
written.

{{ api_signature(value="fn extract_entry_to_path(&self, entry: &Entry, path: impl AsRef<Path>) -> Result<u64>") }}

A member written to a file atomically, with parent directories created. Compressed data streams
through the decoder into the file.

{{ api_signature(value="fn open_entry<'a>(&'a self, entry: &'a Entry) -> Result<Box<dyn Read + 'a>>") }}

{{ api_signature(value="fn open_file(&self, path: impl AsRef<[u8]>) -> Result<Option<Box<dyn Read + '_>>>") }}

{{ api_signature(value="fn open_file_required(&self, path: impl AsRef<[u8]>) -> Result<Box<dyn Read + '_>>") }}

A member as a reader: stored data read straight from the archive, compressed data decoded when
the reader is made.

{{ api_signature(value="fn extracted_len(&self, entry: &Entry) -> Result<u64>") }}

{{ api_signature(value="fn extracted_len_by_id(&self, id: EntryId) -> Result<u64>") }}

The number of bytes a member decodes to, read from its size prefix without decoding it.

{{ api_signature(value="fn extract_to(&self, target_dir: impl AsRef<Path>) -> Result<u64>") }}

{{ api_signature(value="fn extract_to_with_encoding(&self, target_dir: impl AsRef<Path>, encoding: FilenameEncoding) -> Result<u64>") }}

Every entry below `target_dir`, at its stored path or at its path decoded through `encoding`;
`ArchivePathsUnavailable` if any entry has no path. See [Extracting](@/docs/extracting.md).

{{ api_signature(value="fn extract_to_with_paths<P>(&self, target_dir: impl AsRef<Path>, paths: impl IntoIterator<Item = P>) -> Result<u64> where P: AsRef<[u8]>") }}

Every entry one of `paths` finds, written below `target_dir` under that path as given. Paths that
find nothing are skipped; an entry two paths find is written once. For archives without names;
see [Hash-only archives](@/docs/hashes.md#hash-only-archives).

## ArchiveInfo

{{ api_signature(value="struct ArchiveInfo { pub version: ArchiveVersion, pub folder_record_offset: u32, pub archive_flags: ArchiveFlags, pub folder_count: u32, pub file_count: u32, pub folder_names_len: u32, pub file_names_len: u32, pub archive_types: ArchiveTypes }") }}

The header. `folder_record_offset` is always 36, the header's size; the name lengths are the
byte lengths of the folder and file name blocks. `Clone`, `Copy`, `Debug`, `Eq`, `PartialEq`.

{{ api_signature(value="fn uses_xmem(&self) -> bool") }}

Whether the archive's entries are XMem-compressed: the `XBOX_COMPRESSED` flag, in version 104 or
later. Oblivion's version 103 archives set that bit on PC, where it means nothing.

{{ api_signature(value="enum ArchiveVersion { v103 = 103, v104 = 104, v105 = 105 }") }}

The header's version; `v103` is the default. `Clone`, `Copy`, `Debug`, `Default`, `Eq`,
`PartialEq`.

## ArchiveFlags

{{ api_signature(value="struct ArchiveFlags(u32)") }}

The header's flags, every bit kept, named or not. `Clone`, `Copy`, `Debug`, `Default`, `Eq`,
`PartialEq`.

| Constant | Bit | Means |
|---|---|---|
| `DIRECTORY_STRINGS` | 0 | folder names are stored |
| `FILE_STRINGS` | 1 | file names are stored |
| `COMPRESSED` | 2 | members are compressed unless their record toggles it |
| `XBOX_ARCHIVE` | 6 | an Xbox layout; refused |
| `EMBEDDED_FILE_NAMES` | 8 | from version 104, each payload starts with the member's path |
| `XBOX_COMPRESSED` | 9 | from version 104, XMem compression; refused |

{{ api_signature(value="const fn from_bits_retain(bits: u32) -> ArchiveFlags") }}

{{ api_signature(value="const fn bits(self) -> u32") }}

{{ api_signature(value="const fn contains(self, other: ArchiveFlags) -> bool") }}

To and from the raw bits, and whether every bit of `other` is set.

## ArchiveTypes

{{ api_signature(value="struct ArchiveTypes(u16)") }}

The header's content-type bits, which tell the games what kind of files an archive holds. `Clone`,
`Copy`, `Debug`, `Default`, `Eq`, `PartialEq`.

| Constant | Bit |
|---|---|
| `MESHES` | 0 |
| `TEXTURES` | 1 |
| `MENUS` | 2 |
| `SOUNDS` | 3 |
| `VOICES` | 4 |
| `SHADERS` | 5 |
| `TREES` | 6 |
| `FONTS` | 7 |
| `MISC` | 8 |

{{ api_signature(value="const fn from_bits_retain(bits: u16) -> ArchiveTypes") }}

{{ api_signature(value="const fn bits(self) -> u16") }}

{{ api_signature(value="const fn contains(self, other: ArchiveTypes) -> bool") }}

As for `ArchiveFlags`. There is no `|` operator: combine types through their bits,
`ArchiveTypes::from_bits_retain(ArchiveTypes::MESHES.bits() | ArchiveTypes::TEXTURES.bits())`.

## Entry

{{ api_signature(value="struct Entry") }}

One file. `Clone`, `Debug`, `Eq`, `PartialEq`.

{{ api_signature(value="fn path(&self) -> Option<&BStr>") }}

{{ api_signature(value="fn folder(&self) -> Option<&BStr>") }}

{{ api_signature(value="fn name(&self) -> Option<&BStr>") }}

The folder and file name, from the string tables or else from the embedded name, and the path
they make, joined with `\`. A file in the root folder has an empty folder, and its path is its
name. `path` is `None` unless both halves are known.

{{ api_signature(value="fn folder_hash(&self) -> HashFields") }}

{{ api_signature(value="fn file_hash(&self) -> HashFields") }}

{{ api_signature(value="fn file(&self) -> FileRecord") }}

The stored hashes, and the file record.

{{ api_signature(value="struct FileRecord { pub stored_size: u32, pub data_offset: u32, pub compression_toggled: bool, pub checked: bool }") }}

The file record, with its flag bits split out: `stored_size` is the size field without bits 30
and 31, and counts any embedded name and size prefix; `data_offset` is from the start of the
file, without bit 31; `compression_toggled` is bit 30 of the size field and `checked` is bit 31.
`Clone`, `Copy`, `Debug`, `Eq`, `PartialEq`.

{{ api_signature(value="fn is_compressed(self, archive_flags: ArchiveFlags) -> bool") }}

The archive's `COMPRESSED` flag, inverted when `compression_toggled` is set.

## EntryId

{{ api_signature(value="struct EntryId(usize)") }}

{{ api_signature(value="const fn from_index(index: usize) -> EntryId") }}

{{ api_signature(value="const fn index(self) -> usize") }}

An entry's 0-based position in one archive's table. `Clone`, `Copy`, `Debug`, `Eq`, `Hash`,
`PartialEq`.

## Hashes

{{ api_signature(value="struct HashFields { pub last: u8, pub last2: u8, pub length: u8, pub first: u8, pub crc: u32 }") }}

A folder or file hash as the archive stores it. `Clone`, `Copy`, `Debug`, `Default`, `Eq`,
`Hash`, `Ord`, `PartialEq`, `PartialOrd`.

{{ api_signature(value="const fn numeric(self) -> u64") }}

{{ api_signature(value="const fn from_numeric(value: u64) -> HashFields") }}

To and from the 64-bit number the archive stores: `last`, `last2`, `length` and `first` in the
low four bytes, in that order, and `crc` in the high four.

{{ api_signature(value="fn hash_directory(path: &[u8]) -> (HashFields, Vec<u8>)") }}

{{ api_signature(value="fn hash_file(path: &[u8]) -> (HashFields, Vec<u8>)") }}

The hash of a folder, or of the file name at the end of a path, with the normalized bytes that
were hashed. [Hashes](@/docs/hashes.md#tes4) describes both.
