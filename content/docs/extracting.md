+++
title = "Extracting"
description = "Whole archives and single members to disk: where files go, what is refused, atomic writes, parallel extraction and file names in legacy code pages."
weight = 50

[extra]
kind = "guide"
+++

## A whole archive

{{ api_signature(value="fn extract_to(&self, target_dir: impl AsRef<Path>) -> Result<u64>") }}

On the facade and on every format type. Each member is written at its archive path below
`target_dir`, directories are created as needed, and the return value is the number of bytes of
file contents written.

```rust
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
    let archive = Archive::open_path("SomeMod.bsa")?;
    let written = archive.extract_to("SomeMod")?;
    println!("{written} bytes");
    Ok(())
}
```

**Each file is atomic.** A member is written to a temporary file in its destination directory,
named `.<name>.dream-archive-tmp-<process>-<counter>`, and renamed over the destination only when
it is complete. A failure removes the temporary file and leaves whatever was at the destination
before. An existing file at the destination is replaced.

**The archive is not.** Extraction stops at the first error, and the files written before it
stay.

## What is refused

An archive path is split on both `/` and `\`, and empty and `.` components are skipped. A
component that is `..`, contains a NUL byte, or contains `:` (a drive letter or an NTFS stream)
fails the extraction with an I/O error, `InvalidData`, "archive path can not be extracted
safely", before that member is written. A path with no components left is "archive entry has no
file name". Nothing is ever written outside `target_dir`.

Some archives cannot be extracted by path at all:

| Archive | `extract_to` fails with |
|---|---|
| TES4 with no names | `ArchivePathsUnavailable`, before anything is written; see [hash-only archives](@/docs/hashes.md#hash-only-archives) |
| BA2 with no string table | an I/O error, "archive entry has no file name" |
| BA2 GNMF | `NotImplemented("BA2 GNMF extraction")` |

## Parallel extraction

With the `parallel` feature, `extract_to` computes every output path first, creates the
directories, and extracts members on Rayon's thread pool. If two members would land on the same
file, or on the same file of a case-insensitive file system, it extracts one member at a time
instead, in archive order, so the result is the same as without the feature.
`extract_to_with_encoding` and `extract_to_with_paths` are always sequential.

## One member

The format types write one entry to a file the same atomic way, creating its parent directories:

{{ api_signature(value="fn extract_entry_to_path(&self, entry: &Entry, path: impl AsRef<Path>) -> Result<u64>") }}

```rust
use dream_archive::ba2::Archive;

fn main() -> dream_archive::ba2::Result<()> {
    let archive = Archive::open_path("Textures.ba2")?;
    let entry = archive.get_required("textures/example.dds")?;
    let written = archive.extract_entry_to_path(entry, "out/example.dds")?;
    println!("{written} bytes");
    Ok(())
}
```

Here, zlib and LZ4-frame payloads stream through the decoder into the temporary file rather than
being decoded into memory first; BA2 LZ4 chunks are decoded whole. The facade's `extract_file`
writes to any `std::io::Write` instead; opening a `File` for it yourself is not atomic.

## File names

On Unix, a member's path bytes become the file name's bytes, UTF-8 or not. Elsewhere, file names
are Unicode, and a path component that is not UTF-8 fails with `InvalidData`.

Archives from localized games and mods often store their names in a legacy code page, and those
names are not UTF-8. The BSA archive types decode them for you:

{{ api_signature(value="fn extract_to_with_encoding(&self, target_dir: impl AsRef<Path>, encoding: FilenameEncoding) -> Result<u64>") }}

Each path component is decoded through `encoding` before it becomes a file name, replacing
anything that cannot be decoded, and then checked as above. Lookups inside the archive are
unaffected: they still compare bytes.

```rust
use dream_archive::bsa::{FilenameEncoding, tes3::Archive};

fn main() -> dream_archive::bsa::Result<()> {
    let archive = Archive::open_path("Russian.bsa")?;
    let written = archive.extract_to_with_encoding("Russian", FilenameEncoding::Windows1251)?;
    println!("{written} bytes");
    Ok(())
}
```

There is no way to detect a code page reliably from a handful of file names, so there is no
automatic choice. [Archive paths](@/docs/paths.md#legacy-code-pages) lists the code pages.
