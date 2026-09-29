+++
title = "Modules"
description = "@dream/archive and its four submodules: openPath, openBytes, detectPath, guessFormat, normalizePath, the hash functions, filename encodings, and the constant tables."
weight = 10

[extra]
kind = "api"
+++

```lua
local dreamArchive = require("@dream/archive")
local tes4 = require("@dream/archive/bsa/tes4")
assert(tes4 == dreamArchive.bsa.tes4)
```

Five modules, each reachable by its own path and as a field of its parent: `dreamArchive.ba2` is
`require("@dream/archive/ba2")`, and so on down. All of them are frozen.

## @dream/archive

{{ api_signature(value="openPath(path: string) -> dream_archive_Archive") }}

Opens the archive at a host path, whatever its family, memory-mapped. An unknown family raises
"unknown or disabled archive format"; a missing file raises the I/O error.

{{ api_signature(value="openBytes(bytes: buffer | string) -> dream_archive_Archive") }}

The same from bytes in memory, copied once into the archive.

{{ api_signature(value="detectPath(path: string) -> string?") }}

{{ api_signature(value="guessFormat(bytes: buffer | string) -> string?") }}

The family a file's or a payload's first four bytes name: `"ba2"`, `"bsaTes3"` or `"bsaTes4"`.
`nil` for anything else, and for fewer than four bytes. `detectPath` raises when the file cannot
be opened.

{{ api_signature(value="normalizePath(path: string) -> string") }}

`path` with dream-path's normalization, the one BSA lookups apply: `\` to `/`, ASCII lowercase,
leading and repeated separators dropped.

```lua
local dreamArchive = require("@dream/archive")
assert(dreamArchive.detectPath("Morrowind.bsa") == "bsaTes3")
assert(dreamArchive.guessFormat("BTDX") == "ba2")
assert(dreamArchive.normalizePath([[\\Meshes\\X\Ex_Door.NIF]]) == "meshes/x/ex_door.nif")
```

## @dream/archive/ba2

{{ api_signature(value="openPath(path: string) -> dream_archive_Archive") }}

{{ api_signature(value="openBytes(bytes: buffer | string) -> dream_archive_Archive") }}

Open a BA2 without detecting the family. Anything else raises the BA2 parser's error, such as
"invalid magic read from archive header".

{{ api_signature(value="hashFile(path: string) -> { directory: number, file: number, extension: number, normalized: string }") }}

The BA2 hash of a path, and the normalized path that was hashed.

{{ api_signature(value="Builder: { new: () -> dream_archive_Ba2Builder }") }}

{{ api_signature(value="Dx10Builder: { new: () -> dream_archive_Ba2Dx10Builder }") }}

The [builders](@/docs/luau/builders.md) of general and texture archives.

{{ api_signature(value='compression: { none: "none", store: "store", zip: "zip", lz4: "lz4" }') }}

The values `setCompression` takes. `none` and `store` both mean uncompressed.

{{ api_signature(value="version: { V1: number, V2: number, V3: number, V7: number, V8: number }") }}

The versions `setVersion` takes: 1, 2, 3, 7 and 8.

```lua
local ba2 = require("@dream/archive/ba2")
local hash = ba2.hashFile("Interface/Credits.txt")
assert(hash.normalized == [[interface\credits.txt]])
assert(hash.extension == 0x747874) -- "txt"
```

## @dream/archive/bsa

{{ api_signature(value="encodeFilename(text: string, encoding: string) -> string") }}

UTF-8 `text` as archive path bytes in a legacy code page. A character the code page cannot hold
raises "filename can not be represented losslessly as" and the code page.

{{ api_signature(value="decodeFilenameLossy(bytes: string, encoding: string) -> string") }}

Archive path bytes as UTF-8 text, for display.

{{ api_signature(value="normalizePath(path: string) -> string") }}

The same function as `@dream/archive`'s.

{{ api_signature(value='encoding: { utf8: "utf8", windows1250: "windows1250", windows1251: "windows1251", windows1252: "windows1252", cp437: "cp437" }') }}

The encodings. Wherever an encoding is taken, `"cp1250"`, `"cp1251"` and `"cp1252"` are accepted
too; anything else raises "unknown filename encoding".

{{ api_signature(value="tes3: Module__dream_archive_bsa_tes3") }}

{{ api_signature(value="tes4: Module__dream_archive_bsa_tes4") }}

The two generations' modules.

```lua
local bsa = require("@dream/archive/bsa")
local stored = bsa.encodeFilename("textures/zażółć.dds", bsa.encoding.windows1250)
assert(#stored == 19)
assert(bsa.decodeFilenameLossy(stored, "cp1250") == "textures/zażółć.dds")
assert(not pcall(bsa.encodeFilename, "zażółć", "windows1252"))
```

## @dream/archive/bsa/tes3

{{ api_signature(value="openPath(path: string) -> dream_archive_Archive") }}

{{ api_signature(value="openBytes(bytes: buffer | string) -> dream_archive_Archive") }}

Open a TES3 BSA without detecting the family.

{{ api_signature(value="hashFile(path: string) -> { lo: number, hi: number, hex: string, hash: integer, normalized: string }") }}

The TES3 hash of a path: the two stored halves, the 16-digit hex spelling, the 64-bit value as
an integer (what `entry.hash` holds and `getByHash` takes), and the normalized path that was
hashed.

{{ api_signature(value="Builder: { new: () -> dream_archive_Tes3Builder }") }}

The TES3 [builder](@/docs/luau/builders.md#bsa-tes3-builder).

## @dream/archive/bsa/tes4

{{ api_signature(value="openPath(path: string) -> dream_archive_Archive") }}

{{ api_signature(value="openBytes(bytes: buffer | string) -> dream_archive_Archive") }}

Open a TES4 BSA without detecting the family.

{{ api_signature(value="hashDirectory(path: string) -> { last: number, last2: number, length: number, first: number, crc: number, hex: string, hash: integer }") }}

{{ api_signature(value="hashFile(path: string) -> { last: number, last2: number, length: number, first: number, crc: number, hex: string, hash: integer }") }}

The TES4 hash of a folder, or of the file name at the end of a path: its five fields, the hex
spelling, and the 64-bit value as an integer, what `entry.folderHash` and `entry.hash` hold.

{{ api_signature(value="Builder: { new: () -> dream_archive_Tes4Builder }") }}

The TES4 [builder](@/docs/luau/builders.md#bsa-tes4-builder).

{{ api_signature(value='nameMode: { strings: "strings", hashOnly: "hashOnly", embedded: "embedded", stringsAndEmbedded: "stringsAndEmbedded" }') }}

{{ api_signature(value='profile: { oblivion: "oblivion", fallout3: "fallout3", falloutNewVegas: "falloutNewVegas", skyrimLe: "skyrimLe", skyrimSe: "skyrimSe" }') }}

The values `setNameMode` and `setProfile` take.

{{ api_signature(value="archiveTypes: { MESHES: number, TEXTURES: number, MENUS: number, SOUNDS: number, VOICES: number, SHADERS: number, TREES: number, FONTS: number, MISC: number }") }}

The content-type bits, 1 to 256, for `setArchiveTypes`. Add them to combine them.

```lua
local tes4 = require("@dream/archive/bsa/tes4")
local folder = tes4.hashDirectory([[Meshes\Armor\Iron]])
local file = tes4.hashFile("meshes/armor/iron/cuirass.nif")
print(folder.hex, file.hex) --> 555df53a6d116f6e	0a9125a06307f373
assert(tes4.archiveTypes.MESHES + tes4.archiveTypes.TEXTURES == 3)
```
