# Notes for whoever works on dream_archive next

Found on 2026-09-29 while writing the site's documentation against the source. The repository was
being changed by other agents at the time, so nothing here was fixed; the site describes the code
as it is, and says so where the behavior below shows through. Every item was reproduced against
the working tree at 3db40dc.

## Bugs

### DDS files whose depth field is 1 are refused as volume textures

`parse_header` (`src/dds/mod.rs:229`) treats any non-zero `dwDepth` as a volume texture. The DDS
format only gives that field meaning when `DDSD_DEPTH` is in `dwFlags` or `DDSCAPS2_VOLUME` is in
`dwCaps2`, and many tools write 1 there for ordinary 2D textures. The repository's own fixture is
one of them: `tests/fixtures/ba2/dds/Fence006_1K_Roughness.dds` has depth 1, flags `0xa1007`
(no `DDSD_DEPTH`) and caps2 0.

```rust
let dds = std::fs::read("tests/fixtures/ba2/dds/Fence006_1K_Roughness.dds")?;
let mut builder = dream_archive::Ba2Dx10Builder::new();
builder.add_dds_bytes("textures/fence.dds", &dds)?;
// Err(Dds("unsupported DDS volume texture"))
```

So `Dx10Builder::add_dds_file` and `add_dds_bytes`, `examples/build_ba2_dx10.rs`, and Luau's
`addDdsBytes`/`addDdsFile` refuse DDS files that the crate's own extraction would otherwise round
trip (extraction writes depth 0, which is why the tests pass). Suggested rule: a volume texture is
`DDSCAPS2_VOLUME`, or `DDSD_DEPTH` with a depth above 1.

### DX10-header cubemaps use an array size of 6; the DDS format says 1

`parse_dx10_format` (`src/dds/mod.rs:292`) requires `arraySize == 6` when `miscFlag` has
`DDS_RESOURCE_MISC_TEXTURECUBE`, and `base_fields` (`src/dds/mod.rs:502`) writes 6 on extraction.
In `DDS_HEADER_DXT10`, `arraySize` counts cubes, not faces (DirectXTex writes `arraySize / 6` and
multiplies by 6 when reading). Consequences:

- A BC6H/BC7/sRGB cubemap saved by texconv or any DirectXTex-based tool (arraySize 1) is refused:
  `Dds("unsupported DDS array size")`.
- A DX10 cubemap extracted from a BA2 declares six cubes, so DirectXTex-based readers expect 36
  faces and reject or misread the file.

Legacy-header cubemaps (`DXT1`/`DXT5`, `caps2` cube bits) are unaffected; the only real cubemap
fixture (`tests/fixtures/ba2/cubemap/blacksky_e.dds`) is DXT5, and the synthetic tests
`ba2_dx10_writer_ingests_dx10_cubemap_dds` and `synthetic_cubemap_sets_dds_cube_metadata` in
`tests/ba2_synthetic.rs` pin the 6. Repro: a 4x4 BC7 cubemap DDS with a DX10 header, dimension 3,
`miscFlag` 4, `arraySize` 1, and 96 bytes of data fails `add_dds_bytes`; with `arraySize` 6 it is
accepted and extracts with 6.

### The facade matches paths differently per family

`Archive::get`/`contains`/`read_file` promise one lookup for every family, but BSA lookups use
dream-path's rules (`with_lookup_path`, `src/bsa/mod.rs:61`) and BA2 lookups the BA2 hash
normalization (`with_normalized`, `src/ba2/hash.rs:110`):

| Query for a stored `meshes\door.nif` | TES3 / TES4 | BA2 |
|---|---|---|
| `meshes//door.nif` | found | not found (interior repeats kept) |
| `meshes/door.nif/` | not found (trailing separator kept) | found (trimmed) |

A caller switching between a Morrowind BSA and a Fallout 4 BA2 gets different answers for the same
string. Hash-only TES4 lookups (`path_hash`, `src/bsa/tes4/archive.rs:967`) follow the hash
normalization too, so `a//b/c.nif` finds nothing in a hash-only archive and finds the entry in a
named one. If the difference is intended because each follows its engine, the facade's docs should
say so; the site currently documents the table above as the behavior.

### Luau `entry.name` is the whole path for BA2

`Entry::name` in `src/luau.rs:421` returns the BA2 entry's whole stored name, while TES3 returns
the part after the last separator and TES4 the stored file name. For a BA2 entry
`meshes\foo.nif`, `entry.name == entry.path == "meshes\\foo.nif"` and `entry.folder == "meshes"`.
`folder` is derived from the name for BA2 and TES3, so `name` should be the last component there
too.

## API gaps and inconsistencies

- `set_zlib_level` on all three compressing builders takes `flate2::Compression`
  (`src/ba2/builder.rs:102` and the DX10 and TES4 builders), but `flate2` is not re-exported, so a
  caller must add its own `flate2` dependency, at a compatible major version, just to name a level.
  Re-export it, or take a `u32` level.
- Luau: only `ba2.Builder` has `toBuffer()` (`src/luau.rs:1331`); `ba2.Dx10Builder`,
  `bsa.tes3.Builder` and `bsa.tes4.Builder` do not. The old README said every builder had it.
- Luau: a root-level entry's `folder` is `nil` for TES3 and BA2 but `""` for TES4.
- `bsa::Error::InvalidFileRecordFlags` (`src/bsa/mod.rs:121`) is never constructed.
- rustdoc: `tes4::Builder::add_archive_entry_with_compression` (`src/bsa/tes4/builder.rs:366`) has
  no summary line, and `to_vec` and `write_seek` each carry two `# Errors` sections
  (`src/bsa/tes4/builder.rs:458`–`479`).

## Metadata

- `Cargo.toml:8` still points `documentation` at `https://dreamweave-mp.github.io/dream-archive/`,
  the old rustdoc Pages address with a hyphen. With `mod_template: true` StroggForge deploys the
  site to `https://dreamweave-mp.github.io/dream_archive/` instead, which is what crates.io should
  link. `repository` (`Cargo.toml:7`) spells the repository `dream-archive`; GitHub redirects the
  old name, but `dream_archive` is the real one.
- `l3i` is a path dependency (`l3i = { version = "1.0.0", path = "../dream-binder" }`). CI has no
  sibling checkout, so any job that resolves the manifest fails, and 1.0.0 cannot be published to
  crates.io until l3i 1.0.0 is on crates.io.
