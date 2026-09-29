+++
title = "Compatibility and performance"
description = "What the version number promises, the supported Rust, the license, the features, what CI tests, and what reads and Luau calls cost."
weight = 90

[extra]
kind = "reference"
+++

## Versions

From 1.0.0, a change that breaks the Rust API or the `@dream/archive` module waits for 2.0. The
0.x releases broke things in minor versions and said how in their release notes, which are all
on the [project page](@/home/index.md).

The error enums, `Error`, `ba2::Error` and `bsa::Error`, and `FilenameEncoding` are
`#[non_exhaustive]`: a minor release may add a variant, so a `match` on one needs a wildcard arm.

## Rust and license

- **Rust 1.88** or newer, edition 2024, declared as `rust-version`. It is l3i's floor, and it
  applies with or without the `luau` feature.
- **GPL-3.0-only**, for every release so far.

## Features

| Feature | Default | Adds | Dependencies |
|---|---|---|---|
| `ba2` | yes | the `ba2` module, and BA2 in the facade | `flate2` (pure Rust backend), `lz4_flex`, `memmap2` |
| `bsa` | yes | `bsa-tes3` and `bsa-tes4` | |
| `bsa-tes3` | through `bsa` | `bsa::tes3`, and TES3 in the facade | `encoding_rs`, `memmap2` |
| `bsa-tes4` | through `bsa` | `bsa::tes4`, and TES4 in the facade | `encoding_rs`, `flate2`, `lz4_flex` with frames, `memmap2` |
| `parallel` | no | [parallel `extract_to`](@/docs/extracting.md#parallel-extraction) | `rayon` |
| `luau` | no | the [`luau` module](@/docs/luau-hosts.md); turns on `ba2` and `bsa` | `l3i` |
| `lua` | no | the old name of `luau` | |
| `luau-analysis` | no | `luau` plus l3i's `analysis`, for the crate's own typed tests | `l3i` with Luau's type checker |

`dream-path` is always a dependency. With every family turned off, what is left is
`guess_format`, `FileFormat`, `BsaFormat`, `CompressionOverride`, `Copied` and the re-exports;
`Archive`, `Error` and `detect_path` need at least one family.

Without `luau`, the crate is pure Rust. With it, l3i compiles Luau, which is C++, and requires
[l3i's toolchain](@/docs/luau-hosts.md#dependencies-and-toolchain).

## Platforms

Anything Rust and `memmap2` support. Two things differ by platform, both about file names: on
Unix, archive path bytes become file name bytes as they are, and elsewhere a name must be valid
UTF-8, both when [extracting](@/docs/extracting.md#file-names) and when `add_dir` reads a
directory.

## What is tested

Every push runs [StroggForge](https://github.com/DreamWeave-MP/StroggForge)'s library workflow:
`cargo test --all-targets --all-features` on Windows, Linux, and macOS on both Apple silicon and
Intel; Clippy at the pedantic level with warnings as errors; `rustfmt`; `cargo audit`; and a dry
run of the crates.io publish. `--all-features` includes `parallel` and `luau-analysis`.

The tests read real archives where they matter and build synthetic ones for everything else:

- **BA2**: tracked fixtures from the `bsa-rs` corpus (BSD Zero Clause): two compressed GNRL
  archives, DX10 textures and a cubemap compared with the original DDS files, Fallout 4's version
  7 and 8 archives, an archive with no string table, and six malformed archives. Starfield's
  versions 2 and 3 are tested with archives the crate writes itself.
- **TES4**: tracked version 104 and 105 archives from the same corpus, and synthetic archives for
  every name mode, both compressions, the per-file toggle, Oblivion's XMem bit, Fallout 3's padded
  name blocks, and truncated or out-of-bounds tables.
- **TES3**: synthetic archives, including hashes compared with `hash_file` for every entry.
- **Writers**: every builder's output is read back; paths that are unsafe, duplicated or too long
  are refused; output is the same whatever order members were added in.
- **Luau**: every module function and method, in a runtime where the archive type has a userdata
  tag and one where it does not; the generated type definitions checked by Luau's own type
  checker, and a `--!strict` script checked against them.

## What it costs

Measured with Criterion on one machine. The 1.0.0 figures were measured when these changes
landed, on the code paths 1.0.0 ships.

The Rust core, `cargo bench --bench core`, over archives of 4096 members:

| Operation | 0.2.3 | 1.0.0 |
|---|---:|---:|
| TES3 `contains`, 4096 distinct paths | 1.03 ms | 0.39 ms |
| TES4 `contains`, 4096 distinct paths | 1.43 ms | 0.39 ms |
| TES4 read of a 16-byte zlib member | 13.9 µs | 2.6 µs |
| TES4 read of a 64 KiB zlib member | 68 µs | 18 µs |
| BA2 read of a 16-byte zlib member | 5.4 µs | 2.9 µs |
| BA2 read of a 64 KiB zlib member | 28 µs | 20 µs |

Lookups normalize the path into a buffer on the stack instead of allocating a normalized copy,
and zlib members inflate straight from the archive's bytes through one inflater per thread.

The Luau boundary, `cargo bench --bench luau_boundary --features luau`: frozen scripts against
archives of 4096 members, with a 64 KiB member for the reads, comparing 0.2.3's `mlua` binding
with the l3i binding. Each figure is the mean time of one run of the script.

| Script | 0.2.3, mlua | 1.0.0, l3i |
|---|---:|---:|
| TES3 `entries()`, then `es[i].index` for all 4096 | 6.15 ms | 0.73 ms |
| TES3 `for _, e in entries()`, reading `e.path` | 16.8 ms | 1.10 ms |
| TES4 `entries()` indexed, and `for` | 15.6 ms, 15.4 ms | 0.68 ms, 1.06 ms |
| BA2 `entries()` indexed, and `for` | 5.3 ms, 5.3 ms | 0.67 ms, 1.07 ms |
| `contains(path)` that hits, 1000 calls, TES3 and TES4 | 281 µs, 250 µs | 146 µs, 155 µs |
| `containsHash(h)` that hits, 1000 calls, TES3 | | 87 µs |
| `readEntry(1)` of 16 bytes, 1000 calls, TES3 and BA2 | 259 µs, 246 µs | 119 µs, 112 µs |
| `readFile` of 64 KiB, 16 calls, TES3 and TES4 with zlib | 260 µs, 490 µs | 217 µs, 414 µs |
| `readInto` of 64 KiB, 16 calls, TES3 and TES4 with zlib | | 27 µs, 240 µs |

`readFile` hands the script a new string each call, and copying 64 KiB into Luau costs about as
much as finding and decoding it. `readInto` decodes into a buffer the script keeps, which is why
it is the one to use for anything read often.

Each release runs the benchmarks in CI and attaches the results to its
[GitHub release](https://github.com/DreamWeave-MP/dream_archive/releases) as `BENCHMARKS.md`.
