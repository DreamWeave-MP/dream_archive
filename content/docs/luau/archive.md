+++
title = "Archive and Entry"
description = "An opened archive's methods, entry handles and their fields, the entries view, hash lookups, and reading into buffers."
weight = 20

[extra]
kind = "api"
+++

Every `open*` function returns the same type, `dream_archive_Archive`, whatever the family. The
methods that only make sense for one family raise on the others, naming both: "info needs a BA2
archive; this archive is bsaTes3".

```lua
local dreamArchive = require("@dream/archive")

local archive = dreamArchive.openPath("Morrowind.bsa")
print(archive)
print(archive:format(), archive:len())

local entry = archive:get("textures/tx_wood.dds")
if entry then
    print(entry.path, entry.size, entry.hashHex)
end
```

```text
dream.archive.Archive(bsaTes3, 11090 entries, Morrowind.bsa)
bsaTes3	11090
textures\tx_wood.dds	43808	071d635da2af78a8
```

## Archive

### Metadata

{{ api_signature(value="archive:format(): string") }}

`"ba2"`, `"bsaTes3"` or `"bsaTes4"`.

{{ api_signature(value="archive:len(): number") }}

{{ api_signature(value="archive:isEmpty(): boolean") }}

{{ api_signature(value="archive:archiveSize(): number") }}

The number of entries, whether there are none, and the archive's size in bytes.

{{ api_signature(value="archive:info(): { format: string, version: number, compression: string, strings: boolean }") }}

BA2 only: the payload family (`"gnrl"`, `"dx10"` or `"gnmf"`), the version, the compression
method (`"zip"` or `"lz4"`), and whether there is a string table.

`tostring(archive)` gives the family, the entry count, and the host path it was opened from, or
`<memory>`.

### Entries and lookup

{{ api_signature(value="archive:entries(): dream_archive_Entries") }}

A [view](#entries) of every entry, in archive order. Nothing is copied until an entry is read.

{{ api_signature(value="archive:entry(index: number): dream_archive_Entry?") }}

The entry at a 1-based index, or `nil`.

{{ api_signature(value="archive:get(path: string): dream_archive_Entry?") }}

{{ api_signature(value="archive:contains(path: string): boolean") }}

The entry at a path, and whether there is one. Paths match as in Rust:
[Archive paths](@/docs/paths.md#lookups).

{{ api_signature(value="archive:getByHash(hash: integer | number, second: (integer | number)?, third: number?): dream_archive_Entry?") }}

{{ api_signature(value="archive:containsHash(hash: integer | number, second: (integer | number)?, third: number?): boolean") }}

The entry with a stored hash, and whether there is one; see [Hashes](#hashes).

### Reading

{{ api_signature(value="archive:readInto(entry: dream_archive_Entry | string, buffer: buffer, offset: number?): number") }}

Decodes one member, named by an entry handle of this archive or by a path, straight into
`buffer` at `offset` (0 by default), and returns the number of bytes written. It raises, before
writing anything, if the member does not fit in the bytes after `offset`, if `offset` is past the
buffer's end, if the path finds nothing, or if the handle belongs to another archive. A decoding
error part way leaves the buffer partly written. This is the fast path: no string is made.

{{ api_signature(value="archive:readFile(path: string): string?") }}

{{ api_signature(value="archive:readFileRequired(path: string): string") }}

{{ api_signature(value="archive:extractFile(path: string): string?") }}

{{ api_signature(value="archive:extractFileRequired(path: string): string") }}

A member as a new string: `nil` when the path finds nothing, or for the `Required` forms an
error, "archive member not found:" and the path. The `extract` forms go through Rust's
`extract_file` and give the same bytes. Each call copies the member twice, once to decode it and
once into Luau.

{{ api_signature(value="archive:readEntry(index: number): string") }}

{{ api_signature(value="archive:extractEntry(index: number): string") }}

The member at a 1-based index, as a string. An index out of range raises "entry index out of
bounds".

### Extracting

{{ api_signature(value="archive:extractEntryToPath(index: number, path: string): number") }}

The member at a 1-based index, written atomically to a host path; returns the bytes written.

{{ api_signature(value="archive:extractTo(target: string): number") }}

Every entry, below a host directory; see [Extracting](@/docs/extracting.md).

{{ api_signature(value="archive:extractToWithEncoding(target: string, encoding: string): number") }}

BSA only: every entry, with its path decoded through `encoding` into file names.

{{ api_signature(value="archive:extractToWithPaths(target: string, paths: { string }): number") }}

TES4 only: every entry one of `paths` finds, written under that path; elements that are not
strings are ignored. For [hash-only archives](@/docs/hashes.md#hash-only-archives).

## Entry

An entry handle, `dream_archive_Entry`, is an index into its archive's parsed table: holding one
keeps the archive alive, and reading a field reads the table. Every field is read-only. Two
handles of the same entry are `==`.

| Field | Type | TES3 | TES4 | BA2 |
|---|---|---|---|---|
| `index` | `number` | 1-based position | same | same |
| `id` | `number` | the same as `index`, for `addArchiveEntry` | same | same |
| `format` | `string` | `"bsaTes3"` | `"bsaTes4"` | `"ba2"` |
| `path` | `string?` | the stored path | the path, or `nil` without names | the stored name, or `nil` without a string table |
| `name` | `string?` | the path after its last separator | the stored file name, or `nil` | the name after its last separator, or `nil` without a string table |
| `folder` | `string?` | the path before its last separator, `""` for the root | the stored folder, `""` for the root, or `nil` | the name before its last separator, `""` for the root, or `nil` without a string table |
| `size` | `number?` | the member's size | the decoded size | the decoded size, DDS header included; `nil` for GNMF |
| `storedSize` | `number` | the member's size | the stored size, prefix and embedded name included | the sum of its chunks' stored sizes |
| `offset` | `number?` | from the start of the data section | from the start of the file | the first chunk's, from the start of the file; `nil` without chunks |
| `compressed` | `boolean` | `false` | the record's compression | any chunk is compressed |
| `chunks` | `number?` | `nil` | `nil` | the number of chunks |
| `hash` | `integer?` | the entry hash | the file hash | `nil` |
| `hashHex` | `string?` | 16 hex digits | 16 hex digits | `nil` |
| `folderHash` | `integer?` | `nil` | the folder hash | `nil` |
| `folderHashHex` | `string?` | `nil` | 16 hex digits | `nil` |
| `directoryHash`, `fileHash`, `extensionHash` | `number?` | `nil` | `nil` | the three hash fields |

`tostring(entry)` gives its index and path, or `<hash only>`.

## Entries

`archive:entries()` returns a `dream_archive_Entries` view:

| Use | Gives |
|---|---|
| `#entries` | the number of entries |
| `entries[i]` | the entry at 1-based `i`, or `nil` |
| `for i, entry in entries do` | every index and entry, in order |
| `entries:toTable()` | a new table of every entry handle |

The view is not a table: `ipairs`, `pairs` and `table.insert` do not work on it. Call `toTable()`
for a table.

```lua
local dreamArchive = require("@dream/archive")
local archive = dreamArchive.openPath("Morrowind.bsa")

local meshes = 0
for _, entry in archive:entries() do
    if string.sub(entry.path, -4) == ".nif" then
        meshes += 1
    end
end
print(meshes, "meshes")
```

## Hashes

TES3 and TES4 hashes are Luau integers carrying all 64 bits. Compare them with `==`; they are not
numbers, and arithmetic mixing the two raises. BA2 hashes are three 32-bit numbers. The hash
functions of each module return tables with the same values, so a lookup can be built from a path:

| Family | `getByHash` and `containsHash` take | From |
|---|---|---|
| TES3 | `(hash)` | `tes3.hashFile(path).hash`, or `entry.hash` |
| TES4 | `(folderHash, fileHash)` | `tes4.hashDirectory(folder).hash` and `tes4.hashFile(name).hash` |
| BA2 | `(directory, file, extension)` | the fields of `ba2.hashFile(path)` |

Giving a family the wrong number of arguments raises, and says which shape it takes.

```lua
local dreamArchive = require("@dream/archive")
local tes3 = dreamArchive.bsa.tes3

local archive = tes3.openPath("Morrowind.bsa")
local hash = tes3.hashFile("Textures/Tx_Wood.DDS")
local entry = archive:getByHash(hash.hash)
assert(entry and entry.path == [[textures\tx_wood.dds]])
assert(archive:containsHash(entry.hash))
```

## Reading into buffers

`readFile` and its siblings make a Luau string of every member they read. For anything read
often, decode into a buffer the script keeps instead:

```lua
local dreamArchive = require("@dream/archive")
local archive = dreamArchive.openPath("Morrowind.bsa")

local scratch = buffer.create(1024 * 1024)
local largest = 0
for _, entry in archive:entries() do
    if entry.size and entry.size <= buffer.len(scratch) then
        local n = archive:readInto(entry, scratch)
        largest = math.max(largest, n)
    end
end
print(largest)
```

Reading 64 KiB sixteen times takes 27 µs this way against 217 µs with `readFile`
([the numbers](@/docs/compatibility.md#what-it-costs)).
