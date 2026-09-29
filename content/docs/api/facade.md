+++
title = "Archive facade"
description = "The crate root: Archive, Entry, Entries, FileFormat, BsaFormat, guess_format, detect_path, Copied, CompressionOverride, the builder aliases and the re-exports."
weight = 10

[extra]
kind = "api"
+++

Everything here is at the crate root, `dream_archive::`. [Reading archives](@/docs/reading.md)
shows it in use.

## Archive

{{ api_signature(value="enum Archive { BA2(ba2::Archive), Tes3Bsa(bsa::tes3::Archive), Tes4Bsa(bsa::tes4::Archive) }") }}

An opened archive of any family. Each variant exists with its family's feature. Match on it to
reach the format type's own API. `Clone`, `Debug`; cloning shares the archive's bytes and copies
its index.

### Opening

{{ api_signature(value="fn open_path(path: impl AsRef<Path>) -> Result<Archive>") }}

Detects the family from the file's first four bytes, then opens it with that family's
`open_path`, which memory-maps the file. `Error::Io` if the file cannot be opened or has fewer
than four bytes, `Error::UnknownFormat` if the family is unknown or its feature is off, and the
family's parse error inside `Error::Ba2` or `Error::Bsa` if the archive is malformed.

{{ api_signature(value="fn from_slice(bytes: &[u8]) -> Result<Archive>") }}

{{ api_signature(value="fn from_vec(bytes: Vec<u8>) -> Result<Archive>") }}

The same, from bytes in memory. `from_slice` copies them; `from_vec` takes ownership.

### Reading

{{ api_signature(value="fn format(&self) -> FileFormat") }}

{{ api_signature(value="fn len(&self) -> usize") }}

{{ api_signature(value="fn is_empty(&self) -> bool") }}

The family, the number of entries, and whether there are none.

{{ api_signature(value="fn entries(&self) -> Entries<'_>") }}

Every entry, in archive order, without allocating.

{{ api_signature(value="fn read_file(&self, path: impl AsRef<[u8]>) -> Result<Option<Vec<u8>>>") }}

{{ api_signature(value="fn read_file_required(&self, path: impl AsRef<[u8]>) -> Result<Vec<u8>>") }}

A member, decompressed into a new `Vec`. `read_file` returns `Ok(None)` when no entry matches
`path`; `read_file_required` returns `Error::FileNotFound` with the path as given.
[Archive paths](@/docs/paths.md#lookups) says how `path` is matched.

{{ api_signature(value="fn extract_file(&self, path: impl AsRef<[u8]>, out: impl Write) -> Result<Option<u64>>") }}

{{ api_signature(value="fn extract_file_required(&self, path: impl AsRef<[u8]>, out: impl Write) -> Result<u64>") }}

A member, decompressed into `out`; returns the number of bytes written. Compressed data is
decoded before it is written.

{{ api_signature(value="fn open_file(&self, path: impl AsRef<[u8]>) -> Result<Option<Box<dyn Read + '_>>>") }}

{{ api_signature(value="fn open_file_required(&self, path: impl AsRef<[u8]>) -> Result<Box<dyn Read + '_>>") }}

A member as a reader that borrows the archive. A stored member is read straight from the
archive's bytes; a compressed one is decoded when the reader is made, so decoding errors come
from this call and not from `read`.

{{ api_signature(value="fn extract_to(&self, target_dir: impl AsRef<Path>) -> Result<u64>") }}

Every entry, written below `target_dir` at its archive path, each file atomically; returns the
bytes written. [Extracting](@/docs/extracting.md) has the rules and what is refused.

## Entry

{{ api_signature(value="enum Entry<'a> { BA2(&'a ba2::Entry), Tes3Bsa(&'a bsa::tes3::Entry), Tes4Bsa(&'a bsa::tes4::Entry) }") }}

One entry, borrowed from an `Archive`. `Clone`, `Copy`, `Debug`.

{{ api_signature(value="fn path(self) -> Option<&'a BStr>") }}

The path the archive stores, as bytes. `None` for a TES4 entry without names and for a BA2 entry
without a string table; a BA2 entry's empty name counts as no name.

{{ api_signature(value="fn format(self) -> FileFormat") }}

The family of the archive it came from.

## Entries

{{ api_signature(value="enum Entries<'a> { BA2(slice::Iter<'a, ba2::Entry>), Tes3Bsa(slice::Iter<'a, bsa::tes3::Entry>), Tes4Bsa(slice::Iter<'a, bsa::tes4::Entry>) }") }}

The iterator `Archive::entries` returns. `Iterator<Item = Entry<'a>>`, `ExactSizeIterator`,
`FusedIterator`, `Clone`, `Debug`.

## Detecting a family

{{ api_signature(value="enum FileFormat { BA2, BSA(BsaFormat) }") }}

{{ api_signature(value="enum BsaFormat { TES3, TES4 }") }}

An archive family. Both are `Clone`, `Copy`, `Debug`, `Eq`, `PartialEq`.

{{ api_signature(value="fn guess_format(input: &mut impl Read) -> io::Result<Option<FileFormat>>") }}

Reads exactly four bytes from `input` and names the family they begin: `BTDX` is BA2, `BSA\0` is
TES4, and `00 01 00 00` is TES3. `Ok(None)` for anything else, or for a family whose feature is
off. An error if four bytes cannot be read. It checks nothing past the magic, and the four bytes
are consumed.

{{ api_signature(value="fn detect_path(path: impl AsRef<Path>) -> io::Result<Option<FileFormat>>") }}

`guess_format` on the start of a file.

## Copied

{{ api_signature(value="struct Copied<'copy>(pub &'copy [u8])") }}

Marks bytes to be copied. `ba2::Archive`, `bsa::tes3::Archive` and `bsa::tes4::Archive` each
implement `TryFrom<Copied<'_>>` as their `from_slice`.

## CompressionOverride

{{ api_signature(value="enum CompressionOverride { Inherit, Store, Compress }") }}

A member's compression in a builder's `_with_compression` calls: `Inherit`, the default, follows
the builder; `Store` never compresses; `Compress` always does. `Clone`, `Copy`, `Debug`,
`Default`, `Eq`, `PartialEq`. [Compression](@/docs/building.md#compression) says what each builder
does with it.

## Builder aliases

{{ api_signature(value="type Ba2Builder = ba2::Builder") }}

{{ api_signature(value="type Ba2Dx10Builder = ba2::Dx10Builder") }}

{{ api_signature(value="type Tes3BsaBuilder = bsa::tes3::Builder") }}

{{ api_signature(value="type Tes4BsaBuilder = bsa::tes4::Builder") }}

The four builders, named at the root. [BA2 builders](@/docs/api/ba2-builders.md),
[bsa::tes3](@/docs/api/tes3.md#builder) and [TES4 builder](@/docs/api/tes4-builder.md).

## Re-exports

{{ api_signature(value="pub use dream_path") }}

[dream-path](https://DreamWeave-MP.github.io/dream_path/), whose normalization every lookup
uses. Depend on it through this re-export and the versions cannot drift apart.

{{ api_signature(value="pub use dream_path::bstr") }}

{{ api_signature(value="pub use dream_path::bstr::{BStr, BString, ByteSlice, ByteVec}") }}

The [`bstr`](https://crates.io/crates/bstr) crate and its byte-string types, which entry paths
and several errors use.

`Result` and `Error` are on [Errors](@/docs/api/errors.md).
