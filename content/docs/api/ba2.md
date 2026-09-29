+++
title = "ba2"
description = "ba2::Archive and everything it reports: ArchiveInfo, PayloadFormat, ArchiveVersion, Ba2CompressionFormat, Entry, EntryId, ArchiveFile, FileHeader, TextureHeader, Chunk, and the BA2 hash."
weight = 20

[extra]
kind = "api"
+++

The `ba2` module, behind the `ba2` feature. The builders are on
[BA2 builders](@/docs/api/ba2-builders.md), and `Error` and `Result` on
[Errors](@/docs/api/errors.md#ba2-error). Every `Result` here is `ba2::Result`.

## Archive

{{ api_signature(value="struct Archive") }}

An opened BA2 of any payload family. `Clone`, `Debug`, `TryFrom<Copied<'_>>`.

### Opening

{{ api_signature(value="fn open_path(path: impl AsRef<Path>) -> Result<Archive>") }}

{{ api_signature(value="fn from_vec(bytes: Vec<u8>) -> Result<Archive>") }}

{{ api_signature(value="fn from_slice(bytes: &[u8]) -> Result<Archive>") }}

Parse an archive from a memory-mapped file, from an owned buffer, or from a copy of a slice. The
header, every file record and chunk record, and the string table are read and checked; a DX10
archive's textures are validated as [BA2 textures](@/docs/textures.md#the-texture-header)
describes.

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
`entry_by_id_required` returns `Error::OutOfBounds` for an id past the end.

### Lookup

{{ api_signature(value="fn get(&self, path: impl AsRef<[u8]>) -> Option<&Entry>") }}

{{ api_signature(value="fn get_id(&self, path: impl AsRef<[u8]>) -> Option<EntryId>") }}

{{ api_signature(value="fn get_required(&self, path: impl AsRef<[u8]>) -> Result<&Entry>") }}

{{ api_signature(value="fn contains(&self, path: impl AsRef<[u8]>) -> bool") }}

The entry at a path, its id, the entry or `Error::FileNotFound`, and whether it is there. The
path is normalized as BA2 hashes are ([Archive paths](@/docs/paths.md#lookups)) and matched
against the string table's names, or, in an archive without one, hashed and matched by hash.

{{ api_signature(value="fn get_by_hash(&self, hash: FileHash) -> Option<&Entry>") }}

{{ api_signature(value="fn get_id_by_hash(&self, hash: FileHash) -> Option<EntryId>") }}

{{ api_signature(value="fn contains_hash(&self, hash: FileHash) -> bool") }}

The same by hash, without consulting names. The first entry in archive order wins a collision.

### Reading

{{ api_signature(value="fn read_entry(&self, entry: &Entry) -> Result<Vec<u8>>") }}

{{ api_signature(value="fn read_entry_into(&self, entry: &Entry, out: &mut Vec<u8>) -> Result<()>") }}

{{ api_signature(value="fn read_file(&self, path: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>>") }}

{{ api_signature(value="fn read_file_required(&self, path: impl AsRef<[u8]>) -> Result<Vec<u8>>") }}

A member, decompressed into memory. A DX10 texture comes out as a DDS file, header first.
`read_entry_into` appends to `out` and truncates it back on error. `read_entry` reserves the
decoded size before it starts.

{{ api_signature(value="fn extract_entry(&self, entry: &Entry, out: impl Write) -> Result<u64>") }}

{{ api_signature(value="fn extract_entry_by_id(&self, id: EntryId, out: impl Write) -> Result<u64>") }}

{{ api_signature(value="fn extract_file(&self, path: impl AsRef<[u8]>, out: impl Write) -> Result<Option<u64>>") }}

{{ api_signature(value="fn extract_file_required(&self, path: impl AsRef<[u8]>, out: impl Write) -> Result<u64>") }}

A member written to `out`; returns the bytes written. Each compressed chunk is decoded into a
buffer before it is written.

{{ api_signature(value="fn extract_entry_to_path(&self, entry: &Entry, path: impl AsRef<Path>) -> Result<u64>") }}

A member written to a file atomically: to a temporary file beside `path`, renamed over it when
complete, with parent directories created. zlib chunks stream through the decoder into the file.

{{ api_signature(value="fn open_entry<'a>(&'a self, entry: &'a Entry) -> Result<Box<dyn Read + 'a>>") }}

{{ api_signature(value="fn open_file(&self, path: impl AsRef<[u8]>) -> Result<Option<Box<dyn Read + '_>>>") }}

{{ api_signature(value="fn open_file_required(&self, path: impl AsRef<[u8]>) -> Result<Box<dyn Read + '_>>") }}

A member as a reader: stored chunks read straight from the archive, compressed chunks decoded and
buffered one by one when the reader is made.

{{ api_signature(value="fn extracted_len(&self, entry: &Entry) -> Result<u64>") }}

{{ api_signature(value="fn extracted_len_by_id(&self, id: EntryId) -> Result<u64>") }}

The number of bytes a member decodes to, DDS header included, without decoding it.

{{ api_signature(value="fn extract_to(&self, target_dir: impl AsRef<Path>) -> Result<u64>") }}

Every entry below `target_dir` at its name; see [Extracting](@/docs/extracting.md). An archive
without a string table cannot be extracted this way.

Every read of a GNMF entry fails with `Error::NotImplemented("BA2 GNMF extraction")`.

## ArchiveInfo

{{ api_signature(value="struct ArchiveInfo { pub format: PayloadFormat, pub version: ArchiveVersion, pub compression_format: Ba2CompressionFormat, pub strings: bool }") }}

The header. `compression_format` is `LZ4` only in a version 3 archive whose compression field is
3, and `Zip` otherwise. `strings` says whether the header points at a string table. `Clone`,
`Copy`, `Debug`, `Eq`, `PartialEq`, and `Default`: GNRL, version 1, zlib, no strings.

{{ api_signature(value="enum PayloadFormat { GNRL, DX10, GNMF }") }}

The payload family: general files, textures, or console GNM textures. `GNRL` is the default.

{{ api_signature(value="enum ArchiveVersion { v1 = 1, v2 = 2, v3 = 3, v7 = 7, v8 = 8 }") }}

The header's version. `v1` is the default. [Formats](@/docs/formats.md#ba2) says which game
writes which.

{{ api_signature(value="enum Ba2CompressionFormat { Zip, LZ4 }") }}

zlib, the default, or LZ4 blocks.

All three are `Clone`, `Copy`, `Debug`, `Default`, `Eq`, `PartialEq`.

## Entry

{{ api_signature(value="struct Entry") }}

One file record. `Clone`, `Debug`, `Eq`, `PartialEq`.

{{ api_signature(value="fn name(&self) -> &BStr") }}

The name from the string table, as stored: in lowercase with backslashes if the archive's writer
followed the games. Empty when the archive has no string table; an empty name is missing text,
not a file called nothing.

{{ api_signature(value="fn hash(&self) -> FileHash") }}

{{ api_signature(value="fn file(&self) -> &ArchiveFile") }}

The stored hash, and the file's header and chunks.

## EntryId

{{ api_signature(value="struct EntryId(usize)") }}

An entry's position in one archive's table. `Clone`, `Copy`, `Debug`, `Eq`, `Hash`, `PartialEq`.

{{ api_signature(value="const fn from_index(index: usize) -> EntryId") }}

{{ api_signature(value="const fn index(self) -> usize") }}

To and from the 0-based index.

## ArchiveFile

{{ api_signature(value="struct ArchiveFile { pub header: FileHeader, pub chunks: Vec<Chunk> }") }}

A file's metadata and its chunks. `Clone`, `Debug`, `Default`, `Eq`, `PartialEq`.

{{ api_signature(value="fn len(&self) -> usize") }}

{{ api_signature(value="fn is_empty(&self) -> bool") }}

{{ api_signature(value="fn chunks(&self) -> &[Chunk]") }}

The number of chunks, whether there are none, and the chunks.

{{ api_signature(value="enum FileHeader { GNRL, DX10(TextureHeader), GNMF([u32; 8]) }") }}

What the record holds besides its chunks: nothing for a general file, a texture header, or the
eight words of GNMF metadata, kept unread. `GNRL` is the default. `Clone`, `Debug`, `Default`,
`Eq`, `PartialEq`.

{{ api_signature(value="struct TextureHeader { pub height: u16, pub width: u16, pub mip_count: u8, pub format: u8, pub flags: u8, pub tile_mode: u8 }") }}

A DX10 texture's size, mip count, DXGI format number, flags (bit 0: cubemap) and tile mode (the
high byte of the flags field). `Clone`, `Copy`, `Debug`, `Default`, `Eq`, `PartialEq`.
[BA2 textures](@/docs/textures.md#the-texture-header) has the detail.

## Chunk

{{ api_signature(value="struct Chunk { pub mips: Option<RangeInclusive<u16>>, .. }") }}

One stored run of a file's data. `mips` is the range of mips a texture chunk holds, `None` for
general files. `Clone`, `Debug`, `Eq`, `PartialEq`.

{{ api_signature(value="fn size(&self) -> u32") }}

{{ api_signature(value="fn offset(&self) -> u64") }}

{{ api_signature(value="fn packed_size(&self) -> u32") }}

{{ api_signature(value="fn stored_size(&self) -> u32") }}

{{ api_signature(value="fn is_compressed(&self) -> bool") }}

The decompressed size; where the stored data starts in the archive; its compressed size, 0 when
it is stored uncompressed; the number of bytes it occupies, whichever that is; and whether it is
compressed.

## Hashes

{{ api_signature(value="struct Hash { pub file: u32, pub extension: u32, pub directory: u32 }") }}

{{ api_signature(value="struct FileHash(pub Hash)") }}

A file record's hash: [Hashes](@/docs/hashes.md#ba2) says how the fields are computed. Both are
`Clone`, `Copy`, `Debug`, `Default`, `Eq`, `Hash`, `Ord`, `PartialEq`, `PartialOrd`. `FileHash`
dereferences to `Hash` and converts from one; it also implements `From<&str>` and `From<&[u8]>`,
which hash the path.

{{ api_signature(value="fn hash_file(path: &BStr) -> (FileHash, BString)") }}

The hash of a path, and the normalized path that was hashed.

{{ api_signature(value="fn hash_file_in_place(path: &mut BString) -> FileHash") }}

The same, normalizing `path` where it is.
