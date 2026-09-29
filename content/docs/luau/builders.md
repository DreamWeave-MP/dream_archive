+++
title = "Builders"
description = "ba2.Builder, ba2.Dx10Builder, bsa.tes3.Builder and bsa.tes4.Builder: their settings, adding members, and writing."
weight = 30

[extra]
kind = "api"
+++

Each builder is made by its module's `Builder.new()` (or `Dx10Builder.new()`) and works like the
Rust one: [Building archives](@/docs/building.md) explains paths, deferred files, compression and
the TES4 settings. Setters and `add*` calls return nothing; a bad argument or a refused path
raises the Rust error's message.

```lua
local dreamArchive = require("@dream/archive")

local builder = dreamArchive.ba2.Builder.new()
builder:setCompression(dreamArchive.ba2.compression.zip)
builder:addBytes("meshes/example.nif", "payload")
builder:addBytes("textures/example.txt", buffer.fromstring("from a buffer"))

local archive = dreamArchive.openBytes(builder:toBuffer())
assert(archive:format() == "ba2")
assert(archive:readFileRequired("meshes/example.nif") == "payload")
```

## Shared by all four

{{ api_signature(value="builder:len(): number") }}

{{ api_signature(value="builder:isEmpty(): boolean") }}

How many members have been added, and whether none have.

{{ api_signature(value="builder:addBytes(path: string, bytes: buffer | string)") }}

A copy of `bytes`, taken now. On the DX10 builder this is `addTextureBytes` or `addDdsBytes`.

{{ api_signature(value="builder:addFile(archivePath: string, source: string)") }}

The host file at `source`, read when the archive is written. Not on the DX10 builder, whose file
call is `addDdsFile`.

{{ api_signature(value="builder:addArchiveEntry(archivePath: string, archive: dream_archive_Archive, id: number)") }}

Entry `id`, which is `entry.id`, of an opened archive of the builder's family, copied when the
archive is written. The archive is shared with the builder, not copied. Another family raises
"addArchiveEntry needs a BA2 archive", or TES3 or TES4.

{{ api_signature(value="builder:addDir(root: string)") }}

Every file below a host directory. Not on the DX10 builder.

{{ api_signature(value="builder:writePath(path: string)") }}

{{ api_signature(value="builder:toBytes(): string") }}

{{ api_signature(value="builder:toString(): string") }}

Write the archive to a host file, or return it as a string; `toBytes` and `toString` are the same
call.

## ba2.Builder

`dream_archive_Ba2Builder`: a general-file (GNRL) BA2.

{{ api_signature(value="builder:setVersion(version: number)") }}

1, 2, 3, 7 or 8; see `ba2.version`. Anything else raises "unsupported BA2 version".

{{ api_signature(value="builder:setCompression(compression: string?)") }}

`"zip"`, `"lz4"`, or `"none"`, `"store"` or `nil` for none; see `ba2.compression`.

{{ api_signature(value="builder:setZlibLevel(level: number)") }}

0 to 9.

{{ api_signature(value="builder:addBytesWithCompression(path: string, bytes: buffer | string, compression: string?)") }}

{{ api_signature(value="builder:addArchiveEntryWithCompression(archivePath: string, archive: dream_archive_Archive, id: number, compression: string?)") }}

With a per-member override: `"inherit"` (or `nil`), `"store"` or `"compress"`.

{{ api_signature(value="builder:toBuffer(): buffer") }}

The archive as a new `buffer`. Only this builder has it.

## ba2.Dx10Builder

`dream_archive_Ba2Dx10Builder`: a texture (DX10) BA2. `setVersion`, `setCompression` and
`setZlibLevel` are as on `ba2.Builder`. See [BA2 textures](@/docs/textures.md).

{{ api_signature(value="builder:addDdsBytes(path: string, dds: buffer | string)") }}

{{ api_signature(value="builder:addDdsFile(archivePath: string, source: string)") }}

A DDS file, in memory or on the host, parsed and stored without its header. The file is read
when it is added.

{{ api_signature(value="builder:addTextureBytes(path: string, header: { height: number, width: number, mipCount: number, format: number, flags: number, tileMode: number }, bytes: buffer | string)") }}

A texture given as its header and its data. Every key of `header` is required and must be an
exact integer in range; any other key raises.

{{ api_signature(value="builder:addArchiveEntry(archivePath: string, archive: dream_archive_Archive, id: number)") }}

A texture of an opened DX10 archive, copied without decoding.

The Rust builder's `addTextureFile`, `addDirWithTextureHeader` and `_with_compression` forms are
not bound.

## bsa.tes3.Builder

`dream_archive_Tes3Builder`: a TES3 BSA. It has no settings.

{{ api_signature(value="builder:addEncodedPath(path: string, encoding: string, bytes: buffer | string)") }}

`addBytes` with the UTF-8 `path` encoded in a legacy code page first; see `bsa.encoding`.

## bsa.tes4.Builder

`dream_archive_Tes4Builder`: a TES4 BSA. A new one writes version 104, uncompressed, with names
and the `MISC` content type.

{{ api_signature(value="builder:setVersion(version: number)") }}

103, 104 or 105.

{{ api_signature(value="builder:setProfile(profile: string)") }}

A game, which sets the version; see `bsa.tes4.profile`.

{{ api_signature(value="builder:setArchiveTypes(bits: number)") }}

The content-type bits; see `bsa.tes4.archiveTypes`.

{{ api_signature(value="builder:setCompressed(compressed: boolean)") }}

{{ api_signature(value="builder:setNameMode(mode: string)") }}

{{ api_signature(value="builder:setZlibLevel(level: number)") }}

Compression by default, how paths are stored (see `bsa.tes4.nameMode`), and the zlib level, 0 to
9.

{{ api_signature(value="builder:addBytesWithCompression(path: string, bytes: buffer | string, compression: string?)") }}

{{ api_signature(value="builder:addArchiveEntryWithCompression(archivePath: string, archive: dream_archive_Archive, id: number, compression: string?)") }}

With a per-member override: `"inherit"` (or `nil`), `"store"` or `"compress"`.

{{ api_signature(value="builder:addEncodedPath(path: string, encoding: string, bytes: buffer | string)") }}

As on the TES3 builder.

```lua
local dreamArchive = require("@dream/archive")
local tes4 = dreamArchive.bsa.tes4

local builder = tes4.Builder.new()
builder:setProfile(tes4.profile.skyrimSe)
builder:setCompressed(true)
builder:setArchiveTypes(tes4.archiveTypes.MESHES + tes4.archiveTypes.TEXTURES)
builder:addBytes("meshes/example.nif", string.rep("x", 4096))
builder:writePath("Example.bsa")

local archive = tes4.openPath("Example.bsa")
local entry = archive:get("meshes/example.nif")
assert(entry.compressed and entry.size == 4096)
```
