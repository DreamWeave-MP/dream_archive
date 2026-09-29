+++
title = "Hashes and hash-only archives"
description = "Each family's path hash, how to compute it, lookup by hash, and extracting archives that store no names."
weight = 40

[extra]
kind = "guide"
+++

Every family stores a hash of each member's path next to its record, and the games find members
by hash. dream_archive computes the same hashes, reports the stored ones on each entry, and looks
entries up by them. Where two entries share a hash, a hash lookup returns the first in archive
order.

Each hash function normalizes its input first, and returns the bytes it hashed. That normalization
is the game's, not the lookup rules in [Archive paths](@/docs/paths.md): `/` becomes `\`, ASCII
letters are lowercased, leading and trailing separators are removed, and an empty path, or one of
260 bytes or more, hashes as `.`. Repeated separators are kept.

## TES3

{{ api_signature(value="fn bsa::tes3::hash_file(path: &[u8]) -> (FileHash, Vec<u8>)") }}

A 64-bit hash, in two 32-bit halves: `lo` from the first half of the normalized path's bytes,
`hi` from the second. The archive stores `lo` then `hi`. `FileHash::numeric()` is
`hi | lo << 32`, and it is the value `Entry::hash()` returns and `get_by_hash` takes.

```rust
use dream_archive::bsa::tes3::{Archive, hash_file};

fn main() -> dream_archive::bsa::Result<()> {
    let archive = Archive::open_path("Morrowind.bsa")?;
    let (hash, normalized) = hash_file(b"Textures/Tx_Wood.DDS");
    assert_eq!(normalized, br"textures\tx_wood.dds");

    let entry = archive.get_by_hash(hash.numeric()).expect("tx_wood.dds is in Morrowind.bsa");
    assert_eq!(entry.hash(), hash.numeric());
    Ok(())
}
```

## TES4

{{ api_signature(value="fn bsa::tes4::hash_directory(path: &[u8]) -> (HashFields, Vec<u8>)") }}

{{ api_signature(value="fn bsa::tes4::hash_file(path: &[u8]) -> (HashFields, Vec<u8>)") }}

Two hashes per member: one of its folder and one of its file name. `hash_file` discards
everything up to the last separator, so `hash_file(b"meshes/door.nif")` hashes `door.nif`.

A `HashFields` is the hash as the archive lays it out: the last byte, the second-to-last byte,
the length and the first byte of the name, and a 32-bit checksum of the bytes in between. For a
file those describe the name without its extension; the extension's own checksum is added to
the checksum, and `.nif`, `.kf`, `.dds`, `.wav` and `.adp` each adjust the byte fields. `numeric()` packs the fields into the 64-bit number the archive stores, and
`from_numeric` unpacks one.

```rust
use dream_archive::bsa::tes4::{Archive, hash_directory, hash_file};

fn main() -> dream_archive::bsa::Result<()> {
    let archive = Archive::open_path("Meshes.bsa")?;
    let (folder, _) = hash_directory(br"Meshes\Armor\Iron");
    let (file, name) = hash_file(b"Meshes/Armor/Iron/Cuirass.NIF");
    assert_eq!(name, b"cuirass.nif");

    let entry = archive.get_by_hash(folder, file).expect("the cuirass is in Meshes.bsa");
    assert_eq!(entry.file_hash(), file);
    Ok(())
}
```

## BA2

{{ api_signature(value="fn ba2::hash_file(path: &BStr) -> (FileHash, BString)") }}

{{ api_signature(value="fn ba2::hash_file_in_place(path: &mut BString) -> FileHash") }}

Three 32-bit fields: `directory`, a CRC-32 of everything before the last separator; `file`, a
CRC-32 of the name without its extension; and `extension`, the extension's first four bytes, read
as a little-endian number. Bytes above 127 do not count towards either CRC. `hash_file_in_place`
normalizes the `BString` it is given and returns its hash.

`FileHash` wraps the `Hash` of three fields and dereferences to it. It also converts from a
`&str` or `&[u8]`, which hashes it:

```rust
use dream_archive::ba2::{Archive, FileHash};

fn main() -> dream_archive::ba2::Result<()> {
    let archive = Archive::open_path("Misc.ba2")?;
    let hash = FileHash::from("Interface/Credits.txt");
    assert!(archive.contains_hash(hash));
    println!("{:08x} {:08x} {:08x}", hash.directory, hash.file, hash.extension);
    Ok(())
}
```

## Lookup by hash

| | TES3 | TES4 | BA2 |
|---|---|---|---|
| `get_by_hash`, `get_id_by_hash`, `contains_hash` take | `u64` | `HashFields, HashFields` (folder, file) | `FileHash` |
| The entry reports | `hash()` | `folder_hash()`, `file_hash()` | `hash()` |

The facade has no hash lookup, because the three shapes differ; match on the archive to reach
them. [Luau](@/docs/luau/archive.md#hashes) takes the same three shapes as integers.

## Hash-only archives

A TES4 archive can store neither folder nor file names: written that way on purpose, with
`NameMode::HashOnly`, or by tools that saw no reason to keep them. Its entries have no `path`,
`folder` or `name`. A BA2 without a string table is the same: its entries' `name()` is empty and
the facade reports no path.

Lookups still work. When an archive has no names at all, `get`, `contains` and the `read_file`
family hash the path they are given and look the hash up, so code that knows what it wants never
notices. What cannot work is anything that needs the archive to say what is in it: `extract_to`
on a hash-only TES4 archive fails with `ArchivePathsUnavailable` before it writes anything, and on
a BA2 without names with an I/O error saying the entry has no file name.

For TES4, `extract_to_with_paths` takes the names from you instead: a list of candidate paths,
from plugin records, a loose-file tree or a manifest. Each candidate that is in the archive is
extracted under the spelling you gave it; candidates that are not are skipped, and a member that
two candidates match is written once. It returns the bytes written.

```rust
use dream_archive::bsa::tes4::Archive;

fn main() -> dream_archive::bsa::Result<()> {
    let archive = Archive::open_path("Voices.bsa")?;
    assert!(archive.entries().iter().all(|entry| entry.path().is_none()));

    let candidates = [
        r"sound\voice\mymod.esp\maleeventoned\greeting_01.fuz",
        "sound/voice/mymod.esp/femaleeventoned/greeting_01.fuz",
        "sound/voice/mymod.esp/maleeventoned/not_in_the_archive.fuz",
    ];
    let written = archive.extract_to_with_paths("Voices", candidates)?;
    println!("{written} bytes");
    Ok(())
}
```

A BA2 has no such call. Look each candidate up with `get` and write it with
`extract_entry_to_path`.
