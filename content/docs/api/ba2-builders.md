+++
title = "BA2 builders"
description = "ba2::Builder for general-file archives and ba2::Dx10Builder for texture archives: settings, every way to add a member, and writing."
weight = 30

[extra]
kind = "api"
+++

Behind the `ba2` feature, also named `Ba2Builder` and `Ba2Dx10Builder` at the crate root. Every
`Result` here is `ba2::Result`. [Building archives](@/docs/building.md) and
[BA2 textures](@/docs/textures.md) are the guides.

## Builder

{{ api_signature(value="struct Builder") }}

Writes a GNRL archive: every member as one chunk, with a string table. `Clone`, `Debug`,
`Default`.

{{ api_signature(value="fn new() -> Builder") }}

An empty builder: version 1, no compression, zlib level 6.

### Settings

{{ api_signature(value="fn version(&self) -> ArchiveVersion") }}

{{ api_signature(value="fn set_version(&mut self, version: ArchiveVersion) -> &mut Self") }}

The header's version. Versions 2 and 3 write their longer headers; version 3 writes its
compression field, 3 for LZ4 and 0 otherwise.

{{ api_signature(value="fn compression(&self) -> Option<Ba2CompressionFormat>") }}

{{ api_signature(value="fn set_compression(&mut self, compression: Option<Ba2CompressionFormat>) -> &mut Self") }}

The default for members: `None` stores them, `Some(Zip)` compresses with zlib, `Some(LZ4)` with
LZ4 blocks. Writing an LZ4-compressed member into an archive that is not version 3 fails with
`NotImplemented("BA2 LZ4 writer requires version 3")`.

{{ api_signature(value="fn zlib_level(&self) -> Compression") }}

{{ api_signature(value="fn set_zlib_level(&mut self, level: Compression) -> &mut Self") }}

The `flate2::Compression` level for zlib.

{{ api_signature(value="fn len(&self) -> usize") }}

{{ api_signature(value="fn is_empty(&self) -> bool") }}

How many members have been added, and whether none have.

### Adding members

Each call normalizes its archive path and fails with `InvalidArchivePath` or `DuplicatePath` as
[What builders store](@/docs/paths.md#what-builders-store) describes. The `_with_compression` form
of each takes a `CompressionOverride` for that member; the plain form inherits the builder's
default. A member forced to `Compress` when the default is `None` uses zlib.

{{ api_signature(value="fn add_bytes(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>) -> Result<()>") }}

{{ api_signature(value="fn add_bytes_with_compression(&mut self, path: impl AsRef<[u8]>, bytes: impl AsRef<[u8]>, compression: CompressionOverride) -> Result<()>") }}

A copy of `bytes`, taken now.

{{ api_signature(value="fn add_file(&mut self, archive_path: impl AsRef<[u8]>, source: impl AsRef<Path>) -> Result<()>") }}

{{ api_signature(value="fn add_file_with_compression(&mut self, archive_path: impl AsRef<[u8]>, source: impl AsRef<Path>, compression: CompressionOverride) -> Result<()>") }}

The file at `source`, following symlinks. Its size is read now and its bytes when the archive is
written; a size that changed in between fails the write.

{{ api_signature(value="fn add_dir(&mut self, root: impl AsRef<Path>) -> Result<()>") }}

{{ api_signature(value="fn add_dir_with_compression(&mut self, root: impl AsRef<Path>, compression: CompressionOverride) -> Result<()>") }}

Every file below `root`, recursively and in sorted order, each added as `add_file` at its path
relative to `root`. File symlinks are followed; directory symlinks are skipped.

{{ api_signature(value="fn add_archive_entry(&mut self, archive_path: impl AsRef<[u8]>, archive: Arc<Archive>, id: EntryId) -> Result<()>") }}

{{ api_signature(value="fn add_archive_entry_with_compression(&mut self, archive_path: impl AsRef<[u8]>, archive: Arc<Archive>, id: EntryId, compression: CompressionOverride) -> Result<()>") }}

Entry `id` of `archive`, decoded when the archive is written and stored under this builder's
compression. Its decoded size is read now, so an invalid id fails here with `OutOfBounds`, and a
GNMF entry with `NotImplemented`.

### Writing

{{ api_signature(value="fn write_path(&self, path: impl AsRef<Path>) -> Result<()>") }}

{{ api_signature(value="fn write_seek<W: Write + Seek>(&self, out: W) -> Result<()>") }}

{{ api_signature(value="fn to_vec(&self) -> Result<Vec<u8>>") }}

Writes the archive to a new or truncated file, to any seekable writer, or to memory. The records
are written first and patched with each member's offset and stored size once its data is out, so
deferred files stream through without being held in memory; an LZ4 member is compressed whole.
Members are sorted by hash, then by path.

## Dx10Builder

{{ api_signature(value="struct Dx10Builder") }}

Writes a DX10 texture archive. `Clone`, `Debug`, `Default`.

{{ api_signature(value="fn new() -> Dx10Builder") }}

An empty builder: version 1, no compression, zlib level 6.

### Settings

{{ api_signature(value="fn version(&self) -> ArchiveVersion") }}

{{ api_signature(value="fn set_version(&mut self, version: ArchiveVersion) -> &mut Self") }}

{{ api_signature(value="fn compression(&self) -> Option<Ba2CompressionFormat>") }}

{{ api_signature(value="fn set_compression(&mut self, compression: Option<Ba2CompressionFormat>) -> &mut Self") }}

{{ api_signature(value="fn zlib_level(&self) -> Compression") }}

{{ api_signature(value="fn set_zlib_level(&mut self, level: Compression) -> &mut Self") }}

{{ api_signature(value="fn len(&self) -> usize") }}

{{ api_signature(value="fn is_empty(&self) -> bool") }}

As on `Builder`.

### Adding textures

Paths are checked as on `Builder`. Texture metadata is checked when it is added: a zero mip
count, a zero width or height, or a DXGI format outside the
[supported formats](@/docs/textures.md#supported-formats) is a `Dds` error, and so is data whose
length is not exactly what the metadata describes.

{{ api_signature(value="fn add_dds_bytes(&mut self, path: impl AsRef<[u8]>, dds: impl AsRef<[u8]>) -> Result<()>") }}

{{ api_signature(value="fn add_dds_bytes_with_compression(&mut self, path: impl AsRef<[u8]>, dds: impl AsRef<[u8]>, compression: CompressionOverride) -> Result<()>") }}

A DDS file in memory: its header becomes the texture header, and the data after it is stored as
one chunk holding every mip. [Building from DDS files](@/docs/textures.md#building-from-dds-files)
lists what is refused.

{{ api_signature(value="fn add_dds_file(&mut self, archive_path: impl AsRef<[u8]>, source: impl AsRef<Path>) -> Result<()>") }}

{{ api_signature(value="fn add_dds_file_with_compression(&mut self, archive_path: impl AsRef<[u8]>, source: impl AsRef<Path>, compression: CompressionOverride) -> Result<()>") }}

The same, reading the file now.

{{ api_signature(value="fn add_texture_bytes(&mut self, path: impl AsRef<[u8]>, header: TextureHeader, bytes: impl AsRef<[u8]>) -> Result<()>") }}

{{ api_signature(value="fn add_texture_bytes_with_compression(&mut self, path: impl AsRef<[u8]>, header: TextureHeader, bytes: impl AsRef<[u8]>, compression: CompressionOverride) -> Result<()>") }}

A texture given as its header and its data, without a DDS header, copied now.

{{ api_signature(value="fn add_texture_file(&mut self, archive_path: impl AsRef<[u8]>, header: TextureHeader, source: impl AsRef<Path>) -> Result<()>") }}

{{ api_signature(value="fn add_texture_file_with_compression(&mut self, archive_path: impl AsRef<[u8]>, header: TextureHeader, source: impl AsRef<Path>, compression: CompressionOverride) -> Result<()>") }}

The same with the data in a file, read when the archive is written. The file is data, not a DDS
file; its size is checked against `header` now.

{{ api_signature(value="fn add_dir_with_texture_header(&mut self, root: impl AsRef<Path>, header: TextureHeader) -> Result<()>") }}

Every file below `root` as `add_texture_file` with the same header. Only useful for test archives.

{{ api_signature(value="fn add_archive_entry(&mut self, archive_path: impl AsRef<[u8]>, archive: Arc<Archive>, id: EntryId) -> Result<()>") }}

Texture `id` of `archive`, copied without decoding: its header, its chunk records and its stored
chunk bytes. `NotImplemented` now if `archive` is not a DX10 archive or the entry is not a
texture, and when the archive is written if `archive`'s version or compression method differs
from this builder's.

### Writing

{{ api_signature(value="fn write_path(&self, path: impl AsRef<Path>) -> Result<()>") }}

{{ api_signature(value="fn write_seek<W: Write + Seek>(&self, out: W) -> Result<()>") }}

{{ api_signature(value="fn to_vec(&self) -> Result<Vec<u8>>") }}

As on `Builder`, except that every texture's data is prepared, and compressed, before the
archive is written, so deferred texture files are read into memory at that point.
