+++
title = "Type definitions"
description = "The Luau type definitions an l3i plan generates for @dream/archive: every module, method, field and table shape."
weight = 40

[extra]
kind = "api"
+++

Every function, method and field of `@dream/archive` carries a Luau signature, and
`plan.type_definitions()` renders them, with everything else the plan provides, as one `.d.luau`
text for an editor's language server or for Luau's own type checker. [Embedding
Luau](@/docs/luau-hosts.md#types-for-editors-and-checks) shows how to get it.

This is what a plan holding `ArchiveExtension` generates for the archive's part, in the order the
plan writes it. Two details depend on the plan rather than on the crate: the tag numbers in the
comments, and the rest of the text, since l3i adds its own `dream.net` extension to every plan.

- `dream_archive_Archive` and `dream_archive_Entry` are the types scripts hold; the four builder
  types and the `dream_archive_Entries` view are made by the modules.
- `Module__dream_archive` and its siblings are the module tables' types; `require` of a module
  path has that type, so a `--!strict` script is checked all the way down.
- `integer` is Luau's 64-bit integer type, which TES3 and TES4 hashes use; every other count, size
  and index is a `number`.

```lua
-- dream.archive.Archive (owned by dream.archive; tag 3)
-- An opened BA2, TES3 BSA, or TES4 BSA archive.
declare extern type dream_archive_Archive with
    -- "ba2", "bsaTes3", or "bsaTes4".
    function format(self): string
    function len(self): number
    function isEmpty(self): boolean
    function archiveSize(self): number
    -- A view of the entries; nothing is copied until a handle is read.
    function entries(self): dream_archive_Entries
    function entry(self, index: number): dream_archive_Entry?
    function contains(self, path: string): boolean
    function get(self, path: string): dream_archive_Entry?
    -- TES3: (hash); TES4: (folderHash, fileHash); BA2: (directory, file, extension).
    function containsHash(self, hash: integer | number, second: (integer | number)?, third: number?): boolean
    function getByHash(self, hash: integer | number, second: (integer | number)?, third: number?): dream_archive_Entry?
    -- The slow path: two copies. Prefer readInto.
    function readFile(self, path: string): string?
    function readFileRequired(self, path: string): string
    function extractFile(self, path: string): string?
    function extractFileRequired(self, path: string): string
    function readEntry(self, index: number): string
    function extractEntry(self, index: number): string
    function extractEntryToPath(self, index: number, path: string): number
    -- Decodes one entry straight into the buffer at offset (default 0); returns the byte count. Errors leave the buffer partially written.
    function readInto(self, entry: dream_archive_Entry | string, buffer: buffer, offset: number?): number
    function extractTo(self, target: string): number
    -- BSA only.
    function extractToWithEncoding(self, target: string, encoding: string): number
    -- TES4 only: names hash-only entries from candidate paths.
    function extractToWithPaths(self, target: string, paths: { string }): number
    -- BA2 only.
    function info(self): { format: string, version: number, compression: string, strings: boolean }
end

-- dream.archive.Ba2Builder (owned by dream.archive; untagged)
-- Builds a general (GNRL) BA2 archive.
declare extern type dream_archive_Ba2Builder with
    function len(self): number
    function isEmpty(self): boolean
    function setVersion(self, version: number)
    function setCompression(self, compression: string?)
    function setZlibLevel(self, level: number)
    function addBytes(self, path: string, bytes: buffer | string)
    function addBytesWithCompression(self, path: string, bytes: buffer | string, compression: string?)
    function addFile(self, archivePath: string, source: string)
    function addArchiveEntry(self, archivePath: string, archive: dream_archive_Archive, id: number)
    function addArchiveEntryWithCompression(self, archivePath: string, archive: dream_archive_Archive, id: number, compression: string?)
    function addDir(self, root: string)
    function writePath(self, path: string)
    function toBytes(self): string
    function toString(self): string
    function toBuffer(self): buffer
end

-- dream.archive.Ba2Dx10Builder (owned by dream.archive; untagged)
-- Builds a texture (DX10) BA2 archive.
declare extern type dream_archive_Ba2Dx10Builder with
    function len(self): number
    function isEmpty(self): boolean
    function setVersion(self, version: number)
    function setCompression(self, compression: string?)
    function setZlibLevel(self, level: number)
    function addDdsBytes(self, path: string, dds: buffer | string)
    function addTextureBytes(self, path: string, header: { height: number, width: number, mipCount: number, format: number, flags: number, tileMode: number }, bytes: buffer | string)
    function addDdsFile(self, archivePath: string, source: string)
    function addArchiveEntry(self, archivePath: string, archive: dream_archive_Archive, id: number)
    function writePath(self, path: string)
    function toBytes(self): string
    function toString(self): string
end

-- dream.archive.Entries (owned by dream.archive; untagged)
-- The entries of an archive: `#`, `[i]`, `for`, `:toTable()`.
declare extern type dream_archive_Entries with
    function toTable(self): { dream_archive_Entry }
    function __len(self): number
    [number]: dream_archive_Entry?
    function __iter(self): (({}, number) -> (number?, dream_archive_Entry), {}, number)
end

-- dream.archive.Entry (owned by dream.archive; tag 1)
-- One entry of an archive; reads its metadata from the shared index.
declare extern type dream_archive_Entry with
    -- 1-based position in archive table order.
    index: number
    -- The stable id builders' addArchiveEntry takes; equals index.
    id: number
    format: string
    -- Raw archive path bytes; nil for hash-only entries.
    path: string?
    -- The path after its last separator; nil for hash-only entries.
    name: string?
    folder: string?
    -- Decoded payload size; nil when the format cannot tell (BA2 GNMF).
    size: number?
    storedSize: number
    offset: number?
    compressed: boolean
    chunks: number?
    -- TES3: the entry hash; TES4: the file hash; nil for BA2.
    hash: integer?
    hashHex: string?
    -- TES4 only.
    folderHash: integer?
    folderHashHex: string?
    -- BA2 only.
    directoryHash: number?
    -- BA2 only.
    fileHash: number?
    -- BA2 only.
    extensionHash: number?
end

-- dream.archive.Tes3Builder (owned by dream.archive; untagged)
-- Builds a TES3 (Morrowind) BSA archive.
declare extern type dream_archive_Tes3Builder with
    function len(self): number
    function isEmpty(self): boolean
    function addBytes(self, path: string, bytes: buffer | string)
    -- Encodes the UTF-8 path to the archive's legacy filename bytes.
    function addEncodedPath(self, path: string, encoding: string, bytes: buffer | string)
    function addFile(self, archivePath: string, source: string)
    function addArchiveEntry(self, archivePath: string, archive: dream_archive_Archive, id: number)
    function addDir(self, root: string)
    function writePath(self, path: string)
    function toBytes(self): string
    function toString(self): string
end

-- dream.archive.Tes4Builder (owned by dream.archive; untagged)
-- Builds a TES4-family (Oblivion to Skyrim SE) BSA archive.
declare extern type dream_archive_Tes4Builder with
    function len(self): number
    function isEmpty(self): boolean
    -- 103, 104, or 105.
    function setVersion(self, version: number)
    function setProfile(self, profile: string)
    function setArchiveTypes(self, bits: number)
    function setCompressed(self, compressed: boolean)
    function setNameMode(self, mode: string)
    function setZlibLevel(self, level: number)
    function addBytes(self, path: string, bytes: buffer | string)
    function addBytesWithCompression(self, path: string, bytes: buffer | string, compression: string?)
    function addEncodedPath(self, path: string, encoding: string, bytes: buffer | string)
    function addFile(self, archivePath: string, source: string)
    function addArchiveEntry(self, archivePath: string, archive: dream_archive_Archive, id: number)
    function addArchiveEntryWithCompression(self, archivePath: string, archive: dream_archive_Archive, id: number, compression: string?)
    function addDir(self, root: string)
    function writePath(self, path: string)
    function toBytes(self): string
    function toString(self): string
end

-- module @dream/archive/ba2 (provided by dream.archive)
-- BA2 archives: format-specific opening, hashing, and builders.
export type Module__dream_archive_ba2 = {
    openPath: (path: string) -> dream_archive_Archive,
    openBytes: (bytes: buffer | string) -> dream_archive_Archive,
    hashFile: (path: string) -> { directory: number, file: number, extension: number, normalized: string },
    Builder: { new: () -> dream_archive_Ba2Builder },
    Dx10Builder: { new: () -> dream_archive_Ba2Dx10Builder },
    compression: { none: "none", store: "store", zip: "zip", lz4: "lz4" },
    version: { V1: number, V2: number, V3: number, V7: number, V8: number },
}

-- module @dream/archive/bsa/tes3 (provided by dream.archive)
-- TES3 (Morrowind) BSA archives.
export type Module__dream_archive_bsa_tes3 = {
    openPath: (path: string) -> dream_archive_Archive,
    openBytes: (bytes: buffer | string) -> dream_archive_Archive,
    hashFile: (path: string) -> { lo: number, hi: number, hex: string, hash: integer, normalized: string },
    Builder: { new: () -> dream_archive_Tes3Builder },
}

-- module @dream/archive/bsa/tes4 (provided by dream.archive)
-- TES4-family (Oblivion to Skyrim SE) BSA archives.
export type Module__dream_archive_bsa_tes4 = {
    openPath: (path: string) -> dream_archive_Archive,
    openBytes: (bytes: buffer | string) -> dream_archive_Archive,
    hashDirectory: (path: string) -> { last: number, last2: number, length: number, first: number, crc: number, hex: string, hash: integer },
    hashFile: (path: string) -> { last: number, last2: number, length: number, first: number, crc: number, hex: string, hash: integer },
    Builder: { new: () -> dream_archive_Tes4Builder },
    nameMode: { strings: "strings", hashOnly: "hashOnly", embedded: "embedded", stringsAndEmbedded: "stringsAndEmbedded" },
    profile: { oblivion: "oblivion", fallout3: "fallout3", falloutNewVegas: "falloutNewVegas", skyrimLe: "skyrimLe", skyrimSe: "skyrimSe" },
    archiveTypes: { MESHES: number, TEXTURES: number, MENUS: number, SOUNDS: number, VOICES: number, SHADERS: number, TREES: number, FONTS: number, MISC: number },
}

-- module @dream/archive (provided by dream.archive)
-- Bethesda BA2 and BSA archives: open, list, look up, read, extract, and build.
export type Module__dream_archive = {
    openPath: (path: string) -> dream_archive_Archive,
    openBytes: (bytes: buffer | string) -> dream_archive_Archive,
    -- nil for unknown or too-short headers.
    detectPath: (path: string) -> string?,
    guessFormat: (bytes: buffer | string) -> string?,
    normalizePath: (path: string) -> string,
    -- The @dream/archive/ba2 module.
    ba2: Module__dream_archive_ba2,
    -- The @dream/archive/bsa module.
    bsa: Module__dream_archive_bsa,
}

-- module @dream/archive/bsa (provided by dream.archive)
-- BSA archives: filename encodings and the TES3 / TES4 modules.
export type Module__dream_archive_bsa = {
    encodeFilename: (text: string, encoding: string) -> string,
    decodeFilenameLossy: (bytes: string, encoding: string) -> string,
    normalizePath: (path: string) -> string,
    encoding: { utf8: "utf8", windows1250: "windows1250", windows1251: "windows1251", windows1252: "windows1252", cp437: "cp437" },
    -- The @dream/archive/bsa/tes3 module.
    tes3: Module__dream_archive_bsa_tes3,
    -- The @dream/archive/bsa/tes4 module.
    tes4: Module__dream_archive_bsa_tes4,
}
```
