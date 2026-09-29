+++
title = "Formats"
description = "The three archive families: which games use them, their versions, names, hashes and compression, and what dream_archive refuses."
weight = 15

[extra]
kind = "reference"
+++

Bethesda's archives come in three families, and the first four bytes say which one a file is.
`guess_format` and `Archive::open_path` read nothing else to decide.

| First four bytes | Family | `FileFormat` | Module |
|---|---|---|---|
| `00 01 00 00` | TES3 BSA | `BSA(BsaFormat::TES3)` | [`bsa::tes3`](@/docs/api/tes3.md) |
| `BSA\0` | TES4 BSA | `BSA(BsaFormat::TES4)` | [`bsa::tes4`](@/docs/api/tes4.md) |
| `BTDX` | BA2 | `BA2` | [`ba2`](@/docs/api/ba2.md) |

Anything else is `Error::UnknownFormat`, and so is a family whose feature is turned off. A file
shorter than four bytes is an I/O error (`UnexpectedEof`).

## TES3 BSA

Morrowind's archives, and OpenMW's for Morrowind content.

- **Versions**: one, `0x100`, which is the whole header's first field; there is no magic.
- **Names**: always present, NUL-terminated, as raw bytes. Morrowind stores them in lowercase with
  backslashes; nothing forces other tools to.
- **Hashes**: one 64-bit hash per file, stored as two 32-bit halves. See
  [Hashes](@/docs/hashes.md#tes3).
- **Compression**: none. Every member is read straight from the file.

## TES4 BSA

Oblivion through Skyrim Special Edition. One layout, three versions:

| Version | Games | Compression | Folder record |
|---|---|---|---|
| 103 | Oblivion | zlib | 16 bytes |
| 104 | Fallout 3, Fallout: New Vegas, Skyrim | zlib | 16 bytes |
| 105 | Skyrim Special and Anniversary Edition | LZ4 frames | 24 bytes |

- **Header**: 36 bytes. A header that says otherwise is `InvalidHeaderSize`.
- **Names**: files are grouped into folders. The archive flags say whether folder names are stored
  (`DIRECTORY_STRINGS`), whether file names are stored (`FILE_STRINGS`), and, from version 104,
  whether each payload starts with the member's full path (`EMBEDDED_FILE_NAMES`). An archive can
  have any combination, including none: a hash-only archive, whose entries have no path at all.
  When the string tables and the embedded name disagree, the string tables win.
- **Hashes**: a 64-bit hash of the folder and one of the file name. See
  [Hashes](@/docs/hashes.md#tes4).
- **Compression**: the `COMPRESSED` flag sets the archive's default, and bit 30 of a file's size
  field inverts it for that file. A compressed payload starts with its decompressed size as a
  32-bit number, then zlib data (103, 104) or an LZ4 frame (105).
- **Content types**: a 16-bit field of `ArchiveTypes` bits (meshes, textures, sounds…) that the
  games use to decide what to load. The reader keeps it and ignores it.

## BA2

Fallout 4 and Starfield. The magic is `BTDX`.

| Version | Games |
|---|---|
| 1 | Fallout 4, Fallout 76 |
| 7, 8 | Fallout 4 after its 2024 update |
| 2 | Starfield |
| 3 | Starfield, with a compression field that can select LZ4 |

Each archive holds one payload family, named in its header:

| Payload | Holds | Read and extract | Write |
|---|---|---|---|
| `GNRL` | Any file, in one or more chunks | yes | yes, one chunk per file |
| `DX10` | Textures: a texture header per file and one chunk per range of mips | yes, as DDS files | yes |
| `GNMF` | Console GNM (`.gnf`) textures | metadata only | no |

- **Names**: an optional string table at the end of the archive. Without one, entries have only
  hashes, and the facade reports no path for them.
- **Hashes**: three 32-bit fields per file: directory, file stem and extension. See
  [Hashes](@/docs/hashes.md#ba2).
- **Compression**: per chunk. A packed size of zero means the chunk is stored as it is; otherwise
  it is zlib, or an LZ4 block when a version 3 archive's compression field is 3.
- **Chunk records** end in the sentinel `0xBAADF00D`; any other value is `InvalidChunkSentinel`.

A DX10 archive is checked as it opens: every texture's format, size and mip ranges must describe
a texture dream_archive can write a DDS header for. An archive holding a texture in a format it
does not know does not open. [BA2 textures](@/docs/textures.md) lists the formats.

## What is refused

| Archive | Refused | Error |
|---|---|---|
| TES4, Xbox layout (flag bit 6) | when it opens | `NotImplemented("TES4 Xbox archive layout")` |
| TES4 104 or 105, XMem-compressed | when it opens | `NotImplemented("TES4 XMem compression")` |
| BA2 GNMF | when a member is read or extracted | `NotImplemented("BA2 GNMF extraction")` |
| BA2 DX10 in an unknown texture format | when it opens | `Dds("unsupported DXGI format")` |

Oblivion's own version 103 archives set the XMem bit on PC, where it means nothing; for 103 it is
ignored, as OpenMW ignores it. `ArchiveInfo::uses_xmem` applies the same rule.

The target is PC archives as the games ship and mods build them. A PC archive that uses something
refused here, or does not open at all, is a bug worth [reporting](https://github.com/DreamWeave-MP/dream_archive/issues),
with the archive: support is added against real files and tests, not guessed from format notes.

GNMF textures are swizzled for console hardware, so a raw copy of their chunks would not be a
usable texture. Their metadata is parsed so a caller can tell a GNMF archive from a broken one:

```rust
use dream_archive::ba2::{Archive, PayloadFormat};

fn main() -> dream_archive::ba2::Result<()> {
    let archive = Archive::open_path("Textures.ba2")?;
    if archive.info().format == PayloadFormat::GNMF {
        eprintln!("GNMF metadata is readable; its textures cannot be extracted");
    }
    Ok(())
}
```

## Malformed archives

Every offset and size is checked against the file before it is used: a member that points outside
the archive, a table longer than the file, or a count that does not fit the platform's integers is
an error when the archive opens, not a panic or a short read later. Allocations for tables and
payloads are fallible; one that cannot be satisfied is `Capacity`. A decompressed member must come
out at exactly the size its record declares, with no compressed bytes left over, or it is
`DecompressionSizeMismatch` or `TrailingCompressedData`.
