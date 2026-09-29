+++
title = "Errors"
description = "Every error: the facade's Error, ba2::Error and bsa::Error, each variant's message and when it happens, and the Result aliases."
weight = 80

[extra]
kind = "api"
+++

Three error types, one per level, each with a `Result` alias. All three are `Debug`, `Display`
and `std::error::Error`, `Send` and `Sync`, and `#[non_exhaustive]`: match them with a wildcard
arm.

## Error

{{ api_signature(value="type Result<T> = std::result::Result<T, Error>") }}

{{ api_signature(value="enum Error { UnknownFormat, FileNotFound(BString), Io(io::Error), Ba2(ba2::Error), Bsa(bsa::Error) }") }}

What the facade returns. `Ba2` exists with the `ba2` feature, `Bsa` with either BSA feature.

| Variant | Message | When |
|---|---|---|
| `UnknownFormat` | unknown or disabled archive format | the first four bytes name no family, or one whose feature is off |
| `FileNotFound(path)` | archive member not found: *path* | a `_required` call found nothing |
| `Io(error)` | *error*'s | opening the file, fewer than four bytes, or writing |
| `Ba2(error)` | *error*'s | anything the BA2 archive reports |
| `Bsa(error)` | *error*'s | anything a BSA archive reports |

`From<io::Error>`, `From<ba2::Error>` and `From<bsa::Error>` make `?` work across the levels.
`source()` is the inner error of `Io`, `Ba2` and `Bsa`.

## ba2::Error

{{ api_signature(value="type ba2::Result<T> = std::result::Result<T, ba2::Error>") }}

{{ api_signature(value="enum ba2::Error") }}

| Variant | Message | When |
|---|---|---|
| `InvalidMagic(u32)` | invalid magic read from archive header: *0x…* | the file does not start with `BTDX` |
| `InvalidVersion(u32)` | invalid version read from archive header: *n* | a version other than 1, 2, 3, 7 or 8 |
| `InvalidFormat(u32)` | invalid format read from archive header: *0x…* | a payload family other than `GNRL`, `DX10` or `GNMF` |
| `InvalidChunkSize(u16)` | invalid BA2 file header size: *n* | a file record's header size does not match its payload family |
| `InvalidChunkSentinel(u32)` | invalid chunk sentinel: *0x…* | a chunk record does not end in `0xBAADF00D` |
| `OutOfBounds` | archive offset or size is out of bounds | a record points outside the file, an entry id is past the end, or a size overflows |
| `IntegralTruncation` | archive integer field can not fit on this platform | a size or offset does not fit `usize`, or an output field does not fit the format |
| `Capacity` | archive table or payload requests more memory than can be allocated | an allocation failed |
| `Dds(&'static str)` | invalid DDS/texture metadata: *detail* | a texture's metadata or a DDS file is invalid or unsupported; see below |
| `DecompressionSizeMismatch { expected, actual }` | buffer failed to decompress to the expected size: expected *n* bytes, got *n* bytes | a chunk decoded to another size than its record says |
| `TrailingCompressedData` | compressed payload has trailing data | a zlib stream ended before its chunk did |
| `Zlib(String)` | zlib decompression failed: *detail* | corrupt zlib data |
| `Lz4(String)` | lz4 decompression failed: *detail* | corrupt LZ4 data |
| `InvalidArchivePath` | invalid archive path | a builder was given a path it cannot store |
| `DuplicatePath` | duplicate archive path | a builder was given a path it already has |
| `FileNotFound(BString)` | archive member not found: *path* | a `_required` call found nothing |
| `NotImplemented(&'static str)` | support for this feature is not implemented: *feature* | see below |
| `Io(io::Error)` | *error*'s | reading, writing, or an extraction path that is unsafe |

The `Dds` details: `zero-sized texture`, `zero mip count`, `unsupported DXGI format`,
`DDS payload size does not match metadata`, `missing BA2 texture mip range`,
`invalid BA2 texture mip range` and `duplicate BA2 texture mip range` for texture metadata, in
an archive or given to a builder; and `truncated DDS header`, `invalid DDS magic`,
`invalid DDS header size`, `invalid DDS pixel format size`, `truncated DDS DX10 header`,
`unsupported DDS volume texture`, `unsupported DDS resource dimension`,
`unsupported DDS array size`, `unsupported DDS FourCC`, `unsupported DDS pixel format`,
`texture height exceeds BA2 field`, `texture width exceeds BA2 field`,
`mip count exceeds BA2 field` and `truncated DDS payload` for a DDS file given to a builder.

The `NotImplemented` features: `BA2 GNMF extraction`, reading a GNMF entry;
`BA2 LZ4 writer requires version 3`; `BA2 DX10 builder can only preserve DX10 archives` and
`BA2 DX10 builder can only preserve DX10 entries`, from `Dx10Builder::add_archive_entry`; and
`BA2 DX10 archive entry preservation requires matching archive versions` and
`… requires matching compression formats`, when such an entry is written.

`From<io::Error>` makes `Io`; `From<TryFromIntError>` makes `IntegralTruncation`, and
`From<TryReserveError>` makes `Capacity`. `source()` is the inner error of `Io`.

## bsa::Error

{{ api_signature(value="type bsa::Result<T> = std::result::Result<T, bsa::Error>") }}

{{ api_signature(value="enum bsa::Error") }}

Shared by TES3 and TES4, and re-exported from `bsa::tes3` and `bsa::tes4`.

| Variant | Message | When |
|---|---|---|
| `InvalidMagic(u32)` | invalid magic read from archive header: *0x…* | a TES4 file that does not start with `BSA\0` |
| `InvalidVersion(u32)` | invalid version read from archive header: *n* | TES3: a first word other than `0x100`; TES4: a version other than 103, 104 or 105 |
| `InvalidHeaderSize(u32)` | invalid size read from archive header: *n* | a TES4 header that does not say 36 bytes |
| `OutOfBounds` | archive offset or size is out of bounds | a record or table points outside the file, the counts do not add up, an entry id is past the end, or a size overflows |
| `IntegralTruncation` | archive integer field can not fit on this platform | a size or offset does not fit `usize`, or an output field does not fit the format |
| `Capacity` | archive table or payload requests more memory than can be allocated | an allocation failed |
| `DecompressionSizeMismatch { expected, actual }` | buffer failed to decompress to the expected size: expected *n* bytes, got *n* bytes | a member decoded to another size than its prefix says |
| `TrailingCompressedData` | compressed payload has trailing data | compressed data continued past the end of its stream |
| `Zlib(String)` | zlib decompression failed: *detail* | corrupt zlib data |
| `InvalidLz4Frame` | expected TES4 v105 LZ4 frame data | a version 105 member does not start with an LZ4 frame |
| `Lz4Frame(String)` | LZ4 frame operation failed: *detail* | corrupt LZ4 data, or compressing failed |
| `ArchivePathsUnavailable` | archive does not contain filenames required for path-based extraction; use TES4 extract_to_with_paths with a path dictionary when applicable | extracting a TES4 archive by path when an entry has none |
| `InvalidArchivePath` | archive path can not be stored safely | a builder was given a path it cannot store |
| `DuplicatePath` | duplicate normalized archive path | a builder was given a path it already has |
| `FileNotFound(BString)` | archive member not found: *path* | a `_required` call found nothing |
| `FilenameEncoding(FilenameEncodeError)` | filename can not be represented losslessly as *encoding* | `add_encoded_path` got text the code page cannot hold |
| `NotImplemented(&'static str)` | support for this feature is not implemented: *feature* | `TES4 Xbox archive layout` and `TES4 XMem compression` when an archive opens or a member is read; `TES4 embedded file names require version 104 or 105` when a builder writes |
| `Io(io::Error)` | *error*'s | reading, writing, or an extraction path that is unsafe |

`From<io::Error>`, `From<FilenameEncodeError>`, `From<TryFromIntError>` (to `IntegralTruncation`)
and `From<TryReserveError>` (to `Capacity`). `source()` is the inner error of `Io` and
`FilenameEncoding`.

## I/O errors worth knowing

Some failures arrive as `Io` with `io::ErrorKind::InvalidData` and one of these messages:

| Message | When |
|---|---|
| archive path can not be extracted safely | an extracted path has a `..`, NUL or `:` component |
| archive entry has no file name | an extracted path has no components, or a BA2 entry has no name |
| deferred source file size changed before archive write | a file given to `add_file` changed size before the archive was written |

And one with `io::ErrorKind::UnexpectedEof`: a file too short for its header, or a table cut off
before its end.
