+++
title = "Archive paths"
description = "Why archive paths are bytes, how every family matches a lookup, what builders store, and legacy code pages."
weight = 30

[extra]
kind = "guide"
+++

An archive path is the name a game asks for, like `meshes\x\ex_door.nif`. It is not a file on
your disk, and it is not necessarily text: the formats never say which encoding their names use,
and the tools that wrote them used whatever code page the author's Windows had. A Russian
Morrowind mod's BSA holds Windows-1251 bytes; a Polish one, Windows-1250.

So paths are bytes throughout. Every lookup takes `impl AsRef<[u8]>`, so a `&str`, a `String`, a
byte string or a `Vec<u8>` all work, and every entry path is a `&BStr` from the
[`bstr`](https://crates.io/crates/bstr) crate, re-exported with `BString`, `ByteSlice` and
`ByteVec`. A `BStr` prints as text, replacing bytes that are not UTF-8, and dereferences to
`[u8]`. Nothing is decoded unless you decode it.

## Lookups

A lookup normalizes the path it is given, and the stored paths, and compares bytes. Every family
normalizes the same way, with [dream-path](https://DreamWeave-MP.github.io/dream_path/)'s rules,
as OpenMW's VFS does, so one string finds the same member in a BSA and a BA2:

| The query's | Becomes |
|---|---|
| `\` and `/` | the same separator |
| ASCII `A` to `Z` | lowercase |
| Leading separators | dropped |
| Repeated separators | collapsed to one |
| A trailing separator | kept, so the path names a folder and matches no file |
| Anything else, including non-ASCII | kept exactly |

Non-ASCII letters are never case-folded: `É` and `é` are different bytes and different paths.

```rust
use dream_archive::{Archive, Ba2Builder, Tes3BsaBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bsa = Tes3BsaBuilder::new();
    bsa.add_bytes("meshes/door.nif", b"bsa")?;
    let bsa = Archive::from_vec(bsa.to_vec()?)?;

    let mut ba2 = Ba2Builder::new();
    ba2.add_bytes("meshes/door.nif", b"ba2")?;
    let ba2 = Archive::from_vec(ba2.to_vec()?)?;

    for archive in [&bsa, &ba2] {
        assert!(archive.read_file(r"\Meshes\DOOR.NIF")?.is_some());
        assert!(archive.read_file("meshes//door.nif")?.is_some());
        assert!(archive.read_file("meshes/door.nif/")?.is_none());
    }
    Ok(())
}
```

A TES4 archive without names, and a BA2 without a string table, are looked up by hashing the
normalized query instead, which finds what a name lookup would; a query with a trailing
separator still matches nothing. The hash normalizations are in [Hashes](@/docs/hashes.md).

### Normalizing once

A program that looks up the same key in several BSA archives can normalize it once, as a
`NormalizedPath`, and pass it to `get_normalized` or `contains_normalized`.
`dream_archive::bsa::NormalizedPath` is dream-path's type, re-exported, and the whole crate is
re-exported as `dream_archive::dream_path`.

```rust
use dream_archive::bsa::{NormalizedPath, tes3::Archive};

fn main() -> dream_archive::bsa::Result<()> {
    let key = NormalizedPath::new(r"Textures\Tx_Wood.DDS");
    let archive = Archive::open_path("Morrowind.bsa")?;
    assert!(archive.contains_normalized(&key));
    Ok(())
}
```

On a TES4 archive, `get_normalized` compares names only; it does not fall back to hashes for an
archive without them.

## What builders store

Builders normalize the path they are given before storing it, and refuse paths that cannot be
stored safely:

- `/` and `\` both separate components, and the stored path uses `\`.
- Empty components and `.` are dropped: `meshes//./door.nif` is `meshes\door.nif`.
- ASCII letters are lowercased. Other bytes are stored as they are.
- A component that is `..`, contains a NUL byte or contains `:` is refused, and so is a path with
  no components left. The error is `InvalidArchivePath`.
- BA2 also refuses a stored path of 260 bytes or more.
- Adding a path that normalizes to one already added is `DuplicatePath`.

A TES4 builder stores the last component as the file name and the rest as its folder.

## Legacy code pages

`FilenameEncoding` names the code pages Bethesda archives turn up in:

| Variant | Code page | Scripts |
|---|---|---|
| `Utf8` | UTF-8 | any |
| `Windows1250` | Windows-1250 | Central European: Polish, Czech, Hungarian… |
| `Windows1251` | Windows-1251 | Cyrillic |
| `Windows1252` | Windows-1252 | Western European |
| `Cp437` | IBM PC code page 437 | the DOS character set |

`encode_filename` turns text into an archive path in one of them, and fails with
`FilenameEncodeError` rather than substitute a character the code page lacks.
`decode_filename_lossy` turns archive bytes into text for display, replacing what cannot be
decoded. Neither guesses: the caller chooses the code page.

```rust
use dream_archive::bsa::{FilenameEncoding, decode_filename_lossy, encode_filename};

fn main() -> Result<(), dream_archive::bsa::FilenameEncodeError> {
    let stored = encode_filename("textures/zażółć.dds", FilenameEncoding::Windows1250)?;
    assert_eq!(stored.as_ref(), b"textures/za\xbf\xf3\xb3\xe6.dds");

    let shown = decode_filename_lossy(&stored, FilenameEncoding::Windows1250);
    assert_eq!(shown, "textures/zażółć.dds");

    assert!(encode_filename("textures/zażółć.dds", FilenameEncoding::Windows1252).is_err());
    Ok(())
}
```

To look a member up by its text name, encode the text and look up the bytes:

```rust
use dream_archive::Archive;
use dream_archive::bsa::{FilenameEncoding, encode_filename};

fn main() -> dream_archive::Result<()> {
    let archive = Archive::open_path("Polish.bsa")?;
    let path = encode_filename("textures/zażółć.dds", FilenameEncoding::Windows1250)
        .map_err(dream_archive::bsa::Error::from)?;
    let bytes = archive.read_file_required(path)?;
    println!("{} bytes", bytes.len());
    Ok(())
}
```

The TES3 and TES4 builders take text the same way, with `add_encoded_path(path, encoding,
bytes)`, and their archives extract to decoded file names with `extract_to_with_encoding`.
[Extracting](@/docs/extracting.md#file-names) describes what happens to names that are not UTF-8.
