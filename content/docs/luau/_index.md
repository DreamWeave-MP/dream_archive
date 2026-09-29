+++
title = "Luau API"
description = "The @dream/archive modules scripts require, the archive, entry and builder types, their type definitions, and the extension a host composes."
template = "docs/section.html"
page_template = "docs/page.html"
sort_by = "weight"
weight = 110

[extra]
kind = "api"
hide_child_cards = true
+++

| Page | Covers |
|---|---|
| [Modules](@/docs/luau/module.md) | `@dream/archive`, `@dream/archive/ba2`, `@dream/archive/bsa`, `@dream/archive/bsa/tes3` and `@dream/archive/bsa/tes4`: opening, detecting, hashing, encodings and constants |
| [Archive and Entry](@/docs/luau/archive.md) | an opened archive's methods, entry handles, the entries view, hashes, and bytes |
| [Builders](@/docs/luau/builders.md) | the four builders: `ba2.Builder`, `ba2.Dx10Builder`, `bsa.tes3.Builder` and `bsa.tes4.Builder` |
| [Type definitions](@/docs/luau/types.md) | the `.d.luau` text the plan generates for all of it |
| [ArchiveExtension](@/docs/luau/extension.md) | the Rust side: the l3i extension a host composes, its constants, and the userdata types |

[Embedding Luau](@/docs/luau-hosts.md) sets a host up. Everything here mirrors the Rust API with
camelCase names; where the two differ, these pages say so.

## Conventions

- **Archive paths are Luau strings, read as bytes.** Any bytes work, UTF-8 or not, and paths come
  back the same way.
- **Host file system paths must be valid UTF-8.** `openPath`, `extractTo`, `writePath`, `addFile`
  and the other calls that name a file on disk raise "not valid UTF-8" otherwise, and so does text
  given to `encodeFilename` and `addEncodedPath`.
- **Payloads are a `buffer` or a string**, whichever the script has. Payloads come back as strings,
  except through `archive:readInto`, which writes into a buffer, and `Builder:toBuffer`.
- **Indices start at 1**, and integer arguments must be exact: `readEntry(1.5)` is an error, not
  a rounded read.
- **Missing is `nil`, broken is an error.** A lookup that finds nothing returns `nil`; the
  `...Required` calls, bad arguments, and every archive or I/O failure raise an error whose message
  is the Rust error's.
- **Everything is frozen.** Module tables, constant tables and `Builder` tables cannot be written
  to.
