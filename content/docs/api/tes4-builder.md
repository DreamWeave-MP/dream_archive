+++
title = "TES4 builder"
description = "bsa::tes4::Builder: game profiles, version, content types, compression, name modes, every way to add a member, and writing; GameProfile and NameMode."
weight = 70

[extra]
kind = "api"
+++

Behind the `bsa-tes4` feature, also named `Tes4BsaBuilder` at the crate root. Every `Result` here
is `bsa::Result`. [Building archives](@/docs/building.md#tes4-profiles-content-types-and-names) is
the guide.

## Builder

{{ api_signature(value="struct Builder") }}

Writes a TES4 BSA. `Clone`, `Debug`, `Default`.

### Constructors

{{ api_signature(value="fn new() -> Builder") }}

An empty builder: version 104, content type `MISC`, uncompressed, `NameMode::Strings`, zlib level
6.

{{ api_signature(value="fn with_profile(profile: GameProfile) -> Builder") }}

{{ api_signature(value="fn oblivion() -> Builder") }}

{{ api_signature(value="fn fallout3() -> Builder") }}

{{ api_signature(value="fn fallout_new_vegas() -> Builder") }}

{{ api_signature(value="fn skyrim_le() -> Builder") }}

{{ api_signature(value="fn skyrim_se() -> Builder") }}

`new()` with the version the profile's game uses, and nothing else changed.

### Settings

{{ api_signature(value="fn set_profile(&mut self, profile: GameProfile) -> &mut Self") }}

Sets the version the profile's game uses, and only that.

{{ api_signature(value="fn version(&self) -> ArchiveVersion") }}

{{ api_signature(value="fn set_version(&mut self, version: ArchiveVersion) -> &mut Self") }}

The version: 103 and 104 compress with zlib and have 16-byte folder records; 105 compresses with
LZ4 frames and has 24-byte folder records.

{{ api_signature(value="fn archive_types(&self) -> ArchiveTypes") }}

{{ api_signature(value="fn set_archive_types(&mut self, archive_types: ArchiveTypes) -> &mut Self") }}

The content-type bits written to the header.

{{ api_signature(value="fn compressed(&self) -> bool") }}

{{ api_signature(value="fn set_compressed(&mut self, compressed: bool) -> &mut Self") }}

Whether members are compressed by default: the header's `COMPRESSED` flag.

{{ api_signature(value="fn name_mode(&self) -> NameMode") }}

{{ api_signature(value="fn set_name_mode(&mut self, name_mode: NameMode) -> &mut Self") }}

How paths are stored. An embedded mode fails at write time on version 103.

{{ api_signature(value="fn zlib_level(&self) -> Compression") }}

{{ api_signature(value="fn set_zlib_level(&mut self, level: Compression) -> &mut Self") }}

The `flate2::Compression` level for versions 103 and 104.

{{ api_signature(value="fn len(&self) -> usize") }}

{{ api_signature(value="fn is_empty(&self) -> bool") }}

How many members have been added, and whether none have.

### Adding members

Each call normalizes its archive path into a folder and a file name and fails with
`InvalidArchivePath` or `DuplicatePath` as [What builders store](@/docs/paths.md#what-builders-store)
describes. The `_with_compression` forms take a `CompressionOverride` for that member; one that
differs from the archive's default is written with the record's toggle bit set.

{{ api_signature(value="fn add_bytes(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>) -> Result<()>") }}

{{ api_signature(value="fn add_bytes_with_compression(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>, compression: CompressionOverride) -> Result<()>") }}

A copy of `bytes`, taken now.

{{ api_signature(value="fn add_encoded_path(&mut self, path: &str, encoding: FilenameEncoding, bytes: impl AsRef<[u8]>) -> Result<()>") }}

`add_bytes` with `path` encoded by `encode_filename` first; an unrepresentable character is
`FilenameEncoding`.

{{ api_signature(value="fn add_file(&mut self, archive_path: impl AsRef<[u8]>, source: impl AsRef<Path>) -> Result<()>") }}

{{ api_signature(value="fn add_file_with_compression(&mut self, archive_path: impl AsRef<[u8]>, source: impl AsRef<Path>, compression: CompressionOverride) -> Result<()>") }}

The file at `source`, following symlinks: its size now, its bytes when the archive is written.

{{ api_signature(value="fn add_dir(&mut self, root: impl AsRef<Path>) -> Result<()>") }}

{{ api_signature(value="fn add_dir_with_compression(&mut self, root: impl AsRef<Path>, compression: CompressionOverride) -> Result<()>") }}

Every file below `root` as `add_file`, at its path relative to `root`.

{{ api_signature(value="fn add_archive_entry(&mut self, archive_path: impl AsRef<[u8]>, archive: Arc<Archive>, id: EntryId) -> Result<()>") }}

{{ api_signature(value="fn add_archive_entry_with_compression(&mut self, archive_path: impl AsRef<[u8]>, archive: Arc<Archive>, id: EntryId, compression: CompressionOverride) -> Result<()>") }}

Entry `id` of `archive`, decoded when the archive is written and stored under this builder's
compression. Its decoded size is read now, so an invalid id fails here.

### Writing

{{ api_signature(value="fn write_path(&self, path: impl AsRef<Path>) -> Result<()>") }}

{{ api_signature(value="fn write_seek<W: Write + Seek>(&self, out: W) -> Result<()>") }}

{{ api_signature(value="fn to_vec(&self) -> Result<Vec<u8>>") }}

Writes the archive to a new or truncated file, to a writer, or to memory. Every member is read
and compressed before the archive is written, so deferred files are held in memory at that
point. Folders are sorted by hash, then name, and files within each folder likewise. A member
whose stored size does not fit in 30 bits is `OutOfBounds`.

## GameProfile

{{ api_signature(value="enum GameProfile { Oblivion, Fallout3, FalloutNewVegas, SkyrimLe, SkyrimSe }") }}

A PC game, standing for the archive version it uses: 103 for Oblivion, 104 for Fallout 3,
Fallout: New Vegas and Skyrim, 105 for Skyrim Special Edition. `Clone`, `Copy`, `Debug`, `Eq`,
`PartialEq`.

## NameMode

{{ api_signature(value="enum NameMode { Strings, HashOnly, Embedded, StringsAndEmbedded }") }}

How the builder stores paths: folder and file string tables (the default), nothing but hashes,
each member's full path before its data, or both. The embedded modes need version 104 or 105.
`Clone`, `Copy`, `Debug`, `Default`, `Eq`, `PartialEq`.
