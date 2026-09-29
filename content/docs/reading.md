+++
title = "Reading archives"
description = "Opening an archive, the facade and the format types, entries and stable ids, lookups, and the three ways to read a member."
weight = 20

[extra]
kind = "guide"
+++

## Opening

There are two ways in: the `Archive` facade, which detects the family, and the format types,
which do not.

| Call | Detects the family | Storage |
|---|---|---|
| `Archive::open_path(path)` | yes | the file, memory-mapped |
| `Archive::from_vec(bytes)` | yes | takes the `Vec` |
| `Archive::from_slice(bytes)` | yes | copies the slice |
| `ba2::Archive::open_path`, `bsa::tes3::Archive::open_path`, `bsa::tes4::Archive::open_path` | no | the file, memory-mapped |
| their `from_vec`, `from_slice`, and `try_from(Copied(bytes))` | no | as above; `Copied` copies |

Opening parses the whole index: every entry's record and name, checked against the size of the
file. Payloads are not touched until a member is read. A format type given another family's
archive fails with that family's header error, such as `InvalidMagic` or `InvalidVersion`.

A memory-mapped archive must not change while it is open. Another program truncating or rewriting
the file underneath it is outside the contract, and on most systems ends in a crash rather than an
error. Load the bytes with `std::fs::read` and `from_vec` when that cannot be ruled out.

The facade is an enum of the three format types, so matching on it reaches everything the facade
does not expose:

```rust
use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
    match Archive::open_path("Meshes.bsa")? {
        Archive::Tes4Bsa(archive) => {
            let info = archive.info();
            println!("TES4 {:?}, {} folders, {} files", info.version, info.folder_count, info.file_count);
        }
        Archive::Tes3Bsa(archive) => println!("TES3, {} files", archive.len()),
        Archive::BA2(archive) => println!("BA2 {:?}", archive.info().format),
    }
    Ok(())
}
```

## Entries

`Archive::entries()` yields a borrowed `Entry` per member, in the archive's own order, with its
path and family. The format types return their entries as a slice, and their entries carry more:
TES3 records and hashes, TES4 folder and file names and both hashes, BA2 chunks and texture
headers.

| | TES3 | TES4 | BA2 |
|---|---|---|---|
| Path | always | when the archive stores names | when the archive has a string table |
| Hash | `hash()`: `u64` | `folder_hash()`, `file_hash()`: `HashFields` | `hash()`: `FileHash` |
| Location | `file()`: size, offset | `file()`: stored size, offset, compression toggle | `file()`: header and chunks |

An `EntryId` names one entry of one parsed archive: its index in the table. `entries_with_ids()`
pairs each entry with its id, and `entry_by_id` and `entry_by_id_required` turn an id back into
an entry. Ids are what builders take to copy an entry from an opened archive; an id from one
archive means nothing to another.

## Lookups

`get(path)` finds an entry by path, `get_id(path)` its id, and `contains(path)` whether it is
there. `get_required` turns a miss into `FileNotFound`. Paths are bytes, and each family matches
them its own way: [Archive paths](@/docs/paths.md#lookups) has the rules. When two entries match
the same path, the first in archive order wins.

The BSA archives also take a `NormalizedPath`, normalized once, for lookups repeated in a loop:
`get_normalized` and `contains_normalized`. Lookup by hash is in
[Hashes](@/docs/hashes.md#lookup-by-hash).

## Reading a member

A member can come out three ways. All of them decompress.

| Calls | Gives | Notes |
|---|---|---|
| `read_file`, `read_file_required`, `read_entry` | a new `Vec<u8>` | |
| `read_entry_into(entry, &mut vec)` | appends to your `Vec` | on error the `Vec` is truncated back to its length before the call |
| `extract_file`, `extract_file_required`, `extract_entry`, `extract_entry_by_id` | writes into any `std::io::Write`, returns the byte count | compressed data is decoded into a buffer before it is written |
| `open_file`, `open_file_required`, `open_entry` | a `Box<dyn Read + '_>` | stored data is read straight from the archive; compressed data is decoded when the reader is made |

The optional calls return `Ok(None)` for a missing path; the `_required` ones return
`FileNotFound`. Decoding before writing means a corrupt payload is reported as the crate's own
error, before any of it reaches your writer or reader. A BA2 member of several chunks is decoded
and written one chunk at a time, after its DDS header if it is a texture.

`read_entry_into` is the one to use in a loop: the buffer's allocation is kept from member to
member.

```rust
use dream_archive::bsa::tes3::Archive;

fn main() -> dream_archive::bsa::Result<()> {
    let archive = Archive::open_path("Morrowind.bsa")?;
    let mut buffer = Vec::new();
    let mut meshes = 0;
    for entry in archive.entries() {
        if entry.path().ends_with(b".nif") {
            buffer.clear();
            archive.read_entry_into(entry, &mut buffer)?;
            meshes += 1;
        }
    }
    println!("{meshes} meshes read");
    Ok(())
}
```

For BA2 and TES4, `extracted_len(entry)` says how many bytes a member decodes to without decoding
it. For a TES3 entry, the size in its record is the size.

## Sharing

Reading takes `&self`, and an archive is `Send` and `Sync`: one opened archive can serve every
thread.

```rust
use std::sync::Arc;
use std::thread;

use dream_archive::Archive;

fn main() -> dream_archive::Result<()> {
    let archive = Arc::new(Archive::open_path("Morrowind.bsa")?);
    let workers: Vec<_> = ["textures/tx_wood.dds", "meshes/m/probe_journeyman_01.nif"]
        .into_iter()
        .map(|path| {
            let archive = Arc::clone(&archive);
            thread::spawn(move || archive.read_file_required(path).map(|bytes| bytes.len()))
        })
        .collect();
    for worker in workers {
        println!("{} bytes", worker.join().expect("the worker panicked")?);
    }
    Ok(())
}
```

Cloning an archive shares its bytes and copies its index. Builders that copy entries from an
archive take it as an `Arc` of the format type.

## Errors

The facade returns `dream_archive::Error`: `UnknownFormat`, `FileNotFound`, `Io`, or the
family's own error inside `Ba2` or `Bsa`. The format types return `ba2::Error` or `bsa::Error`.
[Errors](@/docs/api/errors.md) lists every variant and its message.
