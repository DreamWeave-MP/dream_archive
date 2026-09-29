+++
title = "Start here"
description = "Add the crate, open an archive, read a member, extract everything, and build an archive of your own."
weight = 10

[extra]
kind = "tutorial"
+++

## Add the crate

```sh
cargo add dream_archive
```

It needs Rust 1.88 or newer. The default features read and write every family: `ba2` for BA2, and
`bsa` for both BSA generations. [Features](@/docs/compatibility.md#features) lists the rest.

## Open an archive

`Archive::open_path` reads the first four bytes to learn the family, then parses the archive's
index from a memory-mapped file. Nothing else is read until you ask for a member.

```rust
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
    let archive = Archive::open_path("Morrowind.bsa")?;
    println!("{:?}: {} entries", archive.format(), archive.len());

    for entry in archive.entries().take(3) {
        if let Some(path) = entry.path() {
            println!("{path}");
        }
    }
    Ok(())
}
```

On Morrowind's own `Morrowind.bsa`, that prints:

```text
BSA(TES3): 11090 entries
meshes\m\probe_journeyman_01.nif
textures\menu_rightbuttonup_top.dds
textures\menu_rightbuttonup_right.dds
```

Entries come in the archive's own order. Paths are the bytes the archive stores, printed here as
text; [Archive paths](@/docs/paths.md) explains why they are not `String`s.

## Read a member

Look a member up by any spelling of its path: case and slash direction do not matter.

```rust
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
    let archive = Archive::open_path("Morrowind.bsa")?;

    let texture = archive.read_file_required("textures/TX_WOOD.dds")?;
    assert_eq!(&texture[..4], b"DDS ");

    assert!(archive.read_file("textures/no_such_texture.dds")?.is_none());
    Ok(())
}
```

`read_file` returns `None` for a member that is not there; `read_file_required` makes that an
error, `Error::FileNotFound`. Both decompress into a new `Vec<u8>`. To stream a member instead,
`open_file_required` returns a `std::io::Read`:

```rust
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
    let archive = Archive::open_path("Morrowind.bsa")?;
    let mut reader = archive.open_file_required(r"meshes\m\probe_journeyman_01.nif")?;

    let mut out = std::fs::File::create("probe_journeyman_01.nif")?;
    let copied = std::io::copy(&mut reader, &mut out)?;
    println!("{copied} bytes");
    Ok(())
}
```

## Extract everything

```rust
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
    let archive = Archive::open_path("SomeMod.bsa")?;
    let written = archive.extract_to("SomeMod")?;
    println!("{written} bytes extracted");
    Ok(())
}
```

Each member lands at its archive path below `SomeMod`, with directories created as needed. Every
file is written to a temporary name first and renamed into place, so a failure never leaves half a
file; files written before the failure stay. A path that would escape the directory, such as one
with `..`, is an error. [Extracting](@/docs/extracting.md) covers the rest.

## Build an archive

Every family has a builder. This one writes a Morrowind BSA from a directory of loose files:

```rust
use dream_archive::Tes3BsaBuilder;

fn main() -> dream_archive::bsa::Result<()> {
    let mut builder = Tes3BsaBuilder::new();
    builder.add_dir("MyMod")?;
    builder.write_path("MyMod.bsa")?;
    Ok(())
}
```

`add_dir` walks the directory and records each file's path and size; the bytes are read when the
archive is written. Paths are stored in lowercase with backslashes, as Morrowind's own archives
store them. [Building archives](@/docs/building.md) covers the TES4 and BA2 builders.

## Next

- [Formats](@/docs/formats.md): which games use which family, and what is not supported.
- [Reading archives](@/docs/reading.md): the format-specific archive types, entry ids and lookups.
- [Embedding Luau](@/docs/luau-hosts.md), if your program runs scripts.
- [dream_archivetool](https://DreamWeave-MP.github.io/dream_archivetool/), if you want all of this
  from a shell.
