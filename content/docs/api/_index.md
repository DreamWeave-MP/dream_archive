+++
title = "Rust API"
description = "Every public type and function in dream_archive: the facade, the ba2, bsa, bsa::tes3 and bsa::tes4 modules, and the errors."
template = "docs/section.html"
page_template = "docs/page.html"
sort_by = "weight"
weight = 100

[extra]
kind = "api"
hide_child_cards = true
+++

| Page | Covers |
|---|---|
| [Archive facade](@/docs/api/facade.md) | `Archive`, `Entry`, `Entries`, `FileFormat`, `BsaFormat`, `guess_format`, `detect_path`, `Copied`, `CompressionOverride`, the builder aliases, and the re-exports |
| [ba2](@/docs/api/ba2.md) | `Archive`, its entries, chunks and texture headers, `ArchiveInfo` and its enums, and the BA2 hash |
| [BA2 builders](@/docs/api/ba2-builders.md) | `ba2::Builder` and `ba2::Dx10Builder` |
| [bsa](@/docs/api/bsa.md) | `FilenameEncoding`, `encode_filename`, `decode_filename_lossy`, `FilenameEncodeError`, and `NormalizedPath` |
| [bsa::tes3](@/docs/api/tes3.md) | `Archive`, `Entry`, `EntryId`, `FileRecord`, `ArchiveInfo`, the TES3 hash, and `Builder` |
| [bsa::tes4](@/docs/api/tes4.md) | `Archive`, `Entry`, `EntryId`, `FileRecord`, `ArchiveInfo` with its flags, types and versions, and the TES4 hashes |
| [TES4 builder](@/docs/api/tes4-builder.md) | `bsa::tes4::Builder`, `GameProfile` and `NameMode` |
| [Errors](@/docs/api/errors.md) | `Error`, `ba2::Error`, `bsa::Error`, `FilenameEncodeError`, and each module's `Result` |
| [Luau extension](@/docs/luau/extension.md) | the `luau` module: `ArchiveExtension`, its constants, and the userdata types |

The facade is the place to start; the format modules are for what the facade cannot say. Items
exist only with the feature of their family: `ba2` for the `ba2` module, `bsa-tes3` and
`bsa-tes4` for theirs, either for `bsa`. [Features](@/docs/compatibility.md#features) has the
list.

The three archive types share most of their methods, with the same names and meanings. Each
module's page lists them all, with the signature that module has.
