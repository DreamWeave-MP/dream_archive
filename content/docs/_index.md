+++
title = "Documentation"
description = "How dream_archive reads, extracts and writes BSA and BA2 archives, what each format allows, and its complete Rust and Luau API."
template = "docs/section.html"
page_template = "docs/page.html"
sort_by = "weight"

[extra]
docs_root = true
docs_project_name = "dream_archive"
docs_short_title = "dream_archive docs"
docs_project_path = "@/home/index.md"
docs_repository_url = "https://github.com/DreamWeave-MP/dream_archive/tree/main/content/docs"
docs_sidebar_label = "Documentation"
hide_child_cards = true
kind = "guide"
+++

dream_archive is a library. It opens BSA and BA2 archives, looks members up, reads and extracts
them, and writes new archives. The command-line tool built on it is
[dream_archivetool](https://DreamWeave-MP.github.io/dream_archivetool/).

## Learn it

- **[Start here](@/docs/start-here.md)**: add the crate, open an archive, read a file, extract
  everything, build an archive.
- **[Formats](@/docs/formats.md)**: the three archive families, which versions and compressions
  each has, and what is refused.

## Use it

- **[Reading archives](@/docs/reading.md)**: opening, entries and ids, lookups, and the three ways
  to read a member.
- **[Archive paths](@/docs/paths.md)**: why paths are bytes, how each family matches them, and
  legacy code pages.
- **[Hashes and hash-only archives](@/docs/hashes.md)**: every family's hash, lookup by hash, and
  extracting a TES4 archive that stored no names.
- **[Extracting](@/docs/extracting.md)**: whole archives, single members, atomic writes, parallel
  extraction and what is refused.
- **[Building archives](@/docs/building.md)**: the four builders, deferred sources, compression,
  TES4 profiles and name modes, and rewriting an archive.
- **[BA2 textures](@/docs/textures.md)**: DX10 archives, the DDS files that go in and come out, and
  the formats supported.
- **[Embedding Luau](@/docs/luau-hosts.md)**: giving scripts `@dream/archive` through l3i.
- **[Compatibility and performance](@/docs/compatibility.md)**: versions, Rust, license, features,
  what CI tests, and what reads cost.

## Look it up

- **[Rust API](@/docs/api/_index.md)**: the `Archive` facade, the `ba2`, `bsa`, `bsa::tes3` and
  `bsa::tes4` modules, and every error.
- **[Luau API](@/docs/luau/_index.md)**: the `@dream/archive` modules, the archive and entry
  types, the builders, and their type definitions.
