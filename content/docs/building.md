+++
title = "Building archives"
description = "The four builders: adding bytes, files, directories and entries of other archives, writing, compression, TES4 profiles and name modes, and rewriting an archive."
weight = 60

[extra]
kind = "guide"
+++

| Builder | Alias at the crate root | Writes |
|---|---|---|
| `bsa::tes3::Builder` | `Tes3BsaBuilder` | a TES3 BSA; no settings, no compression |
| `bsa::tes4::Builder` | `Tes4BsaBuilder` | a TES4 BSA, version 103, 104 or 105 |
| `ba2::Builder` | `Ba2Builder` | a BA2 of general files (GNRL), one chunk per file |
| `ba2::Dx10Builder` | `Ba2Dx10Builder` | a BA2 of textures (DX10); see [BA2 textures](@/docs/textures.md) |

Each starts empty from `new()` (or `Default`), collects members, and writes them when asked. A
builder is not consumed by writing: it can write the same archive again, or to several places.

## Adding members

| Call | The payload |
|---|---|
| `add_bytes(path, bytes)` | copied into the builder now |
| `add_file(archive_path, source)` | read from `source` when the archive is written |
| `add_dir(root)` | every file below `root`, each as `add_file` with its path relative to `root` |
| `add_archive_entry(archive_path, archive, id)` | an entry of an opened archive of the same family, read from it when the archive is written |
| `add_encoded_path(text, encoding, bytes)` | as `add_bytes`, with the path encoded in a legacy code page first; TES3 and TES4 |

Each path is normalized and checked when it is added, so a bad or duplicate path fails at once
with `InvalidArchivePath` or `DuplicatePath`. [What builders store](@/docs/paths.md#what-builders-store)
has the rules.

**Files are deferred.** `add_file` reads only the source's size when it is called, following
symlinks. The bytes are read when the archive is written, so until then a builder holding a
thousand textures holds a thousand paths, not a thousand textures. If a source's size changed in between,
writing fails with an I/O error, "deferred source file size changed before archive write",
rather than produce an archive whose records disagree with its data.

**Directories** are walked recursively, in sorted order. A symlink to a file is followed for its
bytes and stored at the symlink's own path; symlinks to directories are skipped. On Unix a file
name's bytes become the archive path's bytes; elsewhere, a name must be valid UTF-8.

**Entries of other archives** are copied, not re-added from disk. The builder keeps the source
archive in an `Arc` and the entry's id, and reads the entry when it writes. TES3 copies the bytes;
the TES4 and GNRL builders decode the entry and store it under their own compression; the DX10
builder copies texture chunks unchanged. See [Rewriting an archive](#rewriting-an-archive).

## Writing

| Call | Output |
|---|---|
| `write_path(path)` | creates or truncates the file and writes to it |
| `write_seek(out)` | any `Write + Seek` |
| `to_vec()` | a `Vec<u8>` holding the whole archive |

`write_path` writes the file in place. If writing fails part way, for instance because a
deferred source vanished, a partial file is left behind; write to a temporary name and rename it
if nothing may ever see a half-written archive.

The TES3 and GNRL builders stream each deferred file into the output as they reach it. The TES4
and DX10 builders read, and compress, every member before they write the first byte, so while
they write they hold the whole archive's contents in memory.

Output is deterministic. Members are written sorted by hash, then by path (a TES4 archive by
folder first), so the same members produce the same bytes whatever order they were added in.

## Compression

A TES3 archive is never compressed. For the others, a builder has a default for every member,
and the `_with_compression` calls override it per member with a `CompressionOverride`:
`Inherit` (the default), `Store` or `Compress`.

| Builder | Default | Compressed with |
|---|---|---|
| TES4 | `set_compressed(bool)`, off by default | zlib for 103 and 104, an LZ4 frame for 105 |
| GNRL and DX10 | `set_compression(Option<Ba2CompressionFormat>)`, `None` by default | `Zip` (zlib), or `LZ4` (LZ4 blocks), which only version 3 can hold |

A TES4 member whose override differs from the archive default is stored with the per-file toggle
bit set, as the format intends. A BA2 member forced to `Compress` in an archive whose default is
`None` is compressed with zlib. Asking for LZ4 in a BA2 that is not version 3 fails when it is
written, with `NotImplemented("BA2 LZ4 writer requires version 3")`.

`set_zlib_level` takes a `flate2::Compression`; the default is level 6. `flate2` is not
re-exported, so naming a level needs `flate2` 1 in your own `Cargo.toml`.

```rust
use dream_archive::{Ba2Builder, CompressionOverride, ba2::Ba2CompressionFormat};

fn main() -> dream_archive::ba2::Result<()> {
    let mut builder = Ba2Builder::new();
    builder.set_compression(Some(Ba2CompressionFormat::Zip));
    builder.add_dir("MyMod")?;
    // Already compressed: storing it again saves nothing.
    builder.add_file_with_compression("sound/music/theme.xwm", "theme.xwm", CompressionOverride::Store)?;
    builder.write_path("MyMod - Main.ba2")?;
    Ok(())
}
```

BA2 versions are set with `set_version(ArchiveVersion)`: `v1` by default, for Fallout 4. Version
2 and 3 archives are Starfield's.

## TES4 profiles, content types and names

A new TES4 builder writes version 104, uncompressed, with folder and file names, and the `MISC`
content type. The game profiles only choose the version:

| Constructor | `GameProfile` | Version |
|---|---|---|
| `oblivion()` | `Oblivion` | 103 |
| `fallout3()`, `fallout_new_vegas()`, `skyrim_le()` | `Fallout3`, `FalloutNewVegas`, `SkyrimLe` | 104 |
| `skyrim_se()` | `SkyrimSe` | 105 |

Compression, content types and name storage are left as they are, because real archives for the
same game differ in all three. Set them to match the content:

```rust
use dream_archive::Tes4BsaBuilder;
use dream_archive::bsa::tes4::{ArchiveTypes, NameMode};

fn main() -> dream_archive::bsa::Result<()> {
    let mut builder = Tes4BsaBuilder::skyrim_se();
    builder
        .set_compressed(true)
        .set_archive_types(ArchiveTypes::from_bits_retain(
            ArchiveTypes::MESHES.bits() | ArchiveTypes::TEXTURES.bits(),
        ))
        .set_name_mode(NameMode::Strings);
    builder.add_dir("MyMod")?;
    builder.write_path("MyMod.bsa")?;
    Ok(())
}
```

`NameMode` says how paths are stored:

| Mode | Folder and file string tables | Full path before each payload |
|---|---|---|
| `Strings` (default) | yes | no |
| `HashOnly` | no | no |
| `Embedded` | no | yes, version 104 and 105 only |
| `StringsAndEmbedded` | yes | yes, version 104 and 105 only |

An embedded mode on a version 103 builder fails when it is written, with
`NotImplemented("TES4 embedded file names require version 104 or 105")`. A `HashOnly` archive
can only be read back by path through hashes; [Hash-only archives](@/docs/hashes.md#hash-only-archives)
explains what that costs.

## Rewriting an archive

Replacing one member of an archive means writing a new archive. Copy the entries that stay with
`add_archive_entry`, add the new ones, and write:

```rust
use std::sync::Arc;

use dream_archive::bsa::tes3::{Archive, Builder};

fn main() -> dream_archive::bsa::Result<()> {
    let source = Arc::new(Archive::open_path("Old.bsa")?);
    let mut builder = Builder::new();

    for (id, entry) in source.entries_with_ids() {
        if entry.path() != br"textures\replaced.dds".as_slice() {
            builder.add_archive_entry(entry.path(), Arc::clone(&source), id)?;
        }
    }
    builder.add_file("textures/replaced.dds", "loose/replaced.dds")?;
    builder.write_path("New.bsa")?;
    Ok(())
}
```

Nothing is extracted to memory first: each kept entry is read from `Old.bsa` while `New.bsa` is
written. Write to a different file than the one being read; the source is memory-mapped, and
truncating it underneath its own archive is not survivable. The GNRL and TES4 builders also have
`add_archive_entry_with_compression`.

A copied entry is checked as it is written: if it decodes to a different length than it had when
it was added, writing fails.
